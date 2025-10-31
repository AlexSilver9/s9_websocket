# Silver9 WebSocket

[![Crates.io](https://img.shields.io/crates/v/s9_websocket.svg)](https://crates.io/crates/s9_websocket)
[![Documentation](https://docs.rs/s9_websocket/badge.svg)](https://docs.rs/s9_websocket/latest/s9_websocket)
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

## Design Principles
- 🕑 **Predictable latency** - Direct system calls and callbacks, means lower baseline latency
- 📋 **Low overhead** - No task scheduling, futures, or waker infrastructure
- 🤯 **Simple model** - Straightforward usage
- 📐 **Deterministic behavior** - Optional custom spin-wait duration gives precise control over CPU/latency tradeoff
- 🚫️ **No runtime** - No async runtime or framework overhead, no `poll()`/`epoll_wait()`/`kevent()`
- 🔁 **Async message passing** - Optional message passing through channels
- ⛔️ **Graceful Shutdown** - Process all messages util connection is closed by server on graceful shutdown
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

1. **S9NonBlockingWebSocketClient** - Runs on your thread, receive events via callbacks (lowest latency)
2. **S9BlockingWebSocketClient** - Runs on your thread, receive events via callbacks
3. **S9AsyncNonBlockingWebSocketClient** - Spawns background thread, receive events via channels (easiest usage)

### Non-blocking Client (with handler callbacks)

Pure non-blocking client that runs on caller's thread using handler callbacks for events.

**Socket Mode:** Uses non-blocking socket I/O (`set_nonblocking(true)`) internally. The name "NonBlocking" refers to the socket I/O mode, not the behavior of `run()` which blocks the calling thread indefinitely.

**Flushing**: All send methods flush immediately after write

#### Key Features
- Runs entirely on caller's thread
- Receive events through handler callbacks (zero copy, zero allocation)
- Call client methods directly from handler callbacks (send, close, force_quit)
- Lowest possible latency
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

```rust
use s9_websocket::{S9NonBlockingWebSocketClient, S9WebSocketClientHandler, NonBlockingOptions};
use std::time::Duration;

// Implement the handler trait
struct MyHandler {
   message_count: usize,
}

impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for MyHandler {
   // Only override the methods you care about
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

Synchronous client that runs on caller's thread using handler callbacks for events.

**Socket Mode:** Uses blocking socket I/O (standard socket reads/writes) internally, with optional timeouts. The name "Blocking" refers to the socket I/O mode, not the behavior of `run()` which blocks the calling thread indefinitely (same as S9NonBlockingWebSocketClient).

**Flushing**: All send methods flush immediately after write

#### Key Features

- Runs entirely on caller's thread
- Receive events through handler callbacks (zero copy, zero allocation)
- Call client methods directly from handler callbacks (send, close, force_quit)
- Optional read/write timeouts for responsive control message handling (simulates non-blocking behavior)
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

```rust
use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler, BlockingOptions};

// Implement the handler trait
struct MyHandler {
   message_count: usize,
}

impl S9WebSocketClientHandler<S9BlockingWebSocketClient> for MyHandler {
   // Only override the methods you care about
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
   // Handler can call client methods directly from callbacks
   client.run(&mut handler);

   Ok(())
}
```

### Async Non-blocking Client (with channels)

Spawns a background thread for socket operations and communicates via channels.

**Socket Mode:** Uses non-blocking socket I/O (`set_nonblocking(true)`) internally. The name "NonBlocking" refers to the socket I/O mode, not the behavior of `run()` which returns immediately after spawning the thread.

**Flushing**: All send methods flush immediately after write

#### Key Features
- Background thread handles all socket operations
- Receive events through built-in channels (`event_rx`)
- Send commands through built-in channels (`control_tx`)
- Thread-safe for multi-threaded applications
- Configurable CPU/latency trade-off via spin-wait duration
- Configurable socket options like TCP_NODELAY, TTL, etc

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

   // Send a message via control channel
   client.control_tx.send(ControlMessage::SendText("Hello, WebSocket!".to_string()))?;

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

### Accessing the Underlying Socket

All clients provide low-level access to the underlying tungstenite WebSocket for advanced use cases:

```rust
use s9_websocket::{S9NonBlockingWebSocketClient, NonBlockingOptions};

let options = NonBlockingOptions::new();
let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

// Get immutable reference to the socket
let socket = client.get_socket();

// Get mutable reference to the socket for advanced operations
let socket_mut = client.get_socket_mut();
```

**Note for S9AsyncNonBlockingWebSocketClient**: Socket access returns `Option<&WebSocket>` because the socket is moved to event loop thread after calling `run()`. Socket access is only available before `run()` is called.

**Use with caution**: Direct manipulation of the underlying socket may interfere with the client's operation.

## Logging
The library uses the 'tracing' crate for logging. Enable logging in your application:
use tracing_subscriber;

```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();
```

## Performance Tips
1. **Choose the right client**:
    - Use non-blocking client for low-latency applications
    - Use async client for easiest embedding in multithreaded applications 
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

The library's architecture requires one OS thread per connection because each client's `run()` method either
spawns a dedicated thread (async client) or blocks the caller's thread indefinitely (non-blocking and blocking client).
There is no I/O multiplexing support to run multiple connections on a single thread.
For 1000+ connections, use async/await libraries like [tokio-tungstenite](https://docs.rs/tokio-tungstenite) or [async-tungstenite](https://docs.rs/async-tungstenite) that provide
true async I/O multiplexing.

## API documentation
Full API documentation is available at https://docs.rs/s9_websocket/latest/s9_websocket.

## Contributing
Contributions are welcome!
Please feel free to submit bugs and make feature requests [here](https://github.com/AlexSilver9/s9_websocket/issues)
Further information like coding conventions are currently maintained in [CLAUDE.md](CLAUDE.md)  

## License
This project is licensed under the APACHE / MIT License - see the LICENSE files for details.

## Project URL
Project source code is available at https://github.com/AlexSilver9/s9_websocket.

## Authors
[Alexander Silvennoinen](https://www.linkedin.com/in/alexander-silvennoinen/)

## Acknowledgments
 Built on top of:
 - [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite)
 - [crossbeam-channel](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html)
