use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("not found")]
    NotFound,
    #[error("storage backend error: {0}")]
    Backend(String),
}
