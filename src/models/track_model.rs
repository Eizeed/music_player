use uuid::Uuid;

#[derive(sqlx::FromRow, Debug)]
pub struct TrackModel {
    pub uuid: String,
    pub path: String, // into PathBuf
    pub play_count: i64,
    pub play_minutes: f64,
}
