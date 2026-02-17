use std::fs;

use anyhow::Context;

use senka_core::loader;

pub fn list() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no tool.yml found)")?;

    let requests = loader::list_requests(&root).context("failed to list requests")?;

    if requests.is_empty() {
        println!("No request files found in requests/");
    } else {
        for name in &requests {
            println!("  {name}");
        }
    }
    Ok(())
}

pub fn new(name: &str) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no tool.yml found)")?;

    let req_dir = root.join("requests");
    fs::create_dir_all(&req_dir).context("failed to create requests/ directory")?;

    let file_path = req_dir.join(format!("{name}.yml"));
    if file_path.exists() {
        anyhow::bail!("request file already exists: {}", file_path.display());
    }

    let content = format!(
        r#"name: {name}
method: GET
url: "{{{{base_url}}}}"
headers: {{}}
query: {{}}
"#
    );

    fs::write(&file_path, content)
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    println!("Created requests/{name}.yml");
    Ok(())
}
