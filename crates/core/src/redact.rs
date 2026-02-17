use crate::config::RedactionConfig;

const REDACTED: &str = "[REDACTED]";

/// Returns true if the header name should be redacted (case-insensitive).
pub fn should_redact_header(name: &str, config: &RedactionConfig) -> bool {
    let lower = name.to_lowercase();
    config.headers.iter().any(|h| h.to_lowercase() == lower)
}

/// Redact a header value if the name matches redaction rules.
pub fn redact_header_value(name: &str, value: &str, config: &RedactionConfig) -> String {
    if should_redact_header(name, config) {
        REDACTED.to_string()
    } else {
        value.to_string()
    }
}

/// Redact values in a query string for matching parameter names.
pub fn redact_query_param(key: &str, value: &str, config: &RedactionConfig) -> String {
    let lower = key.to_lowercase();
    if config.query_params.iter().any(|q| q.to_lowercase() == lower) {
        REDACTED.to_string()
    } else {
        value.to_string()
    }
}
