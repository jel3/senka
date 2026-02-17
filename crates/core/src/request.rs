use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single HTTP request definition loaded from a YAML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDef {
    pub name: String,
    pub method: String,
    pub url: String,

    #[serde(default)]
    pub headers: HashMap<String, String>,

    #[serde(default)]
    pub query: HashMap<String, String>,

    pub auth: Option<AuthConfig>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthConfig {
    Bearer { token: String },
    Basic { username: String, password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Body {
    Raw(String),
    Json(serde_json::Value),
    Form(HashMap<String, String>),
}
