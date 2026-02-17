use std::time::Instant;

use futures_util::StreamExt;
use reqwest::redirect::Policy;
use senka_core::config::ProjectConfig;
use senka_core::request::{AuthConfig, Body, RequestDef};
use thiserror::Error;

use crate::response::CapturedResponse;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("request definition error: {0}")]
    Definition(String),
}

/// CLI-level overrides that affect client behavior.
#[derive(Default)]
pub struct ClientOptions {
    pub insecure: bool,
    pub timeout_ms: Option<u64>,
    pub max_redirects: Option<usize>,
}

/// Build a reqwest client from project config + CLI overrides.
pub fn build_client(
    config: &ProjectConfig,
    opts: &ClientOptions,
) -> Result<reqwest::Client, RunError> {
    let timeout_ms = opts.timeout_ms.unwrap_or(config.defaults.timeout_ms);
    let max_redirects = opts.max_redirects.unwrap_or(config.defaults.max_redirects);

    let builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .redirect(Policy::limited(max_redirects))
        .danger_accept_invalid_certs(opts.insecure);

    builder.build().map_err(RunError::Network)
}

/// Execute a resolved `RequestDef` and capture the response with bounded body.
pub async fn execute(
    client: &reqwest::Client,
    req: &RequestDef,
    max_body_kb: usize,
) -> Result<CapturedResponse, RunError> {
    let method: reqwest::Method = req
        .method
        .parse()
        .map_err(|_| RunError::Definition(format!("invalid HTTP method: {}", req.method)))?;

    let mut builder = client.request(method, &req.url);

    // Headers
    for (k, v) in &req.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    // Query params
    if !req.query.is_empty() {
        builder = builder.query(&req.query);
    }

    // Auth
    if let Some(ref auth) = req.auth {
        match auth {
            AuthConfig::Bearer { token } => {
                builder = builder.bearer_auth(token);
            }
            AuthConfig::Basic { username, password } => {
                builder = builder.basic_auth(username, Some(password));
            }
        }
    }

    // Body
    if let Some(ref body) = req.body {
        match body {
            Body::Raw(s) => {
                builder = builder.body(s.clone());
            }
            Body::Json(val) => {
                builder = builder.json(val);
            }
            Body::Form(map) => {
                builder = builder.form(map);
            }
        }
    }

    let start = Instant::now();
    let response = builder.send().await.map_err(|e| {
        if e.is_timeout() {
            RunError::Timeout
        } else {
            RunError::Network(e)
        }
    })?;

    let status = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();

    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<non-utf8>").to_string(),
            )
        })
        .collect();

    // Stream body with bounded buffering
    let max_bytes = max_body_kb * 1024;
    let mut body = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut truncated = false;

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let remaining = max_bytes.saturating_sub(body.len());
        if remaining == 0 {
            truncated = true;
            break;
        }
        if chunk.len() <= remaining {
            body.extend_from_slice(&chunk);
        } else {
            body.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(CapturedResponse {
        status,
        status_text,
        headers,
        body,
        truncated,
        duration_ms,
    })
}
