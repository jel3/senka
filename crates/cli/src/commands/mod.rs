mod env;
mod init;
mod log;
mod req;
mod run;
pub mod complete;

use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a new Senka project in the current directory.
    Init,

    /// Manage environments.
    Env {
        #[command(subcommand)]
        action: EnvAction,
    },

    /// List available requests.
    #[command(name = "req")]
    Req {
        #[command(subcommand)]
        action: ReqAction,
    },

    /// Execute a request.
    Run {
        /// Request name to execute.
        #[arg(add = ArgValueCompleter::new(complete::complete_request_names))]
        request: String,

        /// Environment to use.
        #[arg(long, add = ArgValueCompleter::new(complete::complete_env_names))]
        env: Option<String>,

        /// Variable overrides (key=value).
        #[arg(long = "var")]
        vars: Vec<String>,

        /// Output full JSON response.
        #[arg(long)]
        json: bool,

        /// Show response headers.
        #[arg(long)]
        show_headers: bool,

        /// Fail on non-2xx status.
        #[arg(long)]
        fail: bool,

        /// Disable TLS verification (dangerous).
        #[arg(long)]
        insecure: bool,

        /// Disable redaction (dangerous).
        #[arg(long)]
        no_redact: bool,

        /// Disable color output.
        #[arg(long)]
        no_color: bool,
    },

    /// Generate shell completion scripts (pipe output to your shell's config).
    Completions {
        /// Shell to generate completions for.
        shell: clap_complete::Shell,
    },

    /// Launch the interactive TUI.
    #[cfg(feature = "tui")]
    Tui,

    /// Query and manage logs.
    Log {
        #[command(subcommand)]
        action: LogAction,

        /// Output as JSON.
        #[arg(long, global = true)]
        json: bool,

        /// Disable color output.
        #[arg(long, global = true)]
        no_color: bool,
    },
}

#[derive(Subcommand)]
pub enum EnvAction {
    /// List available environments.
    List,
    /// Set default environment.
    Use { name: String },
    /// Set an env variable.
    Set {
        /// KEY=VALUE pair.
        pair: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// Set a secret (stored in OS keychain).
    SetSecret {
        key: String,
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ReqAction {
    /// List available requests.
    List,
    /// Create a new request file.
    New { name: String },
}

#[derive(Subcommand)]
pub enum LogAction {
    /// Show recent log entries.
    Tail,
    /// List log entries with filters.
    List {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        status: Option<u16>,
        #[arg(long)]
        req: Option<String>,
    },
    /// Show a specific log entry.
    Show { id: String },
    /// Prune old log entries.
    Prune {
        #[arg(long, default_value = "30d")]
        keep: String,
    },
    /// Export logs as JSONL.
    Export,
    /// Delete all log entries.
    Clear,
    /// Delete log entries by ID or filters.
    Delete {
        /// Delete a single entry by ID.
        id: Option<String>,
        /// Delete entries with timestamp >= now - duration (e.g. 1h, 7d).
        #[arg(long)]
        since: Option<String>,
        /// Delete entries matching this HTTP status code.
        #[arg(long)]
        status: Option<u16>,
        /// Delete entries matching this request name (substring).
        #[arg(long)]
        req: Option<String>,
    },
}

pub async fn dispatch(cmd: Command) -> anyhow::Result<()> {
    match cmd {
        Command::Init => init::run(),
        Command::Env { action } => match action {
            EnvAction::List => env::list(),
            EnvAction::Use { name } => env::use_env(&name),
            EnvAction::Set { pair, env } => env::set(&pair, env.as_deref()),
            EnvAction::SetSecret { key, env } => env::set_secret(&key, env.as_deref()),
        },
        Command::Req { action } => match action {
            ReqAction::List => req::list(),
            ReqAction::New { name } => req::new(&name),
        },
        Command::Run {
            request,
            env,
            vars,
            json,
            show_headers,
            fail,
            insecure,
            no_redact,
            no_color,
        } => {
            run::run(run::RunArgs {
                request,
                env,
                vars,
                json,
                show_headers,
                fail,
                insecure,
                no_redact,
                no_color,
            })
            .await
        }
        Command::Completions { shell } => {
            use clap::CommandFactory;
            use clap_complete::generate;
            let mut cmd = crate::Cli::command();
            generate(shell, &mut cmd, "senka", &mut std::io::stdout());
            Ok(())
        }
        #[cfg(feature = "tui")]
        Command::Tui => senka_tui::run().await,
        Command::Log {
            action,
            json,
            no_color,
        } => log::handle(action, json, no_color),
    }
}
