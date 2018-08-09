table! {
    anime (anime_id) {
        anime_id -> Int4,
        description -> Text,
        cover_s3 -> Text,
        cover_anilist -> Text,
        average -> Nullable<Int2>,
        native -> Nullable<Text>,
        romaji -> Nullable<Text>,
        english -> Nullable<Text>,
    }
}

table! {
    lists (user_id, anime_id) {
        user_id -> Int4,
        anime_id -> Int4,
        user_title -> Nullable<Text>,
        start_day -> Nullable<Date>,
        end_day -> Nullable<Date>,
        score -> Nullable<Int2>,
    }
}

table! {
    users (user_id) {
        user_id -> Int4,
        name -> Text,
        avatar_s3 -> Text,
        avatar_anilist -> Text,
    }
}

joinable!(lists -> anime (anime_id));
joinable!(lists -> users (user_id));

allow_tables_to_appear_in_same_query!(anime, lists, users,);
