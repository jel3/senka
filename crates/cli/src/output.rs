use std::collections::HashMap;

use senka_core::config::RedactionConfig;
use senka_core::redact;
use senka_runner::response::CapturedResponse;
use serde::Serialize;

/// Options controlling output formatting.
pub struct OutputOptions {
    pub json: bool,
    pub show_headers: bool,
    pub no_color: bool,
    pub no_redact: bool,
    pub secret_values: Vec<String>,
}

/// JSON output envelope.
#[derive(Serialize)]
struct JsonOutput {
    status: u16,
    status_text: String,
    duration_ms: u64,
    headers: HashMap<String, String>,
    body: String,
    truncated: bool,
}

/// Format and print the response.
pub fn print_response(
    resp: &CapturedResponse,
    method: &str,
    url: &str,
    redaction: &RedactionConfig,
    opts: &OutputOptions,
) {
    let should_redact = !opts.no_redact;
    let headers = redact_headers(&resp.headers, redaction, opts.no_redact, &opts.secret_values);
    let body_str = String::from_utf8_lossy(&resp.body);

    // Apply secret value redaction to the body text
    let body_str = if should_redact && !opts.secret_values.is_empty() {
        std::borrow::Cow::Owned(redact::redact_secret_values(&body_str, &opts.secret_values))
    } else {
        body_str
    };

    // Apply JSON field redaction if body parses as JSON
    let body_str = if should_redact && !redaction.json_fields.is_empty() {
        if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&body_str) {
            redact::redact_json_fields(&mut json_val, redaction);
            std::borrow::Cow::Owned(serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| body_str.into_owned()))
        } else {
            body_str
        }
    } else {
        body_str
    };

    if opts.json {
        let output = JsonOutput {
            status: resp.status,
            status_text: resp.status_text.clone(),
            duration_ms: resp.duration_ms,
            headers,
            body: body_str.into_owned(),
            truncated: resp.truncated,
        };
        // Unwrap is safe: our struct is always serializable
        println!("{}", serde_json::to_string(&output).unwrap());
        return;
    }

    // Human mode
    let use_color = !opts.no_color && std::env::var_os("NO_COLOR").is_none();

    let status_label = if resp.status >= 200 && resp.status < 300 {
        if use_color {
            format!("\x1b[32m[OK]\x1b[0m {}", resp.status)
        } else {
            format!("[OK] {}", resp.status)
        }
    } else if resp.status >= 400 {
        if use_color {
            format!("\x1b[31m[ERR]\x1b[0m {}", resp.status)
        } else {
            format!("[ERR] {}", resp.status)
        }
    } else {
        format!("[{}]", resp.status)
    };

    println!(
        "{} {} ({} ms)",
        status_label, resp.status_text, resp.duration_ms
    );
    println!("{} {}", method, url);

    if opts.show_headers {
        println!();
        let mut keys: Vec<&String> = headers.keys().collect();
        keys.sort();
        for k in keys {
            println!("  {}: {}", k, headers[k]);
        }
    }

    if !resp.body.is_empty() {
        println!();
        print!("{}", body_str);
        if !body_str.ends_with('\n') {
            println!();
        }
    }

    if resp.truncated {
        println!("... (body truncated)");
    }
}

fn redact_headers(
    headers: &HashMap<String, String>,
    config: &RedactionConfig,
    no_redact: bool,
    secret_values: &[String],
) -> HashMap<String, String> {
    if no_redact {
        return headers.clone();
    }
    headers
        .iter()
        .map(|(k, v)| {
            let val = redact::redact_header_value(k, v, config);
            let val = redact::redact_secret_values(&val, secret_values);
            (k.clone(), val)
        })
        .collect()
}
