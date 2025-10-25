use std::fmt;
use tungstenite::Error as TungsteniteError;
use crossbeam_channel::SendError;

/// Errors related to WebSocket operations
#[derive(Debug)]
pub enum WebSocketError {
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

/// Errors related to internal control channel operations
#[derive(Debug)]
pub enum ControlChannelError {
    /// Failed to send message through control channel
    SendFailed(String),
}

/// Combined error type for all S9 WebSocket operations
#[derive(Debug)]
pub enum S9WebSocketError {
    /// WebSocket-related error
    WebSocket(WebSocketError),
    /// Control channel-related error
    ControlChannel(ControlChannelError),
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebSocketError::InvalidUri(uri) => write!(f, "Invalid URI: {}", uri),
            WebSocketError::ConnectionClosed(reason) => {
                match reason {
                    Some(r) => write!(f, "Connection closed: {}", r),
                    None => write!(f, "Connection closed without reason"),
                }
            }
            WebSocketError::SocketUnavailable => write!(f, "Socket already moved to thread"),
            WebSocketError::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            WebSocketError::Io(err) => write!(f, "IO error: {}", err),
            WebSocketError::Tungstenite(err) => write!(f, "WebSocket error: {}", err),
        }
    }
}

impl fmt::Display for ControlChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlChannelError::SendFailed(msg) => write!(f, "Control channel send failed: {}", msg),
        }
    }
}

impl fmt::Display for S9WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            S9WebSocketError::WebSocket(err) => write!(f, "{}", err),
            S9WebSocketError::ControlChannel(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for WebSocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WebSocketError::Io(err) => Some(err),
            WebSocketError::Tungstenite(err) => Some(err),
            _ => None,
        }
    }
}

impl std::error::Error for ControlChannelError {}

impl std::error::Error for S9WebSocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            S9WebSocketError::WebSocket(err) => Some(err),
            S9WebSocketError::ControlChannel(err) => Some(err),
        }
    }
}

// Convert from tungstenite errors to WebSocketError
impl From<TungsteniteError> for WebSocketError {
    fn from(err: TungsteniteError) -> Self {
        match err {
            TungsteniteError::ConnectionClosed => {
                WebSocketError::ConnectionClosed(Some(err.to_string()))
            }
            TungsteniteError::Io(io_err) => {
                WebSocketError::Io(io_err)
            }
            TungsteniteError::Url(url_err) => {
                WebSocketError::InvalidUri(url_err.to_string())
            }
            _ => WebSocketError::Tungstenite(err),
        }
    }
}

// Convert from tungstenite errors to S9WebSocketError
impl From<TungsteniteError> for S9WebSocketError {
    fn from(err: TungsteniteError) -> Self {
        S9WebSocketError::WebSocket(WebSocketError::from(err))
    }
}

// Convert from crossbeam SendError to ControlChannel error
impl<T> From<SendError<T>> for ControlChannelError {
    fn from(err: SendError<T>) -> Self {
        ControlChannelError::SendFailed(err.to_string())
    }
}

// Convert from crossbeam SendError to S9WebSocketError
impl<T> From<SendError<T>> for S9WebSocketError {
    fn from(err: SendError<T>) -> Self {
        S9WebSocketError::ControlChannel(ControlChannelError::from(err))
    }
}


// Convert from std::io::Error to WebSocket error
impl From<std::io::Error> for WebSocketError {
    fn from(err: std::io::Error) -> Self {
        WebSocketError::Io(err)
    }
}

// Convert from std::io::Error to S9WebSocketError
impl From<std::io::Error> for S9WebSocketError {
    fn from(err: std::io::Error) -> Self {
        S9WebSocketError::WebSocket(WebSocketError::from(err))
    }
}

// Convert from WebSocketError to S9WebSocketError
impl From<WebSocketError> for S9WebSocketError {
    fn from(err: WebSocketError) -> Self {
        S9WebSocketError::WebSocket(err)
    }
}

// Convert from ControlChannelError error to S9WebSocketError
impl From<ControlChannelError> for S9WebSocketError {
    fn from(err: ControlChannelError) -> Self {
        S9WebSocketError::ControlChannel(err)
    }
}

/// Result type alias for S9 WebSocket operations
pub type S9Result<T> = Result<T, S9WebSocketError>;