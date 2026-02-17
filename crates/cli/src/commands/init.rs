use std::fs;
use std::path::Path;

use anyhow::{bail, Context};

const TOOL_YML: &str = r#"name: my-project

defaults:
  env: dev
  timeout_ms: 30000
  max_redirects: 10

redaction:
  headers:
    - authorization
    - cookie
    - set-cookie
  query_params:
    - token
    - api_key
  json_fields:
    - password
    - access_token

logging:
  enabled: true
  max_body_kb: 256
  retention_days: 30
"#;

const DEV_ENV_YML: &str = r#"base_url: http://localhost:3000
"#;

const EXAMPLE_REQUEST_YML: &str = r#"name: example.get
method: GET
url: "{{base_url}}/health"
"#;

pub fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    if cwd.join("senka.yml").exists() {
        bail!("Senka project already initialized (senka.yml exists)");
    }

    // Create directories
    fs::create_dir_all(cwd.join("env")).context("failed to create env/ directory")?;
    fs::create_dir_all(cwd.join("requests")).context("failed to create requests/ directory")?;
    fs::create_dir_all(cwd.join(".senka")).context("failed to create .senka/ directory")?;

    // Write files
    write_if_missing(&cwd.join("senka.yml"), TOOL_YML)?;
    write_if_missing(&cwd.join("env").join("dev.yml"), DEV_ENV_YML)?;
    write_if_missing(
        &cwd.join("requests").join("example.get.yml"),
        EXAMPLE_REQUEST_YML,
    )?;

    println!("Initialized Senka project in {}", cwd.display());
    println!();
    println!("  senka.yml                   Project configuration");
    println!("  env/dev.yml                Development environment");
    println!("  requests/example.get.yml   Example request");
    println!();
    println!("Next: senka run example.get --env dev");

    Ok(())
}

fn write_if_missing(path: &Path, content: &str) -> anyhow::Result<()> {
    if !path.exists() {
        fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}
