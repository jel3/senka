use std::ffi::OsStr;

use clap_complete::engine::CompletionCandidate;
use senka_core::loader;

pub fn complete_request_names(_current: &OsStr) -> Vec<CompletionCandidate> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let Ok(root) = loader::find_project_root(&cwd) else {
        return vec![];
    };
    loader::list_requests(&root)
        .unwrap_or_default()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

pub fn complete_env_names(_current: &OsStr) -> Vec<CompletionCandidate> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let Ok(root) = loader::find_project_root(&cwd) else {
        return vec![];
    };
    loader::list_envs(&root)
        .unwrap_or_default()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}
