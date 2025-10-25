//! # Silver9 WebSocket
//!
//! A high-performance Rust WebSocket client library providing both non-blocking and blocking implementations.
//!
//! ## Features
//!
//! - 🚀 **Non-blocking and blocking modes** - Choose the right approach for your use case
//! - ⚡ **Low latency** - TCP_NODELAY enabled by default for Non-blocking
//! - 🔒 **TLS backend** - Support for native-tls
//! - 📡 **Event-driven architecture** - Clean separation of concerns with channels
//! - 🎯 **Type-safe API** - Leverage Rust's type system for correctness
//! - 📊 **Built-in tracing** - Comprehensive logging support
//!
//! ## Quick Start
//!
//! ### Non-blocking Client
//!
//! ```rust,no_run
//! use s9_websocket::{S9NonBlockingWebSocketClient, WebSocketEvent, NonBlockingOptions};
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Connect to WebSocket server
//! let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org")?;
//!
//! // Configure non-blocking options
//! let options = NonBlockingOptions::new(Some(Duration::from_millis(10)))?;
//!
//! // Start the event loop
//! client.run_non_blocking(options)?;
//!
//! // Send a message
//! client.send_text_message("Hello, WebSocket!")?;
//!
//! // Handle events
//! loop {
//!     match client.event_rx.recv() {
//!         Ok(WebSocketEvent::TextMessage(data)) => {
//!             let text = String::from_utf8_lossy(&data);
//!             println!("Received: {}", text);
//!         },
//!         Ok(WebSocketEvent::Quit) => break,
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Blocking Client
//!
//! ```rust,no_run
//! use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler, ControlMessage};
//! use crossbeam_channel::unbounded;
//!
//! struct MyHandler;
//!
//! impl S9WebSocketClientHandler for MyHandler {
//!     fn on_text_message(&mut self, data: &[u8]) {
//!         let text = String::from_utf8_lossy(data);
//!         println!("Received: {}", text);
//!     }
//!
//!     fn on_binary_message(&mut self, data: &[u8]) {
//!         println!("Received binary: {} bytes", data.len());
//!     }
//!
//!     fn on_connection_closed(&mut self, reason: Option<String>) {
//!         println!("Connection closed: {:?}", reason);
//!     }
//!
//!     fn on_error(&mut self, error: String) {
//!         eprintln!("Error: {}", error);
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org")?;
//! let mut handler = MyHandler;
//! let (_control_tx, _) = unbounded();
//!
//! client.run_blocking(&mut handler, control_rx);
//! # Ok(())
//! # }
//! ```
//!
//! For more examples and detailed documentation, see the [README on GitHub](https://github.com/AlexanderSilvennoinen/s9_websocket).


pub mod websocket;