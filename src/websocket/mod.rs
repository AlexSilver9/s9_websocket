// Public API modules
pub mod types;
pub mod options;

// Internal modules
mod shared;

// Client implementations
mod async_client;
mod nonblocking_client;
mod blocking_client;

// Re-export public types
pub use types::{S9WebSocketClientHandler, WebSocketEvent, ControlMessage};
pub use options::{NonBlockingOptions, BlockingOptions};

// Re-export client types
pub use async_client::S9AsyncNonBlockingWebSocketClient;
pub use nonblocking_client::S9NonBlockingWebSocketClient;
pub use blocking_client::S9BlockingWebSocketClient;
