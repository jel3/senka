use std::path::{Path, PathBuf};

use crate::config::ProjectConfig;
use crate::env::Environment;
use crate::request::RequestDef;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("no senka.yml found in current or parent directories")]
    ProjectNotFound,

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    ParseYaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },
}

/// Walk up from `start` looking for `senka.yml`, returning its parent directory.
pub fn find_project_root(start: &Path) -> Result<PathBuf, LoadError> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("senka.yml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(LoadError::ProjectNotFound);
        }
    }
}

/// Read and parse `senka.yml` from the project root.
pub fn load_config(root: &Path) -> Result<ProjectConfig, LoadError> {
    let path = root.join("senka.yml");
    let contents = std::fs::read_to_string(&path).map_err(|e| LoadError::ReadFile {
        path: path.clone(),
        source: e,
    })?;
    serde_yaml::from_str(&contents).map_err(|e| LoadError::ParseYaml { path, source: e })
}

/// Read and parse `senka-env/{name}.yml`.
pub fn load_env(root: &Path, name: &str) -> Result<Environment, LoadError> {
    let path = root.join("senka-env").join(format!("{name}.yml"));
    let contents = std::fs::read_to_string(&path).map_err(|e| LoadError::ReadFile {
        path: path.clone(),
        source: e,
    })?;
    serde_yaml::from_str(&contents).map_err(|e| LoadError::ParseYaml { path, source: e })
}

/// Read and parse `senka-requests/{name}.yml`.
pub fn load_request(root: &Path, name: &str) -> Result<RequestDef, LoadError> {
    let path = root.join("senka-requests").join(format!("{name}.yml"));
    let contents = std::fs::read_to_string(&path).map_err(|e| LoadError::ReadFile {
        path: path.clone(),
        source: e,
    })?;
    serde_yaml::from_str(&contents).map_err(|e| LoadError::ParseYaml { path, source: e })
}

/// List environment names (filenames in `senka-env/` without `.yml` extension).
pub fn list_envs(root: &Path) -> Result<Vec<String>, LoadError> {
    list_yml_stems(&root.join("senka-env"))
}

/// List request names (filenames in `senka-requests/` without `.yml` extension).
pub fn list_requests(root: &Path) -> Result<Vec<String>, LoadError> {
    list_yml_stems(&root.join("senka-requests"))
}

fn list_yml_stems(dir: &Path) -> Result<Vec<String>, LoadError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(LoadError::ReadFile {
                path: dir.to_path_buf(),
                source: e,
            })
        }
    };

    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yml") {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_project(dir: &Path) {
        fs::write(dir.join("senka.yml"), "name: test-project\n").unwrap();

        let env_dir = dir.join("senka-env");
        fs::create_dir_all(&env_dir).unwrap();
        fs::write(env_dir.join("dev.yml"), "base_url: http://localhost:3000\n").unwrap();
        fs::write(
            env_dir.join("staging.yml"),
            "base_url: http://staging.example.com\n",
        )
        .unwrap();

        let req_dir = dir.join("senka-requests");
        fs::create_dir_all(&req_dir).unwrap();
        fs::write(
            req_dir.join("users.get.yml"),
            "name: users.get\nmethod: GET\nurl: \"{{base_url}}/users\"\n",
        )
        .unwrap();
    }

    #[test]
    fn find_project_root_finds_tool_yml() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let nested = tmp.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();

        let root = find_project_root(&nested).unwrap();
        assert_eq!(root, tmp.path());
    }

    #[test]
    fn find_project_root_returns_error_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_project_root(tmp.path()).is_err());
    }

    #[test]
    fn load_config_parses_tool_yml() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let config = load_config(tmp.path()).unwrap();
        assert_eq!(config.name, "test-project");
    }

    #[test]
    fn load_env_parses_env_file() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let env = load_env(tmp.path(), "dev").unwrap();
        assert_eq!(env.vars.get("base_url").unwrap(), "http://localhost:3000");
    }

    #[test]
    fn load_request_parses_request_file() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let req = load_request(tmp.path(), "users.get").unwrap();
        assert_eq!(req.name, "users.get");
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "{{base_url}}/users");
    }

    #[test]
    fn list_envs_returns_sorted_names() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let envs = list_envs(tmp.path()).unwrap();
        assert_eq!(envs, vec!["dev", "staging"]);
    }

    #[test]
    fn list_requests_returns_sorted_names() {
        let tmp = tempfile::tempdir().unwrap();
        setup_project(tmp.path());

        let reqs = list_requests(tmp.path()).unwrap();
        assert_eq!(reqs, vec!["users.get"]);
    }
}
