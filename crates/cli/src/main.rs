mod commands;
mod output;

use clap::Parser;

#[derive(Parser)]
#[command(name = "senka", version, about = "CLI-first HTTP runner")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = commands::dispatch(cli.command).await {
        let exit_code = classify_error(&e);
        eprintln!("error: {e:#}");
        std::process::exit(exit_code);
    }
}

/// Map errors to exit codes per spec:
/// 2 = config error, 3 = network/TLS, 4 = timeout, 5 = non-2xx with --fail
fn classify_error(err: &anyhow::Error) -> i32 {
    let msg = format!("{err:#}");

    if msg.contains("request failed with status") {
        5
    } else if msg.contains("timed out") {
        4
    } else if msg.contains("network error") || msg.contains("TLS") || msg.contains("dns") {
        3
    } else {
        2
    }
}
