use autopush_common::db::error::DbError;

/// Errors that may occur in the WebPush WebSocket state machine
#[derive(thiserror::Error, Debug)]
pub enum ClientStateError {
    #[error("Database error: {0}")]
    Database(#[from] DbError),

    #[error("Invalid WebPush WebSocket message: {0}")]
    InvalidClientMessage(String),

    #[error("Already connected to another node")]
    AlreadyConnected,
}
