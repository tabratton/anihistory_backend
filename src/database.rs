use chrono::NaiveDate;
use diesel;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use reqwest::get;
use rusoto_core::Region;
use rusoto_s3::{PutObjectRequest, S3Client, S3, StreamingBody};
use schema::anime;
use schema::lists;
use schema::users;
use std::io::Read;
use std::{env, thread};

use anilist_query;
use models;
use query_structs;

pub fn get_user(username: String) -> Result<models::User, diesel::result::Error> {
    let connection = establish_connection();
    users::table
        .filter(users::name.eq(username))
        .first(&connection)
}

pub fn update_user_profile(user: query_structs::User) {
    // Download their avatar and upload to S3.
    let mut content = Vec::new();
    let ext = download_image(&mut content, &user.avatar.large);
    let new_link = user.avatar.large.clone();
    upload_to_s3(ImageTypes::User, user.id, ext.clone(), content, new_link);

    // Connect to DB and upsert user entry.
    let connection = establish_connection();

    let new_user = models::User {
        user_id: user.id.clone(),
        name: user.name.clone(),
        avatar_s3: format!(
            "https://s3.amazonaws.com/anihistory-images/user_{}.{}",
            user.id, ext
        ),
        avatar_anilist: user.avatar.large.clone(),
    };

    diesel::insert_into(users::table)
        .values(&new_user)
        .on_conflict(users::user_id)
        .do_update()
        .set(&new_user)
        .execute(&connection)
        .expect("Error saving new user");
}

pub fn update_entries(id: i32) {
    let lists: Vec<query_structs::MediaList> = anilist_query::get_lists(id);

    for list in lists {
        if list.name == "Completed" || list.name == "Watching" {
            for entry in list.entries {
                // Download cover images and upload to S3.
                let mut content = Vec::new();
                let ext = download_image(&mut content, &entry.media.cover_image.large);
                let closure_id = entry.media.id.clone();
                let closure_ext = ext.clone();
                let new_link = entry.media.cover_image.large.clone();
                thread::spawn(move || {
                    upload_to_s3(
                        ImageTypes::Anime,
                        closure_id,
                        closure_ext,
                        content,
                        new_link,
                    )
                });

                // Connect to DB and upsert anime and list entries.
                let connection = establish_connection();

                let new_anime = models::Anime {
                    anime_id: entry.media.id,
                    description: entry.media.description,
                    cover_s3: format!(
                        "https://s3.amazonaws.com/anihistory-images/anime_{}.{}",
                        entry.media.id, ext
                    ),
                    cover_anilist: entry.media.cover_image.large.clone(),
                    average: entry.media.average_score,
                    native: entry.media.title.native,
                    romaji: entry.media.title.romaji,
                    english: entry.media.title.english,
                };

                diesel::insert_into(anime::table)
                    .values(&new_anime)
                    .on_conflict(anime::anime_id)
                    .do_update()
                    .set(&new_anime)
                    .execute(&connection)
                    .expect("Error saving new anime");

                let start = construct_date(entry.started_at);
                let end = construct_date(entry.completed_at);

                let new_list = models::List {
                    user_id: id,
                    anime_id: entry.media.id,
                    user_title: entry.media.title.user_preferred,
                    start_day: start,
                    end_day: end,
                    score: entry.score_raw,
                };

                diesel::insert_into(lists::table)
                    .values(&new_list)
                    .on_conflict((lists::anime_id, lists::user_id))
                    .do_update()
                    .set(&new_list)
                    .execute(&connection)
                    .expect("Error saving new anime");
            }
        }
    }
    println!("Database updated for: {}", id);
}

fn upload_to_s3(prefix: ImageTypes, id: i32, ext: String, content: Vec<u8>, new_anilist: String) {
    let image_prefix: String;
    let connection = establish_connection();
    match prefix {
        ImageTypes::Anime => {
            image_prefix = "anime".to_owned();
            let result_anime = anime::table
                .filter(anime::anime_id.eq(id))
                .first::<models::Anime>(&connection);
            match result_anime {
                Ok(anime) => {
                    if anime.cover_anilist == new_anilist {
                        return;
                    }
                }
                _ => (),
            };
        }
        ImageTypes::User => {
            image_prefix = "user".to_owned();
            let result_user = users::table
                .filter(users::user_id.eq(id))
                .first::<models::User>(&connection);
            match result_user {
                Ok(user) => {
                    if user.avatar_anilist == new_anilist {
                        return;
                    }
                }
                _ => (),
            };
        }
    };

    let client = S3Client::new(Region::UsEast1);
    let bucket_name = "anihistory-images";
    let mime = naive_mime(&ext);
    let key = format!("{}_{}.{}", image_prefix, id, ext);

    let put_request = PutObjectRequest {
        bucket: bucket_name.to_owned(),
        key: key.clone(),
        body: Some(content.into()),
        content_type: Some(mime),
        acl: Some("public-read".to_owned()),
        ..PutObjectRequest::default()
    };

    match client.put_object(put_request).sync() {
        Ok(_) => {
            println!("{}_{}.{}", image_prefix, id, ext);
        }
        Err(error) => {
            println!("Error: {}", error);
        }
    }
}

fn construct_date(date: query_structs::Date) -> Option<NaiveDate> {
    match date.year {
        Some(year) => match date.month {
            Some(month) => match date.day {
                Some(day) => Some(NaiveDate::from_ymd(year, month as u32, day as u32)),
                None => None,
            },
            None => None,
        },
        None => None,
    }
}

fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}

fn download_image(content: &mut Vec<u8>, url: &String) -> String {
    let mut resp = get(url).unwrap();
    resp.read_to_end(content).unwrap();

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