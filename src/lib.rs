//! # Silver9 WebSocket
//!
//! For more examples and documentation, see the [README on GitHub](https://github.com/AlexanderSilvennoinen/s9_websocket).

mod websocket;
mod error;

pub use websocket::*;
pub use error::{S9Result, S9WebSocketError};

// TODO: Provide access to underlying streams
// TODO: Add Tests
// TODO: Add API Documentation + change documentation pointer in Cargo.toml to something like https://docs.rs/s9_websocket/0.0.1
// TODO: Add Code Documentation
