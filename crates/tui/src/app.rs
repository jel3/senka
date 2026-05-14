use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use tokio::sync::mpsc;

use senka_core::config::ProjectConfig;
use senka_core::loader;
use senka_core::redact;
use senka_core::request::{Body, RequestDef};
use senka_core::resolve;
use senka_core::util::now_epoch_ms;
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

    // Detail pane (right panel) scroll
    pub detail_focused: bool,
    pub detail_scroll: u16,

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

            // Handle detail pane scroll when focused
            if self.detail_focused {
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
