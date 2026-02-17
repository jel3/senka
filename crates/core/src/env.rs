use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Environment variables loaded from an env YAML file.
/// All values are resolved to strings for templating.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Environment {
    pub vars: HashMap<String, String>,
}
