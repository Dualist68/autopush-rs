use actix_ws::CloseCode;

use autoconnect_ws_sm::SMError;

/// WebPush WebSocket Handler Errors
#[derive(Debug, strum::AsRefStr, thiserror::Error)]
pub enum WSError {
    #[error("State machine error: {0}")]
    SM(#[from] SMError),

    #[error("Couldn't parse WebSocket message JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("WebSocket protocol error: {0}")]
    Protocol(#[from] actix_ws::ProtocolError),

    #[error("WebSocket session unexpectedly closed: {0}")]
    SessionClosed(#[from] actix_ws::Closed),

    #[error("Unsupported WebSocket message: {0}")]
    UnsupportedMessage(String),

    #[error("WebSocket stream unexpectedly closed")]
    StreamClosed,

    #[error("Timeout waiting for handshake")]
    HandshakeTimeout,

    #[error("ClientRegistry unexpectedly disconnected")]
    RegistryDisconnected,

    #[error("ClientRegistry disconnect unexpectedly failed (Client not connected)")]
    RegistryNotConnected,
}

impl WSError {
    /// Return an `actix_ws::CloseCode` for the WS session Close frame
    pub fn close_code(&self) -> actix_ws::CloseCode {
        match self {
            WSError::SM(e) => e.close_code(),
            // TODO: applicable here?
            //WSError::Protocol(_) => CloseCode::Protocol,
            WSError::UnsupportedMessage(_) => CloseCode::Unsupported,
            _ => CloseCode::Error,
        }
    }

    /// Return a description for the WS session Close frame.
    ///
    /// Control frames are limited to 125 bytes so returns just the enum
    /// variant's name (via `strum::AsRefStr`)
    pub fn close_description(&self) -> &str {
        self.as_ref()
    }
}
