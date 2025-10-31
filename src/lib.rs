//! # Silver9 WebSocket
//!
//! For more examples and documentation, see the [README on GitHub](https://github.com/AlexanderSilvennoinen/s9_websocket).

mod websocket;
mod error;

pub use websocket::*;
pub use error::{S9Result, S9WebSocketError};