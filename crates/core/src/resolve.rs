use std::collections::HashMap;

use crate::env::Environment;
use crate::request::{AuthConfig, Body, RequestDef};
use crate::template;

/// Merge variable sources in priority order:
/// 1. CLI overrides (highest)
/// 2. Environment file values
///
/// Secret store will be added in M2.
pub fn merge_vars(
    env: Option<&Environment>,
    cli_overrides: &[(String, String)],
) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Lowest priority first: env file
    if let Some(env) = env {
        vars.extend(env.vars.clone());
    }

    // Highest priority: CLI overrides
    for (k, v) in cli_overrides {
        vars.insert(k.clone(), v.clone());
    }

    vars
}

/// Parse CLI `--var key=value` strings into (key, value) pairs.
pub fn parse_var_overrides(raw: &[String]) -> Result<Vec<(String, String)>, String> {
    raw.iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| format!("invalid --var format (expected KEY=VALUE): {s}"))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect()
}

/// Render all template placeholders in a `RequestDef` in place.
pub fn render_request(
    req: &mut RequestDef,
    vars: &HashMap<String, String>,
) -> Result<(), template::UnresolvedVarsError> {
    req.url = template::render(&req.url, vars)?;

    // Headers
    let headers = std::mem::take(&mut req.headers);
    for (k, v) in headers {
        let rendered_v = template::render(&v, vars)?;
        req.headers.insert(k, rendered_v);
    }

    // Query params
    let query = std::mem::take(&mut req.query);
    for (k, v) in query {
        let rendered_v = template::render(&v, vars)?;
        req.query.insert(k, rendered_v);
    }

    // Auth
    if let Some(ref mut auth) = req.auth {
        match auth {
            AuthConfig::Bearer { token } => {
                *token = template::render(token, vars)?;
            }
            AuthConfig::Basic { username, password } => {
                *username = template::render(username, vars)?;
                *password = template::render(password, vars)?;
            }
        }
    }

    // Body
    if let Some(ref mut body) = req.body {
        match body {
            Body::Raw(s) => {
                *s = template::render(s, vars)?;
            }
            Body::Json(val) => {
                render_json_value(val, vars)?;
            }
            Body::Form(map) => {
                let entries = std::mem::take(map);
                for (k, v) in entries {
                    let rendered_v = template::render(&v, vars)?;
                    map.insert(k, rendered_v);
                }
            }
        }
    }

    Ok(())
}

/// Recursively render template variables inside JSON string values.
fn render_json_value(
    val: &mut serde_json::Value,
    vars: &HashMap<String, String>,
) -> Result<(), template::UnresolvedVarsError> {
    match val {
        serde_json::Value::String(s) => {
            *s = template::render(s, vars)?;
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                render_json_value(item, vars)?;
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                render_json_value(v, vars)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_cli_overrides_env_file() {
        let env = Environment {
            vars: HashMap::from([
                ("host".into(), "env-host".into()),
                ("port".into(), "3000".into()),
            ]),
        };
        let overrides = vec![("host".into(), "cli-host".into())];

        let vars = merge_vars(Some(&env), &overrides);
        assert_eq!(vars["host"], "cli-host");
        assert_eq!(vars["port"], "3000");
    }

    #[test]
    fn merge_no_env() {
        let overrides = vec![("key".into(), "val".into())];
        let vars = merge_vars(None, &overrides);
        assert_eq!(vars["key"], "val");
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn parse_var_overrides_valid() {
        let raw = vec!["key=value".into(), "a=b=c".into()];
        let parsed = parse_var_overrides(&raw).unwrap();
        assert_eq!(parsed, vec![("key".into(), "value".into()), ("a".into(), "b=c".into())]);
    }

    #[test]
    fn parse_var_overrides_invalid() {
        let raw = vec!["no-equals".into()];
        assert!(parse_var_overrides(&raw).is_err());
    }

    #[test]
    fn render_request_templates() {
        let vars = HashMap::from([
            ("host".into(), "example.com".into()),
            ("token".into(), "abc123".into()),
        ]);
        let mut req = RequestDef {
            name: "test".into(),
            method: "GET".into(),
            url: "https://{{host}}/api".into(),
            headers: HashMap::from([("Authorization".into(), "Bearer {{token}}".into())]),
            query: HashMap::from([("key".into(), "{{token}}".into())]),
            auth: None,
            body: None,
        };

        render_request(&mut req, &vars).unwrap();
        assert_eq!(req.url, "https://example.com/api");
        assert_eq!(req.headers["Authorization"], "Bearer abc123");
        assert_eq!(req.query["key"], "abc123");
    }

    #[test]
    fn render_request_json_body() {
        let vars = HashMap::from([("name".into(), "Alice".into())]);
        let mut req = RequestDef {
            name: "test".into(),
            method: "POST".into(),
            url: "http://localhost".into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            auth: None,
            body: Some(Body::Json(serde_json::json!({"user": "{{name}}"}))),
        };

        render_request(&mut req, &vars).unwrap();
        if let Some(Body::Json(val)) = &req.body {
            assert_eq!(val["user"], "Alice");
        } else {
            panic!("expected JSON body");
        }
    }
}
