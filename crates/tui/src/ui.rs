use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use senka_core::util::format_ts;

use crate::app::{App, Tab};
use crate::form::{FormRow, RequestForm, TextInput};

const BANNER: [&str; 5] = [
    "                 _         ",
    " ___  ___ _ __ | | ____ _ ",
    "/ __|/ _ \\ '_ \\| |/ / _` |",
    "\\__ \\  __/ | | |   < (_| |",
    "|___/\\___|_| |_|_|\\_\\__,_|",
];

/// Returns true if color should be disabled.
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

/// Style for text selection highlighting.
fn select_highlight_style() -> Style {
    if no_color() {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().bg(ratatui::style::Color::DarkGray)
    }
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

    draw_status_bar(f, app, chunks[2]);

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
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
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

    // Right: form or response or request preview
    if let Some(ref form) = app.request_form {
        draw_request_form(f, form, chunks[1]);
    } else if app.is_running {
        let text = Paragraph::new("Running request...")
            .block(Block::default().borders(Borders::ALL).title("Response"));
        f.render_widget(text, chunks[1]);
    } else if app.response.is_some() {
        draw_response_view(f, app, chunks[1]);
    } else if let Some(ref req) = app.loaded_request {
        draw_request_preview(f, req, chunks[1]);
    } else {
        draw_welcome(f, chunks[1]);
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

fn draw_response_view(f: &mut Frame, app: &crate::app::App, area: Rect) {
    let resp = app.response.as_ref().unwrap();
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

    let title = if app.select_mode {
        "Response (↑↓:select  y:copy  Esc:cancel)"
    } else if app.detail_focused && app.config.tui.keyboard_select {
        "Response (↑↓:scroll  v:select  y:copy all  Esc:back)"
    } else if app.detail_focused {
        "Response (↑↓:scroll  Esc:back)"
    } else {
        "Response (→:focus)"
    };
    let border_style = if app.detail_focused {
        highlight_style()
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    app.detail_line_count.set(lines.len());
    app.detail_viewport_height.set(inner.height as usize);

    // Apply selection highlighting
    if app.select_mode {
        let sel_start = app.select_anchor.min(app.select_cursor);
        let sel_end = app.select_anchor.max(app.select_cursor);
        for (i, line) in lines.iter_mut().enumerate() {
            if i >= sel_start && i <= sel_end {
                let new_spans: Vec<Span> = line
                    .spans
                    .drain(..)
                    .map(|span| {
                        Span::styled(
                            span.content.to_string(),
                            span.style.patch(select_highlight_style()),
                        )
                    })
                    .collect();
                *line = Line::from(new_spans);
            }
        }
    }

    let text = Paragraph::new(lines)
        .block(block)
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(text, area);
}

fn draw_logs_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
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
    if app.log_detail.is_some() {
        draw_log_detail(f, app, chunks[1]);
    } else {
        draw_welcome(f, chunks[1]);
    }
}

fn draw_log_detail(f: &mut Frame, app: &crate::app::App, area: Rect) {
    let detail = app.log_detail.as_ref().unwrap();
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

    let title = if app.select_mode {
        "Log Detail (↑↓:select  y:copy  Esc:cancel)"
    } else if app.detail_focused && app.config.tui.keyboard_select {
        "Log Detail (↑↓:scroll  v:select  y:copy all  Esc:back)"
    } else if app.detail_focused {
        "Log Detail (↑↓:scroll  Esc:back)"
    } else {
        "Log Detail (→:focus)"
    };
    let border_style = if app.detail_focused {
        highlight_style()
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    app.detail_line_count.set(lines.len());
    app.detail_viewport_height.set(inner.height as usize);

    // Apply selection highlighting
    if app.select_mode {
        let sel_start = app.select_anchor.min(app.select_cursor);
        let sel_end = app.select_anchor.max(app.select_cursor);
        for (i, line) in lines.iter_mut().enumerate() {
            if i >= sel_start && i <= sel_end {
                let new_spans: Vec<Span> = line
                    .spans
                    .drain(..)
                    .map(|span| {
                        Span::styled(
                            span.content.to_string(),
                            span.style.patch(select_highlight_style()),
                        )
                    })
                    .collect();
                *line = Line::from(new_spans);
            }
        }
    }

    let text = Paragraph::new(lines)
        .block(block)
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(text, area);
}

fn draw_request_form(f: &mut Frame, form: &RequestForm, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("New Request (Ctrl+S:save  Esc:cancel)");

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in form.rows.iter().enumerate() {
        let is_focused = i == form.focused_row;
        lines.push(render_form_row(form, row, is_focused));
    }

    if let Some(ref err) = form.error_message {
        lines.push(Line::from(""));
        let err_style = if no_color() {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(ratatui::style::Color::Red)
        };
        lines.push(Line::from(Span::styled(format!("Error: {err}"), err_style)));
    }

    // Compute scroll offset to keep focused row visible
    let scroll = if form.focused_row >= visible_height {
        (form.focused_row - visible_height + 1) as u16
    } else {
        0
    };

    let paragraph = Paragraph::new(lines).scroll((scroll, 0));
    f.render_widget(paragraph, inner);
}

fn render_form_row<'a>(form: &RequestForm, row: &FormRow, focused: bool) -> Line<'a> {
    let prefix = if focused { "> " } else { "  " };
    let style = if focused {
        highlight_style()
    } else {
        Style::default()
    };
    let dim_style = Style::default().add_modifier(Modifier::DIM);

    match row {
        FormRow::Name => {
            let val = display_text_field(&form.name, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}Name:     {val}"), style))
        }
        FormRow::Method => {
            let val = form.method.as_str();
            Line::from(Span::styled(
                format!("{prefix}Method:   < {val} >"),
                style,
            ))
        }
        FormRow::Url => {
            let val = display_text_field(&form.url, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}URL:      {val}"), style))
        }
        FormRow::SectionLabel(label) => {
            Line::from(Span::styled(format!("  --- {label} ---"), dim_style))
        }
        FormRow::HeaderKey(i) => {
            let val = display_text_field(&form.headers[*i].key, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Key:    {val}"), style))
        }
        FormRow::HeaderValue(i) => {
            let val = display_text_field(&form.headers[*i].value, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Value:  {val}"), style))
        }
        FormRow::AddHeader => {
            Line::from(Span::styled(format!("{prefix}  [+ Add Header]"), style))
        }
        FormRow::QueryKey(i) => {
            let val = display_text_field(&form.query[*i].key, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Key:    {val}"), style))
        }
        FormRow::QueryValue(i) => {
            let val = display_text_field(&form.query[*i].value, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Value:  {val}"), style))
        }
        FormRow::AddQuery => {
            Line::from(Span::styled(format!("{prefix}  [+ Add Param]"), style))
        }
        FormRow::AuthType => {
            let val = form.auth_type.label();
            Line::from(Span::styled(
                format!("{prefix}Auth:     < {val} >"),
                style,
            ))
        }
        FormRow::AuthBearerToken => {
            let val = display_text_field(&form.auth_bearer_token, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Token:  {val}"), style))
        }
        FormRow::AuthBasicUsername => {
            let val = display_text_field(&form.auth_basic_username, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  User:   {val}"), style))
        }
        FormRow::AuthBasicPassword => {
            let val = display_text_field(&form.auth_basic_password, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Pass:   {val}"), style))
        }
        FormRow::BodyType => {
            let val = form.body_type.label();
            Line::from(Span::styled(
                format!("{prefix}Body:     < {val} >"),
                style,
            ))
        }
        FormRow::BodyRawContent => {
            let val = display_text_field(&form.body_raw, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Content: {val}"), style))
        }
        FormRow::BodyJsonContent => {
            let val = display_text_field(&form.body_json, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  JSON:    {val}"), style))
        }
        FormRow::BodyFormKey(i) => {
            let val = display_text_field(&form.body_form[*i].key, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Key:    {val}"), style))
        }
        FormRow::BodyFormValue(i) => {
            let val = display_text_field(&form.body_form[*i].value, focused && form.editing);
            Line::from(Span::styled(format!("{prefix}  Value:  {val}"), style))
        }
        FormRow::AddBodyFormPair => {
            Line::from(Span::styled(format!("{prefix}  [+ Add Field]"), style))
        }
        FormRow::Spacer => Line::from(""),
        FormRow::Save => Line::from(Span::styled(
            format!("{prefix}[Save Request]"),
            style,
        )),
    }
}

fn display_text_field(input: &TextInput, editing: bool) -> String {
    if editing {
        let (before, after) = input.value.split_at(input.cursor);
        format!("{before}|{after}")
    } else if input.value.is_empty() {
        "(empty)".to_string()
    } else {
        input.value.clone()
    }
}

fn draw_welcome(f: &mut Frame, area: Rect) {
    let banner_style = if no_color() {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(ratatui::style::Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for banner_line in &BANNER {
        lines.push(Line::from(Span::styled(*banner_line, banner_style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("  CLI-first HTTP execution engine"));
    lines.push(Line::from(""));
    lines.push(Line::from("  Enter: run request   n: new request"));
    lines.push(Line::from("  Tab: switch tab      e: select env"));
    lines.push(Line::from("  q: quit"));

    let text = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(text, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let bold = Style::default().add_modifier(Modifier::BOLD);

    let line = if let Some(ref form) = app.request_form {
        if form.editing {
            Line::from(vec![
                Span::styled(" Esc/Enter", bold),
                Span::raw(":stop editing  "),
                Span::styled("Ctrl+S", bold),
                Span::raw(":save"),
            ])
        } else {
            Line::from(vec![
                Span::styled(" \u{2191}\u{2193}", bold),
                Span::raw(":nav  "),
                Span::styled("Enter", bold),
                Span::raw(":edit  "),
                Span::styled("\u{2190}\u{2192}", bold),
                Span::raw(":cycle  "),
                Span::styled("Ctrl+D", bold),
                Span::raw(":delete  "),
                Span::styled("Ctrl+S", bold),
                Span::raw(":save  "),
                Span::styled("Esc", bold),
                Span::raw(":cancel"),
            ])
        }
    } else if app.select_mode && app.detail_focused {
        Line::from(vec![
            Span::styled(" \u{2191}\u{2193}", bold),
            Span::raw(":select  "),
            Span::styled("PgUp/PgDn", bold),
            Span::raw(":page  "),
            Span::styled("y/Enter", bold),
            Span::raw(":copy  "),
            Span::styled("Esc", bold),
            Span::raw(":cancel  "),
            Span::styled("q", bold),
            Span::raw(":quit"),
        ])
    } else if app.detail_focused {
        let mut spans = vec![
            Span::styled(" \u{2191}\u{2193}", bold),
            Span::raw(":scroll  "),
            Span::styled("PgUp/PgDn", bold),
            Span::raw(":page  "),
            Span::styled("Home", bold),
            Span::raw(":top  "),
        ];
        if app.config.tui.keyboard_select {
            spans.extend([
                Span::styled("v", bold),
                Span::raw(":select  "),
                Span::styled("y", bold),
                Span::raw(":copy all  "),
            ]);
        }
        spans.extend([
            Span::styled("Esc/\u{2190}", bold),
            Span::raw(":back  "),
            Span::styled("q", bold),
            Span::raw(":quit"),
        ]);
        if let Some((ref msg, _)) = app.status_message {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("[{msg}]"),
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        Line::from(spans)
    } else if app.current_tab == Tab::Logs {
        Line::from(vec![
            Span::styled(" Tab", bold),
            Span::raw(":switch  "),
            Span::styled("\u{2191}\u{2193}", bold),
            Span::raw(":nav  "),
            Span::styled("Enter", bold),
            Span::raw(":view  "),
            Span::styled("\u{2192}", bold),
            Span::raw(":detail  "),
            Span::styled("d", bold),
            Span::raw(":delete  "),
            Span::styled("Ctrl+D", bold),
            Span::raw(":clear all  "),
            Span::styled("e", bold),
            Span::raw(":env  "),
            Span::styled("q", bold),
            Span::raw(":quit"),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Tab", bold),
            Span::raw(":switch  "),
            Span::styled("\u{2191}\u{2193}", bold),
            Span::raw(":nav  "),
            Span::styled("Enter", bold),
            Span::raw(":run  "),
            Span::styled("\u{2192}", bold),
            Span::raw(":detail  "),
            Span::styled("n", bold),
            Span::raw(":new  "),
            Span::styled("e", bold),
            Span::raw(":env  "),
            Span::styled("Esc", bold),
            Span::raw(":clear  "),
            Span::styled("q", bold),
            Span::raw(":quit"),
        ])
    };
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
