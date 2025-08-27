CREATE TABLE IF NOT EXISTS anime (
    anime_id INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    cover_s3 TEXT NOT NULL,
    cover_anilist TEXT NOT NULL,
    average SMALLINT,
    native TEXT,
    romaji TEXT,
    english TEXT
);

CREATE TABLE IF NOT EXISTS lists (
    user_id INTEGER NOT NULL,
    anime_id INTEGER NOT NULL,
    user_title TEXT,
    start_day DATE,
    end_day DATE,
    score SMALLINT,
    PRIMARY KEY(user_id, anime_id)
);

CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    avatar_s3 TEXT NOT NULL,
    avatar_anilist TEXT NOT NULL
)