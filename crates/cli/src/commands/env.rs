use anyhow::Context;

use senka_core::loader;

pub fn list() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no tool.yml found)")?;

    let envs = loader::list_envs(&root).context("failed to list environments")?;

    if envs.is_empty() {
        println!("No environment files found in env/");
    } else {
        for name in &envs {
            println!("  {name}");
        }
    }
    Ok(())
}
