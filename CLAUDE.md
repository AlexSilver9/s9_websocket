# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

See @README for project overview and user-facing documentation.

## Quick Reference

### Most Important Files
- `src/websocket/` - WebSocket client module (refactored into separate files)
  - `nonblocking_client.rs` - S9NonBlockingWebSocketClient implementation
  - `blocking_client.rs` - S9BlockingWebSocketClient implementation
  - `async_client.rs` - S9AsyncNonBlockingWebSocketClient implementation
  - `types.rs` - Public API types (traits, enums, macros)
  - `options.rs` - Configuration options
  - `shared.rs` - Shared internal helpers
  - `mod.rs` - Module declarations and re-exports
- `src/error.rs` - Error types
- `src/lib.rs` - Public API exports
- `examples/` - Usage examples for each client type

## Development Commands
```bash
# Build and check
cargo build
cargo check

# Run examples (demonstrates all three client types)
cargo run --example echo_client_non_blocking        # S9NonBlockingWebSocketClient (caller thread, handler)
cargo run --example echo_client_blocking            # S9BlockingWebSocketClient (caller thread, handler)
cargo run --example echo_client_blocking_timeout    # S9BlockingWebSocketClient with timeout (caller thread, handler)
cargo run --example echo_client_non_blocking_async  # S9AsyncNonBlockingWebSocketClient (spawns thread, channels)
```

## Project Overview

s9_websocket is a lightweight, low-latency Rust WebSocket client library providing three distinct implementations: async/threaded with channels, non-blocking with callbacks, and blocking with callbacks. Built on top of [tungstenite-rs](https://docs.rs/tungstenite/latest/tungstenite) and [crossbeam-channel](https://docs.rs/crossbeam/latest/crossbeam/channel/index.html).

## Architecture

### Three Client Implementations

The library provides three distinct client implementations, each optimized for different use cases:

| Feature          | S9NonBlockingWebSocketClient    | S9BlockingWebSocketClient                        | S9AsyncNonBlockingWebSocketClient |
|------------------|---------------------------------|--------------------------------------------------|-----------------------------------|
| Threading        | Caller's thread                 | Caller's thread                                  | Spawns thread                     |
| Socket Mode      | Non-blocking                    | Blocking with optional timeout                   | Non-blocking                      |
| Event Delivery   | Handler callbacks               | Handler callbacks                                | Channels (`event_rx`)             |
| Control Messages | Direct function calls on client | Direct function calls on client                  | Built-in (`control_tx`)           |
| CPU Usage        | Configurable via spin_wait      | Low (blocks on read / write)                     | Configurable via spin_wait        |
| Use Case         | Single-thread non-blocking      | Simple blocking apps (non-blocking when timeout) | Multi-threaded async apps         |

#### S9NonBlockingWebSocketClient
The "pure" non-blocking client with handler callbacks (zero overhead):
- **Threading model**: Runs entirely on caller's thread (no thread spawning)
- **Communication**: Uses handler trait (`S9WebSocketClientHandler<Self>`) for direct callbacks
  - Handler receives `&mut self` as a parameter to each callback function
  - Can call `send_text_message()`, `send_binary_message()`, `send_ping()`, `send_pong()`, `close()`, `force_quit()` directly from handler callbacks
- **Socket mode**: Non-blocking socket with `set_nonblocking(true)`
- **Performance tuning**: Same `NonBlockingOptions::spin_wait_duration` as async client
- **TCP optimization**: Same `NonBlockingOptions::nodelay` as async client
- **Use case**: Zero-overhead version for processing incoming messages with direct callbacks on caller's thread

#### S9BlockingWebSocketClient
The synchronous blocking client:
- **Threading model**: Runs entirely on caller's thread
- **Communication**: Uses handler trait (`S9WebSocketClientHandler<Self>`) for direct callbacks
  - Handler receives `&mut self` as a parameter to each callback function
  - Can call `send_text_message()`, `send_binary_message()`, `send_ping()`, `send_pong()`, `close()`, `force_quit()` directly from handler callbacks
- **Socket mode**: Blocking socket reads (can be configured with timeout via `BlockingOptions` to simulate non-blocking behavior)
- **Performance tuning**: `BlockingOptions::spin_wait_duration` controls CPU/latency tradeoff with same options as async client
- **TCP optimization**: Configurable `TCP_NODELAY` for lower latency on socket write
- **Timeout support**: `BlockingOptions::read_timeout` and `write_timeout` for configurable blocking behavior
- **Use case**: Simple synchronous applications where blocking is acceptable

#### S9AsyncNonBlockingWebSocketClient
The async/threaded client with channel-based event delivery:
- **Threading model**: Spawns a dedicated thread via `run()` that returns `JoinHandle<()>`
- **Socket ownership**: Socket is moved into the spawned thread
- **Communication**: Uses `crossbeam-channel` for bidirectional communication:
  - `control_tx` (Sender) → Send commands (SendText, Close, ForceQuit) to the client thread
  - `event_rx` (Receiver) → Receive events (TextMessage, BinaryMessage, ConnectionClosed, etc.) from the client thread
- **Socket mode**: Non-blocking socket with `set_nonblocking(true)`
- **Performance tuning**: `NonBlockingOptions::spin_wait_duration` controls CPU/latency tradeoff
  - `None`: Maximum performance, 100% CPU usage (busy spin loop)
  - `Some(Duration)`: Sleeps between reads, lower CPU usage, predictable latency increase
- **TCP optimization**: Configurable `TCP_NODELAY` for lower latency on socket write
- **Use case**: Best for applications that need async event processing with channels

### Error Handling Architecture
The library uses a single unified error type:
- `S9WebSocketError` - Encompasses all WebSocket operation errors including:
  - `WebSocket(WebSocketError)` - Connection, I/O, protocol errors
  - `InvalidUri(String)` - Invalid URI provided
  - `ConnectionClosed(Option<String>)` - Connection closed with optional reason
  - `SocketUnavailable` - Socket already moved to thread
  - `InvalidConfiguration(String)` - Invalid configuration
  - `Io(std::io::Error)` - I/O errors
  - `Tungstenite(TungsteniteError)` - Underlying tungstenite errors

Errors are exposed via:
- **Non-blocking**: `WebSocketEvent::Error(String)` through `event_rx` channel
- **Blocking**: `S9WebSocketClientHandler::on_error(String)` callback
- **Result types**: All public API functions return `S9Result<T>` (alias for `Result<T, S9WebSocketError>`)

### Connection Lifecycle
All clients follow a similar lifecycle:
1. **Connect** - Establish WebSocket connection (with optional custom headers)
2. **Run** - Enter event loop (non-blocking spawns thread, blocking runs on caller thread)
3. **Active** - Start process messages and control commands (before entering event loop)
4. **Close** - Graceful shutdown (sends Close frame, waits for server acknowledgment)
5. **Quit** - Cleanup and exit (triggered by ConnectionClosed event or ForceQuit)

**Graceful shutdown**: Implemented via `Drop` trait - automatically sends Close frame when client is dropped.

### Thread Safety and Performance
- **S9AsyncNonBlockingWebSocketClient**: Thread-safe via channels, spawns one thread per connection
- **S9NonBlockingWebSocketClient**: Not thread-safe, runs on caller's thread
- **S9BlockingWebSocketClient**: Not thread-safe, runs on caller's thread
- **Channels**: All cross-thread communication uses `crossbeam-channel` (lock-free)
- **Scaling limitation**: Does not scale to thousands of connections (see Scalability Constraints below)

### Memory Allocation Patterns

Each client has different memory allocation characteristics based on their architecture:

#### S9AsyncNonBlockingWebSocketClient (Most Allocations)
**Channel Infrastructure:**
- `unbounded()` creates heap-allocated channel buffers for `control_tx`/`control_rx` and `event_tx`/`event_rx`
- Channels persist for the lifetime of the client

**Message Handling (Per Message):**
- `WebSocketEvent::TextMessage`: Allocates `Vec<u8>`
- `WebSocketEvent::BinaryMessage`: Allocates `Vec<u8>`
- `WebSocketEvent::Ping`: Allocates `Vec<u8>``
- `WebSocketEvent::Pong`: Allocates `Vec<u8>`

**Rationale:** Messages must be cloned from the tungstenite types to owned `Vec<u8>` for safe cross-thread transfer through channels. This ensures thread safety but comes with allocation overhead per message.

#### S9NonBlockingWebSocketClient (Zero Allocations)
**Zero-Copy Message Delivery:**
- Handler callbacks receive `&[u8]` slice references directly from tungstenite messages
- Handler receives `&mut` client reference to call functions directly (no channel overhead)

**Direct Function Calls:**
- Direct function calls from handler callbacks

**Rationale:** Single-threaded execution allows zero-copy message delivery via slice references. This is the most memory-efficient implementation.

#### S9BlockingWebSocketClient (Zero Allocations)
**Zero-Copy Message Delivery:**
- Handler callbacks receive `&[u8]` slice references directly from tungstenite messages
- Handler receives `&mut` client reference to call functions directly (no channel overhead)

**Direct Function Calls:**
- Direct function calls from handler callbacks

**Rationale:** Single-threaded execution allows zero-copy message delivery via slice references. This is the most memory-efficient implementation.

#### Error Path Allocations

- All clients may be allocated error strings on error paths  (e.g., `format!("Error reading message: {}", e)`)

### Memory Efficiency Summary
1. **Most efficient**: S9NonBlockingWebSocketClient and S9BlockingWebSocketClient (zero-copy message delivery)
2. **Less efficient**: S9AsyncNonBlockingWebSocketClient (Vec allocations per message for thread-safe channel transfer)
3. **Trade-off**: The async client sacrifices memory efficiency for thread-safety and async event processing

### Scalability Constraints

**None of the clients scale to thousands of connections.**

The library's architecture requires one OS thread per connection because each client's `run()` function either spawns a dedicated thread or blocks the caller's thread indefinitely.
There is no I/O multiplexing support to run multiple connections on a single thread.

## Code Modules and Key Types

### Module Structure
The websocket module is organized into separate files:
- `src/websocket/types.rs` - Public API types (traits, enums, macros)
- `src/websocket/options.rs` - Configuration options (NonBlockingOptions, BlockingOptions)
- `src/websocket/shared.rs` - Shared internal helpers (connection, message sending, tracing)
- `src/websocket/nonblocking_client.rs` - S9NonBlockingWebSocketClient implementation
- `src/websocket/blocking_client.rs` - S9BlockingWebSocketClient implementation
- `src/websocket/async_client.rs` - S9AsyncNonBlockingWebSocketClient implementation
- `src/websocket/mod.rs` - Module declarations and public re-exports

### Public API Types
- `S9NonBlockingWebSocketClient` - Non-blocking client with handler callbacks (caller's thread)
- `S9BlockingWebSocketClient` - Blocking client with handler callbacks
- `S9AsyncNonBlockingWebSocketClient` - Async/threaded client with channels (spawns thread)
- `S9WebSocketClientHandler<C>` - Trait for handler-based client callbacks (generic over client type)
  - **All methods have default no-op implementations - only implement what you need!**
  - `on_activated()` - Called once before entering the event loop
  - `on_poll()` - Called every loop iteration before socket read (highest priority)
  - `on_idle()` - Called only when no data available - WouldBlock/TimedOut (lower priority)
  - `on_text_message()` - Text message received
  - `on_binary_message()` - Binary message received
  - `on_ping()` - Ping frame received
  - `on_pong()` - Pong frame received
  - `on_connection_closed()` - Connection closed
  - `on_error()` - Error occurred
  - `on_quit()` - Called once when event loop is about to break
- `WebSocketEvent` - Event enum for async client channel communication
- `ControlMessage` - Control enum for managing connections (async client only via channels)
  - `SendText(String)` - Send text message
  - `SendBinary(Vec<u8>)` - Send binary message
  - `SendPing(Vec<u8>)` - Send ping frame
  - `SendPong(Vec<u8>)` - Send pong frame
  - `Close()` - Graceful close (sends CloseFrame)
  - `ForceQuit()` - Immediate shutdown
- `NonBlockingOptions` - Configuration for async and non-blocking clients
- `BlockingOptions` - Configuration for blocking client (with timeout support)

### Error Types (in `src/error.rs`)
- `S9WebSocketError` - Library- and WebSocket-specific errors
- `S9Result<T>` - Convenience type alias

## Coding Conventions

### Error Handling
- Use `S9Result<T>` return type for all fallible operations
- Log errors with `tracing::error!` macro at error sites
- Expose errors to callers via Result types or channels (non-blocking) / callbacks (blocking)

### Logging
- Use `tracing` crate for all logging
- Check log level with `tracing::enabled!` before logging at levels lower than ERROR
- Log levels:
  - `TRACE`: Detailed message content, connection details
  - `DEBUG`: Connection lifecycle events
  - `ERROR`: Error conditions

### Channel Communication (async non-blocking)
- Use provided macros for consistent error handling:
  - `send_or_break!` - Send event or break loop on error
  - `send_or_log!` - Send event or log on error (continue execution)

## Git Commit Conventions
Follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):
- Format: `<type>[optional scope]: <description>`
- Breaking changes: `<type>[optional scope]!: <description>`
- Types that generate CHANGELOG entries:
  - `feat` - New features
  - `fix` - Bug fixes
  - `doc` - Documentation
  - `perf` - Performance improvements
  - `refactor` - Code refactoring
  - `style` - Styling/formatting
  - `test` - Tests
  - `chore` - Maintenance (except `chore(release|deps.*|pr|pull)`)
  - `ci` - CI/CD
  - `*security` - Security fixes
  - `revert` - Reverts

See [cliff.toml](cliff.toml) for full configuration.

## Version and Releases Management
- **Versioning**: Follows [semver](https://semver.org)
- **CHANGELOG**: Maintained via [git-cliff](https://git-cliff.org/) and [cargo-release](https://crates.io/crates/cargo-release)
- **Release branch**: Only release from `main` branch
- **Release tool**: Uses `cargo-release` to publish to [crates.io](https://crates.io)

### Release Management
```bash
# Generate/update CHANGELOG using git-cliff
git cliff -o CHANGELOG.md

# Create a release (from main branch only)
cargo release <type>
```

## Known Limitations & Future Work
1. **TLS backends**: Only `native-tls` currently supported (`rustls` and maybe `wolfssl` planned)
2. **Testing**: No tests exist yet
3. **Documentation**: No comprehensive API and code documentation yet

## Project Information
- **License**: MIT / Apache-2.0
- **Repository**: https://github.com/AlexSilver9/s9_websocket
- **Author**: Alexander Silvennoinen
- **Minimum Rust Version**: 1.80.1
