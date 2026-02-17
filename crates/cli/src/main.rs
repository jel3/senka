mod commands;

use clap::Parser;

#[derive(Parser)]
#[command(name = "senka", version, about = "CLI-first HTTP runner")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::dispatch(cli.command).await
}
