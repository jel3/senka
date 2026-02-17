use std::collections::HashMap;

use anyhow::{bail, Context};

use senka_core::config::{ProjectConfig, RedactionConfig};
use senka_core::loader;
use senka_core::redact;
use senka_core::request::{Body, RequestDef};
use senka_core::resolve;
use senka_runner::execute::{self, ClientOptions, RunError};
use senka_runner::response::CapturedResponse;
use senka_store::db;
use senka_store::models::{Payload, Run};

use crate::commands::log::now_epoch_ms;
use crate::output::{self, OutputOptions};

pub struct RunArgs {
    pub request: String,
    pub env: Option<String>,
    pub vars: Vec<String>,
    pub json: bool,
    pub show_headers: bool,
    pub fail: bool,
    pub insecure: bool,
    pub no_redact: bool,
    pub no_color: bool,
}

pub async fn run(args: RunArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    // Load project
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no tool.yml found)")?;
    let config = loader::load_config(&root).context("failed to load tool.yml")?;

    // Determine env name
    let env_name = args
        .env
        .as_deref()
        .or(config.defaults.env.as_deref());

    // Load env file (if specified)
    let env = match env_name {
        Some(name) => Some(
            loader::load_env(&root, name)
                .with_context(|| format!("failed to load env/{name}.yml"))?,
        ),
        None => None,
    };

    // Parse CLI var overrides
    let overrides = resolve::parse_var_overrides(&args.vars)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Merge variables
    let mut vars = resolve::merge_vars(env.as_ref(), &overrides);

    // Load request (before rendering, to discover needed vars)
    let mut req = loader::load_request(&root, &args.request)
        .with_context(|| format!("failed to load request '{}'", args.request))?;

    // Resolve secrets for any unresolved template variables
    let mut secret_values = Vec::new();
    let needed_vars = resolve::collect_template_vars(&req);
    if let Some(env_name) = env_name {
        for var_name in &needed_vars {
            if vars.contains_key(var_name.as_str()) {
                continue;
            }
            match senka_secrets::get(&config.name, env_name, var_name) {
                Ok(Some(val)) => {
                    secret_values.push(val.clone());
                    vars.insert(var_name.clone(), val);
                }
                Ok(None) => {} // not in keychain either; render will report unresolved
                Err(e) => {
                    eprintln!("warning: failed to read secret '{var_name}': {e}");
                }
            }
        }
    }

    // Render request templates
    resolve::render_request(&mut req, &vars)
        .context("failed to resolve template variables in request")?;

    // Build client and execute
    let client_opts = ClientOptions {
        insecure: args.insecure,
        timeout_ms: None,
        max_redirects: None,
    };
    let client = execute::build_client(&config, &client_opts)
        .context("failed to build HTTP client")?;

    let exec_result = execute::execute(&client, &req, config.logging.max_body_kb).await;

    // Log the result (always redact for storage, regardless of --no-redact)
    if config.logging.enabled {
        insert_log_entry(
            &root,
            &config,
            &req,
            env_name.unwrap_or("default"),
            &secret_values,
            &exec_result,
        );
    }

    let resp = exec_result.map_err(|e| match &e {
        RunError::Timeout => anyhow::anyhow!("{e}"),
        RunError::Network(_) => anyhow::anyhow!("{e}"),
        RunError::Definition(_) => anyhow::anyhow!("{e}"),
    })?;

    // Output
    let out_opts = OutputOptions {
        json: args.json,
        show_headers: args.show_headers,
        no_color: args.no_color,
        no_redact: args.no_redact,
        secret_values,
    };
    output::print_response(&resp, &req.method, &req.url, &config.redaction, &out_opts);

    // Exit code for --fail
    if args.fail && resp.status >= 400 {
        bail!("request failed with status {}", resp.status);
    }

    Ok(())
}

/// Insert a log entry after request execution. Failures warn to stderr, never abort.
fn insert_log_entry(
    root: &std::path::Path,
    config: &ProjectConfig,
    req: &RequestDef,
    env_name: &str,
    secret_values: &[String],
    exec_result: &Result<CapturedResponse, RunError>,
) {
    let db_path = root.join(".senka").join("logs.db");
    let conn = match db::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to open log database: {e}");
            return;
        }
    };

    let id = ulid::Ulid::new().to_string();
    let ts = now_epoch_ms();

    let (status, duration_ms, error, resp_headers, resp_body) = match exec_result {
        Ok(resp) => {
            let headers_redacted =
                redact_headers_for_storage(&resp.headers, &config.redaction, secret_values);
            let body_str = String::from_utf8_lossy(&resp.body);
            let body_redacted =
                redact_body_for_storage(&body_str, &config.redaction, secret_values);
            let body_truncated = truncate_body(&body_redacted, config.logging.max_body_kb);
            (
                Some(resp.status),
                resp.duration_ms,
                None,
                serde_json::to_string(&headers_redacted).unwrap_or_default(),
                Some(body_truncated),
            )
        }
        Err(e) => (None, 0, Some(e.to_string()), String::new(), None),
    };

    let req_headers_redacted =
        redact_headers_for_storage(&req.headers, &config.redaction, secret_values);
    let req_body_str = build_request_body_string(&req.body);
    let req_body_redacted = req_body_str
        .as_deref()
        .map(|b| redact_body_for_storage(b, &config.redaction, secret_values));
    let req_body_truncated =
        req_body_redacted.map(|b| truncate_body(&b, config.logging.max_body_kb));

    let run = Run {
        id: id.clone(),
        ts,
        project: config.name.clone(),
        env: env_name.to_string(),
        request_name: req.name.clone(),
        method: req.method.clone(),
        url: req.url.clone(),
        status,
        duration_ms,
        error,
    };

    let payload = Payload {
        run_id: id,
        request_headers: serde_json::to_string(&req_headers_redacted).unwrap_or_default(),
        request_body: req_body_truncated,
        response_headers: resp_headers,
        response_body: resp_body,
    };

    if let Err(e) = db::insert_run(&conn, &run, &payload) {
        eprintln!("warning: failed to log request: {e}");
    }
}

/// Redact headers for storage (always redacts, ignores --no-redact).
fn redact_headers_for_storage(
    headers: &HashMap<String, String>,
    config: &RedactionConfig,
    secret_values: &[String],
) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            let val = redact::redact_header_value(k, v, config);
            let val = redact::redact_secret_values(&val, secret_values);
            (k.clone(), val)
        })
        .collect()
}

/// Redact body text for storage (secret values + JSON field redaction).
fn redact_body_for_storage(
    body: &str,
    config: &RedactionConfig,
    secret_values: &[String],
) -> String {
    let mut result = redact::redact_secret_values(body, secret_values);

    if !config.json_fields.is_empty() {
        if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&result) {
            redact::redact_json_fields(&mut json_val, config);
            if let Ok(s) = serde_json::to_string(&json_val) {
                result = s;
            }
        }
    }

    result
}

/// Convert request body to a string representation.
fn build_request_body_string(body: &Option<Body>) -> Option<String> {
    match body {
        Some(Body::Raw(s)) => Some(s.clone()),
        Some(Body::Json(v)) => serde_json::to_string(v).ok(),
        Some(Body::Form(m)) => serde_json::to_string(m).ok(),
        None => None,
    }
}

/// Truncate body string to max_body_kb kilobytes.
fn truncate_body(body: &str, max_body_kb: usize) -> String {
    let max_bytes = max_body_kb * 1024;
    if body.len() <= max_bytes {
        body.to_string()
    } else {
        let truncated = &body[..max_bytes];
        format!("{truncated}... (truncated)")
    }
}
