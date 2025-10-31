//! Error types for S9 WebSocket operations.
//!
//! This module provides a unified error type [`S9WebSocketError`] that encompasses all possible
//! errors that can occur during WebSocket operations.
//!
//! # Examples
//!
//! ```no_run
//! use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketError, NonBlockingOptions};
//!
//! # fn main() {
//! match S9NonBlockingWebSocketClient::connect("wss://invalid-uri", NonBlockingOptions::new()) {
//!     Ok(client) => { /* use client */ },
//!     Err(S9WebSocketError::InvalidUri(msg)) => {
//!         eprintln!("Invalid URI: {}", msg);
//!     },
//!     Err(S9WebSocketError::Io(io_err)) => {
//!         eprintln!("Network error: {}", io_err);
//!     },
//!     Err(e) => {
//!         eprintln!("Connection failed: {}", e);
//!     }
//! }
//! # }
//! ```

use std::fmt;
use tungstenite::Error as TungsteniteError;

/// Error type for all S9 WebSocket operations.
///
/// This enum represents all possible errors that can occur when using the S9 WebSocket clients.
/// It wraps underlying errors from I/O operations, URI parsing, and the tungstenite library.
///
/// # Error Categories
///
/// - **Connection errors**: [`InvalidUri`](Self::InvalidUri), [`ConnectionClosed`](Self::ConnectionClosed)
/// - **Configuration errors**: [`InvalidConfiguration`](Self::InvalidConfiguration)
/// - **Runtime errors**: [`SocketUnavailable`](Self::SocketUnavailable), [`Io`](Self::Io), [`Tungstenite`](Self::Tungstenite)
///
/// # Examples
///
/// ```no_run
/// use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketError, BlockingOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let result = S9BlockingWebSocketClient::connect("ws://example.com", BlockingOptions::new());
///
/// match result {
///     Ok(client) => println!("Connected successfully"),
///     Err(S9WebSocketError::InvalidUri(msg)) => {
///         eprintln!("Bad URI: {}", msg);
///     }
///     Err(S9WebSocketError::Io(err)) => {
///         eprintln!("I/O error: {}", err);
///     }
///     Err(err) => {
///         eprintln!("Other error: {}", err);
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub enum S9WebSocketError {
    /// Invalid WebSocket URI was provided.
    ///
    /// This error occurs when the URI cannot be parsed or doesn't follow the WebSocket URI scheme
    /// (`ws://` or `wss://`).
    ///
    /// # Example
    /// ```no_run
    /// use s9_websocket::{S9NonBlockingWebSocketClient, NonBlockingOptions};
    ///
    /// # fn main() {
    /// // This will fail with InvalidUri because it's not a valid WebSocket URI
    /// let result = S9NonBlockingWebSocketClient::connect("not-a-uri", NonBlockingOptions::new());
    /// # }
    /// ```
    InvalidUri(String),

    /// WebSocket connection was closed by the server or due to an error.
    ///
    /// The optional `String` contains the close reason if provided by the server.
    ///
    /// # Example
    /// ```no_run
    /// use s9_websocket::{S9AsyncNonBlockingWebSocketClient, WebSocketEvent, NonBlockingOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut client = S9AsyncNonBlockingWebSocketClient::connect("wss://echo.websocket.org", NonBlockingOptions::new())?;
    /// # let _handle = client.run()?;
    /// match client.event_rx.recv() {
    ///     Ok(WebSocketEvent::ConnectionClosed(reason)) => {
    ///         println!("Connection closed: {:?}", reason);
    ///     }
    ///     _ => {}
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ConnectionClosed(Option<String>),

    /// Socket is unavailable because it was already moved to event loop thread.
    ///
    /// This error only occurs with [`S9AsyncNonBlockingWebSocketClient`](crate::S9AsyncNonBlockingWebSocketClient)
    /// when attempting to call [`run()`](crate::S9AsyncNonBlockingWebSocketClient::run) multiple times.
    SocketUnavailable,

    /// Invalid configuration was provided.
    ///
    /// This error occurs when configuration options contain invalid values, such as:
    /// - Zero-duration timeouts
    /// - Invalid spin-wait durations
    ///
    /// # Example
    /// ```no_run
    /// use s9_websocket::{NonBlockingOptions};
    /// use std::time::Duration;
    ///
    /// # fn main() {
    /// // This will fail because spin_wait_duration cannot be zero
    /// let result = NonBlockingOptions::new()
    ///     .spin_wait_duration(Some(Duration::from_secs(0)));
    /// assert!(result.is_err());
    /// # }
    /// ```
    InvalidConfiguration(String),

    /// An I/O operation failed.
    ///
    /// This wraps standard [`std::io::Error`] and can occur during:
    /// - Network operations (connect, read, write)
    /// - Socket configuration (setting timeouts, TCP options)
    Io(std::io::Error),

    /// An error from the underlying tungstenite WebSocket library.
    ///
    /// This wraps errors that don't fit into other categories, such as:
    /// - Protocol violations
    /// - Invalid WebSocket frames
    /// - HTTP upgrade failures
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

/// Convenience type alias for `Result<T, S9WebSocketError>`.
///
/// This type is used throughout the S9 WebSocket API for operations that can fail.
///
/// # Examples
///
/// ```no_run
/// use s9_websocket::{S9Result, S9NonBlockingWebSocketClient, NonBlockingOptions};
///
/// fn connect_to_server() -> S9Result<S9NonBlockingWebSocketClient> {
///     S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", NonBlockingOptions::new())
/// }
/// ```
pub type S9Result<T> = Result<T, S9WebSocketError>;