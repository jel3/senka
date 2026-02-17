use serde::Serialize;
use std::collections::HashMap;

/// Captured HTTP response with bounded body.
#[derive(Debug, Clone, Serialize)]
pub struct CapturedResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    #[serde(skip)]
    pub body: Vec<u8>,
    pub truncated: bool,
    pub duration_ms: u64,
}
