use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use senka_core::util::format_ts;

use crate::app::{App, Tab};

/// Returns true if color should be disabled.
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

/// Style for selected/highlighted items. Uses reversed if NO_COLOR, else cyan.
fn highlight_style() -> Style {
    if no_color() {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
            .fg(ratatui::style::Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }
}

/// Style for active tab indicator.
fn active_tab_style() -> Style {
    if no_color() {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default()
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }
}

/// Style for status codes.
fn status_style(status: Option<u16>) -> Style {
    if no_color() {
        return Style::default();
    }
    match status {
        Some(s) if (200..300).contains(&s) => Style::default().fg(ratatui::style::Color::Green),
        Some(s) if s >= 400 => Style::default().fg(ratatui::style::Color::Red),
        None => Style::default().fg(ratatui::style::Color::Red),
        _ => Style::default(),
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(0),   // content
            Constraint::Length(1), // status bar
        ])
        .split(f.area());

    draw_title_bar(f, app, chunks[0]);

    match app.current_tab {
        Tab::Requests => draw_requests_tab(f, app, chunks[1]),
        Tab::Logs => draw_logs_tab(f, app, chunks[1]),
    }

    draw_status_bar(f, chunks[2]);

    // Overlay env popup if open
    if app.env_popup.is_some() {
        draw_env_popup(f, app);
    }
}

fn draw_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let env_label = app
        .active_env
        .as_deref()
        .unwrap_or("none");

    let req_style = if app.current_tab == Tab::Requests {
        active_tab_style()
    } else {
        Style::default()
    };
    let log_style = if app.current_tab == Tab::Logs {
        active_tab_style()
    } else {
        Style::default()
    };

    let line = Line::from(vec![
        Span::styled(" senka ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("| "),
        Span::raw(format!("project: {} ", app.config.name)),
        Span::raw("| "),
        Span::raw(format!("env: {env_label} ")),
        Span::raw("| "),
        Span::styled("[Requests]", req_style),
        Span::raw(" "),
        Span::styled("[Logs]", log_style),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

fn draw_requests_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Left: request list
    let items: Vec<ListItem> = app
        .request_names
        .iter()
        .map(|name| ListItem::new(name.as_str()))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Requests"))
        .highlight_style(highlight_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.request_names.is_empty() {
        state.select(Some(app.req_list_idx));
    }
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Right: response or request preview
    if app.is_running {
        let text = Paragraph::new("Running request...")
            .block(Block::default().borders(Borders::ALL).title("Response"));
        f.render_widget(text, chunks[1]);
    } else if let Some(ref resp) = app.response {
        draw_response_view(f, resp, chunks[1]);
    } else if let Some(ref req) = app.loaded_request {
        draw_request_preview(f, req, chunks[1]);
    } else {
        let text = Paragraph::new("No request selected")
            .block(Block::default().borders(Borders::ALL).title("Detail"));
        f.render_widget(text, chunks[1]);
    }
}

fn draw_request_preview(f: &mut Frame, req: &senka_core::request::RequestDef, area: Rect) {
    let mut lines = vec![
        Line::from(format!("{} {}", req.method, req.url)),
        Line::from(""),
    ];

    if !req.headers.is_empty() {
        lines.push(Line::from("Headers:"));
        let mut keys: Vec<&String> = req.headers.keys().collect();
        keys.sort();
        for k in keys {
            lines.push(Line::from(format!("  {}: {}", k, req.headers[k])));
        }
        lines.push(Line::from(""));
    }

    if !req.query.is_empty() {
        lines.push(Line::from("Query:"));
        let mut keys: Vec<&String> = req.query.keys().collect();
        keys.sort();
        for k in keys {
            lines.push(Line::from(format!("  {}: {}", k, req.query[k])));
        }
        lines.push(Line::from(""));
    }

    if let Some(ref body) = req.body {
        lines.push(Line::from("Body:"));
        let body_str = match body {
            senka_core::request::Body::Raw(s) => s.clone(),
            senka_core::request::Body::Json(v) => {
                serde_json::to_string_pretty(v).unwrap_or_else(|_| format!("{v:?}"))
            }
            senka_core::request::Body::Form(m) => {
                serde_json::to_string_pretty(m).unwrap_or_else(|_| format!("{m:?}"))
            }
        };
        for line in body_str.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
    }

    let text = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Preview: {}", req.name)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(text, area);
}

fn draw_response_view(f: &mut Frame, resp: &crate::app::ResponseView, area: Rect) {
    let mut lines = Vec::new();

    if let Some(ref err) = resp.error {
        lines.push(Line::from(Span::styled(
            format!("Error: {err}"),
            status_style(None),
        )));
    } else {
        let status_str = match resp.status {
            Some(s) => format!("{s} {}", resp.status_text),
            None => "ERR".to_string(),
        };
        lines.push(Line::from(Span::styled(
            status_str,
            status_style(resp.status),
        )));
        lines.push(Line::from(format!("Duration: {} ms", resp.duration_ms)));
        lines.push(Line::from(""));

        if !resp.headers_text.is_empty() {
            lines.push(Line::from("--- Headers ---"));
            for line in resp.headers_text.lines() {
                lines.push(Line::from(line.to_string()));
            }
            lines.push(Line::from(""));
        }

        if !resp.body_text.is_empty() {
            lines.push(Line::from("--- Body ---"));
            for line in resp.body_text.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
    }

    let text = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Response"))
        .wrap(Wrap { trim: false });
    f.render_widget(text, area);
}

fn draw_logs_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Left: log entries list
    let items: Vec<ListItem> = app
        .log_entries
        .iter()
        .map(|run| {
            let status_str = match run.status {
                Some(s) => format!("{s}"),
                None => "ERR".to_string(),
            };
            let text = format!(
                "{} {} {} ms",
                status_str, run.request_name, run.duration_ms
            );
            ListItem::new(text).style(status_style(run.status))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Logs"))
        .highlight_style(highlight_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.log_entries.is_empty() {
        state.select(Some(app.log_list_idx));
    }
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Right: log detail
    if let Some(ref detail) = app.log_detail {
        draw_log_detail(f, detail, chunks[1]);
    } else {
        let text = Paragraph::new("Press Enter to view log details")
            .block(Block::default().borders(Borders::ALL).title("Detail"));
        f.render_widget(text, chunks[1]);
    }
}

fn draw_log_detail(f: &mut Frame, detail: &senka_store::models::RunWithPayload, area: Rect) {
    let status_str = match detail.run.status {
        Some(s) => s.to_string(),
        None => "ERR".to_string(),
    };

    let mut lines = vec![
        Line::from(format!("ID:       {}", detail.run.id)),
        Line::from(format!("Time:     {}", format_ts(detail.run.ts))),
        Line::from(format!("Request:  {}", detail.run.request_name)),
        Line::from(format!("Method:   {}", detail.run.method)),
        Line::from(format!("URL:      {}", detail.run.url)),
        Line::from(format!("Status:   {status_str}")),
        Line::from(format!("Duration: {} ms", detail.run.duration_ms)),
        Line::from(format!("Env:      {}", detail.run.env)),
    ];

    if let Some(ref err) = detail.run.error {
        lines.push(Line::from(format!("Error:    {err}")));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("--- Request Headers ---"));
    for line in detail.request_headers.lines() {
        lines.push(Line::from(line.to_string()));
    }

    if let Some(ref body) = detail.request_body {
        lines.push(Line::from(""));
        lines.push(Line::from("--- Request Body ---"));
        for line in body.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("--- Response Headers ---"));
    for line in detail.response_headers.lines() {
        lines.push(Line::from(line.to_string()));
    }

    if let Some(ref body) = detail.response_body {
        lines.push(Line::from(""));
        lines.push(Line::from("--- Response Body ---"));
        for line in body.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    let text = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Log Detail"))
        .wrap(Wrap { trim: false });
    f.render_widget(text, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":switch  "),
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":nav  "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":run  "),
        Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":env  "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":clear  "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":quit"),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_env_popup(f: &mut Frame, app: &App) {
    let popup = match app.env_popup.as_ref() {
        Some(p) => p,
        None => return,
    };

    let area = centered_rect(40, 50, f.area());

    f.render_widget(Clear, area);

    let items: Vec<ListItem> = popup
        .envs
        .iter()
        .map(|name| ListItem::new(name.as_str()))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select Environment"),
        )
        .highlight_style(highlight_style())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(popup.selected));
    f.render_stateful_widget(list, area, &mut state);
}

/// Create a centered rectangle with given percentage width/height.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
