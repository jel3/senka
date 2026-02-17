use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use thiserror::Error;

static TEMPLATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_.\-]*)\s*\}\}").unwrap());

#[derive(Debug, Error)]
#[error("unresolved template variables: {}", missing.join(", "))]
pub struct UnresolvedVarsError {
    pub missing: Vec<String>,
}

/// Extract all `{{var}}` placeholder names from `input`.
/// Returns a list of variable names (may contain duplicates).
pub fn extract_vars(input: &str) -> Vec<String> {
    TEMPLATE_RE
        .captures_iter(input)
        .map(|caps| caps[1].to_string())
        .collect()
}

/// Render all `{{var}}` placeholders in `input` using the provided `vars` map.
/// Returns an error listing any unresolved variable names.
pub fn render(input: &str, vars: &HashMap<String, String>) -> Result<String, UnresolvedVarsError> {
    let mut missing = Vec::new();

    let result = TEMPLATE_RE.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];
        match vars.get(key) {
            Some(val) => val.clone(),
            None => {
                missing.push(key.to_string());
                caps[0].to_string()
            }
        }
    });

    if missing.is_empty() {
        Ok(result.into_owned())
    } else {
        missing.dedup();
        Err(UnresolvedVarsError { missing })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_vars() {
        let mut vars = HashMap::new();
        vars.insert("host".into(), "example.com".into());
        vars.insert("id".into(), "42".into());

        let result = render("https://{{host}}/users/{{id}}", &vars).unwrap();
        assert_eq!(result, "https://example.com/users/42");
    }

    #[test]
    fn reports_missing_vars() {
        let vars = HashMap::new();
        let err = render("{{missing}}", &vars).unwrap_err();
        assert_eq!(err.missing, vec!["missing"]);
    }

    #[test]
    fn extract_vars_finds_all_placeholders() {
        let vars = extract_vars("https://{{host}}/users/{{id}}?token={{token}}");
        assert_eq!(vars, vec!["host", "id", "token"]);
    }

    #[test]
    fn extract_vars_empty_when_none() {
        let vars = extract_vars("https://example.com/users");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_vars_with_whitespace() {
        let vars = extract_vars("{{ host }}");
        assert_eq!(vars, vec!["host"]);
    }

    #[test]
    fn handles_whitespace_in_braces() {
        let mut vars = HashMap::new();
        vars.insert("x".into(), "val".into());
        let result = render("{{ x }}", &vars).unwrap();
        assert_eq!(result, "val");
    }
}
