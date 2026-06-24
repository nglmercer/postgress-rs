use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("protocol error: {0}")]
    ProtocolError(String),
    #[error("transaction error: {0}")]
    TransactionError(String),
    #[error("catalog error: {0}")]
    CatalogError(String),
    #[error("executor error: {0}")]
    ExecutorError(String),
}

pub type Result<T> = std::result::Result<T, Error>;
