use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Context;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use tokio::sync::mpsc;

use senka_core::config::ProjectConfig;
use senka_core::loader;
use senka_core::redact;
use senka_core::request::{Body, RequestDef};
use senka_core::resolve;
use senka_core::util::{format_ts, now_epoch_ms};
use senka_runner::execute::{self, ClientOptions, RunError};
use senka_store::db;
use senka_store::models::{Payload, Run, RunWithPayload};

use crate::form::{FormRow, RequestForm};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Requests,
    Logs,
}

pub struct EnvSelector {
    pub envs: Vec<String>,
    pub selected: usize,
}

pub struct ResponseView {
    pub status: Option<u16>,
    pub status_text: String,
    pub duration_ms: u64,
    pub headers_text: String,
    pub body_text: String,
    pub error: Option<String>,
}

pub enum TaskResult {
    RequestDone(ResponseView),
}

pub struct App {
    // Project context
    pub root: PathBuf,
    pub config: ProjectConfig,

    // Tab state
    pub current_tab: Tab,
    pub should_quit: bool,

    // Requests tab
    pub request_names: Vec<String>,
    pub req_list_idx: usize,
    pub loaded_request: Option<RequestDef>,
    pub response: Option<ResponseView>,
    pub is_running: bool,

    // Logs tab
    pub log_entries: Vec<Run>,
    pub log_list_idx: usize,
    pub log_detail: Option<RunWithPayload>,

    // Detail pane (right panel) scroll & selection
    pub detail_focused: bool,
    pub detail_scroll: u16,
    pub select_mode: bool,
    pub select_anchor: (usize, usize), // (line, col)
    pub select_cursor: (usize, usize), // (line, col)
    pub detail_line_count: Cell<usize>,
    pub detail_viewport_height: Cell<usize>,
    pub status_message: Option<(String, Instant)>,

    // Mouse selection state
    pub mouse_selecting: bool,
    /// Inner rect of the detail panel: (x, y, width, height). Set during draw.
    pub detail_inner_rect: Cell<(u16, u16, u16, u16)>,
    /// Cumulative wrapped-line offsets per logical line. Set during draw.
    pub detail_row_offsets: RefCell<Vec<u16>>,

    // Env
    pub active_env: Option<String>,
    pub env_popup: Option<EnvSelector>,

    // Request form editor
    pub request_form: Option<RequestForm>,

    // Async
    tx: mpsc::UnboundedSender<TaskResult>,
    rx: mpsc::UnboundedReceiver<TaskResult>,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("failed to get current directory")?;
        let root = loader::find_project_root(&cwd)
            .context("not inside a Senka project (no senka.yml found)")?;
        let config = loader::load_config(&root).context("failed to load senka.yml")?;

        let request_names = loader::list_requests(&root).unwrap_or_default();
        let envs = loader::list_envs(&root).unwrap_or_default();

        let active_env = config
            .defaults
            .env
            .clone()
            .or_else(|| envs.first().cloned());

        // Load initial logs
        let log_entries = Self::load_logs(&root, 50);

        // Load initial request detail
        let loaded_request = request_names
            .first()
            .and_then(|name| loader::load_request(&root, name).ok());

        let (tx, rx) = mpsc::unbounded_channel();

        Ok(App {
            root,
            config,
            current_tab: Tab::Requests,
            should_quit: false,
            request_names,
            req_list_idx: 0,
            loaded_request,
            response: None,
            is_running: false,
            log_entries,
            log_list_idx: 0,
            log_detail: None,
            active_env,
            env_popup: None,
            request_form: None,
            detail_focused: false,
            detail_scroll: 0,
            select_mode: false,
            select_anchor: (0, 0),
            select_cursor: (0, 0),
            detail_line_count: Cell::new(0),
            detail_viewport_height: Cell::new(20),
            status_message: None,
            mouse_selecting: false,
            detail_inner_rect: Cell::new((0, 0, 0, 0)),
            detail_row_offsets: RefCell::new(Vec::new()),
            tx,
            rx,
        })
    }

    fn load_logs(root: &std::path::Path, n: u32) -> Vec<Run> {
        let db_path = root.join(".senka").join("logs.db");
        match db::open(&db_path) {
            Ok(conn) => db::tail(&conn, n).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    pub fn handle_event(&mut self, ev: Event) -> bool {
        if let Event::Mouse(mouse) = ev {
            return self.handle_mouse(mouse);
        }

        if let Event::Key(key) = ev {
            // Only process key-press events; ignore key-release and repeat
            // (on Windows, crossterm reports both Press and Release)
            if key.kind != crossterm::event::KeyEventKind::Press {
                return false;
            }

            // Handle env popup first if open
            if self.env_popup.is_some() {
                return self.handle_env_popup_key(key.code);
            }

            // Handle request form if open
            if self.request_form.is_some() {
                return self.handle_form_key(key.code, key.modifiers);
            }

            // Handle detail pane when focused
            if self.detail_focused {
                if self.select_mode {
                    return self.handle_select_mode_key(key.code, key.modifiers);
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.detail_scroll = self.detail_scroll.saturating_sub(1);
                        return true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.detail_scroll = self.detail_scroll.saturating_add(1);
                        return true;
                    }
                    KeyCode::PageUp => {
                        self.detail_scroll = self.detail_scroll.saturating_sub(20);
                        return true;
                    }
                    KeyCode::PageDown => {
                        self.detail_scroll = self.detail_scroll.saturating_add(20);
                        return true;
                    }
                    KeyCode::Home => {
                        self.detail_scroll = 0;
                        return true;
                    }
                    KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => {
                        self.detail_focused = false;
                        self.select_mode = false;
                        return true;
                    }
                    KeyCode::Char('v') if self.config.tui.keyboard_select => {
                        self.enter_select_mode();
                        return true;
                    }
                    KeyCode::Char('y') if self.config.tui.keyboard_select => {
                        self.copy_all_detail();
                        return true;
                    }
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                        return true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.should_quit = true;
                        return true;
                    }
                    _ => { return false; }
                }
            }

            match key.code {
                KeyCode::Char('q') => {
                    self.should_quit = true;
                    return true;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                    return true;
                }
                KeyCode::Tab => {
                    self.current_tab = match self.current_tab {
                        Tab::Requests => Tab::Logs,
                        Tab::Logs => Tab::Requests,
                    };
                    self.detail_focused = false;
                    self.detail_scroll = 0;
                    return true;
                }
                KeyCode::Char('n') if self.current_tab == Tab::Requests => {
                    self.open_new_request_form();
                    return true;
                }
                KeyCode::Char('e') => {
                    self.open_env_popup();
                    return true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.navigate_up();
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.navigate_down();
                    return true;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if self.has_detail_content() {
                        self.detail_focused = true;
                        self.detail_scroll = 0;
                        return true;
                    }
                }
                KeyCode::Enter => {
                    self.handle_enter();
                    return true;
                }
                KeyCode::Esc => {
                    match self.current_tab {
                        Tab::Requests => {
                            self.response = None;
                            self.detail_scroll = 0;
                        }
                        Tab::Logs => {
                            self.log_detail = None;
                            self.detail_scroll = 0;
                        }
                    }
                    return true;
                }
                KeyCode::Char('d') if self.current_tab == Tab::Logs => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        self.clear_logs();
                    } else {
                        self.delete_selected_log();
                    }
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub fn tick(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            match result {
                TaskResult::RequestDone(view) => {
                    self.response = Some(view);
                    self.is_running = false;
                    // Refresh logs after request execution
                    self.log_entries = Self::load_logs(&self.root, 50);
                }
            }
        }
        // Clear status message after 2 seconds
        if let Some((_, ts)) = &self.status_message {
            if ts.elapsed().as_secs() >= 2 {
                self.status_message = None;
            }
        }
    }

    fn navigate_up(&mut self) {
        match self.current_tab {
            Tab::Requests => {
                if self.req_list_idx > 0 {
                    self.req_list_idx -= 1;
                    self.reload_request_detail();
                    self.detail_scroll = 0;
                    self.detail_focused = false;
                }
            }
            Tab::Logs => {
                if self.log_list_idx > 0 {
                    self.log_list_idx -= 1;
                    self.detail_scroll = 0;
                    self.detail_focused = false;
                }
            }
        }
    }

    fn navigate_down(&mut self) {
        match self.current_tab {
            Tab::Requests => {
                if !self.request_names.is_empty()
                    && self.req_list_idx < self.request_names.len() - 1
                {
                    self.req_list_idx += 1;
                    self.reload_request_detail();
                    self.detail_scroll = 0;
                    self.detail_focused = false;
                }
            }
            Tab::Logs => {
                if !self.log_entries.is_empty()
                    && self.log_list_idx < self.log_entries.len() - 1
                {
                    self.log_list_idx += 1;
                    self.detail_scroll = 0;
                    self.detail_focused = false;
                }
            }
        }
    }

    fn has_detail_content(&self) -> bool {
        match self.current_tab {
            Tab::Requests => self.response.is_some(),
            Tab::Logs => self.log_detail.is_some(),
        }
    }

    // -----------------------------------------------------------------------
    // Text selection & copy
    // -----------------------------------------------------------------------

    fn handle_select_mode_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let line_count = self.detail_line_count.get();
        let lines = self.current_detail_lines();
        // Keyboard selection moves by whole lines: col = line length for cursor
        let line_len = |l: usize| -> usize { lines.get(l).map_or(0, |s| s.len()) };
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                let new_line = self.select_cursor.0.saturating_sub(1);
                self.select_cursor = (new_line, line_len(new_line));
                self.scroll_to_cursor();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let new_line = (self.select_cursor.0 + 1).min(line_count.saturating_sub(1));
                self.select_cursor = (new_line, line_len(new_line));
                self.scroll_to_cursor();
            }
            KeyCode::PageUp => {
                let page = self.detail_viewport_height.get().max(1);
                let new_line = self.select_cursor.0.saturating_sub(page);
                self.select_cursor = (new_line, line_len(new_line));
                self.scroll_to_cursor();
            }
            KeyCode::PageDown => {
                let page = self.detail_viewport_height.get().max(1);
                let new_line = (self.select_cursor.0 + page).min(line_count.saturating_sub(1));
                self.select_cursor = (new_line, line_len(new_line));
                self.scroll_to_cursor();
            }
            KeyCode::Home => {
                self.select_cursor = (0, 0);
                self.scroll_to_cursor();
            }
            KeyCode::End => {
                let last = line_count.saturating_sub(1);
                self.select_cursor = (last, line_len(last));
                self.scroll_to_cursor();
            }
            KeyCode::Char('y') | KeyCode::Enter => {
                self.copy_selection();
            }
            KeyCode::Esc | KeyCode::Char('v') => {
                self.select_mode = false;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            _ => return false,
        }
        true
    }

    fn enter_select_mode(&mut self) {
        let lines = self.current_detail_lines();
        if lines.is_empty() {
            return;
        }
        self.detail_line_count.set(lines.len());
        self.select_mode = true;
        let start_line = (self.detail_scroll as usize).min(lines.len().saturating_sub(1));
        self.select_anchor = (start_line, 0);
        self.select_cursor = (start_line, 0);
    }

    fn scroll_to_cursor(&mut self) {
        let cursor_line = self.select_cursor.0 as u16;
        if cursor_line < self.detail_scroll {
            self.detail_scroll = cursor_line;
        }
        let viewport = self.detail_viewport_height.get() as u16;
        if viewport > 0 && cursor_line >= self.detail_scroll + viewport {
            self.detail_scroll = cursor_line - viewport + 1;
        }
    }

    // -----------------------------------------------------------------------
    // Mouse handling
    // -----------------------------------------------------------------------

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(pos) = self.mouse_to_position(mouse.column, mouse.row) {
                    self.detail_focused = true;
                    self.select_mode = true;
                    self.mouse_selecting = true;
                    self.select_anchor = pos;
                    self.select_cursor = pos;
                    return true;
                }
                // Click outside detail panel — cancel any mouse selection
                self.mouse_selecting = false;
                self.select_mode = false;
                false
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if !self.mouse_selecting {
                    return false;
                }
                let (ix, iy, _iw, ih) = self.detail_inner_rect.get();
                // Clamp mouse position to detail panel bounds
                let clamped_col = mouse.column.max(ix);
                let clamped_row = mouse.row.max(iy).min(iy + ih.saturating_sub(1));
                if let Some(pos) = self.mouse_to_position(clamped_col, clamped_row) {
                    let line_count = self.detail_line_count.get();
                    self.select_cursor = (pos.0.min(line_count.saturating_sub(1)), pos.1);
                    self.scroll_to_cursor();
                    return true;
                }
                // Mouse outside panel bounds vertically — auto-scroll
                if mouse.row < iy {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    if let Some(pos) = self.mouse_to_position(clamped_col, iy) {
                        self.select_cursor = pos;
                    }
                } else if mouse.row >= iy + ih {
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                    if let Some(pos) = self.mouse_to_position(clamped_col, iy + ih.saturating_sub(1)) {
                        let line_count = self.detail_line_count.get();
                        self.select_cursor = (pos.0.min(line_count.saturating_sub(1)), pos.1);
                    }
                }
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if !self.mouse_selecting {
                    return false;
                }
                self.mouse_selecting = false;
                if self.select_anchor != self.select_cursor {
                    self.copy_selection();
                } else {
                    self.select_mode = false;
                }
                true
            }
            MouseEventKind::ScrollUp => {
                if self.is_mouse_over_detail(mouse.column, mouse.row) && self.has_detail_content() {
                    self.detail_scroll = self.detail_scroll.saturating_sub(3);
                    return true;
                }
                false
            }
            MouseEventKind::ScrollDown => {
                if self.is_mouse_over_detail(mouse.column, mouse.row) && self.has_detail_content() {
                    self.detail_scroll = self.detail_scroll.saturating_add(3);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Convert terminal (col, row) to a logical (line, char_offset), or None if outside the detail panel.
    fn mouse_to_position(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let (ix, iy, iw, ih) = self.detail_inner_rect.get();
        if iw == 0 || ih == 0 {
            return None;
        }
        if row < iy || row >= iy + ih {
            return None;
        }
        let display_row = row - iy;
        let wrapped_line = self.detail_scroll as usize + display_row as usize;

        let offsets = self.detail_row_offsets.borrow();
        if offsets.is_empty() {
            return None;
        }
        // Binary search: find last offset <= wrapped_line
        let idx = offsets.partition_point(|&off| (off as usize) <= wrapped_line);
        let logical_line = idx.saturating_sub(1);

        // Compute character offset within the logical line
        let inner_col = col.saturating_sub(ix) as usize;
        let line_start_row = offsets[logical_line] as usize;
        let wrapped_row_of_line = wrapped_line.saturating_sub(line_start_row);
        let char_offset = wrapped_row_of_line * (iw as usize) + inner_col;

        // Clamp to actual line length
        let lines = self.current_detail_lines();
        let max_col = lines.get(logical_line).map_or(0, |l| l.len());
        Some((logical_line, char_offset.min(max_col)))
    }

    /// Check if the mouse position is within the detail panel area.
    fn is_mouse_over_detail(&self, col: u16, row: u16) -> bool {
        let (ix, iy, iw, ih) = self.detail_inner_rect.get();
        if iw == 0 || ih == 0 {
            return false;
        }
        col >= ix && col < ix + iw && row >= iy && row < iy + ih
    }

    fn copy_selection(&mut self) {
        let lines = self.current_detail_lines();
        if lines.is_empty() {
            self.select_mode = false;
            return;
        }

        // Normalize so start <= end
        let (start, end) = if self.select_anchor <= self.select_cursor {
            (self.select_anchor, self.select_cursor)
        } else {
            (self.select_cursor, self.select_anchor)
        };

        let (start_line, start_col) = start;
        let (end_line, end_col) = end;
        let last_line = lines.len().saturating_sub(1);

        let text = if start_line == end_line {
            // Single line selection
            let line = &lines[start_line.min(last_line)];
            let sc = start_col.min(line.len());
            let ec = end_col.min(line.len());
            line[sc..ec].to_string()
        } else {
            // Multi-line selection
            let mut parts = Vec::new();
            // First line: from start_col to end
            let first = &lines[start_line.min(last_line)];
            let sc = start_col.min(first.len());
            parts.push(&first[sc..]);
            // Middle lines: full
            for line in lines.iter().take(end_line.min(lines.len())).skip(start_line + 1) {
                parts.push(line.as_str());
            }
            // Last line: from start to end_col
            if end_line <= last_line {
                let last = &lines[end_line];
                let ec = end_col.min(last.len());
                parts.push(&last[..ec]);
            }
            parts.join("\n")
        };

        self.copy_to_clipboard(&text);
        self.select_mode = false;
    }

    fn copy_all_detail(&mut self) {
        let lines = self.current_detail_lines();
        if lines.is_empty() {
            return;
        }
        let text = lines.join("\n");
        self.copy_to_clipboard(&text);
    }

    fn copy_to_clipboard(&mut self, text: &str) {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let result = if cfg!(target_os = "windows") {
            Command::new("clip")
                .stdin(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                })
        } else if cfg!(target_os = "macos") {
            Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                })
        } else {
            // Linux: try xclip, fall back to xsel
            Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
                .or_else(|_| {
                    Command::new("xsel")
                        .args(["--clipboard", "--input"])
                        .stdin(Stdio::piped())
                        .spawn()
                })
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                })
        };

        match result {
            Ok(status) if status.success() => {
                self.status_message = Some(("Copied!".to_string(), Instant::now()));
            }
            _ => {
                self.status_message = Some(("Copy failed".to_string(), Instant::now()));
            }
        }
    }

    fn current_detail_lines(&self) -> Vec<String> {
        match self.current_tab {
            Tab::Requests => {
                if let Some(ref resp) = self.response {
                    Self::build_response_lines(resp)
                } else {
                    Vec::new()
                }
            }
            Tab::Logs => {
                if let Some(ref detail) = self.log_detail {
                    Self::build_log_detail_lines(detail)
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn build_response_lines(resp: &ResponseView) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(ref err) = resp.error {
            lines.push(format!("Error: {err}"));
        } else {
            let status_str = match resp.status {
                Some(s) => format!("{s} {}", resp.status_text),
                None => "ERR".to_string(),
            };
            lines.push(status_str);
            lines.push(format!("Duration: {} ms", resp.duration_ms));
            lines.push(String::new());

            if !resp.headers_text.is_empty() {
                lines.push("--- Headers ---".to_string());
                for line in resp.headers_text.lines() {
                    lines.push(line.to_string());
                }
                lines.push(String::new());
            }

            if !resp.body_text.is_empty() {
                lines.push("--- Body ---".to_string());
                for line in resp.body_text.lines() {
                    lines.push(line.to_string());
                }
            }
        }
        lines
    }

    fn build_log_detail_lines(detail: &RunWithPayload) -> Vec<String> {
        let status_str = match detail.run.status {
            Some(s) => s.to_string(),
            None => "ERR".to_string(),
        };

        let mut lines = vec![
            format!("ID:       {}", detail.run.id),
            format!("Time:     {}", format_ts(detail.run.ts)),
            format!("Request:  {}", detail.run.request_name),
            format!("Method:   {}", detail.run.method),
            format!("URL:      {}", detail.run.url),
            format!("Status:   {status_str}"),
            format!("Duration: {} ms", detail.run.duration_ms),
            format!("Env:      {}", detail.run.env),
        ];

        if let Some(ref err) = detail.run.error {
            lines.push(format!("Error:    {err}"));
        }

        lines.push(String::new());
        lines.push("--- Request Headers ---".to_string());
        for line in detail.request_headers.lines() {
            lines.push(line.to_string());
        }

        if let Some(ref body) = detail.request_body {
            lines.push(String::new());
            lines.push("--- Request Body ---".to_string());
            for line in body.lines() {
                lines.push(line.to_string());
            }
        }

        lines.push(String::new());
        lines.push("--- Response Headers ---".to_string());
        for line in detail.response_headers.lines() {
            lines.push(line.to_string());
        }

        if let Some(ref body) = detail.response_body {
            lines.push(String::new());
            lines.push("--- Response Body ---".to_string());
            for line in body.lines() {
                lines.push(line.to_string());
            }
        }

        lines
    }

    fn handle_enter(&mut self) {
        match self.current_tab {
            Tab::Requests => {
                if !self.is_running {
                    self.execute_selected_request();
                    self.detail_scroll = 0;
                }
            }
            Tab::Logs => {
                self.reload_log_detail();
                self.detail_scroll = 0;
            }
        }
    }

    fn reload_request_detail(&mut self) {
        if let Some(name) = self.request_names.get(self.req_list_idx) {
            self.loaded_request = loader::load_request(&self.root, name).ok();
            self.response = None;
        }
    }

    fn reload_log_detail(&mut self) {
        if let Some(run) = self.log_entries.get(self.log_list_idx) {
            let db_path = self.root.join(".senka").join("logs.db");
            if let Ok(conn) = db::open(&db_path) {
                self.log_detail = db::show(&conn, &run.id).ok().flatten();
            }
        }
    }

    fn clear_logs(&mut self) {
        let db_path = self.root.join(".senka").join("logs.db");
        if let Ok(conn) = db::open(&db_path) {
            let _ = db::clear(&conn);
        }
        self.log_entries.clear();
        self.log_list_idx = 0;
        self.log_detail = None;
        self.detail_scroll = 0;
        self.detail_focused = false;
    }

    fn delete_selected_log(&mut self) {
        let id = match self.log_entries.get(self.log_list_idx) {
            Some(run) => run.id.clone(),
            None => return,
        };
        let db_path = self.root.join(".senka").join("logs.db");
        if let Ok(conn) = db::open(&db_path) {
            let _ = db::delete_by_id(&conn, &id);
        }
        self.log_entries.remove(self.log_list_idx);
        if self.log_list_idx >= self.log_entries.len() && self.log_list_idx > 0 {
            self.log_list_idx -= 1;
        }
        self.log_detail = None;
        self.detail_scroll = 0;
        self.detail_focused = false;
    }

    fn execute_selected_request(&mut self) {
        let name = match self.request_names.get(self.req_list_idx) {
            Some(n) => n.clone(),
            None => return,
        };

        let mut req = match loader::load_request(&self.root, &name) {
            Ok(r) => r,
            Err(e) => {
                self.response = Some(ResponseView {
                    status: None,
                    status_text: String::new(),
                    duration_ms: 0,
                    headers_text: String::new(),
                    body_text: String::new(),
                    error: Some(format!("failed to load request: {e}")),
                });
                return;
            }
        };

        let env_name = self.active_env.clone();
        let env = env_name
            .as_deref()
            .and_then(|name| loader::load_env(&self.root, name).ok());

        let mut vars = resolve::merge_vars(env.as_ref(), &[]);

        // Resolve secrets
        let mut secret_values = Vec::new();
        let needed_vars = resolve::collect_template_vars(&req);
        if let Some(ref env_name) = env_name {
            for var_name in &needed_vars {
                if vars.contains_key(var_name.as_str()) {
                    continue;
                }
                match senka_secrets::get(&self.config.name, env_name, var_name) {
                    Ok(Some(val)) => {
                        secret_values.push(val.clone());
                        vars.insert(var_name.clone(), val);
                    }
                    Ok(None) => {}
                    Err(_) => {}
                }
            }
        }

        // Render templates
        if let Err(e) = resolve::render_request(&mut req, &vars) {
            self.response = Some(ResponseView {
                status: None,
                status_text: String::new(),
                duration_ms: 0,
                headers_text: String::new(),
                body_text: String::new(),
                error: Some(format!("failed to resolve variables: {e}")),
            });
            return;
        }

        let config = self.config.clone();
        let root = self.root.clone();
        let env_name_for_log = env_name.clone().unwrap_or_else(|| "default".to_string());
        let tx = self.tx.clone();

        self.is_running = true;
        self.response = None;

        tokio::spawn(async move {
            let client_opts = ClientOptions::default();
            let client = match execute::build_client(&config, &client_opts) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(TaskResult::RequestDone(ResponseView {
                        status: None,
                        status_text: String::new(),
                        duration_ms: 0,
                        headers_text: String::new(),
                        body_text: String::new(),
                        error: Some(format!("failed to build HTTP client: {e}")),
                    }));
                    return;
                }
            };

            let exec_result = execute::execute(&client, &req, config.logging.max_body_kb).await;

            // Log the result
            if config.logging.enabled {
                insert_log_entry(
                    &root,
                    &config,
                    &req,
                    &env_name_for_log,
                    &secret_values,
                    &exec_result,
                );
            }

            let view = match exec_result {
                Ok(resp) => {
                    // Format headers
                    let mut headers_lines = Vec::new();
                    let mut keys: Vec<&String> = resp.headers.keys().collect();
                    keys.sort();
                    for k in keys {
                        let v = redact::redact_header_value(k, &resp.headers[k], &config.redaction);
                        let v = redact::redact_secret_values(&v, &secret_values);
                        headers_lines.push(format!("{k}: {v}"));
                    }

                    // Format body
                    let body_str = String::from_utf8_lossy(&resp.body);
                    let body_str = redact::redact_secret_values(&body_str, &secret_values);
                    let body_text = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_str) {
                        let mut val = json_val;
                        redact::redact_json_fields(&mut val, &config.redaction);
                        serde_json::to_string_pretty(&val).unwrap_or(body_str)
                    } else {
                        body_str
                    };

                    ResponseView {
                        status: Some(resp.status),
                        status_text: resp.status_text,
                        duration_ms: resp.duration_ms,
                        headers_text: headers_lines.join("\n"),
                        body_text,
                        error: None,
                    }
                }
                Err(e) => ResponseView {
                    status: None,
                    status_text: String::new(),
                    duration_ms: 0,
                    headers_text: String::new(),
                    body_text: String::new(),
                    error: Some(e.to_string()),
                },
            };

            let _ = tx.send(TaskResult::RequestDone(view));
        });
    }

    // -----------------------------------------------------------------------
    // Request form
    // -----------------------------------------------------------------------

    fn open_new_request_form(&mut self) {
        self.request_form = Some(RequestForm::new_blank());
    }

    fn handle_form_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let form = self.request_form.as_mut().unwrap();

        // Clear previous error on any key press
        form.error_message = None;

        // Ctrl+S: save from anywhere
        if code == KeyCode::Char('s') && modifiers.contains(KeyModifiers::CONTROL) {
            return self.save_request_form();
        }

        if form.editing {
            // === EDITING MODE ===
            match code {
                KeyCode::Esc | KeyCode::Enter => {
                    form.editing = false;
                }
                KeyCode::Char(ch) => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.insert_char(ch);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.delete_back();
                    }
                }
                KeyCode::Delete => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.delete_forward();
                    }
                }
                KeyCode::Left => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.move_left();
                    }
                }
                KeyCode::Right => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.move_right();
                    }
                }
                KeyCode::Home => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.move_home();
                    }
                }
                KeyCode::End => {
                    if let Some(input) = form.focused_text_input_mut() {
                        input.move_end();
                    }
                }
                _ => {}
            }
        } else {
            // === NAVIGATION MODE ===
            match code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.request_form = None;
                    return true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    form.focus_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    form.focus_down();
                }
                KeyCode::Enter => {
                    if matches!(form.rows.get(form.focused_row), Some(FormRow::Save)) {
                        return self.save_request_form();
                    } else if form.focused_is_action() {
                        form.activate_action();
                    } else if form.focused_is_selector() {
                        // Selectors use Left/Right, Enter does nothing
                    } else if form.focused_text_input().is_some() {
                        form.editing = true;
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if form.focused_is_selector() {
                        form.cycle_left();
                        form.rebuild_rows();
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if form.focused_is_selector() {
                        form.cycle_right();
                        form.rebuild_rows();
                    }
                }
                KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some((kind, idx)) = form.focused_is_deletable_pair() {
                        form.delete_pair(kind, idx);
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn save_request_form(&mut self) -> bool {
        let form = self.request_form.as_mut().unwrap();

        if let Err(msg) = form.validate(&self.request_names) {
            form.error_message = Some(msg);
            return true;
        }

        let req = form.to_request_def();
        let req_dir = self.root.join("senka-requests");

        if let Err(e) = std::fs::create_dir_all(&req_dir) {
            form.error_message = Some(format!("failed to create directory: {e}"));
            return true;
        }

        let file_path = req_dir.join(format!("{}.yml", req.name));

        if file_path.exists() {
            form.error_message = Some(format!("file already exists: {}", file_path.display()));
            return true;
        }

        let yaml = match serde_yaml::to_string(&req) {
            Ok(y) => y,
            Err(e) => {
                form.error_message = Some(format!("serialization error: {e}"));
                return true;
            }
        };

        if let Err(e) = std::fs::write(&file_path, &yaml) {
            form.error_message = Some(format!("failed to write file: {e}"));
            return true;
        }

        // Refresh request list and select the new entry
        self.request_names = loader::list_requests(&self.root).unwrap_or_default();
        if let Some(pos) = self.request_names.iter().position(|n| n == &req.name) {
            self.req_list_idx = pos;
        }
        self.reload_request_detail();
        self.request_form = None;
        true
    }

    fn open_env_popup(&mut self) {
        let envs = loader::list_envs(&self.root).unwrap_or_default();
        if envs.is_empty() {
            return;
        }
        let selected = self
            .active_env
            .as_ref()
            .and_then(|active| envs.iter().position(|e| e == active))
            .unwrap_or(0);
        self.env_popup = Some(EnvSelector { envs, selected });
    }

    fn handle_env_popup_key(&mut self, code: KeyCode) -> bool {
        let popup = match self.env_popup.as_mut() {
            Some(p) => p,
            None => return false,
        };

        match code {
            KeyCode::Esc => {
                self.env_popup = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if popup.selected > 0 {
                    popup.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if popup.selected < popup.envs.len().saturating_sub(1) {
                    popup.selected += 1;
                }
            }
            KeyCode::Enter => {
                let name = popup.envs[popup.selected].clone();
                self.active_env = Some(name);
                self.env_popup = None;
            }
            _ => {}
        }
        true
    }
}

/// Insert a log entry after request execution. Failures are silently ignored in TUI.
fn insert_log_entry(
    root: &std::path::Path,
    config: &ProjectConfig,
    req: &RequestDef,
    env_name: &str,
    secret_values: &[String],
    exec_result: &Result<senka_runner::response::CapturedResponse, RunError>,
) {
    let db_path = root.join(".senka").join("logs.db");
    let conn = match db::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let id = ulid::Ulid::new().to_string();
    let ts = now_epoch_ms();

    let (status, duration_ms, error, resp_headers, resp_body) = match exec_result {
        Ok(resp) => {
            let headers_redacted = redact_headers_for_storage(&resp.headers, config, secret_values);
            let body_str = String::from_utf8_lossy(&resp.body);
            let body_redacted = redact_body_for_storage(&body_str, config, secret_values);
            let body_truncated = truncate_body(&body_redacted, config.logging.max_body_kb);
            (
                Some(resp.status),
                resp.duration_ms,
                None,
                serde_json::to_string(&headers_redacted).unwrap_or_default(),
                Some(body_truncated),
            )
        }
        Err(e) => (None, 0, Some(e.to_string()), String::new(), None),
    };

    let req_headers_redacted = redact_headers_for_storage(&req.headers, config, secret_values);
    let req_body_str = build_request_body_string(&req.body);
    let req_body_redacted = req_body_str
        .as_deref()
        .map(|b| redact_body_for_storage(b, config, secret_values));
    let req_body_truncated =
        req_body_redacted.map(|b| truncate_body(&b, config.logging.max_body_kb));

    let run = Run {
        id: id.clone(),
        ts,
        project: config.name.clone(),
        env: env_name.to_string(),
        request_name: req.name.clone(),
        method: req.method.clone(),
        url: req.url.clone(),
        status,
        duration_ms,
        error,
    };

    let payload = Payload {
        run_id: id,
        request_headers: serde_json::to_string(&req_headers_redacted).unwrap_or_default(),
        request_body: req_body_truncated,
        response_headers: resp_headers,
        response_body: resp_body,
    };

    let _ = db::insert_run(&conn, &run, &payload);
}

fn redact_headers_for_storage(
    headers: &HashMap<String, String>,
    config: &ProjectConfig,
    secret_values: &[String],
) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            let val = redact::redact_header_value(k, v, &config.redaction);
            let val = redact::redact_secret_values(&val, secret_values);
            (k.clone(), val)
        })
        .collect()
}

fn redact_body_for_storage(
    body: &str,
    config: &ProjectConfig,
    secret_values: &[String],
) -> String {
    let mut result = redact::redact_secret_values(body, secret_values);
    if !config.redaction.json_fields.is_empty() {
        if let Ok(mut json_val) = serde_json::from_str::<serde_json::Value>(&result) {
            redact::redact_json_fields(&mut json_val, &config.redaction);
            if let Ok(s) = serde_json::to_string(&json_val) {
                result = s;
            }
        }
    }
    result
}

fn build_request_body_string(body: &Option<Body>) -> Option<String> {
    match body {
        Some(Body::Raw(s)) => Some(s.clone()),
        Some(Body::Json(v)) => serde_json::to_string(v).ok(),
        Some(Body::Form(m)) => serde_json::to_string(m).ok(),
        None => None,
    }
}

fn truncate_body(body: &str, max_body_kb: usize) -> String {
    let max_bytes = max_body_kb * 1024;
    if body.len() <= max_bytes {
        body.to_string()
    } else {
        let truncated = &body[..max_bytes];
        format!("{truncated}... (truncated)")
    }
}
