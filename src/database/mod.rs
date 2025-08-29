use crate::database::models::{
    Anime, ListItem, ListItemMap, ListResult, ResponseList, RestResponse, User,
};
use crate::s3::{ImageTypes, S3Client};
use crate::{anilist_models, anilist_query, get_ext};
use anyhow::anyhow;
use chrono::NaiveDate;
use futures_util::TryStreamExt;
use futures_util::stream::BoxStream;
use sqlx::postgres::{PgPoolOptions, PgQueryResult};
use sqlx::{Pool, Postgres};
use tracing::{error, info};

mod models;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Postgres>,
}

impl Database {
    pub async fn try_new() -> Result<Self, anyhow::Error> {
        let postgres_url = std::env::var("DATABASE_URL")?;
        let db_pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(postgres_url.as_ref())
            .await?;

        Ok(Self { pool: db_pool })
    }

    pub async fn get_list_for_username(&self, name: &str) -> Result<Vec<ListResult>, sqlx::Error> {
        sqlx::query_as!(
            ListResult,
            "SELECT u.user_id as user_id, u.name as name, u.avatar_s3 as avatar_s3, u.avatar_anilist as avatar_anilist, a.anime_id as anime_id, a.description as description, a.cover_s3 as cover_s3, a.cover_anilist as cover_anilist, a.average as average, a.native as native, a.romaji as romaji, a.english as english, l.user_title as user_title, l.start_day as start_day, l.end_day as end_day, l.score as score FROM lists as l INNER JOIN users as u ON l.user_id=u.user_id INNER JOIN anime as a ON l.anime_id=a.anime_id WHERE u.name = $1",
            &name
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn insert_user(&self, new_user: &User) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
            "INSERT INTO users (user_id, name, avatar_s3, avatar_anilist) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id) DO UPDATE SET name = excluded.name, avatar_s3 = excluded.avatar_s3, avatar_anilist = excluded.avatar_anilist",
            &new_user.user_id,
            &new_user.name,
            &new_user.avatar_s3,
            &new_user.avatar_anilist,
        )
        .execute(&self.pool)
        .await
    }

    pub async fn insert_anime(&self, new_anime: &Anime) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
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
        .execute(&self.pool)
        .await
    }

    pub async fn insert_list(&self, new_list: &ListItem) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
          "INSERT INTO lists (user_id, anime_id, user_title, start_day, end_day, score) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id, anime_id) DO UPDATE SET user_title = excluded.user_title, start_day = excluded.start_day, end_day = excluded.end_day, score = excluded.score",
          &new_list.user_id,
          &new_list.anime_id,
          new_list.user_title,
          new_list.start_day,
          new_list.end_day,
          new_list.score,
        )
        .execute(&self.pool)
        .await
    }

    pub fn get_list_stream(&self, id: i32) -> BoxStream<'_, Result<ListItem, sqlx::Error>> {
        sqlx::query_as!(
            ListItem,
            "SELECT user_id, anime_id, user_title, start_day, end_day, score FROM lists WHERE user_id = $1",
            &id
        )
        .fetch(&self.pool)
    }

    pub async fn delete_from_list(
        &self,
        user_id: i32,
        anime_id: i32,
    ) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
            "DELETE FROM lists WHERE user_id = $1 AND anime_id = $2",
            &user_id,
            &anime_id
        )
        .execute(&self.pool)
        .await
    }
}

pub async fn get_list(name: &str, db: &Database) -> Result<Option<RestResponse>, anyhow::Error> {
    let database_list: Vec<ListItemMap> = match db.get_list_for_username(name).await {
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

    Ok(Some(RestResponse {
        users: ResponseList {
            id: database_list[0].user.name.clone(),
            avatar: database_list[0].user.avatar_s3.clone(),
            list: database_list.iter().map(|l| l.into()).collect(),
        },
    }))
}

pub async fn update_user_profile(
    user: anilist_models::User,
    db: &Database,
    s3_client: S3Client,
) -> Result<(), anyhow::Error> {
    let ext = get_ext(&user.avatar.large);

    // Download their avatar and upload to S3.
    s3_client
        .upload_to_s3(ImageTypes::User, user.id, &user.avatar.large)
        .await?;

    let new_user = User {
        user_id: user.id,
        name: user.name,
        avatar_s3: format!(
            "https://s3.amazonaws.com/anihistory-images/assets/images/user_{}.{}",
            user.id, ext
        ),
        avatar_anilist: user.avatar.large,
    };

    if let Err(err) = db.insert_user(&new_user).await {
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
    db: &Database,
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

    let mut user_db_list_result = db.get_list_stream(id);
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
            info!("deleting anime:{}", list_item.anime_id);

            if let Err(error) = db
                .delete_from_list(list_item.user_id, list_item.anime_id)
                .await
            {
                error!(
                    "error deleting list_entry={:?}. Error: {}",
                    list_item, error
                );
            }
        }
    }

    Ok(())
}

pub async fn update_entries(
    id: i32,
    db: &Database,
    s3_client: S3Client,
) -> Result<(), anyhow::Error> {
    let lists = anilist_query::get_lists(id).await?;

    delete_entries(lists.clone(), id, db).await?;

    for list in lists.iter().filter(|list| {
        list.name.to_lowercase().contains("completed")
            || list.name.to_lowercase().contains("watching")
    }) {
        for entry in list.entries.iter() {
            let ext = get_ext(&entry.media.cover_image.large);

            let new_anime = Anime {
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

            if let Err(error) = db.insert_anime(&new_anime).await {
                error!("error saving anime={:?}. Error: {}", new_anime, error);
            } else {
                // Download cover images and upload to S3.
                let closure_id = entry.media.id;
                let client = s3_client.clone();
                let url = entry.media.cover_image.large.clone();
                tokio::spawn(async move {
                    if let Err(error) = client
                        .upload_to_s3(ImageTypes::Anime, closure_id, &url)
                        .await
                    {
                        error!("error uploading to S3: {error}");
                    }
                });
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

            if let Err(error) = db.insert_list(&new_list).await {
                error!("error saving list_entry={:?}. Error: {}", new_list, error);
            }
        }
    }

    info!("Database updated for user_id={}", id);

    Ok(())
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
