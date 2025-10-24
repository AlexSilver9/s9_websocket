# Silver9 WebSocket Project - AI Assistant Guide

## Project Overview
s9_websocket is a lightweight library implementation for blocking and non-blocking stream-based WebSocket client.

This is a Rust WebSocket client library that provides both blocking and non-blocking implementations for WebSocket communication.
It's built on top of the [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite) WebSocket library
and uses [crossbeam-channel](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html) for thread-safe message passing.

## Architecture
The **S9NonBlockingWebSocketClient** non-blocking implementation is based on a thread with tight loop reading messages from a websocket and publishing
them through [crossbeam-channels](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html).

The **S9BlockingWebSocketClient** blocking implementation is runs on the caller's thread and provides websocket messages through a callback.

### Core Components

1. **S9NonBlockingWebSocketClient**: Asynchronous WebSocket client using threads and channels
2  **S9BlockingWebSocketClient**: Synchronous WebSocket client that blocks on read operations
3. **S9WebSocketClientHandler**: Trait for handling WebSocket events in blocking mode
4. **WebSocketEvent**: Enum representing all possible WebSocket events
5. **ControlMessage**: Enum for controlling the WebSocket connection

### Design Patterns

- **Event-driven architecture**: Non-blocking client uses channels to communicate events
- **Thread-based concurrency**: Non-blocking client spawns separate thread(s) for socket reading and event processing
- **Arc<Mutex<>>**: Non-blocking client shared socket access between reader and writer threads
- **Handler pattern**: Blocking client uses trait-based callbacks

## Key Features

### Non-blocking Client
- Event-based communication via channels
- Separate reader thread for socket operations
- Configurable spin-wait duration to reduce CPU usage
- Control messages for sending data and managing connection
- Suitable for high-performance applications

### Blocking Client
- Simple synchronous API
- Blocking socket read means, message send and control message will only be executed after at least a WebSocket Frame got read 
- Handler trait for event callbacks
- Direct control flow
- Suitable for simple use cases or when blocking is acceptable

## Configuration

### TLS Support
The library for now only supports follwing TLS backend via Cargo features:
- `native-tls` (default): Uses platform's native TLS

### Non-blocking Options
```rust
NonBlockingOptions {
    spin_wait_duration: Option<Duration>  // Sleep duration between read attempts
}
```

## Common Patterns

### Connecting with Custom Headers
```rust
let mut headers = HashMap::new();
headers.insert("Authorization".to_string(), "Bearer token".to_string());
let client = S9NonBlockingWebSocketClient::connect_with_headers(uri, &headers)?;
```

### Handling Events (Non-blocking)
```rust
loop {
    match client.event_rx.recv() {
        Ok(WebSocketEvent::TextMessage(data)) => { /* handle */ },
        Ok(WebSocketEvent::ConnectionClosed(_)) => break,
        // ... other events
    }
}
```

### Implementing Handler (Blocking)
```rust
struct MyHandler;
impl S9WebSocketClientHandler for MyHandler {
    fn on_text_message(&mut self, data: &[u8]) { /* handle */ }
    fn on_binary_message(&mut self, data: &[u8]) { /* handle */ }
    fn on_connection_closed(&mut self, reason: Option<String>) { /* handle */ }
    fn on_error(&mut self, error: String) { /* handle */ }
}
```

## Error Handling
Currently uses `tungstenite::Error` directly. There's a TODO to implement a custom error type for better error handling and consistency.

## Tracing
The library uses the `tracing` crate for logging at different levels:
- **TRACE**: Detailed message content, connection details
- **DEBUG**: Connection lifecycle events
- **ERROR**: Error conditions

## Thread Safety
- Non-blocking client spawns two threads:
  1. **Reader thread**: Continuously reads from socket
  2. **Event loop thread**: Processes control messages and socket write events
- Channels are used for all cross-thread communication
- Socket is protected by Arc<Mutex<>> for shared access

## Performance Considerations
1. **Non-blocking Mode**: Set `spin_wait_duration` to balance CPU usage vs latency
   - `None`: Maximum performance, high CPU usage
   - `Some(Duration)`: Lower CPU usage, slight latency increase
2. **TCP Settings**: Both clients set TCP_NODELAY for lower latency
3. **Message Handling**: Zero-copy where possible, but some allocations for Vec<u8> conversions

## Testing Recommendations
When testing or using this library:
1. **Connection Testing**: Test with both valid and invalid URIs
2. **Reconnection**: Library doesn't auto-reconnect, implement in application layer
3. **Graceful Shutdown**: Always send ControlMessage::Close() before dropping
4. **Error Scenarios**: Test network interruptions, server disconnects
5. **Message Ordering**: Events are delivered in order received


## Dependencies
`tungstenite`: WebSocket protocol implementation
`crossbeam-channel`: Lock-free channels for thread communication
`tracing`: Structured logging

For Secure WebSockets the TLS features currently only native-tls supported. 

## Known Limitations & TODOs
1. **Custom Error Type**: Need unified error handling (see TODO in code)
2. **Socket Unthreading**: Would prefer to e.g. split socket into read/write halves or unthread read instead of using Arc<Mutex<>>
3. **Blocking Timeout**: Blocking socket should support optional timeout. For now message send and control message will only be processed after at least a WebSocket Frame got read.
4. **Tests**: Currently no tests are written
5. **API Documentation**: Currently no API documentation is written
6. **Code Documentation**: Currently no code documentation is written

## Future Improvements
1. Implement custom error type for better error handling
2. Add support for socket read timeout for blocking socket
2. Add support for rustls
3. Add tests
4. Add API documentation
5. Add code documentation
6. Add metrics/statistics collection
7. Add benchmarks, e.g. blocking vs. non-blocking

## Known Issues & Gotchas
- None

## License
This project is licensed under the APACHE and MIT License - see the LICENSE files for details.

## Project URL
Project source code is at https://github.com/AlexSilver9/s9_websocket.

## Authors
Alexander Silvennoinen