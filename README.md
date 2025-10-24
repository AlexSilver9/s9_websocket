# Silver9 WebSocket

A high-performance Rust WebSocket client library providing both blocking and non-blocking implementations.

## Features

- 🚀 **Non-blocking and blocking modes** - Choose the right approach for your use case
- ⚡ **Low latency** - TCP_NODELAY enabled by default for Non-blocking
- 🔒 **Multiple TLS backends** - Support for both native-tls and rustls
- 📡 **Event-driven architecture** - Clean separation of concerns with channels
- 🎯 **Type-safe API** - Leverage Rust's type system for correctness
- 📊 **Built-in tracing** - Comprehensive logging support

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
s9_websocket = "0.0.1"
```

## TLS Backend Selection
By default, the library uses native-tls. To use rustls instead:
[dependencies]
s9_websocket = { version = "0.0.1", default-features = false, features = ["rustls"] }

## Quick Start

### Non-blocking Client
```rust
use s9_websocket::{S9NonBlockingWebSocketClient, WebSocketEvent, ControlMessage, NonBlockingOptions};
use std::time::Duration;

// Connect to WebSocket server
let mut client = S9NonBlockingWebSocketClient::connect("wss://example.com/ws")?;

// Configure non-blocking options, duration of None means full cpu power busy spin loop
let options = NonBlockingOptions::new(Some(Duration::from_millis(10)))?;

// Start the event loop in a separate thread
client.run_non_blocking(options)?;

// Send a message
client.send_text_message("Hello, WebSocket!")?;

// Send another message from another thread and close the connection
let tx = client.control_tx.clone();
std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(10));

    // Send a message via control channel 
    tx.send(ControlMessage::SendText("I'll close in 5 sec!".to_string())).ok();

    std::thread::sleep(Duration::from_millis(5));

    // Optionally close connection - gracefull close is implemented on Drop
    tx.send(ControlMessage::Close())?;
});


// Handle events
loop {
    match client.event_rx.recv() {
        Ok(WebSocketEvent::Activated) => {
            println!("WebSocket connection activated");
        },
        Ok(WebSocketEvent::TextMessage(data)) => {
            let text = String::from_utf8_lossy(&data);
            println!("Received: {}", text);
        },
        Ok(WebSocketEvent::BinaryMessage(data)) => {
            println!("Received binary data: {} bytes", data.len());
        },
        Ok(WebSocketEvent::ConnectionClosed(reason)) => {
            println!("Connection closed: {:?}", reason);
            // No need to beak as client sends another Quit message
        },
        Ok(WebSocketEvent::Error(err)) => {
            eprintln!("Error: {}", err);
        },
        Ok(WebSocketEvent::Quit) => {
            println!("Client quit");
            break;
        },
        _ => {}
    }
}
```

### Blocking Client

**GOTCHA**: For now the Blocking client blocks on socket read infinitly. See Limitations.

```rust
use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler};
use crossbeam_channel::unbounded;

// Implement the handler trait
struct MyHandler;

impl S9WebSocketClientHandler for MyHandler {
    fn on_text_message(&mut self, data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        println!("Received: {}", text);
    }

    fn on_binary_message(&mut self, data: &[u8]) {
        println!("Received binary: {} bytes", data.len());
    }

    fn on_connection_closed(&mut self, reason: Option<String>) {
        println!("Connection closed: {:?}", reason);
    }

    fn on_error(&mut self, error: String) {
        eprintln!("Error: {}", error);
    }
}

// Connect and run
let mut client = S9BlockingWebSocketClient::connect("wss://example.com/ws")?;
let mut handler = MyHandler;
let (control_tx, control_rx) = unbounded();

// Send a message from another thread
let tx = control_tx.clone();
std::thread::spawn(move || {
    tx.send(ControlMessage::SendText("Hello!".to_string())).ok();
});

// Run the blocking event loop
client.run_blocking(&mut handler, control_rx);
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
impl S9WebSocketClientHandler for MyHandler {
    fn on_ping(&mut self, data: &[u8]) {
        println!("Received ping: {:?}", data);
        // Pong is automatically sent by the underlying library
    }

    fn on_pong(&mut self, data: &[u8]) {
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

### S9NonBlockingWebSocketClient

#### Key Features
- Event-based communication via channels
- Separate reader thread for socket operations
- Configurable spin-wait duration to reduce CPU usage
- Control messages for sending data and managing connection
- Suitable for high-performance applications

#### Methods
- `connect(uri: &str) -> Result<Self, Error>` - Connect to WebSocket server
- `connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> Result<Self, Error>` - Connect with custom headers
- `run_non_blocking(options: NonBlockingOptions) -> Result<(), Box<dyn std::error::Error>>` - Start the event loop
- `send_text_message(s: &str) -> Result<(), SendError<ControlMessage>>` - Send text message

#### Fields
- `control_tx: Sender<ControlMessage>` - Send control messages to the client
- `event_rx: Receiver<WebSocketEvent>` - Receive events from the client

#### WebSocketEvent
```rust
pub enum WebSocketEvent {
    Activated,                         // The socket and control channel poll loop is entered after this message
    TextMessage(Vec<u8>),              // Text message received
    BinaryMessage(Vec<u8>),            // Binary message received
    Ping(Vec<u8>),                     // Ping frame received
    Pong(Vec<u8>),                     // Pong frame received
    ConnectionClosed(Option<String>),  // Connection closing - received a Close Frame
    Error(String),                     // Error occurred
    Quit,                              // Client quitting
}
```


### S9BlockingWebSocketClient

#### Key Features
- Simple synchronous API
- Blocking socket read means, message send and control message will only be executed after at least a WebSocket Frame got read
- Handler trait for event callbacks
- Direct control flow
- Suitable for simple use cases or when blocking is acceptable

#### Limitations
- For now the Blocking client blocks on socket read infinitly.
That means that send messages and processing control messages will only be performed after a WebSocket Frame got read.
This is target of future improvement, by adding support for a read timeout.

#### Methods
- `connect(uri: &str) -> Result<Self, Error>` - Connect to WebSocket server
- `connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> Result<Self, Error>` - Connect with custom headers
- `run_blocking<HANDLER>(handler: &mut HANDLER, control_rx: Receiver<ControlMessage>)` - Run blocking event loop
- `send_text_message(s: &str) -> Result<(), Error>` - Send text message
- `send_text_pong_for_text_ping(ping_message: &str) -> Result<(), Error>` - Send pong response for text ping

#### Callback
```rust
pub trait S9WebSocketClientHandler {
   fn on_text_message(&mut self, data: &[u8]);                  // Text message received
   fn on_binary_message(&mut self, data: &[u8]);                // Binary message received
   fn on_connection_closed(&mut self, reason: Option<String>);  // Connection closing - received a Close Frame
   fn on_error(&mut self, error: String);                       // Error occurred
   fn on_ping(&mut self, _data: &[u8]) {                        // Ping frame received
      // Default: noop
   }
   fn on_pong(&mut self, _data: &[u8]) {                        // Pong frame received
      // Default: noop
   }
   fn on_quit(&mut self) {                                      // Client quitting
      // Default: noop
   }
}
```

### ControlMessage
```rust
pub enum ControlMessage {
    SendText(String),  // Send text message
    Close(),           // Close connection gracefully - sends a Close Frame
    ForceQuit(),       // Force quit - immediately break the socket and control channel poll loop
}
```

### Close and Quit
Graceful close is implemented by `Drop` trait. Graceful close means a Close Frame is sent to the server.
Whenever a Close frame is received from the server
- **non-blocking**: a `WebSocketEvent::ConnectionClosed` is published, followed by a `WebSocketEvent::Quit` event before breaking the socket and control channel poll loop
- **blocking**: the `S9WebSocketClientHandler::on_connection_closed()` callback is invoked, followed by a `S9WebSocketClientHandler::on_quit()` call before breaking the socket and control channel poll loop

### Force Quit
Send a 'ControlMessage::ForceQuit' to immediatelly break the socket and control channel poll loop

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
The library uses `tungstenite::Error` for WebSocket-related errors. Common error scenarios:
- Connection failures: Invalid URI, network issues, TLS errors
- Protocol errors: Invalid WebSocket frames, handshake failures
- I/O errors: Network interruptions, timeouts
```rust
match client.connect("wss://example.com/ws") {
    Ok(client) => { /* use client */ },
    Err(e) => {
        eprintln!("Failed to connect: {}", e);
        // Handle error appropriately
    }
}
```

## Performance Tips
1. **Choose the right mode**:
    - Use blocking mode for simple applications or when you need direct control flow
    - Use non-blocking mode for high-performance applications
2. **Tune spin wait duration**:
   - `None`: Best latency, highest CPU usage
   - `Some(Duration::from_millis(1-10))`: Good balance
   - `Some(Duration::from_millis(50-100))`: Lower CPU, higher latency
3. **Connection pooling**: For multiple connections, create separate client instances

## Thread Safety
- **Non-blocking client**: Thread-safe via channels, can be shared across threads
- **Blocking client**: Not thread-safe, use from a single thread or wrap in Arc<Mutex<>>

## API documentation
No API and code documentation is included in this project yet. This is a target of future improvement.

## Testing
No tests are included in this project yet. This is a target of future improvement.

## Contributing
Contributions are welcome!
Please feel free to submit bugs and make feature requests [here](https://github.com/AlexSilver9/s9_websocket/issues)

## License
This project is licensed under the APACHE and MIT License - see the LICENSE files for details.

## Project URL
Please find the project source code at https://github.com/AlexSilver9/s9_websocket.

## Authors
Alexander Silvennoinen

## Acknowledgments
Built on top of the tungstenite-rs WebSocket library.