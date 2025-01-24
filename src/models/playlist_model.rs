use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlaylistModel {
    pub uuid: String,
    pub title: String,
    pub tracks: Value,
}
