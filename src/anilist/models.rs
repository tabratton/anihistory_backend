/*
 * Copyright (c) 2018, Tyler Bratton
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use serde_derive::{Deserialize, Serialize};

// User Structs
#[derive(Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub data: UserData,
    pub errors: Option<Vec<Error>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Error {
    pub message: String,
    pub status: i32,
    pub locations: Vec<Location>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Location {
    pub line: i32,
    pub column: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserData {
    #[serde(rename = "User")]
    pub user: Option<User>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i32,
    pub name: String,
    pub avatar: Avatar,
}

// List Structs
#[derive(Serialize, Deserialize, Clone)]
pub struct ListResponse {
    pub data: MediaListCollectionData,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MediaListCollectionData {
    #[serde(rename = "MediaListCollection")]
    pub media_list_collection: MediaListCollection,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MediaListCollection {
    pub lists: Vec<MediaList>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Avatar {
    pub large: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MediaList {
    pub name: String,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    #[serde(rename = "scoreRaw")]
    pub score_raw: Option<i16>,
    #[serde(rename = "startedAt")]
    pub started_at: Date,
    #[serde(rename = "completedAt")]
    pub completed_at: Date,
    pub media: Media,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Date {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub day: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Media {
    pub id: i32,
    pub title: Title,
    pub description: String,
    #[serde(rename = "coverImage")]
    pub cover_image: Image,
    #[serde(rename = "averageScore")]
    pub average_score: Option<i16>,
    #[serde(rename = "siteUrl")]
    pub site_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Title {
    #[serde(rename = "userPreferred")]
    pub user_preferred: Option<String>,
    pub english: Option<String>,
    pub romaji: Option<String>,
    pub native: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Image {
    pub large: String,
}
