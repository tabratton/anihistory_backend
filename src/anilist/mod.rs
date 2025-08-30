pub mod models;

use crate::anilist::models::{ListResponse, MediaList, User, UserResponse};
use std::collections::HashMap;
use tracing::error;

pub async fn get_id(username: &str) -> Result<Option<User>, anyhow::Error> {
    // Construct query to anilist GraphQL to find corresponding id for username
    let query = USER_QUERY.replace("{}", username.as_ref());
    let mut body = HashMap::new();
    body.insert("query", query);
    let client = reqwest::Client::new();
    let res = client.post(ANILIST_URL).json(&body).send().await?;
    let user_response: UserResponse = res.json().await?;

    // If the username was valid, there will be some data, else there will be errors
    match user_response.data.user {
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

pub async fn get_lists(id: i32) -> Result<Vec<MediaList>, anyhow::Error> {
    let query = LIST_QUERY.replace("{}", id.to_string().as_ref());
    let mut body = HashMap::new();
    body.insert("query", query);

    let client = reqwest::Client::new();
    let res = client.post(ANILIST_URL).json(&body).send().await?;
    let list_response: ListResponse = res.json().await?;
    Ok(list_response.data.media_list_collection.lists.clone())
}

static ANILIST_URL: &str = "https://graphql.anilist.co";

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
