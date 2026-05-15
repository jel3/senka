mod commands;
mod output;

use clap::{CommandFactory, Parser};
use rand::Rng;

#[derive(Parser)]
#[command(name = "senka", version, about = "CLI-first HTTP runner")]
struct Cli {
    #[command(subcommand)]
    command: Option<commands::Command>,
}

#[tokio::main]
async fn main() {
    // Handle shell completion requests before anything else.
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            print_banner();
            println!();
            // Re-invoke clap help
            let _ = Cli::parse_from(["senka", "--help"]);
            return;
        }
    };

    if let Err(e) = commands::dispatch(command).await {
        let exit_code = classify_error(&e);
        eprintln!("error: {e:#}");
        std::process::exit(exit_code);
    }
}

fn print_banner() {
    const LINES: [&str; 6] = [
        "                 _         ",
        " ___  ___ _ __ | | ____ _ ",
        "/ __|/ _ \\ '_ \\| |/ / _` |",
        "\\__ \\  __/ | | |   < (_| |",
        "|___/\\___|_| |_|_|\\_\\__,_|",
        "                           ",
    ];

    if std::env::var_os("NO_COLOR").is_some() {
        for line in LINES {
            println!("{line}");
        }
        return;
    }

    const STAR_CHARS: [char; 5] = ['*', '·', '✦', '✧', '⋆'];
    const COLORS: [&str; 7] = ["31", "93", "33", "32", "36", "34", "35"];

    let mut rng = rand::thread_rng();
    for (i, &line) in LINES.iter().enumerate() {
        let color_code = COLORS[i % COLORS.len()];
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        for (j, &c) in chars.iter().enumerate() {
            if c == ' ' {
                let is_safe_space = j > 1
                    && j < len - 2
                    && chars[j - 1] == ' '
                    && chars[j + 1] == ' '
                    && (chars[j - 2] == ' ' || chars[j + 2] == ' ');

                if is_safe_space && rng.gen_bool(0.15) {
                    let star = STAR_CHARS[rng.gen_range(0..STAR_CHARS.len())];
                    print!("\x1b[37m{star}\x1b[0m");
                } else {
                    print!("\x1b[{color_code}m{c}\x1b[0m");
                }
            } else {
                print!("\x1b[{color_code}m{c}\x1b[0m");
            }
        }
        println!();
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
