use anyhow::Context;

use senka_core::loader;
use senka_core::util::{format_ts, now_epoch_ms};
use senka_store::db::{self, ListFilters};
use senka_store::models::Run;

use crate::commands::LogAction;

pub fn handle(action: LogAction, json: bool, no_color: bool) -> anyhow::Result<()> {
    match action {
        LogAction::Tail => handle_tail(json, no_color),
        LogAction::List { since, status, req } => handle_list(since, status, req, json, no_color),
        LogAction::Show { id } => handle_show(&id, json, no_color),
        LogAction::Prune { keep } => handle_prune(&keep),
        LogAction::Export => handle_export(),
        LogAction::Clear => handle_clear(),
        LogAction::Delete { id, since, status, req } => handle_delete(id, since, status, req),
    }
}

fn open_log_db() -> anyhow::Result<senka_store::Connection> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = loader::find_project_root(&cwd)
        .context("not inside a Senka project (no tool.yml found)")?;
    let db_path = root.join(".senka").join("logs.db");
    db::open(&db_path).context("failed to open log database")
}

fn handle_tail(json: bool, no_color: bool) -> anyhow::Result<()> {
    let conn = open_log_db()?;
    let runs = db::tail(&conn, 20)?;

    if json {
        println!("{}", serde_json::to_string(&runs)?);
    } else {
        print_run_table(&runs, no_color);
    }
    Ok(())
}

fn handle_list(
    since: Option<String>,
    status: Option<u16>,
    req: Option<String>,
    json: bool,
    no_color: bool,
) -> anyhow::Result<()> {
    let conn = open_log_db()?;

    let since_ts = match since {
        Some(ref s) => {
            let duration_ms = db::parse_duration_str(s)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let now = now_epoch_ms();
            Some(now.saturating_sub(duration_ms))
        }
        None => None,
    };

    let filters = ListFilters {
        since: since_ts,
        status,
        request_name: req,
    };

    let runs = db::list(&conn, &filters)?;

    if json {
        println!("{}", serde_json::to_string(&runs)?);
    } else {
        print_run_table(&runs, no_color);
    }
    Ok(())
}

fn handle_show(id: &str, json: bool, _no_color: bool) -> anyhow::Result<()> {
    let conn = open_log_db()?;
    let entry = db::show(&conn, id)?;

    match entry {
        Some(rwp) => {
            if json {
                println!("{}", serde_json::to_string(&rwp)?);
            } else {
                let status_str = match rwp.run.status {
                    Some(s) => s.to_string(),
                    None => "ERR".to_string(),
                };
                println!("ID:       {}", rwp.run.id);
                println!("Time:     {}", format_ts(rwp.run.ts));
                println!("Request:  {}", rwp.run.request_name);
                println!("Method:   {}", rwp.run.method);
                println!("URL:      {}", rwp.run.url);
                println!("Status:   {status_str}");
                println!("Duration: {} ms", rwp.run.duration_ms);
                println!("Env:      {}", rwp.run.env);

                if let Some(ref err) = rwp.run.error {
                    println!("Error:    {err}");
                }

                println!();
                println!("--- Request Headers ---");
                println!("{}", rwp.request_headers);

                if let Some(ref body) = rwp.request_body {
                    println!();
                    println!("--- Request Body ---");
                    println!("{body}");
                }

                println!();
                println!("--- Response Headers ---");
                println!("{}", rwp.response_headers);

                if let Some(ref body) = rwp.response_body {
                    println!();
                    println!("--- Response Body ---");
                    println!("{body}");
                }
            }
        }
        None => {
            anyhow::bail!("no log entry found with id '{id}'");
        }
    }
    Ok(())
}

fn handle_prune(keep: &str) -> anyhow::Result<()> {
    let conn = open_log_db()?;
    let duration_ms = db::parse_duration_str(keep)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let cutoff = now_epoch_ms().saturating_sub(duration_ms);
    let deleted = db::prune(&conn, cutoff)?;
    eprintln!("pruned {deleted} log entries older than {keep}");
    Ok(())
}

fn handle_export() -> anyhow::Result<()> {
    let conn = open_log_db()?;
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    let count = db::export_jsonl(&conn, &mut writer)?;
    eprintln!("exported {count} entries");
    Ok(())
}

fn handle_clear() -> anyhow::Result<()> {
    let conn = open_log_db()?;
    let deleted = db::clear(&conn)?;
    eprintln!("cleared {deleted} log entries");
    Ok(())
}

fn handle_delete(
    id: Option<String>,
    since: Option<String>,
    status: Option<u16>,
    req: Option<String>,
) -> anyhow::Result<()> {
    let conn = open_log_db()?;

    // If an ID is provided, delete that single entry
    if let Some(ref id) = id {
        let found = db::delete_by_id(&conn, id)?;
        if found {
            eprintln!("deleted log entry '{id}'");
        } else {
            anyhow::bail!("no log entry found with id '{id}'");
        }
        return Ok(());
    }

    // Otherwise use filters (at least one must be provided)
    if since.is_none() && status.is_none() && req.is_none() {
        anyhow::bail!("provide an ID or at least one filter (--since, --status, --req)");
    }

    let since_ts = match since {
        Some(ref s) => {
            let duration_ms = db::parse_duration_str(s)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let now = now_epoch_ms();
            Some(now.saturating_sub(duration_ms))
        }
        None => None,
    };

    let filters = ListFilters {
        since: since_ts,
        status,
        request_name: req,
    };

    let deleted = db::delete_filtered(&conn, &filters)?;
    eprintln!("deleted {deleted} log entries");
    Ok(())
}

fn print_run_table(runs: &[Run], no_color: bool) {
    if runs.is_empty() {
        println!("no log entries found");
        return;
    }

    let use_color = !no_color && std::env::var_os("NO_COLOR").is_none();

    // Header
    println!(
        "{:<28} {:<6} {:<20} {:<6} {:>6} URL",
        "ID", "STATUS", "REQUEST", "METHOD", "MS"
    );
    println!("{}", "-".repeat(90));

    for run in runs {
        let status_str = match run.status {
            Some(s) => {
                if use_color {
                    if (200..300).contains(&s) {
                        format!("\x1b[32m{s:<6}\x1b[0m")
                    } else if s >= 400 {
                        format!("\x1b[31m{s:<6}\x1b[0m")
                    } else {
                        format!("{s:<6}")
                    }
                } else {
                    format!("{s:<6}")
                }
            }
            None => {
                if use_color {
                    "\x1b[31mERR   \x1b[0m".to_string()
                } else {
                    "ERR   ".to_string()
                }
            }
        };

        // Truncate long values for table display
        let req_name = truncate_str(&run.request_name, 20);
        let url = truncate_str(&run.url, 50);

        println!(
            "{:<28} {} {:<20} {:<6} {:>6} {}",
            run.id, status_str, req_name, run.method, run.duration_ms, url
        );
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

