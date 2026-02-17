use serde::Serialize;

/// A logged HTTP run record.
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub id: String,
    pub ts: u64,
    pub project: String,
    pub env: String,
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Request/response payloads associated with a run.
#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    pub run_id: String,
    pub request_headers: String,
    pub request_body: Option<String>,
    pub response_headers: String,
    pub response_body: Option<String>,
}

/// A run joined with its payload, used for `show` and `export`.
#[derive(Debug, Clone, Serialize)]
pub struct RunWithPayload {
    #[serde(flatten)]
    pub run: Run,
    pub request_headers: String,
    pub request_body: Option<String>,
    pub response_headers: String,
    pub response_body: Option<String>,
}
