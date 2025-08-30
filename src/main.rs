/*
 * Copyright (c) 2018, Tyler Bratton
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use crate::database::Database;
use crate::s3::S3Client;
use axum::extract::{FromRef, Path, State};
use axum::http::{Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

mod anilist;
mod database;
mod s3;

async fn user(State(db): State<Database>, Path(username): Path<String>) -> impl IntoResponse {
    match database::get_list(username.as_ref(), &db).await {
        Ok(Some(list)) => Ok(Json(list)),
        Ok(None) => Err((StatusCode::NOT_FOUND, "User or list not found".to_string())),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )),
    }
}

async fn update(
    State(db): State<Database>,
    State(s3_client): State<S3Client>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match anilist::get_id(username.as_ref()).await {
        Ok(Some(user)) => {
            let client = s3_client.clone();
            if let Err(err) = database::update_user_profile(user.clone(), &db, client).await {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
            }

            tokio::spawn(async move { database::update_entries(user.id, &db, s3_client).await });
            Ok((StatusCode::ACCEPTED, "Added to the queue".to_string()))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "User not found".to_string())),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "User not found".to_string(),
        )),
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_logging();

    let db = Database::try_new().await?;
    let s3_client = S3Client::new().await;

    let app_state = AppState { db, s3_client };

    let allowed_origins = [
        "http://localhost:4200".parse()?,
        "https://anihistory.moe".parse()?,
        "https://www.anihistory.moe".parse()?,
    ];
    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        // allow `GET` and `POST` when accessing the resource
        .allow_methods([Method::GET, Method::POST])
        // allow requests from any origin
        .allow_credentials(true)
        .allow_headers(Any);

    let app: Router<()> = Router::new()
        .route("/users/{username}", get(user).post(update))
        .with_state(app_state)
        .layer(cors);

    let addr = format!(
        "0.0.0.0:{}",
        std::env::var("PORT").unwrap_or("8080".to_string())
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}

fn setup_logging() {
    Registry::default()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[derive(Clone)]
struct AppState {
    db: Database,
    s3_client: S3Client,
}

impl FromRef<AppState> for Database {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl FromRef<AppState> for S3Client {
    fn from_ref(state: &AppState) -> Self {
        state.s3_client.clone()
    }
}

fn get_ext(url: &str) -> String {
    let link_parts: Vec<&str> = url.split('/').collect();
    let split: Vec<&str> = link_parts[link_parts.len() - 1].split(".").collect();
    split[1].to_owned()
}
