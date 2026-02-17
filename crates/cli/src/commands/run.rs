use anyhow::{bail, Context};

use senka_core::loader;
use senka_core::resolve;
use senka_runner::execute::{self, ClientOptions};

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

    let resp = execute::execute(&client, &req, config.logging.max_body_kb)
        .await
        .map_err(|e| match &e {
            execute::RunError::Timeout => anyhow::anyhow!("{e}"),
            execute::RunError::Network(_) => anyhow::anyhow!("{e}"),
            execute::RunError::Definition(_) => anyhow::anyhow!("{e}"),
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
