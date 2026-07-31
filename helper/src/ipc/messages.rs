use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HelperMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub time: u64,
    pub data: Value,
}

impl HelperMessage {
    pub fn new(kind: impl Into<String>, data: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            time: timestamp_ms(),
            data,
        }
    }
}

pub fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
