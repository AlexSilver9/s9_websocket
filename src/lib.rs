//! # Silver9 WebSocket
//!
//! A simplified high-performance low-latency Rust WebSocket client library providing three distinct
//! implementations: async/threaded with channels, non-blocking with callbacks, and blocking with callbacks.
//!
//! ## Features
//!
//! - **Low latency** - Built as a thin layer over [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite)
//! - **Multiple client types** - Choose between async channels, non-blocking callbacks, or blocking callbacks
//! - **TLS support** - Built-in support for secure WebSocket connections via `native-tls`
//! - **Event-driven** - Handler callbacks or channel-based event delivery
//! - **Type-safe API** - Leverage Rust's type system for correctness
//! - **Built-in tracing** - Comprehensive logging support via the `tracing` crate
//!
//! ## Client Types
//!
//! This library provides three WebSocket client implementations:
//!
//! ### 1. [`S9NonBlockingWebSocketClient`]
//!
//! Pure non-blocking client with zero-overhead handler callbacks. Best for lowest possible
//! latency with direct control.
//!
//! **Key characteristics:**
//! - Runs on caller's thread
//! - Non-blocking socket I/O
//! - Zero-copy message delivery via callbacks
//! - Direct method calls from handler
//! - Lowest latency
//!
//! ### 2. [`S9BlockingWebSocketClient`]
//!
//! Synchronous blocking client with handler callbacks. Best for simple applications where
//! blocking is acceptable.
//!
//! **Key characteristics:**
//! - Runs on caller's thread
//! - Blocking socket I/O (with optional timeouts)
//! - Zero-copy message delivery via callbacks
//! - Direct method calls from handler
//! - Simple synchronous model
//!
//! ### 3. [`S9AsyncNonBlockingWebSocketClient`]
//!
//! Spawns a background thread and communicates via channels. Best for applications that need
//! async event processing with channels.
//!
//! **Key characteristics:**
//! - Spawns dedicated background thread
//! - Non-blocking socket I/O
//! - Event delivery via [`Receiver<WebSocketEvent>`](crossbeam_channel::Receiver)
//! - Control via [`Sender<ControlMessage>`](crossbeam_channel::Sender)
//! - Thread-safe
//!
//! ## Quick Start
//!
//! ### Non-blocking Client (with handler callbacks)
//!
//! ```no_run
//! use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketClientHandler, NonBlockingOptions};
//! use std::time::Duration;
//!
//! struct MyHandler;
//!
//! impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for MyHandler {
//!     // Only implement the methods you care about
//!     fn on_text_message(&mut self, client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
//!         println!("Received: {}", String::from_utf8_lossy(data));
//!         client.close();
//!     }
//!
//!     fn on_connection_closed(&mut self, _client: &mut S9NonBlockingWebSocketClient, reason: Option<String>) {
//!         println!("Connection closed: {:?}", reason);
//!     }
//!
//!     fn on_error(&mut self, _client: &mut S9NonBlockingWebSocketClient, error: String) {
//!         eprintln!("Error: {}", error);
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let options = NonBlockingOptions::new()
//!     .spin_wait_duration(Some(Duration::from_millis(10)))?;
//!
//! let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;
//! client.send_text_message("Hello!")?;
//!
//! let mut handler = MyHandler;
//! client.run(&mut handler);
//! # Ok(())
//! # }
//! ```
//!
//! ### Blocking Client
//!
//! ```no_run
//! use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler, BlockingOptions};
//!
//! struct MyHandler;
//!
//! impl S9WebSocketClientHandler<S9BlockingWebSocketClient> for MyHandler {
//!     // Only implement the methods you care about
//!     fn on_text_message(&mut self, client: &mut S9BlockingWebSocketClient, data: &[u8]) {
//!         println!("Received: {}", String::from_utf8_lossy(data));
//!         client.close();
//!     }
//!
//!     fn on_connection_closed(&mut self, _client: &mut S9BlockingWebSocketClient, reason: Option<String>) {
//!         println!("Connection closed: {:?}", reason);
//!     }
//!
//!     fn on_error(&mut self, _client: &mut S9BlockingWebSocketClient, error: String) {
//!         eprintln!("Error: {}", error);
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let options = BlockingOptions::new();
//! let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;
//! client.send_text_message("Hello!")?;
//!
//! let mut handler = MyHandler;
//! client.run(&mut handler);
//! # Ok(())
//! # }
//! ```
//!
//! ### Async Non-blocking Client (with channels)
//!
//! ```no_run
//! use s9_websocket::{S9AsyncNonBlockingWebSocketClient, WebSocketEvent, ControlMessage, NonBlockingOptions};
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure with 10ms sleep between reads
//! let options = NonBlockingOptions::new()
//!     .spin_wait_duration(Some(Duration::from_millis(10)))?;
//!
//! // Connect to WebSocket server
//! let mut client = S9AsyncNonBlockingWebSocketClient::connect(
//!     "wss://echo.websocket.org",
//!     options
//! )?;
//!
//! // Start the event loop (spawns thread)
//! let _handle = client.run()?;
//!
//! // Send a message via control channel
//! client.control_tx.send(ControlMessage::SendText("Hello!".to_string()))?;
//!
//! // Handle events from channel
//! loop {
//!     match client.event_rx.recv() {
//!         Ok(WebSocketEvent::TextMessage(data)) => {
//!             println!("Received: {}", String::from_utf8_lossy(&data));
//!             client.control_tx.send(ControlMessage::Close())?;
//!         },
//!         Ok(WebSocketEvent::Quit) => break,
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Accessing the Underlying Socket
//!
//! All clients provide access to the underlying tungstenite WebSocket for advanced use cases:
//!
//! ```no_run
//! # use s9_websocket::{S9NonBlockingWebSocketClient, NonBlockingOptions};
//! # let options = NonBlockingOptions::new();
//! # let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;
//! // Get immutable reference to the socket
//! let socket = client.get_socket();
//!
//! // Get mutable reference to the socket
//! let socket_mut = client.get_socket_mut();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! **Note for S9AsyncNonBlockingWebSocketClient**: Socket access returns `Option<&WebSocket>`
//! because the socket is moved to a background thread after calling `run()`.
//!
//! ## Performance Tips
//!
//! 1. **Tune spin wait duration**: Balance CPU usage vs latency
//!    - `None`: Maximum performance, high CPU usage
//!    - `Some(Duration::from_millis(1-10))`: Good balance
//!    - `Some(Duration::from_millis(50-100))`: Lower CPU, higher latency
//!
//! 2. **Enable TCP_NODELAY**: Reduce latency for small messages
//!    ```no_run
//!    # use s9_websocket::NonBlockingOptions;
//!    let options = NonBlockingOptions::new().nodelay(true);
//!    ```
//!
//! ## Scalability
//!
//! **Important:** This library does not scale to thousands of connections. Each client requires
//! one OS thread (either spawned or caller's thread). For applications requiring 1000+ concurrent
//! connections, consider async libraries like `tokio-tungstenite` or `async-tungstenite`.
//!
//! ## Error Handling
//!
//! All fallible operations return [`S9Result<T>`], which is an alias for `Result<T, S9WebSocketError>`.
//! See [`S9WebSocketError`] for detailed error types.
//!
//! - **non-blocking**: A `WebSocketEvent::Quit` event is published after any error during reading messages from underlying WebSocket
//! - **blocking**: The `S9WebSocketClientHandler::on_quit()` callback is called after any error during reading messages from underlying WebSocket
//!
//! ## Logging
//!
//! The library uses the `tracing` crate. Enable logging in your application:
//!
//! ### Log levels:
//! - **TRACE**: Detailed message content and connection details
//! - **DEBUG**: Connection lifecycle events
//! - **ERROR**: Error conditions
//!
//! ```no_run
//! tracing_subscriber::fmt()
//!     .with_max_level(tracing::Level::INFO)
//!     .init();
//! ```
//!
//! For more examples and detailed documentation, see the [README on GitHub](https://github.com/AlexSilver9/s9_websocket).

mod websocket;
mod error;

pub use websocket::*;
pub use error::{S9Result, S9WebSocketError};
