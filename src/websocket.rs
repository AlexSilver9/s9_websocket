use std::collections::HashMap;
use std::net::TcpStream;
use std::{str, thread};
use std::str::FromStr;
use std::thread::JoinHandle;
use std::time::Duration;
use crossbeam_channel::{Receiver, Sender};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;
use crate::error::{S9Result, WebSocketError};

// TODO: Refactor clients and structs/enums to separate files
// TODO: Convinient functions for send_binry, pong, pong, etc...
// TODO: Provide access to underlysing streams
// TODO: Add Tests
// TODO: Add API Documentation + change documentation pointer in Cargo.toml to something like https://docs.rs/s9_websocket/0.0.1
// TODO: Add Code Documentation

// ============================================================================
// Macros
// ============================================================================

macro_rules! send_or_break {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("Failed to send context {} through channel: {}", $context, e);
            break;
        }
    };
}

macro_rules! send_or_log {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("Failed to send context {} through channel: {}", $context, e);
        }
    };
}

// ============================================================================
// Public API Types
// ============================================================================

pub trait S9WebSocketClientHandler {
    fn on_activated(&mut self) {
        // Default: noop
    }
    fn on_text_message(&mut self, data: &[u8]);
    fn on_binary_message(&mut self, data: &[u8]);
    fn on_connection_closed(&mut self, reason: Option<String>);
    fn on_ping(&mut self, _data: &[u8]) {
        // Default: noop
    }
    fn on_pong(&mut self, _data: &[u8]) {
        // Default: noop
    }
    fn on_error(&mut self, error: String);
    fn on_quit(&mut self) {
        // Default: noop
    }
}

#[derive(Debug)]
pub enum WebSocketEvent {
    Activated,
    TextMessage(Vec<u8>),
    BinaryMessage(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    ConnectionClosed(Option<String>),
    Error(String),
    Quit,
}

pub enum ControlMessage {
    SendText(String),
    Close(),
    ForceQuit(),
}

// ============================================================================
// Configuration options
// ============================================================================

#[derive(Debug, Clone, Default)]
struct SharedOptions {
    spin_wait_duration: Option<Duration>,
    nodelay: Option<bool>,
    ttl: Option<u32>,
}

/// Configuration options for the non-blocking WebSocket client.
#[derive(Debug, Clone, Default)]
pub struct NonBlockingOptions {
    shared: SharedOptions,
}

impl NonBlockingOptions {
    /// Creates a new `NonBlockingOptions` builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the duration to wait in the event loop after control message processing and socket reads.
    /// Must be None or greater than zero
    /// This prevents the event loop from consuming 100% CPU.
    pub fn spin_wait_duration(mut self, duration: Option<Duration>) -> S9Result<Self> {
        if let Some(duration) = duration {
            if duration.is_zero() {
                return Err(WebSocketError::InvalidConfiguration("Spin wait duration cannot be zero".to_string()).into());
            }
        }
        self.shared.spin_wait_duration = duration;
        Ok(self)
    }

    /// Enables or disables the `TCP_NODELAY` option for messages to be sent.
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.shared.nodelay = Some(nodelay);
        self
    }

    /// Sets the TTL (Time To Live, # of hops) for the socket.
    /// None for the system default
    pub fn ttl(mut self, ttl: Option<u32>) -> S9Result<Self> {
        self.shared.ttl = ttl;
        Ok(self)
    }
}

/// Configuration options for the blocking WebSocket client.
#[derive(Debug, Clone, Default)]
pub struct BlockingOptions {
    shared: SharedOptions,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl BlockingOptions {
    /// Creates a new `BlockingOptions` builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the duration to wait in the event loop after control message processing and socket reads.
    /// Must be None or greater than zero
    /// This prevents the event loop from consuming 100% CPU.
    pub fn spin_wait_duration(mut self, duration: Option<Duration>) -> S9Result<Self> {
        if let Some(duration) = duration {
            if duration.is_zero() {
                return Err(WebSocketError::InvalidConfiguration("Spin wait duration cannot be zero".to_string()).into());
            }
        }
        self.shared.spin_wait_duration = duration;
        Ok(self)
    }

    /// Enables or disables the `TCP_NODELAY` option for messages to be sent.
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.shared.nodelay = Some(nodelay);
        self
    }

    /// Sets the TTL (Time To Live, # of hops) for the socket.
    /// None for the system default
    pub fn ttl(mut self, ttl: Option<u32>) -> S9Result<Self> {
        self.shared.ttl = ttl;
        Ok(self)
    }

    /// Sets the read timeout for the socket.
    /// Must be None for the infinite blocking of socket read or greater than zero
    pub fn read_timeout(mut self, timeout: Option<Duration>) -> S9Result<Self> {
        if let Some(timeout) = timeout {
            if timeout.is_zero() {
                return Err(WebSocketError::InvalidConfiguration("Read timeout duration cannot be zero".to_string()).into());
            }
        }
        self.read_timeout = timeout;
        Ok(self)
    }

    /// Sets the write timeout for the socket.
    /// /// Must be None for the infinite blocking of socket write or greater than zero
    pub fn write_timeout(mut self, timeout: Option<Duration>) -> S9Result<Self> {
        if let Some(timeout) = timeout {
            if timeout.is_zero() {
                return Err(WebSocketError::InvalidConfiguration("Write timeout duration cannot be zero".to_string()).into());
            }
        }
        self.write_timeout = timeout;
        Ok(self)
    }
}

// ============================================================================
// Shared Internal Helpers
// ============================================================================

mod shared {
    use super::*;

    /// Control flow indicator for message handling loops
    pub(crate) enum ControlFlow {
        Continue,
        Break,
    }

    /// Establishes WebSocket connection with optional custom headers
    pub(crate) fn connect_socket(uri: &str, headers: &HashMap<String, String>) -> S9Result<(WebSocket<MaybeTlsStream<TcpStream>>, Response)> {
        let uri = Uri::from_str(uri).map_err(|e| {
            tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
            WebSocketError::InvalidUri(e.to_string())
        })?;

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let (sock, response) = tungstenite::connect(builder)?;
        trace_on_connected(&response);

        Ok((sock, response))
    }

    /// Configures socket for non-blocking operation with TCP_NODELAY
    pub(crate) fn configure_non_blocking(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, options: &NonBlockingOptions) -> S9Result<()> {
        let stream = match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::NativeTls(stream) => stream.get_mut(),
            // TODO: Add support for rustls
            _ => return Ok(()),
        };

        stream.set_nonblocking(true)?;

        if let Some(nodelay) = options.shared.nodelay {
            stream.set_nodelay(nodelay)?;
        }
        if let Some(ttl) = options.shared.ttl {
            stream.set_ttl(ttl)?;
        }

        Ok(())
    }

    /// Configures socket for blocking operation
    pub(crate) fn configure_blocking(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, options: &BlockingOptions) -> S9Result<()> {
        let stream = match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::NativeTls(stream) => stream.get_mut(),
            // TODO: Add support for rustls
            _ => return Ok(()),
        };

        if let Some(nodelay) = options.shared.nodelay {
            stream.set_nodelay(nodelay)?;
        }
        if let Some(ttl) = options.shared.ttl {
            stream.set_ttl(ttl)?;
        }
        stream.set_read_timeout(options.read_timeout)?;
        stream.set_write_timeout(options.write_timeout)?;

        Ok(())
    }

    /// Handles control messages for non-blocking clients
    #[inline]
    pub(crate) fn handle_control_message(control_msg: ControlMessage, socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Result<ControlFlow, String> {
        match control_msg {
            ControlMessage::SendText(text) => {
                if let Err(e) = send_text_message_to_websocket(socket, &text) {
                    return Err(format!("Error sending text: {}", e));
                }
                Ok(ControlFlow::Continue)
            },
            ControlMessage::Close() => {
                close_websocket(socket);
                Ok(ControlFlow::Continue)
            },
            ControlMessage::ForceQuit() => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("Forcibly quitting message loop");
                }
                Ok(ControlFlow::Break)
            }
        }
    }

    /// Handles incoming WebSocket messages by calling appropriate handler functions
    #[inline]
    pub(crate) fn handle_message<H: S9WebSocketClientHandler>(msg: Message, handler: &mut H) -> ControlFlow {
        match msg {
            Message::Text(message) => {
                trace_on_text_message(&message);
                handler.on_text_message(message.as_bytes());
                ControlFlow::Continue
            },
            Message::Binary(bytes) => {
                trace_on_binary_message(&bytes);
                handler.on_binary_message(&bytes);
                ControlFlow::Continue
            },
            Message::Close(close_frame) => {
                trace_on_close_frame(&close_frame);
                let reason = close_frame.map(|cf| cf.to_string());
                handler.on_connection_closed(reason);
                handler.on_quit();
                ControlFlow::Break
            },
            Message::Ping(bytes) => {
                trace_on_ping_message(&bytes);
                handler.on_ping(&bytes);
                ControlFlow::Continue
            },
            Message::Pong(bytes) => {
                trace_on_pong_message(&bytes);
                handler.on_pong(&bytes);
                ControlFlow::Continue
            },
            Message::Frame(_) => {
                trace_on_frame();
                ControlFlow::Continue
            }
        }
    }

    /// Handles socket read errors consistently across clients
    pub(crate) fn handle_read_error(error: Error) -> (Option<String>, bool) {
        match error {
            Error::Io(io_err) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available, continue loop (expected in non-blocking mode)
                (None, false)
            },
            Error::Io(io_err) if io_err.kind() == std::io::ErrorKind::TimedOut => {
                // No data available (e.g. Windows), continue loop (expected in non-blocking mode)
                (None, false)
            },
            Error::ConnectionClosed => {
                let reason = "Connection closed normally".to_string();
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!(reason);
                }
                (Some(reason), true)
            },
            e => {
                let error = format!("Failed to read from socket: {:?}", e);
                tracing::error!(error);
                (Some(error), true)
            }
        }
    }

    /// Sends text message to WebSocket
    #[inline]
    pub(crate) fn send_text_message_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, text: &str) -> S9Result<()> {
        socket.send(Message::text(text))
            .map(|_| {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("Sent text message: {}", text);
                }
            })
            .map_err(|e| {
                tracing::error!("Error sending text message: {}", e);
                WebSocketError::from(e).into()
            })
    }

    /// Determines if an error message indicates a connection closure
    #[inline]
    pub(crate) fn is_connection_closed_error(error_msg: &str) -> bool {
        // TODO: Find a type safe and reliable way to detect connection closure errors
        error_msg.contains("Connection closed") || error_msg.contains("closed")
    }

    /// Closes WebSocket connection
    pub(crate) fn close_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
        close_websocket_with_logging(socket, "on close");
    }

    /// Closes WebSocket connection with context logging
    pub(crate) fn close_websocket_with_logging(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, context: &str) {
        socket.close(None)
            .map(|_| {
                tracing::trace!("Connection close successfully requested {}", context);
            })
            .unwrap_or_else(|e| {
                tracing::error!("Error on connection close request {}: {}", context, e);
            });
    }

    /// Traces connection establishment
    pub(crate) fn trace_on_connected(response: &Response) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Connected to the server");
            tracing::trace!("Response HTTP code: {}", response.status());
            tracing::trace!("Response contains the following headers:");
            for (header, _value) in response.headers() {
                tracing::trace!("* {header}");
            }
        }
    }

    /// Traces text message receipt
    #[inline]
    pub(crate) fn trace_on_text_message(message: &Utf8Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Received text message: {}", message);
        }
    }

    /// Traces binary message receipt
    #[inline]
    pub(crate) fn trace_on_binary_message(bytes: &Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Received binary message: {:?}", bytes);
        }
    }

    /// Traces connection close frame receipt
    pub(crate) fn trace_on_close_frame(close_frame: &Option<CloseFrame>) {
        if tracing::enabled!(tracing::Level::TRACE) {
            match close_frame {
                Some(reason) => {
                    tracing::trace!("Connection closed with reason: {}", reason)
                },
                None => {
                    tracing::trace!("Connection closed without reason")
                },
            }
        }
    }

    /// Traces ping message receipt
    #[inline]
    pub(crate) fn trace_on_ping_message(bytes: &Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Received ping frame: {}", String::from_utf8_lossy(&bytes));
        }
    }

    /// Traces poing message receipt
    #[inline]
    pub(crate) fn trace_on_pong_message(bytes: &Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Received pong frame: {}", String::from_utf8_lossy(&bytes));
        }
    }

    /// Traces frame message receipt
    #[inline]
    pub(crate) fn trace_on_frame() {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("Received frame from server");
        }
    }
}

// ============================================================================
// S9AsyncNonBlockingWebSocketClient - Async client with channels
// ============================================================================

pub struct S9AsyncNonBlockingWebSocketClient {
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    options: NonBlockingOptions,
    pub control_tx: Sender<ControlMessage>,
    control_rx: Receiver<ControlMessage>,
    event_tx: Sender<WebSocketEvent>,
}

impl S9AsyncNonBlockingWebSocketClient {
    pub fn connect(
        uri: &str,
        options: NonBlockingOptions,
        control_tx: Sender<ControlMessage>,
        control_rx: Receiver<ControlMessage>,
        event_tx: Sender<WebSocketEvent>
    ) -> S9Result<S9AsyncNonBlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new(), options, control_tx, control_rx, event_tx)
    }

    pub fn connect_with_headers(
        uri: &str,
        headers: &HashMap<String, String>,
        options: NonBlockingOptions,
        control_tx: Sender<ControlMessage>,
        control_rx: Receiver<ControlMessage>,
        event_tx: Sender<WebSocketEvent>,
    ) -> S9Result<S9AsyncNonBlockingWebSocketClient> {
        let (mut socket, _response) = shared::connect_socket(uri, headers)?;

        shared::configure_non_blocking(&mut socket, &options)?;

        Ok(S9AsyncNonBlockingWebSocketClient {
            socket: Some(socket),
            options,
            control_tx,
            control_rx,
            event_tx,
        })
    }

    #[inline]
    pub fn run(&mut self) -> S9Result<JoinHandle<()>> {
        // Take ownership of the socket to put it into the tread by replacing it with a dummy value
        // This is safe because we'll never use the original socket again after spawning
        let socket = self.socket.take();
        let mut socket = match socket {
            Some(s) => s,
            None => {
                tracing::error!("Socket just consumed");
                return Err(WebSocketError::SocketUnavailable.into());
            },
        };
        let control_rx = self.control_rx.clone();
        let event_tx = self.event_tx.clone();

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting non-blocking event loop thread...");
        }

        let spin_wait_duration = self.options.shared.spin_wait_duration.clone();

        let join_handle = thread::spawn(move || {
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::debug!("Starting event loop");
            }

            // Send Activate event before entering the main loop
            send_or_log!(event_tx, "WebSocketEvent::Activated", WebSocketEvent::Activated);

            loop {
                // 1. Check for control messages (non-blocking)
                if let Ok(control_msg) = control_rx.try_recv() {
                    match shared::handle_control_message(control_msg, &mut socket) {
                        Ok(shared::ControlFlow::Continue) => {},
                        Ok(shared::ControlFlow::Break) => {
                            send_or_log!(event_tx, "WebSocketEvent::Quit on ControlMessage::ForceQuit", WebSocketEvent::Quit);
                            break;
                        },
                        Err(error) => {
                            send_or_break!(event_tx, "WebSocketEvent::Error on ControlMessage", WebSocketEvent::Error(error));
                        }
                    }
                }

                // 2. Try to read from socket (non-blocking)
                match socket.read() {
                    Ok(msg) => {
                        match msg {
                            Message::Text(message) => {
                                shared::trace_on_text_message(&message);
                                send_or_break!(event_tx, "WebSocketEvent::TextMessage on Message::Text", WebSocketEvent::TextMessage(message.as_bytes().to_vec()));
                            },
                            Message::Binary(bytes) => {
                                shared::trace_on_binary_message(&bytes);
                                send_or_break!(event_tx, "WebSocketEvent::BinaryMessage on Message::Binary", WebSocketEvent::BinaryMessage(bytes.to_vec()));
                            },
                            Message::Close(close_frame) => {
                                shared::trace_on_close_frame(&close_frame);
                                let reason = close_frame.map(|cf| cf.to_string());
                                send_or_log!(event_tx, "WebSocketEvent::ConnectionClosed on Message::Close", WebSocketEvent::ConnectionClosed(reason));
                                send_or_log!(event_tx, "WebSocketEvent::Quit on Message::Close", WebSocketEvent::Quit);
                                break;
                            },
                            Message::Ping(bytes) => {
                                shared::trace_on_ping_message(&bytes);
                                send_or_break!(event_tx, "WebSocketEvent::Ping on Message::Ping", WebSocketEvent::Ping(bytes.to_vec()));
                            },
                            Message::Pong(bytes) => {
                                shared::trace_on_pong_message(&bytes);
                                send_or_break!(event_tx, "WebSocketEvent::Pong on Message::Pong", WebSocketEvent::Pong(bytes.to_vec()));
                            },
                            Message::Frame(_) => {
                                shared::trace_on_frame();
                                // No handling for frames until use case needs it
                            }
                        }
                    },
                    Err(error) => {
                        let (reason, should_break) = shared::handle_read_error(error);
                        if let Some(error_msg) = reason {
                            if should_break {
                                let (context, event) = {
                                    if shared::is_connection_closed_error(&error_msg) {
                                        ("WebSocketEvent::ConnectionClosed  on Error::ConnectionClosed", WebSocketEvent::ConnectionClosed(Some(error_msg)))
                                    } else {
                                        ("WebSocketEvent::Error", WebSocketEvent::Error(error_msg))
                                    }
                                };
                                send_or_log!(event_tx, context, event);
                                send_or_break!(event_tx, "WebSocketEvent::Quit", WebSocketEvent::Quit);
                                break;
                            }
                        }
                    }
                };

                // Optionally sleep to reduce CPU usage
                if let Some(duration) = spin_wait_duration {
                    thread::sleep(duration);
                }
            }
        });
        Ok(join_handle)
    }
}

// ============================================================================
// S9NonBlockingWebSocketClient - Pure non-blocking client with handler callbacks
// ============================================================================

pub struct S9NonBlockingWebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    options: NonBlockingOptions,
}

impl S9NonBlockingWebSocketClient {
    pub fn connect(uri: &str, options: NonBlockingOptions) -> S9Result<S9NonBlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new(), options)
    }

    pub fn connect_with_headers(
        uri: &str,
        headers: &HashMap<String, String>,
        options: NonBlockingOptions
    ) -> S9Result<S9NonBlockingWebSocketClient> {
        let (mut sock, _response) = shared::connect_socket(uri, headers)?;

        shared::configure_non_blocking(&mut sock, &options)?;

        Ok(S9NonBlockingWebSocketClient {
            options,
            socket: sock,
        })
    }

    #[inline]
    pub fn run<HANDLER>(&mut self, handler: &mut HANDLER, control_rx: Receiver<ControlMessage>)
    where
        HANDLER: S9WebSocketClientHandler,
    {
        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting event loop");
        }

        // Notify activate before entering the main loop
        handler.on_activated();

        loop {
            // 1. Check for control messages (non-blocking)
            if let Ok(control_msg) = control_rx.try_recv() {
                match shared::handle_control_message(control_msg, &mut self.socket) {
                    Ok(shared::ControlFlow::Continue) => {},
                    Ok(shared::ControlFlow::Break) => {
                        handler.on_quit();
                        break;
                    },
                    Err(error) => {
                        handler.on_error(error);
                    }
                }
            }

            // 2. Try to read from socket (non-blocking)
            match self.socket.read() {
                Ok(msg) => {
                    match shared::handle_message(msg, handler) {
                        shared::ControlFlow::Continue => {},
                        shared::ControlFlow::Break => break,
                    }
                },
                Err(error) => {
                    let (reason, should_break) = shared::handle_read_error(error);
                    if let Some(error_msg) = reason {
                        if should_break {
                            if shared::is_connection_closed_error(&error_msg) {
                                handler.on_connection_closed(Some(error_msg));
                            } else {
                                handler.on_error(error_msg);
                            }
                            handler.on_quit();
                            break;
                        }
                    }
                }
            };

            // Optionally sleep to reduce CPU usage
            if let Some(duration) = self.options.shared.spin_wait_duration {
                thread::sleep(duration);
            }
        }
    }
}

// ============================================================================
// S9BlockingWebSocketClient - Blocking client with handler callbacks
// ============================================================================

pub struct S9BlockingWebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    options: BlockingOptions,
}

impl S9BlockingWebSocketClient{
    pub fn connect(uri: &str, options: BlockingOptions,) -> S9Result<S9BlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new(), options)
    }

    pub fn connect_with_headers(
        uri: &str,
        headers: &HashMap<String, String>,
        options: BlockingOptions
    ) -> S9Result<S9BlockingWebSocketClient> {
        let (mut socket, _response) = shared::connect_socket(uri, headers)?;

        shared::configure_blocking(&mut socket, &options)?;

        Ok(S9BlockingWebSocketClient {
            options,
            socket,
        })
    }

    #[inline]
    pub fn run<HANDLER>(&mut self, handler: &mut HANDLER, control_rx: Receiver<ControlMessage>)
    where
        HANDLER: S9WebSocketClientHandler,
    {
        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting event loop");
        }

        // Notify activate before entering the main loop
        handler.on_activated();

        loop {
            if let Ok(control_msg) = control_rx.try_recv() {
                match shared::handle_control_message(control_msg, &mut self.socket) {
                    Ok(shared::ControlFlow::Continue) => {},
                    Ok(shared::ControlFlow::Break) => {
                        handler.on_quit();
                        break;
                    },
                    Err(error) => {
                        handler.on_error(error);
                    }
                }
            }

            let msg = match self.socket.read() {
                Ok(msg) => msg,
                Err(e) => {
                    match e {
                        Error::Io(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            if self.options.read_timeout.is_some() {
                                continue; // No data available, continue loop (expected in timeout mode)
                            } else {
                                handler.on_error(format!("Error reading message: {}", e));
                                handler.on_quit();
                                break;
                            }
                        },
                        Error::Io(ref err) if err.kind() == std::io::ErrorKind::TimedOut => {
                            if self.options.read_timeout.is_some() {
                                continue; // No data available (e.g. Windows), continue loop (expected in timeout mode)
                            } else {
                                handler.on_error(format!("Error reading message: {}", e));
                                handler.on_quit();
                                break;
                            }
                        }
                        Error::ConnectionClosed => {
                            handler.on_connection_closed(Some("Connection closed".to_string()));
                            handler.on_quit();
                            break;
                        },
                        _ => {
                            handler.on_error(format!("Error reading message: {}", e));
                            handler.on_quit();
                            break;
                        }
                    }

                }
            };

            match shared::handle_message(msg, handler) {
                shared::ControlFlow::Continue => {},
                shared::ControlFlow::Break => break,
            }

            // Optionally sleep to reduce CPU usage
            if let Some(duration) = self.options.shared.spin_wait_duration {
                thread::sleep(duration);
            }
        }
    }
}

impl Drop for S9AsyncNonBlockingWebSocketClient {
    fn drop(&mut self) {
        if let Some(socket) = &mut self.socket {
            shared::close_websocket_with_logging(socket, "on Drop");
        }
    }
}

impl Drop for S9NonBlockingWebSocketClient {
    fn drop(&mut self) {
        shared::close_websocket_with_logging(&mut self.socket, "on Drop");
    }
}

impl Drop for S9BlockingWebSocketClient {
    fn drop(&mut self) {
        shared::close_websocket_with_logging(&mut self.socket, "on Drop");
    }
}