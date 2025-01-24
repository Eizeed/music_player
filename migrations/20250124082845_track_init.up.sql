CREATE TABLE IF NOT EXISTS tracks (
    uuid            TEXT PRIMARY KEY NOT NULL,
    path            TEXT NOT NULL,
    play_count      INTEGER NOT NULL CHECK(play_count >= 0),
    play_minutes    REAL NOT NULL CHECK(play_minutes >= 0.0)
)
