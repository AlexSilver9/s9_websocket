//! Core types for WebSocket event handling and messaging.
//!
//! This module provides the public API types used for WebSocket communication:
//! - [`S9WebSocketClientHandler`] - Trait for handler-based event callbacks
//! - [`WebSocketEvent`] - Events received from async non-blocking client
//! - [`ControlMessage`] - Control messages sent to async non-blocking client

// ============================================================================
// Macros
// ============================================================================

// Send message to channel or break when sending a message fails.
macro_rules! send_or_break {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("Failed to send context {} through channel: {}", $context, e);
            break;
        }
    };
}

// Send message to channel or log when sending a message fails.
macro_rules! send_or_log {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("Failed to send context {} through channel: {}", $context, e);
        }
    };
}

pub(crate) use send_or_break;
pub(crate) use send_or_log;

// ============================================================================
// Public API Types
// ============================================================================

/// Trait for handling WebSocket events via callbacks.
///
/// This trait is used with [`S9NonBlockingWebSocketClient`](crate::S9NonBlockingWebSocketClient)
/// and [`S9BlockingWebSocketClient`](crate::S9BlockingWebSocketClient) to receive events
/// through callback methods.
///
/// The trait is generic over the client type `C`, which is passed as `&mut C` to each handler
/// method, allowing direct calls to client functions from within callbacks.
///
/// # Event Loop Lifecycle
///
/// Handler methods are called in this order:
/// 1. [`on_activated`](Self::on_activated) - Called once before entering the event loop
/// 2. [`on_poll`](Self::on_poll) - Called every iteration before socket read (highest priority)
/// 3. Message handlers ([`on_text_message`](Self::on_text_message), [`on_binary_message`](Self::on_binary_message), etc.) - Called when data arrives
/// 4. [`on_idle`](Self::on_idle) - Called only when no data available (WouldBlock/TimedOut)
/// 5. [`on_quit`](Self::on_quit) - Called once when event loop is about to break
///
/// # All Methods Have Default Implementations
///
/// All trait methods have default no-op implementations. Implement only the methods you need:
///
/// - [`on_activated`](Self::on_activated) - Initialization before event loop
/// - [`on_poll`](Self::on_poll) - High-priority tasks every iteration
/// - [`on_idle`](Self::on_idle) - Low-priority tasks when idle
/// - [`on_text_message`](Self::on_text_message) - Handle text messages
/// - [`on_binary_message`](Self::on_binary_message) - Handle binary messages
/// - [`on_ping`](Self::on_ping) - Handle ping frames
/// - [`on_pong`](Self::on_pong) - Handle pong frames
/// - [`on_connection_closed`](Self::on_connection_closed) - Handle connection closure
/// - [`on_error`](Self::on_error) - Handle errors
/// - [`on_quit`](Self::on_quit) - Cleanup before exit
///
/// # Examples
///
/// ## Basic Handler
///
/// ```no_run
/// use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketClientHandler, NonBlockingOptions};
///
/// struct MyHandler {
///     message_count: usize,
/// }
///
/// impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for MyHandler {
///     fn on_text_message(&mut self, client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
///         println!("Received: {}", String::from_utf8_lossy(data));
///         self.message_count += 1;
///
///         if self.message_count >= 5 {
///             client.close();  // Direct call to client method
///         }
///     }
///
///     fn on_binary_message(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
///         println!("Received {} bytes", data.len());
///     }
///
///     fn on_connection_closed(&mut self, _client: &mut S9NonBlockingWebSocketClient, reason: Option<String>) {
///         println!("Connection closed: {:?}", reason);
///     }
///
///     fn on_error(&mut self, _client: &mut S9NonBlockingWebSocketClient, error: String) {
///         eprintln!("Error: {}", error);
///     }
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", NonBlockingOptions::new())?;
/// let mut handler = MyHandler { message_count: 0 };
/// client.run(&mut handler);
/// # Ok(())
/// # }
/// ```
///
/// ## Using Lifecycle Hooks
///
/// ```no_run
/// use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketClientHandler, NonBlockingOptions};
/// use crossbeam_channel::{unbounded, Receiver};
///
/// enum Signal { Close, ForceQuit }
///
/// struct HandlerWithSignals {
///     signal_rx: Receiver<Signal>,
/// }
///
/// impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for HandlerWithSignals {
///     fn on_activated(&mut self, _client: &mut S9NonBlockingWebSocketClient) {
///         println!("Handler activated - ready to receive messages");
///     }
///
///     fn on_idle(&mut self, client: &mut S9NonBlockingWebSocketClient) {
///         // Check for external signals when no WebSocket data available
///         if let Ok(signal) = self.signal_rx.try_recv() {
///             match signal {
///                 Signal::Close => client.close(),
///                 Signal::ForceQuit => client.force_quit(),
///             }
///         }
///     }
///
///     fn on_text_message(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
///         println!("Message: {}", String::from_utf8_lossy(data));
///     }
///
///     fn on_binary_message(&mut self, _client: &mut S9NonBlockingWebSocketClient, _data: &[u8]) {}
///     fn on_connection_closed(&mut self, _client: &mut S9NonBlockingWebSocketClient, _reason: Option<String>) {}
///     fn on_error(&mut self, _client: &mut S9NonBlockingWebSocketClient, _error: String) {}
///
///     fn on_quit(&mut self, _client: &mut S9NonBlockingWebSocketClient) {
///         println!("Handler shutting down");
///     }
/// }
/// ```
pub trait S9WebSocketClientHandler<C> {
    /// Called once before entering the event loop.
    ///
    /// Use this for initialization tasks that should happen after the connection is established
    /// but before processing messages.
    ///
    /// **Default**: No-op (does nothing)
    fn on_activated(&mut self, client: &mut C) {
        let _ = client;
    }

    /// Called every event loop iteration before attempting to read from the socket.
    ///
    /// This is called regardless of whether data is available, making it suitable for
    /// highest-priority tasks that must execute frequently.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Use Cases
    /// - Heartbeat checks
    /// - Timeout tracking
    /// - High-frequency state updates
    fn on_poll(&mut self, client: &mut C) {
        let _ = client;
    }

    /// Called only when no data is available from the socket (WouldBlock/TimedOut errors).
    ///
    /// This is suitable for lower-priority tasks that should only run when the connection is idle.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Use Cases
    /// - Checking external signals/channels
    /// - Background maintenance tasks
    /// - Graceful shutdown coordination
    fn on_idle(&mut self, client: &mut C) {
        let _ = client;
    }

    /// Called when a text message is received.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client, allowing direct function calls
    /// - `data`: Raw UTF-8 bytes of the text message
    ///
    /// # Note
    /// The `data` slice is borrowed from the underlying WebSocket message and is only
    /// valid for the duration of this callback (zero-copy delivery).
    fn on_text_message(&mut self, client: &mut C, data: &[u8]) {
        let _ = (client, data);
    }

    /// Called when a binary message is received.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client, allowing direct function calls
    /// - `data`: Raw bytes of the binary message
    ///
    /// # Note
    /// The `data` slice is borrowed from the underlying WebSocket message and is only
    /// valid for the duration of this callback (zero-copy delivery).
    fn on_binary_message(&mut self, client: &mut C, data: &[u8]) {
        let _ = (client, data);
    }

    /// Called when a Ping frame is received.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Note
    /// Pong responses are handled automatically by the underlying tungstenite library.
    /// This callback is for monitoring/logging purposes only.
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client
    /// - `data`: Ping frame payload (if any)
    fn on_ping(&mut self, client: &mut C, _data: &[u8]) {
        let _ = client;
    }

    /// Called when a Pong frame is received.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client
    /// - `data`: Pong frame payload (if any)
    fn on_pong(&mut self, client: &mut C, _data: &[u8]) {
        let _ = client;
    }

    /// Called when the WebSocket connection is closed.
    ///
    /// This is called when:
    /// - The server sends a Close frame
    /// - The client sends a Close frame and receives acknowledgment
    /// - The connection is lost
    ///
    /// After this callback, [`on_quit`](Self::on_quit) will be called and the event loop will terminate.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client
    /// - `reason`: Optional close reason string from the server
    fn on_connection_closed(&mut self, client: &mut C, reason: Option<String>) {
        let _ = (client, reason);
    }

    /// Called when an error occurs during WebSocket operations.
    ///
    /// After this callback, [`on_quit`](Self::on_quit) will be called and the event loop will terminate.
    ///
    /// **Default**: No-op (does nothing)
    ///
    /// # Parameters
    /// - `client`: Mutable reference to the client
    /// - `error`: Error description string
    fn on_error(&mut self, client: &mut C, error: String) {
        let _ = (client, error);
    }
    
    /// Called once when the event loop is about to terminate.
    ///
    /// This is called after:
    /// - [`on_connection_closed`](Self::on_connection_closed) (for graceful closes)
    /// - [`on_error`](Self::on_error) (for errors)
    /// - `force_quit()` is called
    ///
    /// Use this for cleanup tasks.
    ///
    /// **Default**: No-op (does nothing)
    fn on_quit(&mut self, client: &mut C) {
        let _ = client;
    }
}

/// Events received from [`S9AsyncNonBlockingWebSocketClient`](crate::S9AsyncNonBlockingWebSocketClient).
///
/// These events are delivered via the [`event_rx`](crate::S9AsyncNonBlockingWebSocketClient::event_rx)
/// channel and represent all possible WebSocket events.
///
/// # Event Flow
///
/// 1. [`Activated`](Self::Activated) - Sent once when the event loop starts
/// 2. Message events - [`TextMessage`](Self::TextMessage), [`BinaryMessage`](Self::BinaryMessage), etc.
/// 3. [`ConnectionClosed`](Self::ConnectionClosed) or [`Error`](Self::Error) - Terminal events
/// 4. [`Quit`](Self::Quit) - Final event before thread terminates
///
/// # Examples
///
/// ```no_run
/// use s9_websocket::{S9AsyncNonBlockingWebSocketClient, WebSocketEvent, ControlMessage, NonBlockingOptions};
/// use std::time::Duration;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let options = NonBlockingOptions::new()
///     .spin_wait_duration(Some(Duration::from_millis(10)))?;
///
/// let mut client = S9AsyncNonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;
/// let _handle = client.run()?;
///
/// client.control_tx.send(ControlMessage::SendText("Hello!".to_string()))?;
///
/// loop {
///     match client.event_rx.recv() {
///         Ok(WebSocketEvent::Activated) => {
///             println!("Client activated");
///         }
///         Ok(WebSocketEvent::TextMessage(data)) => {
///             println!("Received: {}", String::from_utf8_lossy(&data));
///             client.control_tx.send(ControlMessage::Close())?;
///         }
///         Ok(WebSocketEvent::BinaryMessage(data)) => {
///             println!("Received {} bytes", data.len());
///         }
///         Ok(WebSocketEvent::Ping(data)) => {
///             println!("Ping: {} bytes", data.len());
///         }
///         Ok(WebSocketEvent::Pong(data)) => {
///             println!("Pong: {} bytes", data.len());
///         }
///         Ok(WebSocketEvent::ConnectionClosed(reason)) => {
///             println!("Closed: {:?}", reason);
///         }
///         Ok(WebSocketEvent::Error(error)) => {
///             eprintln!("Error: {}", error);
///         }
///         Ok(WebSocketEvent::Quit) => {
///             println!("Quitting");
///             break;
///         }
///         Err(e) => {
///             eprintln!("Channel error: {}", e);
///             break;
///         }
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub enum WebSocketEvent {
    /// Event loop has started and is ready to process messages.
    ///
    /// This is the first event sent after calling [`run()`](crate::S9AsyncNonBlockingWebSocketClient::run).
    Activated,

    /// A text message was received.
    ///
    /// Contains the raw UTF-8 bytes of the message. The message is allocated and owned,
    /// allowing it to be sent across threads safely.
    TextMessage(Vec<u8>),

    /// A binary message was received.
    ///
    /// Contains the raw bytes of the message. The message is allocated and owned,
    /// allowing it to be sent across threads safely.
    BinaryMessage(Vec<u8>),

    /// A Ping frame was received.
    ///
    /// Contains the ping payload (if any). Pong responses are sent automatically.
    Ping(Vec<u8>),

    /// A Pong frame was received.
    ///
    /// Contains the pong payload (if any).
    Pong(Vec<u8>),

    /// The WebSocket connection was closed.
    ///
    /// Contains an optional reason string. This event is sent when:
    /// - The server sends a Close frame
    /// - [`ControlMessage::Close`] is sent and acknowledged
    /// - The connection is lost
    ///
    /// A [`Quit`](Self::Quit) event will follow this.
    ConnectionClosed(Option<String>),

    /// An error occurred during WebSocket operations.
    ///
    /// Contains a description of the error. A [`Quit`](Self::Quit) event will follow this.
    Error(String),

    /// The event loop is terminating.
    ///
    /// This is the final event sent before the background thread exits. It follows either:
    /// - [`ConnectionClosed`](Self::ConnectionClosed) (graceful close)
    /// - [`Error`](Self::Error) (error condition)
    /// - [`ControlMessage::ForceQuit`] (immediate shutdown)
    Quit,
}

/// Control messages sent to [`S9AsyncNonBlockingWebSocketClient`](crate::S9AsyncNonBlockingWebSocketClient).
///
/// These messages are sent via the [`control_tx`](crate::S9AsyncNonBlockingWebSocketClient::control_tx)
/// channel to control the WebSocket connection from other threads.
///
/// # Examples
///
/// ```no_run
/// use s9_websocket::{S9AsyncNonBlockingWebSocketClient, ControlMessage, WebSocketEvent, NonBlockingOptions};
/// use std::time::Duration;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let options = NonBlockingOptions::new()
///     .spin_wait_duration(Some(Duration::from_millis(10)))?;
///
/// let mut client = S9AsyncNonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;
/// let _handle = client.run()?;
///
/// // Send different types of messages
/// client.control_tx.send(ControlMessage::SendText("Hello!".to_string()))?;
/// client.control_tx.send(ControlMessage::SendBinary(vec![1, 2, 3]))?;
/// client.control_tx.send(ControlMessage::SendPing(vec![]))?;
///
/// // Graceful close
/// client.control_tx.send(ControlMessage::Close())?;
///
/// // Or force immediate quit (not recommended unless necessary)
/// // client.control_tx.send(ControlMessage::ForceQuit())?;
/// # Ok(())
/// # }
/// ```
pub enum ControlMessage {
    /// Send a text message to the server.
    ///
    /// The string will be encoded as UTF-8 and sent as a WebSocket text frame.
    SendText(String),

    /// Send a binary message to the server.
    ///
    /// The bytes will be sent as a WebSocket binary frame.
    SendBinary(Vec<u8>),

    /// Send a Ping frame to the server.
    ///
    /// The server should respond with a Pong frame. The payload is optional application data.
    SendPing(Vec<u8>),

    /// Send a Pong frame to the server.
    ///
    /// This is typically used to respond to Ping frames, though pong responses are sent
    /// automatically. The payload is optional application data.
    SendPong(Vec<u8>),

    /// Gracefully close the WebSocket connection.
    ///
    /// This sends a Close frame to the server and waits for the server's Close frame response.
    /// After receiving the response, [`WebSocketEvent::ConnectionClosed`] and
    /// [`WebSocketEvent::Quit`] events will be sent.
    Close(),

    /// Immediately break the event loop without sending a Close frame.
    ///
    /// This bypasses the graceful shutdown process and terminates the event loop immediately.
    /// A [`WebSocketEvent::Quit`] event will be sent.
    ///
    /// # Note
    /// Prefer [`Close()`](Self::Close) for graceful shutdowns.
    ForceQuit(),
}