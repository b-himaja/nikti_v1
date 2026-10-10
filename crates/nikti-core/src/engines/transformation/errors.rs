use polars::prelude::PolarsError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransformError {
    #[error("column '{0}' not found")]
    ColumnNotFound(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
    #[error("formula error: {0}")]
    Formula(String),
}
