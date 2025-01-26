use std::path::PathBuf;

use crate::{
    models::{playlist_model::*, track_model::TrackModel},
    playlist::Playlist,
    track::Track,
    utils::path_buf_vec_to_string,
};
use serde_json::Value;
use sqlx::{query, SqlitePool};
use uuid::Uuid;

pub async fn init(pool: &SqlitePool) {
    let track_migration = include_str!("../migrations/20250124082845_track_init.up.sql");
    let playlist_migration = include_str!("../migrations/20250124084234_playlist_init.up.sql");

    sqlx::query(track_migration)
        .execute(pool)
        .await
        .expect("Unable to init db");
    sqlx::query(playlist_migration)
        .execute(pool)
        .await
        .expect("Unable to init db");

    let liked_exists = sqlx::query_as!(
        PlaylistModel,
        r#"
            SELECT uuid, title, tracks AS "tracks: Value" FROM playlists WHERE title = $1 
        "#,
        "Liked"
    )
    .fetch_optional(pool)
    .await
    .unwrap()
    .is_some();

    if !liked_exists {
        let uuid = Uuid::new_v4().to_string();

        sqlx::query!(
            r#"
                INSERT INTO playlists
                (uuid, title, tracks)
                VALUES
                ($1, $2, $3)
            "#,
            uuid,
            "Liked",
            "[]"
        )
        .execute(pool)
        .await
        .unwrap();
    }
}

pub async fn update_track_state(pool: &SqlitePool, paths: &[PathBuf]) {
    let mut transaction = pool.begin().await.unwrap();
    for path in paths {
        let path = path.to_str().unwrap();
        let track = sqlx::query_as!(
            TrackModel,
            r#"
                SELECT * FROM tracks WHERE path = $1
            "#,
            path
        )
        .fetch_optional(transaction.as_mut())
        .await
        .unwrap();

        if let None = track {
            let uuid = Uuid::new_v4().to_string();
            let a = sqlx::query_as!(
                TrackModel,
                r#"
                    INSERT INTO tracks
                    (uuid, path, play_count, play_minutes)
                    VALUES
                    ($1,$2,$3,$4)
                    RETURNING *
                "#,
                uuid,
                path,
                0,
                0.0,
            )
            .fetch_one(transaction.as_mut())
            .await
            .unwrap();
            println!("Inserted: {a:?}");
        }
    }

    let paths = path_buf_vec_to_string(paths);
    let qry = format!("DELETE FROM tracks WHERE path NOT IN ({})", paths);

    println!("{qry}");

    sqlx::query(&qry)
        .execute(transaction.as_mut())
        .await
        .unwrap();

    transaction.commit().await.unwrap();

    return ();
}

pub async fn get_tracks(pool: &SqlitePool) -> Vec<TrackModel> {
    let tracks = sqlx::query_as!(
        TrackModel,
        r#"
            SELECT * FROM tracks
        "#
    )
    .fetch_all(pool)
    .await
    .unwrap();

    return tracks;
}

pub async fn get_playlists(pool: &SqlitePool) -> Vec<PlaylistModel> {
    let playlists = sqlx::query_as!(
        PlaylistModel,
        r#"
            SELECT * FROM playlists
        "#
    )
    .fetch_all(pool)
    .await
    .unwrap();

    return playlists;
}

pub async fn get_tracks_from_playlist(pool: &SqlitePool, playlist_uuid: Uuid) -> Vec<TrackModel> {
    let mut transaction = pool.begin().await.unwrap();
    let uuid = playlist_uuid.to_string();
    let playlist = sqlx::query_as!(
        PlaylistModel,
        r#"
            SELECT uuid, title, tracks AS "tracks: Value" FROM playlists WHERE uuid = $1 
        "#,
        uuid
    )
    .fetch_one(transaction.as_mut())
    .await
    .unwrap();

    let uuids: Vec<Uuid> = serde_json::from_value(playlist.tracks).unwrap();

    let mut tracks: Vec<TrackModel> = vec![];

    for uuid in uuids {
        let uuid = uuid.to_string();
        let track = sqlx::query_as!(
            TrackModel,
            r#"
                SELECT * FROM tracks WHERE uuid = $1
            "#,
            uuid
        )
        .fetch_one(transaction.as_mut())
        .await
        .unwrap();

        tracks.push(track);
    }

    transaction.commit().await.unwrap();

    return tracks;
}

pub async fn insert_into_playlist(pool: &SqlitePool, mut playlist: Playlist, track_uuid: Uuid) -> Vec<PlaylistModel> {
    playlist.tracks.push(track_uuid);
    let tracks = serde_json::to_value(playlist.tracks).unwrap();
    let uuid = playlist.uuid.to_string();

    sqlx::query!(
        r#"
            UPDATE playlists 
            SET
                tracks = $1
            WHERE 
                uuid = $2
        "#,
        tracks,
        uuid,
    )
    .execute(pool)
    .await
    .unwrap();

    return get_playlists(pool).await;
}
pub async fn delete_from_playlist(pool: &SqlitePool, mut playlist: Playlist, track_uuid: Uuid) -> Vec<PlaylistModel> {
    for (i, uuid) in playlist.tracks.iter().enumerate() {
        if *uuid == track_uuid {
            playlist.tracks.remove(i);
            break;
        }
    }

    let tracks = serde_json::to_value(playlist.tracks).unwrap();
    let uuid = playlist.uuid.to_string();
    sqlx::query!(
        r#"
            UPDATE playlists 
            SET
                tracks = $1
            WHERE 
                uuid = $2
        "#,
        tracks,
        uuid,
    )
    .execute(pool)
    .await
    .unwrap();

    return get_playlists(pool).await;
}
