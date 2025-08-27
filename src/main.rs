/*
 * Copyright (c) 2018, Tyler Bratton
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use axum::extract::{FromRef, Path, State};
use axum::http::{Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::str::FromStr;
use tokio_postgres::{Config, NoTls};
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

mod anilist_models;
mod anilist_query;
mod database;
mod models;

type DbPool = Pool<PostgresConnectionManager<NoTls>>;

async fn user(State(pool): State<DbPool>, Path(username): Path<String>) -> impl IntoResponse {
    match database::get_list(username.as_ref(), &pool).await {
        Ok(Some(list)) => Ok(Json(list)),
        Ok(None) => Err((StatusCode::NOT_FOUND, "User or list not found".to_string())),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )),
    }
}

async fn update(
    State(database_conn): State<DbPool>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match anilist_query::get_id(username.as_ref()).await {
        Ok(Some(user)) => {
            let pool = database_conn.clone();
            if let Err(err) = database::update_user_profile(user.clone(), &pool).await {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
            }

            tokio::spawn(async move { database::update_entries(user.id, pool).await });
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

    let postgres_url = std::env::var("DATABASE_URL")?;
    let postgres_config = Config::from_str(postgres_url.as_ref())?;
    let manager = PostgresConnectionManager::new(postgres_config, NoTls);
    let pool = Pool::builder().max_size(10).build(manager).await?;

    let allowed_origins = [
        "http://localhost:4200".parse()?,
        "https://anihistory.moe".parse()?,
        "https://www.anihistory.moe".parse()?,
    ];

    let app_state = AppState { pool };

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
    pool: DbPool,
}

impl FromRef<AppState> for DbPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
