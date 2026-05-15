use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// A single HTTP request definition loaded from a YAML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDef {
    pub name: String,
    pub method: String,
    pub url: String,

    #[serde(default, deserialize_with = "null_as_default")]
    pub headers: HashMap<String, String>,

    #[serde(default, deserialize_with = "null_as_default")]
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
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Body {
    Raw(String),
    Json(serde_json::Value),
    Form(HashMap<String, String>),
}
