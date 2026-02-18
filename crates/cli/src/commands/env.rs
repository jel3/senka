use anyhow::{bail, Context};

use senka_core::loader;

pub fn list() -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no senka.yml found)")?;

    let envs = loader::list_envs(&root).context("failed to list environments")?;

    if envs.is_empty() {
        println!("No environment files found in senka-env/");
    } else {
        for name in &envs {
            println!("  {name}");
        }
    }
    Ok(())
}

pub fn set_secret(key: &str, env: Option<&str>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no senka.yml found)")?;
    let config = loader::load_config(&root).context("failed to load senka.yml")?;

    let env_name = env
        .or(config.defaults.env.as_deref())
        .ok_or_else(|| anyhow::anyhow!("no environment specified and no default set in senka.yml"))?;

    // Verify env file exists
    let envs = loader::list_envs(&root).context("failed to list environments")?;
    if !envs.iter().any(|e| e == env_name) {
        bail!("environment '{env_name}' not found (create senka-env/{env_name}.yml first)");
    }

    let value = rpassword::prompt_password(format!("Enter secret value for '{key}': "))
        .context("failed to read secret value")?;

    if value.is_empty() {
        bail!("secret value cannot be empty");
    }

    senka_secrets::set(&config.name, env_name, key, &value)
        .context("failed to store secret in keychain")?;

    println!("Secret '{key}' stored for env '{env_name}'");
    Ok(())
}
