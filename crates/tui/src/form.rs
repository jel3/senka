use std::collections::HashMap;

use senka_core::request::{AuthConfig, Body, RequestDef};

// ---------------------------------------------------------------------------
// Selector enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

impl HttpMethod {
    pub const ALL: &'static [HttpMethod] = &[
        Self::GET,
        Self::POST,
        Self::PUT,
        Self::PATCH,
        Self::DELETE,
        Self::HEAD,
        Self::OPTIONS,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::POST => "POST",
            Self::PUT => "PUT",
            Self::PATCH => "PATCH",
            Self::DELETE => "DELETE",
            Self::HEAD => "HEAD",
            Self::OPTIONS => "OPTIONS",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    None,
    Bearer,
    Basic,
}

impl AuthType {
    pub const ALL: &'static [AuthType] = &[Self::None, Self::Bearer, Self::Basic];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bearer => "Bearer",
            Self::Basic => "Basic",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&a| a == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&a| a == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    None,
    Raw,
    Json,
    Form,
}

impl BodyType {
    pub const ALL: &'static [BodyType] = &[Self::None, Self::Raw, Self::Json, Self::Form];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Raw => "Raw",
            Self::Json => "JSON",
            Self::Form => "Form",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&b| b == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&b| b == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

// ---------------------------------------------------------------------------
// Text input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub value: String,
    pub cursor: usize,
}

impl TextInput {
    #[allow(dead_code)]
    pub fn new(initial: &str) -> Self {
        Self {
            cursor: initial.len(),
            value: initial.to_string(),
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.value.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn delete_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.value.drain(prev..self.cursor);
        self.cursor = prev;
    }

    pub fn delete_forward(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.value.len());
        self.value.drain(self.cursor..next);
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn move_right(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.cursor = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.value.len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
    }
}

// ---------------------------------------------------------------------------
// Key-value pair
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct KvPair {
    pub key: TextInput,
    pub value: TextInput,
}

// ---------------------------------------------------------------------------
// Form row model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormRow {
    Name,
    Method,
    Url,
    SectionLabel(&'static str),
    HeaderKey(usize),
    HeaderValue(usize),
    AddHeader,
    QueryKey(usize),
    QueryValue(usize),
    AddQuery,
    AuthType,
    AuthBearerToken,
    AuthBasicUsername,
    AuthBasicPassword,
    BodyType,
    BodyRawContent,
    BodyJsonContent,
    BodyFormKey(usize),
    BodyFormValue(usize),
    AddBodyFormPair,
    Spacer,
    Save,
}

#[derive(Debug, Clone, Copy)]
pub enum PairKind {
    Header,
    Query,
    BodyForm,
}

// ---------------------------------------------------------------------------
// Request form
// ---------------------------------------------------------------------------

pub struct RequestForm {
    // Data
    pub name: TextInput,
    pub method: HttpMethod,
    pub url: TextInput,
    pub headers: Vec<KvPair>,
    pub query: Vec<KvPair>,
    pub auth_type: AuthType,
    pub auth_bearer_token: TextInput,
    pub auth_basic_username: TextInput,
    pub auth_basic_password: TextInput,
    pub body_type: BodyType,
    pub body_raw: TextInput,
    pub body_json: TextInput,
    pub body_form: Vec<KvPair>,

    // Navigation
    pub rows: Vec<FormRow>,
    pub focused_row: usize,
    pub editing: bool,
    #[allow(dead_code)]
    pub scroll_offset: usize,

    // Validation feedback
    pub error_message: Option<String>,
}

impl RequestForm {
    pub fn new_blank() -> Self {
        let mut form = Self {
            name: TextInput::default(),
            method: HttpMethod::GET,
            url: TextInput::default(),
            headers: Vec::new(),
            query: Vec::new(),
            auth_type: AuthType::None,
            auth_bearer_token: TextInput::default(),
            auth_basic_username: TextInput::default(),
            auth_basic_password: TextInput::default(),
            body_type: BodyType::None,
            body_raw: TextInput::default(),
            body_json: TextInput::default(),
            body_form: Vec::new(),
            rows: Vec::new(),
            focused_row: 0,
            editing: false,
            scroll_offset: 0,
            error_message: None,
        };
        form.rebuild_rows();
        form
    }

    /// Pre-populate from an existing request (for future edit support).
    #[allow(dead_code)]
    pub fn from_request(req: &RequestDef) -> Self {
        let method = HttpMethod::ALL
            .iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(&req.method))
            .copied()
            .unwrap_or(HttpMethod::GET);

        let headers: Vec<KvPair> = {
            let mut keys: Vec<&String> = req.headers.keys().collect();
            keys.sort();
            keys.into_iter()
                .map(|k| KvPair {
                    key: TextInput::new(k),
                    value: TextInput::new(&req.headers[k]),
                })
                .collect()
        };

        let query: Vec<KvPair> = {
            let mut keys: Vec<&String> = req.query.keys().collect();
            keys.sort();
            keys.into_iter()
                .map(|k| KvPair {
                    key: TextInput::new(k),
                    value: TextInput::new(&req.query[k]),
                })
                .collect()
        };

        let (auth_type, auth_bearer_token, auth_basic_username, auth_basic_password) =
            match &req.auth {
                None => (
                    AuthType::None,
                    TextInput::default(),
                    TextInput::default(),
                    TextInput::default(),
                ),
                Some(AuthConfig::Bearer { token }) => (
                    AuthType::Bearer,
                    TextInput::new(token),
                    TextInput::default(),
                    TextInput::default(),
                ),
                Some(AuthConfig::Basic { username, password }) => (
                    AuthType::Basic,
                    TextInput::default(),
                    TextInput::new(username),
                    TextInput::new(password),
                ),
            };

        let (body_type, body_raw, body_json, body_form) = match &req.body {
            None => (
                BodyType::None,
                TextInput::default(),
                TextInput::default(),
                Vec::new(),
            ),
            Some(Body::Raw(s)) => (
                BodyType::Raw,
                TextInput::new(s),
                TextInput::default(),
                Vec::new(),
            ),
            Some(Body::Json(v)) => (
                BodyType::Json,
                TextInput::default(),
                TextInput::new(&serde_json::to_string_pretty(v).unwrap_or_default()),
                Vec::new(),
            ),
            Some(Body::Form(m)) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let pairs = keys
                    .into_iter()
                    .map(|k| KvPair {
                        key: TextInput::new(k),
                        value: TextInput::new(&m[k]),
                    })
                    .collect();
                (BodyType::Form, TextInput::default(), TextInput::default(), pairs)
            }
        };

        let mut form = Self {
            name: TextInput::new(&req.name),
            method,
            url: TextInput::new(&req.url),
            headers,
            query,
            auth_type,
            auth_bearer_token,
            auth_basic_username,
            auth_basic_password,
            body_type,
            body_raw,
            body_json,
            body_form,
            rows: Vec::new(),
            focused_row: 0,
            editing: false,
            scroll_offset: 0,
            error_message: None,
        };
        form.rebuild_rows();
        form
    }

    // -----------------------------------------------------------------------
    // Row management
    // -----------------------------------------------------------------------

    pub fn rebuild_rows(&mut self) {
        let mut rows = vec![
            FormRow::Name,
            FormRow::Method,
            FormRow::Url,
            FormRow::Spacer,
            FormRow::SectionLabel("Headers"),
        ];
        for i in 0..self.headers.len() {
            rows.push(FormRow::HeaderKey(i));
            rows.push(FormRow::HeaderValue(i));
        }
        rows.push(FormRow::AddHeader);
        rows.push(FormRow::Spacer);

        rows.push(FormRow::SectionLabel("Query Params"));
        for i in 0..self.query.len() {
            rows.push(FormRow::QueryKey(i));
            rows.push(FormRow::QueryValue(i));
        }
        rows.push(FormRow::AddQuery);
        rows.push(FormRow::Spacer);

        rows.push(FormRow::AuthType);
        match self.auth_type {
            AuthType::None => {}
            AuthType::Bearer => {
                rows.push(FormRow::AuthBearerToken);
            }
            AuthType::Basic => {
                rows.push(FormRow::AuthBasicUsername);
                rows.push(FormRow::AuthBasicPassword);
            }
        }
        rows.push(FormRow::Spacer);

        rows.push(FormRow::BodyType);
        match self.body_type {
            BodyType::None => {}
            BodyType::Raw => {
                rows.push(FormRow::BodyRawContent);
            }
            BodyType::Json => {
                rows.push(FormRow::BodyJsonContent);
            }
            BodyType::Form => {
                for i in 0..self.body_form.len() {
                    rows.push(FormRow::BodyFormKey(i));
                    rows.push(FormRow::BodyFormValue(i));
                }
                rows.push(FormRow::AddBodyFormPair);
            }
        }
        rows.push(FormRow::Spacer);

        rows.push(FormRow::Save);

        // Clamp and fix focus
        if self.focused_row >= rows.len() {
            self.focused_row = rows.len().saturating_sub(1);
        }
        while self.focused_row < rows.len() && !Self::is_focusable(&rows[self.focused_row]) {
            self.focused_row += 1;
        }
        if self.focused_row >= rows.len() {
            self.focused_row = rows.len().saturating_sub(1);
            while self.focused_row > 0 && !Self::is_focusable(&rows[self.focused_row]) {
                self.focused_row -= 1;
            }
        }

        self.rows = rows;
    }

    fn is_focusable(row: &FormRow) -> bool {
        !matches!(row, FormRow::Spacer | FormRow::SectionLabel(_))
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    pub fn focus_up(&mut self) {
        if self.focused_row == 0 {
            return;
        }
        let mut idx = self.focused_row - 1;
        while idx > 0 && !Self::is_focusable(&self.rows[idx]) {
            idx -= 1;
        }
        if Self::is_focusable(&self.rows[idx]) {
            self.focused_row = idx;
        }
    }

    pub fn focus_down(&mut self) {
        if self.focused_row >= self.rows.len().saturating_sub(1) {
            return;
        }
        let mut idx = self.focused_row + 1;
        while idx < self.rows.len() - 1 && !Self::is_focusable(&self.rows[idx]) {
            idx += 1;
        }
        if idx < self.rows.len() && Self::is_focusable(&self.rows[idx]) {
            self.focused_row = idx;
        }
    }

    #[allow(dead_code)]
    pub fn ensure_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.focused_row < self.scroll_offset {
            self.scroll_offset = self.focused_row;
        } else if self.focused_row >= self.scroll_offset + visible_height {
            self.scroll_offset = self.focused_row - visible_height + 1;
        }
    }

    // -----------------------------------------------------------------------
    // Field access
    // -----------------------------------------------------------------------

    pub fn focused_text_input(&self) -> Option<&TextInput> {
        match self.rows.get(self.focused_row)? {
            FormRow::Name => Some(&self.name),
            FormRow::Url => Some(&self.url),
            FormRow::HeaderKey(i) => Some(&self.headers[*i].key),
            FormRow::HeaderValue(i) => Some(&self.headers[*i].value),
            FormRow::QueryKey(i) => Some(&self.query[*i].key),
            FormRow::QueryValue(i) => Some(&self.query[*i].value),
            FormRow::AuthBearerToken => Some(&self.auth_bearer_token),
            FormRow::AuthBasicUsername => Some(&self.auth_basic_username),
            FormRow::AuthBasicPassword => Some(&self.auth_basic_password),
            FormRow::BodyRawContent => Some(&self.body_raw),
            FormRow::BodyJsonContent => Some(&self.body_json),
            FormRow::BodyFormKey(i) => Some(&self.body_form[*i].key),
            FormRow::BodyFormValue(i) => Some(&self.body_form[*i].value),
            _ => None,
        }
    }

    pub fn focused_text_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.rows.get(self.focused_row)?.clone() {
            FormRow::Name => Some(&mut self.name),
            FormRow::Url => Some(&mut self.url),
            FormRow::HeaderKey(i) => Some(&mut self.headers[i].key),
            FormRow::HeaderValue(i) => Some(&mut self.headers[i].value),
            FormRow::QueryKey(i) => Some(&mut self.query[i].key),
            FormRow::QueryValue(i) => Some(&mut self.query[i].value),
            FormRow::AuthBearerToken => Some(&mut self.auth_bearer_token),
            FormRow::AuthBasicUsername => Some(&mut self.auth_basic_username),
            FormRow::AuthBasicPassword => Some(&mut self.auth_basic_password),
            FormRow::BodyRawContent => Some(&mut self.body_raw),
            FormRow::BodyJsonContent => Some(&mut self.body_json),
            FormRow::BodyFormKey(i) => Some(&mut self.body_form[i].key),
            FormRow::BodyFormValue(i) => Some(&mut self.body_form[i].value),
            _ => None,
        }
    }

    pub fn focused_is_selector(&self) -> bool {
        matches!(
            self.rows.get(self.focused_row),
            Some(FormRow::Method | FormRow::AuthType | FormRow::BodyType)
        )
    }

    pub fn focused_is_action(&self) -> bool {
        matches!(
            self.rows.get(self.focused_row),
            Some(
                FormRow::AddHeader
                    | FormRow::AddQuery
                    | FormRow::AddBodyFormPair
                    | FormRow::Save
            )
        )
    }

    pub fn focused_is_deletable_pair(&self) -> Option<(PairKind, usize)> {
        match self.rows.get(self.focused_row)? {
            FormRow::HeaderKey(i) | FormRow::HeaderValue(i) => Some((PairKind::Header, *i)),
            FormRow::QueryKey(i) | FormRow::QueryValue(i) => Some((PairKind::Query, *i)),
            FormRow::BodyFormKey(i) | FormRow::BodyFormValue(i) => Some((PairKind::BodyForm, *i)),
            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Selector cycling
    // -----------------------------------------------------------------------

    pub fn cycle_left(&mut self) {
        match self.rows.get(self.focused_row) {
            Some(FormRow::Method) => self.method = self.method.prev(),
            Some(FormRow::AuthType) => self.auth_type = self.auth_type.prev(),
            Some(FormRow::BodyType) => self.body_type = self.body_type.prev(),
            _ => {}
        }
    }

    pub fn cycle_right(&mut self) {
        match self.rows.get(self.focused_row) {
            Some(FormRow::Method) => self.method = self.method.next(),
            Some(FormRow::AuthType) => self.auth_type = self.auth_type.next(),
            Some(FormRow::BodyType) => self.body_type = self.body_type.next(),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    pub fn activate_action(&mut self) {
        match self.rows.get(self.focused_row) {
            Some(FormRow::AddHeader) => {
                self.headers.push(KvPair::default());
                self.rebuild_rows();
                // Focus the new key field
                if let Some(pos) = self
                    .rows
                    .iter()
                    .position(|r| *r == FormRow::HeaderKey(self.headers.len() - 1))
                {
                    self.focused_row = pos;
                }
            }
            Some(FormRow::AddQuery) => {
                self.query.push(KvPair::default());
                self.rebuild_rows();
                if let Some(pos) = self
                    .rows
                    .iter()
                    .position(|r| *r == FormRow::QueryKey(self.query.len() - 1))
                {
                    self.focused_row = pos;
                }
            }
            Some(FormRow::AddBodyFormPair) => {
                self.body_form.push(KvPair::default());
                self.rebuild_rows();
                if let Some(pos) = self
                    .rows
                    .iter()
                    .position(|r| *r == FormRow::BodyFormKey(self.body_form.len() - 1))
                {
                    self.focused_row = pos;
                }
            }
            _ => {}
        }
    }

    pub fn delete_pair(&mut self, kind: PairKind, idx: usize) {
        match kind {
            PairKind::Header => {
                if idx < self.headers.len() {
                    self.headers.remove(idx);
                }
            }
            PairKind::Query => {
                if idx < self.query.len() {
                    self.query.remove(idx);
                }
            }
            PairKind::BodyForm => {
                if idx < self.body_form.len() {
                    self.body_form.remove(idx);
                }
            }
        }
        self.rebuild_rows();
    }

    // -----------------------------------------------------------------------
    // Validation & conversion
    // -----------------------------------------------------------------------

    pub fn validate(&self, existing_names: &[String]) -> Result<(), String> {
        let name = self.name.value.trim();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        if name.contains(['/', '\\', '\0']) {
            return Err("Name contains invalid characters".to_string());
        }
        if self.url.value.trim().is_empty() {
            return Err("URL cannot be empty".to_string());
        }
        if existing_names.iter().any(|n| n == name) {
            return Err(format!("Request '{name}' already exists"));
        }
        Ok(())
    }

    pub fn to_request_def(&self) -> RequestDef {
        let headers: HashMap<String, String> = self
            .headers
            .iter()
            .filter(|kv| !kv.key.value.is_empty())
            .map(|kv| (kv.key.value.clone(), kv.value.value.clone()))
            .collect();

        let query: HashMap<String, String> = self
            .query
            .iter()
            .filter(|kv| !kv.key.value.is_empty())
            .map(|kv| (kv.key.value.clone(), kv.value.value.clone()))
            .collect();

        let auth = match self.auth_type {
            AuthType::None => None,
            AuthType::Bearer => Some(AuthConfig::Bearer {
                token: self.auth_bearer_token.value.clone(),
            }),
            AuthType::Basic => Some(AuthConfig::Basic {
                username: self.auth_basic_username.value.clone(),
                password: self.auth_basic_password.value.clone(),
            }),
        };

        let body = match self.body_type {
            BodyType::None => None,
            BodyType::Raw => {
                if self.body_raw.value.is_empty() {
                    None
                } else {
                    Some(Body::Raw(self.body_raw.value.clone()))
                }
            }
            BodyType::Json => {
                if self.body_json.value.is_empty() {
                    None
                } else {
                    match serde_json::from_str(&self.body_json.value) {
                        Ok(val) => Some(Body::Json(val)),
                        Err(_) => Some(Body::Json(serde_json::Value::String(
                            self.body_json.value.clone(),
                        ))),
                    }
                }
            }
            BodyType::Form => {
                let map: HashMap<String, String> = self
                    .body_form
                    .iter()
                    .filter(|kv| !kv.key.value.is_empty())
                    .map(|kv| (kv.key.value.clone(), kv.value.value.clone()))
                    .collect();
                if map.is_empty() {
                    None
                } else {
                    Some(Body::Form(map))
                }
            }
        };

        RequestDef {
            name: self.name.value.trim().to_string(),
            method: self.method.as_str().to_string(),
            url: self.url.value.clone(),
            headers,
            query,
            auth,
            body,
        }
    }
}
