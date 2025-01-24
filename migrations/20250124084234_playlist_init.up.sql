CREATE TABLE IF NOT EXISTS playlists (
    uuid            TEXT PRIMARY KEY NOT NULL,
    title           TEXT NOT NULL,
    tracks          JSON NOT NULL
)
