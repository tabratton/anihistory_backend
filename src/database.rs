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
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use chrono::NaiveDate;
use futures_util::TryStreamExt;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use tracing::{error, info};

struct ListResult {
    user_id: i32,
    name: String,
    avatar_s3: String,
    avatar_anilist: String,
    anime_id: i32,
    description: String,
    cover_s3: String,
    cover_anilist: String,
    average: Option<i16>,
    native: Option<String>,
    romaji: Option<String>,
    english: Option<String>,
    user_title: Option<String>,
    start_day: Option<NaiveDate>,
    end_day: Option<NaiveDate>,
    score: Option<i16>,
}

impl From<&ListResult> for ListItemMap {
    fn from(list: &ListResult) -> Self {
        let user = models::User {
            user_id: list.user_id,
            name: list.name.clone(),
            avatar_s3: list.avatar_s3.clone(),
            avatar_anilist: list.avatar_anilist.clone(),
        };

        let anime = models::Anime {
            anime_id: list.anime_id,
            description: list.description.clone(),
            cover_s3: list.cover_s3.clone(),
            cover_anilist: list.cover_anilist.clone(),
            average: list.average,
            native: list.native.clone(),
            romaji: list.romaji.clone(),
            english: list.english.clone(),
        };

        let list_item = ListItem {
            user_id: list.user_id,
            anime_id: list.anime_id,
            user_title: list.user_title.clone(),
            start_day: list.start_day,
            end_day: list.end_day,
            score: list.score,
        };

        ListItemMap {
            user,
            anime,
            list_item,
        }
    }
}

pub async fn get_list(
    name: &str,
    pool: &DbPool,
) -> Result<Option<models::RestResponse>, anyhow::Error> {
    let query = sqlx::query_as!(
        ListResult,
        "SELECT u.user_id as user_id, u.name as name, u.avatar_s3 as avatar_s3, u.avatar_anilist as avatar_anilist, a.anime_id as anime_id, a.description as description, a.cover_s3 as cover_s3, a.cover_anilist as cover_anilist, a.average as average, a.native as native, a.romaji as romaji, a.english as english, l.user_title as user_title, l.start_day as start_day, l.end_day as end_day, l.score as score FROM lists as l INNER JOIN users as u ON l.user_id=u.user_id INNER JOIN anime as a ON l.anime_id=a.anime_id WHERE u.name = $1",
        &name
    )
    .fetch_all(pool);
    let database_list: Vec<ListItemMap> = match query.await {
        Ok(rows) => rows.iter().map(|row| row.into()).collect(),
        Err(error) => {
            error!(
                "error getting list for user_name={}. Error: {}",
                name, error
            );
            return Err(anyhow!(error));
        }
    };

    if database_list.is_empty() {
        return Ok(None);
    }

    Ok(Some(models::RestResponse {
        users: models::ResponseList {
            id: database_list[0].user.name.clone(),
            avatar: database_list[0].user.avatar_s3.clone(),
            list: database_list.iter().map(|l| l.into()).collect(),
        },
    }))
}

pub async fn update_user_profile(
    user: anilist_models::User,
    pool: &DbPool,
    s3_client: Arc<Client>,
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

    let result = sqlx::query!(
        "INSERT INTO users (user_id, name, avatar_s3, avatar_anilist) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id) DO UPDATE SET name = excluded.name, avatar_s3 = excluded.avatar_s3, avatar_anilist = excluded.avatar_anilist",
        &new_user.user_id,
        &new_user.name,
        &new_user.avatar_s3,
        &new_user.avatar_anilist,
    )
    .execute(pool);

    // Download their avatar and upload to S3.
    let content = download_image(&user.avatar.large).await?;
    upload_to_s3(s3_client, ImageTypes::User, user.id, ext.clone(), content).await?;

    if let Err(err) = result.await {
        let error = format!("error saving user={:?}. Error: {}", new_user, err);
        error!(error);
        Err(anyhow!(error))
    } else {
        Ok(())
    }
}

pub async fn delete_entries(
    mut lists: Vec<anilist_models::MediaList>,
    id: i32,
    pool: &DbPool,
) -> Result<(), anyhow::Error> {
    let mut used_lists = Vec::new();

    for list in lists.iter_mut().filter(|list| {
        list.name.to_lowercase().contains("completed")
            || list.name.to_lowercase().contains("watching")
    }) {
        list.entries
            .sort_unstable_by(|a, b| a.media.id.cmp(&b.media.id));
        used_lists.push(list.clone());
    }

    let mut user_db_list_result = sqlx::query_as!(
        ListItem,
        "SELECT user_id, anime_id, user_title, start_day, end_day, score FROM lists WHERE user_id = $1",
        &id
    )
    .fetch(pool);

    while let Some(list_item) = user_db_list_result.try_next().await? {
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
            let delete_result = sqlx::query!(
                "DELETE FROM lists WHERE user_id = $1 AND anime_id = $2",
                &list_item.user_id,
                &list_item.anime_id
            )
            .execute(pool)
            .await;

            if delete_result.is_err() {
                error!(
                    "error deleting list_entry={:?}. Error: {}",
                    list_item,
                    delete_result.expect_err("?")
                );
            }
        }
    }

    Ok(())
}

pub async fn update_entries(
    id: i32,
    pool: DbPool,
    s3_client: Arc<Client>,
) -> Result<(), anyhow::Error> {
    let lists = anilist_query::get_lists(id).await?;

    delete_entries(lists.clone(), id, &pool).await?;

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

            let anime_result = sqlx::query!(
                "INSERT INTO anime (anime_id, description, cover_s3, cover_anilist, average, native, romaji, english) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (anime_id) DO UPDATE SET description = excluded.description, cover_s3 = excluded.cover_s3, cover_anilist = excluded.cover_anilist, average = excluded.average, native = excluded.native, romaji = excluded.romaji, english = excluded.english",
                &new_anime.anime_id,
                &new_anime.description,
                &new_anime.cover_s3,
                &new_anime.cover_anilist,
                new_anime.average,
                new_anime.native,
                new_anime.romaji,
                new_anime.english,
            )
            .execute(&pool)
            .await;

            match anime_result {
                Ok(_) => {
                    // Download cover images and upload to S3.
                    let content = download_image(&entry.media.cover_image.large).await?;
                    let closure_id = entry.media.id;
                    let closure_ext = ext.clone();
                    let client = s3_client.clone();
                    tokio::spawn(async move {
                        match upload_to_s3(
                            client,
                            ImageTypes::Anime,
                            closure_id,
                            closure_ext,
                            content,
                        )
                        .await
                        {
                            Ok(()) => (),
                            Err(error) => error!("error uploading to S3: {error}"),
                        }
                    });
                }
                Err(error) => {
                    error!("error saving anime={:?}. Error: {}", new_anime, error);
                }
            }

            let start = construct_date(entry.started_at.clone());
            let end = construct_date(entry.completed_at.clone());

            let new_list = ListItem {
                user_id: id,
                anime_id: entry.media.id,
                user_title: entry.media.title.user_preferred.clone(),
                start_day: start,
                end_day: end,
                score: entry.score_raw,
            };

            let list_result = sqlx::query!(
              "INSERT INTO lists (user_id, anime_id, user_title, start_day, end_day, score) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id, anime_id) DO UPDATE SET user_title = excluded.user_title, start_day = excluded.start_day, end_day = excluded.end_day, score = excluded.score",
                &new_list.user_id,
                &new_list.anime_id,
                new_list.user_title,
                new_list.start_day,
                new_list.end_day,
                new_list.score,
            )
            .execute(&pool)
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

static BUCKET_NAME: &str = "anihistory-images";

async fn upload_to_s3(
    client: Arc<Client>,
    prefix: ImageTypes,
    id: i32,
    ext: String,
    content: Vec<u8>,
) -> Result<(), anyhow::Error> {
    let key = format!("assets/images/{prefix}_{id}.{ext}");

    let body = ByteStream::from(content);
    match client
        .put_object()
        .bucket(BUCKET_NAME)
        .key(key)
        .content_type(naive_mime(&ext))
        .body(body)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(error) => {
            error!("error uploading assets/images/{prefix}_{id}.{ext} to S3. Error: {error}",);
            Err(error)?
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

impl Display for ImageTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageTypes::Anime => write!(f, "anime"),
            ImageTypes::User => write!(f, "user"),
        }
    }
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
