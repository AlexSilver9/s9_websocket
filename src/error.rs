use std::fmt;
use tungstenite::Error as TungsteniteError;

/// Errors related to WebSocket operations
#[derive(Debug)]
pub enum S9WebSocketError {
    /// Invalid URI provided
    InvalidUri(String),
    /// WebSocket connection was closed
    ConnectionClosed(Option<String>),
    /// Socket is unavailable (already moved to thread)
    SocketUnavailable,
    /// Invalid configuration provided
    InvalidConfiguration(String),
    /// IO operation failed
    Io(std::io::Error),
    /// Tungstenite error
    Tungstenite(TungsteniteError),
}

impl fmt::Display for S9WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            S9WebSocketError::InvalidUri(uri) => write!(f, "Invalid URI: {}", uri),
            S9WebSocketError::ConnectionClosed(reason) => {
                match reason {
                    Some(r) => write!(f, "Connection closed: {}", r),
                    None => write!(f, "Connection closed without reason"),
                }
            }
            S9WebSocketError::SocketUnavailable => write!(f, "Socket already moved to thread"),
            S9WebSocketError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            S9WebSocketError::Io(err) => write!(f, "IO error: {}", err),
            S9WebSocketError::Tungstenite(err) => write!(f, "WebSocket error: {}", err),
        }
    }
}

impl std::error::Error for S9WebSocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            S9WebSocketError::Io(err) => Some(err),
            S9WebSocketError::Tungstenite(err) => Some(err),
            _ => None,
        }
    }
}


// Convert from tungstenite errors to S9WebSocketError
impl From<TungsteniteError> for S9WebSocketError {
    fn from(err: TungsteniteError) -> Self {
        match err {
            TungsteniteError::ConnectionClosed => {
                S9WebSocketError::ConnectionClosed(Some(err.to_string()))
            }
            TungsteniteError::Io(io_err) => {
                S9WebSocketError::Io(io_err)
            }
            TungsteniteError::Url(url_err) => {
                S9WebSocketError::InvalidUri(url_err.to_string())
            }
            _ => S9WebSocketError::Tungstenite(err),
        }
    }
}

// Convert from std::io::Error to S9WebSocketError error
impl From<std::io::Error> for S9WebSocketError {
    fn from(err: std::io::Error) -> Self {
        S9WebSocketError::Io(err)
    }
}

/// Result type alias for S9 WebSocket operations
pub type S9Result<T> = Result<T, S9WebSocketError>;