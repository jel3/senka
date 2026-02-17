use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Open (or create) the log database at the given path and run migrations.
pub fn open(_path: &std::path::Path) -> Result<Connection, StoreError> {
    todo!("initialize SQLite and run migrations")
}
