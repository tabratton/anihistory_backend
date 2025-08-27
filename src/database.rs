/*
 * Copyright (c) 2018, Tyler Bratton
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::models::{ListItem, ListItemMap, ResponseItem};
use crate::{DbPool, anilist_models, anilist_query, models};
use anyhow::anyhow;
use chrono::NaiveDate;
use rusoto_core::Region;
use rusoto_s3::{PutObjectRequest, S3, S3Client};
use tokio_postgres::Row;
use tracing::{error, info};

pub async fn get_list(
    name: &str,
    pool: &DbPool,
) -> Result<Option<models::RestResponse>, anyhow::Error> {
    let connection = pool.get().await?;
    let stmt = connection
        .prepare(
            "SELECT u.user_id, u.name, u.avatar_s3, u.avatar_anilist, a.anime_id, a\
	  .description, a.cover_s3, a.cover_anilist, a.average, a.native, a.romaji, a.english, l\
	  .user_title, l.start_day, l.end_day, l.score FROM lists as l INNER JOIN users as u ON l\
	  .user_id=u.user_id INNER JOIN anime as a ON l.anime_id=a.anime_id WHERE u.name = $1",
        )
        .await?;

    match connection.query(&stmt, &[&name]).await {
        Ok(result) => {
            // TODO: write from/into for these structs
            let database_list: Vec<models::ListItemMap> = result
                .iter()
                .map(|row| {
                    let user = models::User {
                        user_id: row.get(0),
                        name: row.get(1),
                        avatar_s3: row.get(2),
                        avatar_anilist: row.get(3),
                    };

                    let anime = models::Anime {
                        anime_id: row.get(4),
                        description: row.get(5),
                        cover_s3: row.get(6),
                        cover_anilist: row.get(7),
                        average: row.get(8),
                        native: row.get(9),
                        romaji: row.get(10),
                        english: row.get(11),
                    };

                    let list_item = models::ListItem {
                        user_id: row.get(0),
                        anime_id: row.get(4),
                        user_title: row.get(12),
                        start_day: row.get(13),
                        end_day: row.get(14),
                        score: row.get(15),
                    };

                    models::ListItemMap {
                        user,
                        anime,
                        list_item,
                    }
                })
                .collect();

            if !database_list.is_empty() {
                let response_items: Vec<ResponseItem> =
                    database_list.iter().map(|l| l.into()).collect();
                Ok(Some(models::RestResponse {
                    users: models::ResponseList {
                        id: database_list[0].user.name.clone(),
                        avatar: database_list[0].user.avatar_s3.clone(),
                        list: response_items,
                    },
                }))
            } else {
                Ok(None)
            }
        }
        Err(error) => {
            error!(
                "error getting list for user_name={}. Error: {}",
                name, error
            );
            Err(anyhow!(error))
        }
    }
}

pub async fn update_user_profile(
    user: anilist_models::User,
    pool: &DbPool,
) -> Result<(), anyhow::Error> {
    let ext = get_ext(&user.avatar.large);

    let new_user = models::User {
        user_id: user.id,
        name: user.name.clone(),
        avatar_s3: format!(
            "https://s3.amazonaws.com/anihistory-images/assets/images/user_{}.{}",
            user.id, ext
        ),
        avatar_anilist: user.avatar.large.clone(),
    };

    let connection = pool.get().await?;
    let stmt = connection.prepare("INSERT INTO users (user_id, name, avatar_s3, avatar_anilist) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id) DO UPDATE SET name = excluded.name, avatar_s3 = excluded.avatar_s3, avatar_anilist = excluded.avatar_anilist").await?;

    let result = connection
        .execute(
            &stmt,
            &[
                &new_user.user_id,
                &new_user.name,
                &new_user.avatar_s3,
                &new_user.avatar_anilist,
            ],
        )
        .await;

    // Download their avatar and upload to S3.
    let content = download_image(&user.avatar.large).await?;
    upload_to_s3(ImageTypes::User, user.id, ext.clone(), content).await;

    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            let error = format!("error saving user={:?}. Error: {}", new_user, err);
            error!(error);
            Err(anyhow!(error))
        }
    }
}

pub async fn delete_entries(
    mut lists: Vec<anilist_models::MediaList>,
    id: i32,
    pool: &DbPool,
) -> Result<(), anyhow::Error> {
    let connection = pool.get().await?;
    let mut used_lists = Vec::new();

    for list in lists.iter_mut().filter(|list| {
        list.name.to_lowercase().contains("completed")
            || list.name.to_lowercase().contains("watching")
    }) {
        list.entries
            .sort_unstable_by(|a, b| a.media.id.cmp(&b.media.id));
        used_lists.push(list.clone());
    }

    let stmt = connection.prepare("SELECT user_id, anime_id, user_title, start_day, end_day, score FROM lists WHERE user_id = $1").await?;

    let user_db_list_result = connection.query(&stmt, &[&id]).await;

    match user_db_list_result {
        Ok(rows) => {
            for row in rows.iter() {
                let list_item: ListItem = row.into();

                let mut found = false;

                for list in used_lists.clone() {
                    let result = list
                        .entries
                        .binary_search_by(|e| e.media.id.cmp(&list_item.anime_id));
                    if result.is_ok() {
                        found = true;
                        break;
                    }
                }

                if !found {
                    println!("deleting anime:{}", list_item.anime_id);
                    let stmt = connection
                        .prepare("DELETE FROM lists WHERE user_id = $1 AND anime_id = $2")
                        .await?;

                    let delete_result = connection
                        .execute(&stmt, &[&list_item.user_id, &list_item.anime_id])
                        .await;

                    if delete_result.is_err() {
                        error!(
                            "error deleting list_entry={:?}. Error: {}",
                            row,
                            delete_result.expect_err("?")
                        );
                    }
                }
            }

            Ok(())
        }
        Err(err) => {
            let error = format!("error retrieving list for user_id={:?}. Error: {}", id, err);
            error!(error);
            Err(anyhow!(error))
        }
    }
}

pub async fn update_entries(id: i32, pool: DbPool) -> Result<(), anyhow::Error> {
    let lists = anilist_query::get_lists(id).await?;

    delete_entries(lists.clone(), id, &pool).await?;
    let connection = pool.get().await?;

    for list in lists.iter().filter(|list| {
        list.name.to_lowercase().contains("completed")
            || list.name.to_lowercase().contains("watching")
    }) {
        for entry in list.entries.iter() {
            let ext = get_ext(&entry.media.cover_image.large);

            let new_anime = models::Anime {
                anime_id: entry.media.id,
                description: entry.media.description.clone(),
                cover_s3: format!(
                    "https://s3.amazonaws.com/anihistory-images/assets/images/anime_{}.{}",
                    entry.media.id, ext
                ),
                cover_anilist: entry.media.cover_image.large.clone(),
                average: entry.media.average_score,
                native: entry.media.title.native.clone(),
                romaji: entry.media.title.romaji.clone(),
                english: entry.media.title.english.clone(),
            };

            let stmt = connection.prepare("INSERT INTO anime (anime_id, description, cover_s3, cover_anilist, average, native, romaji, english) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (anime_id) DO UPDATE SET description = excluded.description, cover_s3 = excluded.cover_s3, cover_anilist = excluded.cover_anilist, average = excluded.average, native = excluded.native, romaji = excluded.romaji, english = excluded.english").await?;

            let anime_result = connection
                .execute(
                    &stmt,
                    &[
                        &new_anime.anime_id,
                        &new_anime.description,
                        &new_anime.cover_s3,
                        &new_anime.cover_anilist,
                        &new_anime.average,
                        &new_anime.native,
                        &new_anime.romaji,
                        &new_anime.english,
                    ],
                )
                .await;

            match anime_result {
                Ok(_) => {
                    // Download cover images and upload to S3.
                    let content = download_image(&entry.media.cover_image.large).await?;
                    let closure_id = entry.media.id;
                    let closure_ext = ext.clone();
                    tokio::spawn(async move {
                        upload_to_s3(ImageTypes::Anime, closure_id, closure_ext, content).await;
                    });
                }
                Err(error) => {
                    error!("error saving anime={:?}. Error: {}", new_anime, error);
                }
            }

            let start = construct_date(entry.started_at.clone());
            let end = construct_date(entry.completed_at.clone());

            let new_list = models::ListItem {
                user_id: id,
                anime_id: entry.media.id,
                user_title: entry.media.title.user_preferred.clone(),
                start_day: start,
                end_day: end,
                score: entry.score_raw,
            };

            let stmt = connection.prepare("INSERT INTO lists (user_id, anime_id, user_title, start_day, end_day, score) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id, anime_id) DO UPDATE SET user_title = excluded.user_title, start_day = excluded.start_day, end_day = excluded.end_day, score = excluded.score").await?;

            let list_result = connection
                .execute(
                    &stmt,
                    &[
                        &new_list.user_id,
                        &new_list.anime_id,
                        &new_list.user_title,
                        &new_list.start_day,
                        &new_list.end_day,
                        &new_list.score,
                    ],
                )
                .await;

            if list_result.is_err() {
                error!(
                    "error saving list_entry={:?}. Error: {}",
                    new_list,
                    list_result.expect_err("?")
                );
            }
        }
    }

    info!("Database updated for user_id={}", id);

    Ok(())
}

async fn upload_to_s3(prefix: ImageTypes, id: i32, ext: String, content: Vec<u8>) {
    let image_prefix = match prefix {
        ImageTypes::Anime => "anime",
        ImageTypes::User => "user",
    };

    let client = S3Client::new(Region::UsEast1);
    let bucket_name = "anihistory-images";
    let mime = naive_mime(&ext);
    let key = format!("assets/images/{}_{}.{}", image_prefix, id, ext);

    let put_request = PutObjectRequest {
        bucket: bucket_name.to_owned(),
        key: key.clone(),
        body: Some(content.into()),
        content_type: Some(mime),
        acl: Some("public-read".to_owned()),
        ..PutObjectRequest::default()
    };

    match client.put_object(put_request).await {
        Ok(_) => (),
        Err(error) => {
            error!(
                "error uploading assets/images/{}_{}.{} to S3. Error: {}",
                image_prefix, id, ext, error
            );
        }
    }
}

fn construct_date(date: anilist_models::Date) -> Option<NaiveDate> {
    match date.year {
        Some(year) => match date.month {
            Some(month) => match date.day {
                Some(day) => NaiveDate::from_ymd_opt(year, month as u32, day as u32),
                None => None,
            },
            None => None,
        },
        None => None,
    }
}

async fn download_image(url: &String) -> Result<Vec<u8>, anyhow::Error> {
    Ok(reqwest::get(url).await?.bytes().await?.into())
}

fn get_ext(url: &str) -> String {
    let link_parts: Vec<&str> = url.split('/').collect();
    let splitted: Vec<&str> = link_parts[link_parts.len() - 1].split(".").collect();
    splitted[1].to_owned()
}

fn naive_mime(ext: &String) -> String {
    if ext.contains("jp") {
        "image/jpeg".to_owned()
    } else {
        format!("image/{}", ext)
    }
}

enum ImageTypes {
    Anime,
    User,
}

impl From<&ListItemMap> for ResponseItem {
    fn from(list_item: &ListItemMap) -> Self {
        Self {
            user_title: list_item.list_item.user_title.clone(),
            start_day: list_item.list_item.start_day,
            end_day: list_item.list_item.end_day,
            score: list_item.list_item.score,
            average: list_item.anime.average,
            native: list_item.anime.native.clone(),
            romaji: list_item.anime.romaji.clone(),
            english: list_item.anime.english.clone(),
            description: list_item.anime.description.clone(),
            cover: list_item.anime.cover_s3.clone(),
            id: list_item.anime.anime_id,
        }
    }
}

impl From<&Row> for ListItem {
    fn from(row: &Row) -> Self {
        Self {
            user_id: row.get(0),
            anime_id: row.get(1),
            user_title: row.get(2),
            start_day: row.get(3),
            end_day: row.get(4),
            score: row.get(5),
        }
    }
}
