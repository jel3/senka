use serde::{Deserialize, Serialize};

/// Top-level project configuration loaded from `senka.yml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,

    #[serde(default)]
    pub defaults: Defaults,

    #[serde(default)]
    pub redaction: RedactionConfig,

    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Defaults {
    /// Default environment name (e.g. "dev").
    pub env: Option<String>,

    /// Default request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Maximum redirect hops.
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    /// Headers to redact (case-insensitive).
    #[serde(default = "default_redacted_headers")]
    pub headers: Vec<String>,

    /// Query parameter names to redact.
    #[serde(default = "default_redacted_query_params")]
    pub query_params: Vec<String>,

    /// JSON field names to redact.
    #[serde(default = "default_redacted_json_fields")]
    pub json_fields: Vec<String>,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            headers: default_redacted_headers(),
            query_params: default_redacted_query_params(),
            json_fields: default_redacted_json_fields(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Whether logging is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum body size to store, in KB.
    #[serde(default = "default_max_body_kb")]
    pub max_body_kb: usize,

    /// Retention period in days.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_body_kb: default_max_body_kb(),
            retention_days: default_retention_days(),
        }
    }
}

fn default_timeout_ms() -> u64 {
    30_000
}
fn default_max_redirects() -> usize {
    10
}
fn default_true() -> bool {
    true
}
fn default_max_body_kb() -> usize {
    256
}
fn default_retention_days() -> u32 {
    30
}

fn default_redacted_headers() -> Vec<String> {
    vec![
        "authorization".into(),
        "cookie".into(),
        "set-cookie".into(),
    ]
}

fn default_redacted_query_params() -> Vec<String> {
    vec!["token".into(), "api_key".into()]
}

fn default_redacted_json_fields() -> Vec<String> {
    vec!["password".into(), "access_token".into()]
}
