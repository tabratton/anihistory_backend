use chrono::NaiveDate;
use serde_derive::{Deserialize, Serialize};

pub struct ListResult {
    pub user_id: i32,
    pub name: String,
    pub avatar_s3: String,
    pub avatar_anilist: String,
    pub anime_id: i32,
    pub description: String,
    pub cover_s3: String,
    pub cover_anilist: String,
    pub average: Option<i16>,
    pub native: Option<String>,
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub user_title: Option<String>,
    pub start_day: Option<NaiveDate>,
    pub end_day: Option<NaiveDate>,
    pub score: Option<i16>,
}

impl From<&ListResult> for ListItemMap {
    fn from(list: &ListResult) -> Self {
        let user = User {
            user_id: list.user_id,
            name: list.name.clone(),
            avatar_s3: list.avatar_s3.clone(),
            avatar_anilist: list.avatar_anilist.clone(),
        };

        let anime = Anime {
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

#[derive(Debug, Clone)]
pub struct User {
    pub user_id: i32,
    pub name: String,
    pub avatar_s3: String,
    pub avatar_anilist: String,
}

#[derive(Debug, Clone)]
pub struct Anime {
    pub anime_id: i32,
    pub description: String,
    pub cover_s3: String,
    pub cover_anilist: String,
    pub average: Option<i16>,
    pub native: Option<String>,
    pub romaji: Option<String>,
    pub english: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub user_id: i32,
    pub anime_id: i32,
    pub user_title: Option<String>,
    pub start_day: Option<NaiveDate>,
    pub end_day: Option<NaiveDate>,
    pub score: Option<i16>,
}

#[derive(Debug, Clone)]
pub struct ListItemMap {
    pub user: User,
    pub anime: Anime,
    pub list_item: ListItem,
}

#[derive(Serialize, Deserialize)]
pub struct RestResponse {
    pub users: ResponseList,
}

#[derive(Serialize, Deserialize)]
pub struct ResponseList {
    pub id: String,
    pub avatar: String,
    pub list: Vec<ResponseItem>,
}

#[derive(Serialize, Deserialize)]
pub struct ResponseItem {
    pub user_title: Option<String>,
    pub start_day: Option<NaiveDate>,
    pub end_day: Option<NaiveDate>,
    pub score: Option<i16>,
    pub average: Option<i16>,
    pub native: Option<String>,
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub description: String,
    pub cover: String,
    pub id: i32,
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
