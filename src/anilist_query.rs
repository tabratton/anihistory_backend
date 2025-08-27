/*
 * Copyright (c) 2018, Tyler Bratton
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::anilist_models;
use serde_json::from_str;
use std::collections::HashMap;
use tracing::error;

pub async fn get_id(username: &str) -> Result<Option<anilist_models::User>, anyhow::Error> {
    // Construct query to anilist GraphQL to find corresponding id for username
    let query = USER_QUERY.replace("{}", username.as_ref());
    let mut body = HashMap::new();
    body.insert("query", query);
    let client = reqwest::Client::new();
    let res = client.post(ANILSIT_URL).json(&body).send().await?;
    let res_text = res.text().await?;
    let json: anilist_models::UserResponse = from_str(res_text.as_ref())?;

    // If the username was valid, there will be some data, else there will be errors
    match json.data.user {
        Some(user) => Ok(Some(user)),
        None => {
            error!(
                "user_name={} was not found in anilist/external database",
                username
            );
            Ok(None)
        }
    }
}

pub async fn get_lists(id: i32) -> Result<Vec<anilist_models::MediaList>, anyhow::Error> {
    let query = LIST_QUERY.replace("{}", id.to_string().as_ref());
    let mut body = HashMap::new();
    body.insert("query", query);

    let client = reqwest::Client::new();
    let res = client.post(ANILSIT_URL).json(&body).send().await?;
    let res_text = res.text().await?;
    let json: anilist_models::ListResponse = from_str(res_text.as_ref())?;
    Ok(json.data.media_list_collection.lists.clone())
}

static ANILSIT_URL: &str = "https://graphql.anilist.co";

static LIST_QUERY: &str = "query {
    MediaListCollection(userId: {}, type: ANIME) {
      lists {
        name
        entries {
          ...mediaListEntry
        }
      }
    }
  }

  fragment mediaListEntry on MediaList {
    scoreRaw: score(format: POINT_100)
    startedAt {
      year
      month
      day
    }
    completedAt {
      year
      month
      day
    }
    media {
	  id
      title {
        userPreferred
        english
        romaji
        native
      }
      description(asHtml: true)
      coverImage {
        large
      }
      averageScore
      siteUrl
      }
    }";

static USER_QUERY: &str = "query {
  	User(name: \"{}\") {
	  id
      name
      avatar {
        large
      }
	}
  }";
