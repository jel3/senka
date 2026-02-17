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

/// Replace all occurrences of secret values in `text` with `[REDACTED]`.
/// Skips empty strings in `secret_values`.
pub fn redact_secret_values(text: &str, secret_values: &[String]) -> String {
    let mut result = text.to_string();
    for secret in secret_values {
        if !secret.is_empty() {
            result = result.replace(secret.as_str(), REDACTED);
        }
    }
    result
}

/// Recursively redact JSON object keys matching `config.json_fields` (case-insensitive).
pub fn redact_json_fields(val: &mut serde_json::Value, config: &RedactionConfig) {
    match val {
        serde_json::Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                let lower = key.to_lowercase();
                if config.json_fields.iter().any(|f| f.to_lowercase() == lower) {
                    *v = serde_json::Value::String(REDACTED.to_string());
                } else {
                    redact_json_fields(v, config);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_json_fields(item, config);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secret_values_replaces_secrets() {
        let text = "Bearer my-secret-token and also my-api-key";
        let secrets = vec!["my-secret-token".to_string(), "my-api-key".to_string()];
        let result = redact_secret_values(text, &secrets);
        assert_eq!(result, "Bearer [REDACTED] and also [REDACTED]");
    }

    #[test]
    fn redact_secret_values_skips_empty() {
        let text = "Bearer token123";
        let secrets = vec!["".to_string(), "token123".to_string()];
        let result = redact_secret_values(text, &secrets);
        assert_eq!(result, "Bearer [REDACTED]");
    }

    #[test]
    fn redact_secret_values_no_secrets() {
        let text = "no secrets here";
        let result = redact_secret_values(text, &[]);
        assert_eq!(result, "no secrets here");
    }

    #[test]
    fn redact_json_fields_simple() {
        let config = RedactionConfig {
            headers: vec![],
            query_params: vec![],
            json_fields: vec!["password".into(), "access_token".into()],
        };
        let mut val = serde_json::json!({
            "user": "alice",
            "password": "s3cret",
            "access_token": "tok123"
        });
        redact_json_fields(&mut val, &config);
        assert_eq!(val["user"], "alice");
        assert_eq!(val["password"], "[REDACTED]");
        assert_eq!(val["access_token"], "[REDACTED]");
    }

    #[test]
    fn redact_json_fields_nested() {
        let config = RedactionConfig {
            headers: vec![],
            query_params: vec![],
            json_fields: vec!["password".into()],
        };
        let mut val = serde_json::json!({
            "data": {
                "user": "bob",
                "password": "secret"
            },
            "items": [{"password": "abc"}, {"name": "ok"}]
        });
        redact_json_fields(&mut val, &config);
        assert_eq!(val["data"]["password"], "[REDACTED]");
        assert_eq!(val["data"]["user"], "bob");
        assert_eq!(val["items"][0]["password"], "[REDACTED]");
        assert_eq!(val["items"][1]["name"], "ok");
    }

    #[test]
    fn redact_json_fields_case_insensitive() {
        let config = RedactionConfig {
            headers: vec![],
            query_params: vec![],
            json_fields: vec!["Password".into()],
        };
        let mut val = serde_json::json!({"password": "secret", "PASSWORD": "ALSO"});
        redact_json_fields(&mut val, &config);
        assert_eq!(val["password"], "[REDACTED]");
        assert_eq!(val["PASSWORD"], "[REDACTED]");
    }
}
