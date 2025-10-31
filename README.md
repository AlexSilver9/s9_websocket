# Silver9 WebSocket

[![Crates.io](https://img.shields.io/crates/v/s9_websocket.svg)](https://crates.io/crates/s9_websocket)
[![Documentation](https://docs.rs/s9_websocket/badge.svg)](https://docs.rs/s9_websocket)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/AlexanderSilvennoinen/s9_websocket#license)
[![Rust](https://img.shields.io/badge/rust-1.80.1+-orange.svg)](https://www.rust-lang.org)

A simplified high-performance low-latency Rust WebSocket client library providing three distinct implementations: async/threaded with channels, non-blocking with callbacks, and blocking with callbacks.

## Features
- ⚡ **Low latency** - Built with [Rust](https://rust-lang.org) as a thin layer over [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite) and optional for [crossbeam-channel](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html)
- 🚀 **Non-blocking, Blocking, Async Channels** - Choose the right approach for your use case
- 🔒 **TLS backend** - Support for native-tls
- 📡 **Event-driven architecture** - With either handler callbacks or channels
- 🎯 **Type-safe API** - Leverage Rust's type system for correctness
- 📊 **Built-in tracing** - Comprehensive logging support
- 🤖 **AI Coding Ready** - Support for AI-assisted development with [Claude](https://claude.ai) (Vibe Coding)

## Design Principled
- 🕑 **Predictable latency** - Direct system calls and callbacks, means lower baseline latency
- 📋 **Low overhead** - No task scheduling, futures, or waker infrastructure
- 🤯 **Simple model** - Straightforward usage
- 📐 **Deterministic behavior** - Optional custom spin-wait duration gives precise control over CPU/latency tradeoff
- 🚫️ **No runtime** - No async runtime or framework overhead, no `poll()`/`epoll_wait()`/`kevent()`
- 🔁 **Async message passing** - Optional message passing through channels
- ⛔️ **Graceful Shutdown** - Process all messages util connection is closed by server
- 📛 **Forcibly Shutdown** - Break the client's event loop immediately
- 📛 **Forcibly Shutdown** - Break the client's event loop immediately


## Disadvantages:
- **Scalability** - None of the clients scale to thousands of connections (see Scalability Constraints)
- **CPU overhead** - The fastest mode `non-blocking` **without** `spin-wait` utilizes 100% CPU core usage by design

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
s9_websocket = "0.0.1"
```

## TLS Backend Selection
The library uses `native-tls`.

## Quick Start

Inspect [examples](examples) for various usages and use cases.

### Three Client Options

Choose the client that fits your application architecture:

1. **S9AsyncNonBlockingWebSocketClient** - Spawns background thread, receive events via channels
2. **S9NonBlockingWebSocketClient** - Runs on your thread, receive events via callbacks
3. **S9BlockingWebSocketClient** - Runs on your thread, receive events via callbacks


### Async Non-blocking Client (with channels)
```rust
use s9_websocket::{S9AsyncNonBlockingWebSocketClient, WebSocketEvent, ControlMessage, NonBlockingOptions};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
   // Configure options
   let options = NonBlockingOptions::new()
           .spin_wait_duration(Some(Duration::from_millis(10)))?;

   // Connect to WebSocket server
   let mut client = S9AsyncNonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

   // Start the event loop (spawns thread)
   let _handle = client.run()?;

   // Send a message
   client.send_text_message("Hello, WebSocket!".to_string())?;

   // Handle events from channel
   loop {
      match client.event_rx.recv() {
         Ok(WebSocketEvent::Activated) => {
            println!("WebSocket connection activated");
         },
         Ok(WebSocketEvent::TextMessage(data)) => {
            let text = String::from_utf8_lossy(&data);
            println!("Received: {}", text);

            client.control_tx.send(ControlMessage::Close())?;
         },
         Ok(WebSocketEvent::Quit) => {
            println!("Client quit");
            break;
         },
         _ => {}
      }
   }
   Ok(())
}
```

### Non-blocking Client (with handler callbacks)
```rust
use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketClientHandler, NonBlockingOptions};
use std::time::Duration;

// Implement the handler trait
struct MyHandler {
   message_count: usize,
}

impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for MyHandler {
   fn on_text_message(&mut self, client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
      let text = String::from_utf8_lossy(data);
      println!("Received: {}", text);
      self.message_count += 1;

      if self.message_count >= 2 {
         println!("Closing connection...");
         client.close();
      } else {
         // Send another message
         client.send_text_message(&format!("Echo: {}", text)).ok();
      }
   }

   fn on_binary_message(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
      println!("Received binary: {} bytes", data.len());
   }

   fn on_connection_closed(&mut self, _client: &mut S9NonBlockingWebSocketClient, reason: Option<String>) {
      println!("Connection closed: {:?}", reason);
   }

   fn on_error(&mut self, _client: &mut S9NonBlockingWebSocketClient, error: String) {
      eprintln!("Error: {}", error);
   }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
   // Configure options
   let options = NonBlockingOptions::new()
           .spin_wait_duration(Some(Duration::from_millis(10)))?;

   // Connect to WebSocket server
   let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

   // Send initial message
   client.send_text_message("Hello!")?;

   // Create handler
   let mut handler = MyHandler { message_count: 0 };

   // Run the non-blocking event loop (blocks on this thread)
   client.run(&mut handler);

   Ok(())
}
```

### Blocking Client

```rust
use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler, BlockingOptions};

// Implement the handler trait
struct MyHandler {
   message_count: usize,
}

impl S9WebSocketClientHandler<S9BlockingWebSocketClient> for MyHandler {
   fn on_text_message(&mut self, client: &mut S9BlockingWebSocketClient, data: &[u8]) {
      let text = String::from_utf8_lossy(data);
      println!("Received: {}", text);
      self.message_count += 1;

      if self.message_count >= 2 {
         println!("Closing connection...");
         client.close();
      } else {
         // Send another message
         client.send_text_message(&format!("Echo: {}", text)).ok();
      }
   }

   fn on_binary_message(&mut self, _client: &mut S9BlockingWebSocketClient, data: &[u8]) {
      println!("Received binary: {} bytes", data.len());
   }

   fn on_connection_closed(&mut self, _client: &mut S9BlockingWebSocketClient, reason: Option<String>) {
      println!("Connection closed: {:?}", reason);
   }

   fn on_error(&mut self, _client: &mut S9BlockingWebSocketClient, error: String) {
      eprintln!("Error: {}", error);
   }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
   // Configure with default blocking behavior
   let options = BlockingOptions::new();

   // Connect to WebSocket server
   let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

   // Send initial message
   client.send_text_message("Hello!")?;

   // Create handler
   let mut handler = MyHandler { message_count: 0 };

   // Run the blocking event loop (blocks on this thread)
   // Handler can call client functions directly from callbacks
   client.run(&mut handler);

   Ok(())
}
```

## Advanced Usage

### Custom Headers
```rust
use std::collections::HashMap;

let mut headers = HashMap::new();
headers.insert("Authorization".to_string(), "Bearer token123".to_string());
headers.insert("X-Custom-Header".to_string(), "value".to_string());

let client = S9NonBlockingWebSocketClient::connect_with_headers(
    "wss://api.example.com/ws",
    &headers
)?;
```

### Handling Ping/Pong
```rust
impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for MyHandler {
    fn on_ping(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
        println!("Received ping: {:?}", data);
    }

    fn on_pong(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
        println!("Received pong: {:?}", data);
    }
}
```

### Non-blocking Configuration
```rust
use std::time::Duration;

// Maximum performance (no sleep between reads, high CPU usage)
let options = NonBlockingOptions::new(None)?;

// Balanced (10ms sleep between reads)
let options = NonBlockingOptions::new(Some(Duration::from_millis(10)))?;

// Low CPU usage (100ms sleep between reads, higher latency)
let options = NonBlockingOptions::new(Some(Duration::from_millis(100)))?;
```


## API Reference

### S9AsyncNonBlockingWebSocketClient

Spawns a background thread for socket operations and communicates via channels.

**Socket Mode:** Uses non-blocking socket I/O (`set_nonblocking(true)`) internally. The name "NonBlocking" refers to the socket I/O mode, not the behavior of `run()` which returns immediately after spawning the thread.

#### Key Features
- Background thread handles all socket operations
- Receive events through built-in channels (`event_rx`)
- Send commands through built-in channels (`control_tx`)
- Thread-safe for multi-threaded applications
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

#### Functions
- `connect(uri: &str, options: NonBlockingOptions) -> S9Result<Self>`
- `connect_with_headers(uri: &str, headers: &HashMap<String, String>, options: NonBlockingOptions) -> S9Result<Self>`
- `run(&mut self) -> S9Result<JoinHandle<()>>` - Starts background thread
- `send_text_message(&mut self, text: String) -> S9Result<()>`

#### Fields
- `control_tx: Sender<ControlMessage>` - Send control messages
- `event_rx: Receiver<WebSocketEvent>` - Receive events

### S9NonBlockingWebSocketClient

Pure non-blocking client that runs on caller's thread using handler callbacks for events.

**Socket Mode:** Uses non-blocking socket I/O (`set_nonblocking(true)`) internally. The name "NonBlocking" refers to the socket I/O mode, not the behavior of `run()` which blocks the calling thread indefinitely.

#### Key Features
- Runs entirely on caller's thread
- Receive events through handler callbacks (zero copy, zero allocation)
- Call client functions directly from handler callbacks (send, close, force_quit)
- Lowest possible latency
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

#### Functions
- `connect(uri: &str, options: NonBlockingOptions) -> S9Result<Self>`
- `connect_with_headers(uri: &str, headers: &HashMap<String, String>, options: NonBlockingOptions) -> S9Result<Self>`
- `run<HANDLER>(&mut self, handler: &mut HANDLER)` - Blocks on this thread, passes `&mut self` to handler functions
- `send_text_message(&mut self, text: &str) -> S9Result<()>`
- `close(&mut self)` - Sends Close Frame to server
- `force_quit(&mut self)` - Immediately breaks event loop

### S9BlockingWebSocketClient

Synchronous client that runs on caller's thread using handler callbacks for events.

**Socket Mode:** Uses blocking socket I/O (standard socket reads/writes) internally, with optional timeouts. The name "Blocking" refers to the socket I/O mode, not the behavior of `run()` which blocks the calling thread indefinitely (same as S9NonBlockingWebSocketClient).

#### Key Features

- Runs entirely on caller's thread
- Receive events through handler callbacks (zero copy, zero allocation)
- Call client functions directly from handler callbacks (send, close, force_quit)
- Optional read/write timeouts for responsive control message handling (simulates non-blocking behavior)
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

#### Functions
- `connect(uri: &str, options: BlockingOptions) -> S9Result<Self>`
- `connect_with_headers(uri: &str, headers: &HashMap<String, String>, options: BlockingOptions) -> S9Result<Self>`
- `run<HANDLER>(&mut self, handler: &mut HANDLER)` - Blocks on this thread, passes `&mut self` to handler functions
- `send_text_message(&mut self, text: &str) -> S9Result<()>`
- `close(&mut self)` - Sends Close Frame to server
- `force_quit(&mut self)` - Immediately breaks event loop

### Shared Types

#### WebSocketEvent
Used by S9AsyncNonBlockingWebSocketClient for channel-based event delivery:
```rust
pub enum WebSocketEvent {
    Activated,                         // Event loop started
    TextMessage(Vec<u8>),              // Text message received
    BinaryMessage(Vec<u8>),            // Binary message received
    Ping(Vec<u8>),                     // Ping frame received
    Pong(Vec<u8>),                     // Pong frame received
    ConnectionClosed(Option<String>),  // Connection closed
    Error(String),                     // Error occurred
    Quit,                              // Client quitting
}
```

#### S9WebSocketClientHandler
Used by S9NonBlockingWebSocketClient and S9BlockingWebSocketClient for callback-based event delivery.
The trait is generic over the client type (`C`), which is passed as `&mut C` to each handler function.

```rust
pub trait S9WebSocketClientHandler<C> {
   fn on_activated(&mut self, client: &mut C) {}                               // Event loop started (default: noop)
   fn on_poll(&mut self, client: &mut C) {}                                    // Called every loop iteration before socket read (default: noop)
   fn on_idle(&mut self, client: &mut C) {}                                    // Called only when no data available (default: noop)
   fn on_text_message(&mut self, client: &mut C, data: &[u8]);                 // Text message received
   fn on_binary_message(&mut self, client: &mut C, data: &[u8]);               // Binary message received
   fn on_connection_closed(&mut self, client: &mut C, reason: Option<String>); // Connection closed
   fn on_error(&mut self, client: &mut C, error: String);                      // Error occurred
   fn on_ping(&mut self, client: &mut C, _data: &[u8]) {}                      // Ping frame received (default: noop)
   fn on_pong(&mut self, client: &mut C, _data: &[u8]) {}                      // Pong frame received (default: noop)
   fn on_quit(&mut self, client: &mut C) {}                                    // Client quitting (default: noop)
}
```

**Event Loop Lifecycle, Callback Priority and Use Cases:**
- `on_activated()`: Called once **before** entering the event loop. Use for initialization tasks.
- `on_poll()`: Called on **every** event loop iteration **before** reading from socket. Use for highest priority tasks.
- `on_idle()`: Called **only** when no data is available (WouldBlock/TimedOut errors). Use for lower priority tasks.
- `on_quit()`: Called once **when** event loop is about to break. Use graceful shutdown tasks.

### ControlMessage
```rust
pub enum ControlMessage {
    SendText(String),  // Send text message
    Close(),           // Close connection gracefully - sends a Close Frame
    ForceQuit(),       // Force quit - immediately break the socket and control channel poll loop
}
```

### Close and Quit
Graceful close means a Close Frame is sent to the server.  When a Close frame is received from the server (either initiated by client or server):
- **non-blocking**: a `WebSocketEvent::ConnectionClosed` is published, followed by a `WebSocketEvent::Quit` event before breaking the socket and control channel poll loop
- **blocking**: the `S9WebSocketClientHandler::on_connection_closed()` callback is invoked, followed by a `S9WebSocketClientHandler::on_quit()` call before breaking the socket and control channel poll loop

Sending a Close fame is implemented by `Drop` trait as well, but due to the immediate drop:
- **non-blocking**: `S9WebSocketClientHandler::on_quit()` is not called
- **blocking**: `WebSocketEvent::Quit` event is not generated

### Force Quit
Send a 'ControlMessage::ForceQuit' to immediately break event loop

## Logging
The library uses the 'tracing' crate for logging. Enable logging in your application:
use tracing_subscriber;

```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::TRACE)
    .init();
```

### Log levels:
- **TRACE**: Detailed message content and connection details
- **DEBUG**: Connection lifecycle events
- **ERROR**: Error conditions

## Error Handling

- **non-blocking**: A `WebSocketEvent::Quit` event is published after any error during reading messages from underlying WebSocket
- **blocking**: The `S9WebSocketClientHandler::on_quit()` callback is called after any error during reading messages from underlying WebSocket

The library uses a custom error hierarchy for error categorization:

### Error Types

```rust
/// Top-level error type for all S9WebSocket operations
pub enum S9WebSocketError {
    WebSocket(WebSocketError),           // WebSocket-related errors
    ControlChannel(ControlChannelError), // Internal channel errors
}

/// WebSocket-specific errors
pub enum WebSocketError {
    ConnectionClosed(Option<String>),    // Connection closed by server
    InvalidUri(String),                  // Invalid WebSocket URI
    Io(std::io::Error),                  // I/O errors
    Tungstenite(tungstenite::Error),     // Underlying tungstenite errors
    SocketUnavailable,                   // Socket already taken
    InvalidConfiguration(String),        // Invalid client configuration
}

/// Internal control channel errors
pub enum ControlChannelError {
    SendError(String),                   // Failed to send control message
}

/// Convenience type alias
pub type S9Result<T> = Result<T, S9WebSocketError>;
```

### Connection Failures
```rust
use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketError, WebSocketError};

match S9NonBlockingWebSocketClient::connect("wss://invalid-uri") {
    Ok(client) => { /* use client */ },
    Err(S9WebSocketError::WebSocket(WebSocketError::InvalidUri(msg))) => {
        eprintln!("Invalid URI: {}", msg);
    },
    Err(S9WebSocketError::WebSocket(WebSocketError::Io(io_err))) => {
        eprintln!("Network error: {}", io_err);
    },
    Err(e) => {
        eprintln!("Connection failed: {}", e);
    }
}
```

### Runtime Errors from S9AsyncNonBlockingWebSocketClient
```rust
// Handle errors from client's event loop
match client.event_rx.recv() {
    Ok(WebSocketEvent::Error(err)) => {
        eprintln!("WebSocket error: {}", err);
        // Error string contains context about which operation failed
    },
    Ok(WebSocketEvent::ConnectionClosed(reason)) => {
        println!("Connection closed: {:?}", reason);
    },
    _ => {}
}
```

### Control Channel Errors
```rust
use s9_websocket::ControlMessage;

// Sending messages can fail if the event loop has stopped
if let Err(e) = client.control_tx.send(ControlMessage::SendText("Hello".to_string())) {
    eprintln!("Failed to send control message: {}", e);
}
```

## Performance Tips
1. **Choose the right mode**:
    - Use non-blocking mode for low-latency applications
2. **Tune spin wait duration**:
   - `None`: Best latency, highest CPU usage
   - `Some(Duration::from_millis(1-10))`: Good balance for up to 100 message/sec
   - `Some(Duration::from_millis(50-100))`: Lower CPU, higher latency, less throughput
3. **Tune no_delay**:
   - `None`: use OS default
   - `Some(true)`: socket write operations are immediately executed
   - `Some(false)`: socket write operations are scheduled by OS


## Scalability Constraints

**None of the clients scale to thousands of connections.**

The library's architecture requires one OS thread per connection because each client's `run()` function either
spawns a dedicated thread (async client) or blocks the caller's thread indefinitely (non-blocking and blocking client).
There is no I/O multiplexing support to run multiple connections on a single thread.
For 1000+ connections, use async/await libraries like [tokio-tungstenite](https://docs.rs/tokio-tungstenite) or [async-tungstenite](https://docs.rs/async-tungstenite) that provide
true async I/O multiplexing.

## API documentation
No API and code documentation is included in this project yet.

## Testing
No tests are included in this project yet.

## Contributing
Contributions are welcome!
Please feel free to submit bugs and make feature requests [here](https://github.com/AlexSilver9/s9_websocket/issues)
Further information like Coding Conventions are currently maintained in [CLAUDE.md](CLAUDE.md)  

## License
This project is licensed under the APACHE / MIT License - see the LICENSE files for details.

## Project URL
Project source code is online available at https://github.com/AlexSilver9/s9_websocket.

## Authors
[Alexander Silvennoinen](https://www.linkedin.com/in/alexander-silvennoinen/)

## Acknowledgments
 Built on top of:
 - [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite)
 - [crossbeam-channel](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html)