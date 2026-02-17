use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("request definition error: {0}")]
    Definition(String),
}
