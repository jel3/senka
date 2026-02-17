use clap::Subcommand;

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
        request: String,

        /// Environment to use.
        #[arg(long)]
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

    /// Query and manage logs.
    Log {
        #[command(subcommand)]
        action: LogAction,
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
}

pub async fn dispatch(cmd: Command) -> anyhow::Result<()> {
    match cmd {
        Command::Init => {
            todo!("senka init")
        }
        Command::Env { action: _ } => {
            todo!("env commands")
        }
        Command::Req { action: _ } => {
            todo!("req commands")
        }
        Command::Run { .. } => {
            todo!("run command")
        }
        Command::Log { action: _ } => {
            todo!("log commands")
        }
    }
}
