use std::fs;

use anyhow::{bail, Context};

use senka_core::env::Environment;
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

pub fn use_env(name: &str) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no senka.yml found)")?;

    // Verify environment file exists
    let envs = loader::list_envs(&root).context("failed to list environments")?;
    if !envs.iter().any(|e| e == name) {
        bail!("environment '{name}' not found (create senka-env/{name}.yml first)");
    }

    // Load config, update default env, write back
    let mut config = loader::load_config(&root).context("failed to load senka.yml")?;
    config.defaults.env = Some(name.to_string());

    let yaml = serde_yaml::to_string(&config).context("failed to serialize config")?;
    let config_path = root.join("senka.yml");
    fs::write(&config_path, yaml)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    println!("Default environment set to '{name}'");
    Ok(())
}

pub fn set_secret(key: &str, env: Option<&str>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no senka.yml found)")?;
    let config = loader::load_config(&root).context("failed to load senka.yml")?;

    let env_name = env.or(config.defaults.env.as_deref()).ok_or_else(|| {
        anyhow::anyhow!("no environment specified and no default set in senka.yml")
    })?;

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

pub fn set(pair: &str, env: Option<&str>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no senka.yml found)")?;
    let config = loader::load_config(&root).context("failed to load senka.yml")?;

    // Determine environment name from flag or config default
    let env_name = env.or(config.defaults.env.as_deref()).ok_or_else(|| {
        anyhow::anyhow!("no environment specified and no default set in senka.yml")
    })?;

    // Parse KEY=VALUE
    let (key, value) = pair
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("invalid format: expected KEY=VALUE, got '{pair}'"))?;

    if key.is_empty() {
        bail!("key cannot be empty");
    }

    // Load existing env or start fresh
    let env_path = root.join("senka-env").join(format!("{env_name}.yml"));
    let mut environment = if env_path.exists() {
        loader::load_env(&root, env_name)
            .with_context(|| format!("failed to load senka-env/{env_name}.yml"))?
    } else {
        let env_dir = root.join("senka-env");
        fs::create_dir_all(&env_dir).context("failed to create senka-env/ directory")?;
        Environment::default()
    };

    // Insert/update the variable
    environment.vars.insert(key.to_string(), value.to_string());

    // Write back
    let yaml = serde_yaml::to_string(&environment).context("failed to serialize environment")?;
    fs::write(&env_path, yaml)
        .with_context(|| format!("failed to write {}", env_path.display()))?;

    println!("Set '{key}' = '{value}' in env '{env_name}'");
    Ok(())
}
