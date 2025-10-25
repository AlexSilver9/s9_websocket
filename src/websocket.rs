use std::collections::HashMap;
use std::net::TcpStream;
use std::{str, thread};
use std::str::FromStr;
use std::time::Duration;
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;
use crate::error::{S9WebSocketError, S9Result, WebSocketError, ControlChannelError};

// TODO: Refactor clients and structs/enums to separate files
// TODO  Socket Unthreading, e.g. split socket into read/write halves or unthread read instead of using Arc<Mutex<>>
// TODO: Optional timeout for blocking socket
// TODO: Add Tests
// TODO: Add API Documentation + change documentation pointer in Cargo.toml to something like https://docs.rs/s9_websocket/0.0.1
// TODO: Add Code Documentation

macro_rules! send_or_break {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("S9NonBlockingWebSocketClient failed to send context {} through crossbeam channel: {}", $context, e);
            break;
        }
    };
}

macro_rules! send_or_log {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("S9NonBlockingWebSocketClient failed to send context {} through crossbeam channel: {}", $context, e);
        }
    };
}


pub trait S9WebSocketClientHandler {
    fn on_text_message(&mut self, data: &[u8]);
    fn on_binary_message(&mut self, data: &[u8]);
    fn on_connection_closed(&mut self, reason: Option<String>);
    fn on_error(&mut self, error: String);
    fn on_ping(&mut self, _data: &[u8]) {
        // Default: noop
    }
    fn on_pong(&mut self, _data: &[u8]) {
        // Default: noop
    }
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

#[derive(Debug, Clone, Copy)]
pub struct NonBlockingOptions {
    spin_wait_duration: Option<Duration>
}

impl NonBlockingOptions {
    pub fn new(spin_wait_duration: Option<Duration>) -> S9Result<Self> {
        if let Some(duration) = spin_wait_duration {
            if duration.is_zero() {
                return Err(WebSocketError::InvalidConfiguration("Spin wait duration cannot be zero".to_string()).into());
            }
        }
        Ok(Self {
            spin_wait_duration
        })
    }
}

pub struct S9NonBlockingWebSocketClient {
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    pub control_tx: Sender<ControlMessage>,
    control_rx: Receiver<ControlMessage>,
    event_tx: Sender<WebSocketEvent>,
    pub event_rx: Receiver<WebSocketEvent>,
}

pub struct S9BlockingWebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl S9NonBlockingWebSocketClient {
    pub fn connect(uri: &str) -> S9Result<S9NonBlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new())
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> S9Result<S9NonBlockingWebSocketClient> {
        let uri = get_uri(uri)?;

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let (sock, response) = tungstenite::connect(builder)?;
        trace_on_connected(response);

        let (control_tx, control_rx) = unbounded::<ControlMessage>();
        let (event_tx, event_rx) = unbounded::<WebSocketEvent>();

        Ok(S9NonBlockingWebSocketClient {
            socket: Some(sock),
            control_tx,
            control_rx,
            event_tx,
            event_rx
        })
    }

    #[inline]
    pub fn run_non_blocking(&mut self, non_blocking_options: NonBlockingOptions) -> S9Result<()> {
        // Take ownership of the socket by replacing it with a dummy value
        // This is safe because we'll never use the original socket again after spawning
        let socket = self.socket.take();
        let mut socket = match socket {
            Some(s) => s,
            None => return Err(WebSocketError::SocketUnavailable.into()),
        };
        let control_rx = self.control_rx.clone();
        let event_tx = self.event_tx.clone();

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting WebSocket non-blocking event loop thread...");
        }

        Self::set_non_blocking(&mut socket)?;

        thread::spawn(move || {
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::debug!("Starting WebSocket non-blocking event loop thread started");
            }
            send_or_log!(event_tx, "WebSocketEvent::Activated", WebSocketEvent::Activated);

            // Split socket into read and write halves would be ideal, but tungstenite doesn't support it
            // So we need to use Arc<Mutex<>> to share the socket
            let socket = std::sync::Arc::new(std::sync::Mutex::new(socket));
            let socket_reader = socket.clone();

            let (socket_tx, socket_rx) = unbounded::<Result<Message, S9WebSocketError>>();

            let event_tx_for_socket_thread = event_tx.clone();

            thread::spawn(move || {
                loop {
                    // Socket reader thread loop:
                    // 1. Acquires mutex lock and reads from WebSocket
                    // 2. Filters WouldBlock errors (normal in non-blocking mode)
                    // 3. Forwards valid messages/errors to main event loop via channel
                    // 4. Breaks loop on read errors or if main thread disconnects
                    // 5. Sleeps between iterations if spin_wait_duration is configured

                    let msg = match socket_reader.lock() {
                        Ok(mut sock) => sock.read().map_err(|e| S9WebSocketError::from(e)),
                        Err(e) => {
                            send_or_log!(event_tx_for_socket_thread, "Mutex::Lock", WebSocketEvent::Error(format!("Failed to acquire lock for socket read: {}", e)));
                            return;
                        }
                    };

                    // Filter out WouldBlock errors (expected in non-blocking mode)
                    let send_to_channel = match &msg {
                        Err(S9WebSocketError::WebSocket(WebSocketError::Io(io_err))) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
                            false
                        },
                        Err(_) => {
                            tracing::error!("S9NonBlockingWebSocketClient failed to read from socket: {:?}", msg);
                            true
                        },
                        _ => true
                    };

                    // Send valid messages to main event loop, break on errors
                    if send_to_channel {
                        let should_break = msg.is_err();
                        let result = socket_tx.send(msg);
                        if result.is_err() {
                            // Main thread has dropped, try notify channel
                            send_or_log!(event_tx_for_socket_thread, "Socket Message ", WebSocketEvent::Error(format!("Error reading from socket: {}", result.unwrap_err())));
                            break;
                        }
                        if should_break {
                            break;
                        }
                    }

                    // Sleep to reduce CPU usage
                    if let Some(duration) = non_blocking_options.spin_wait_duration {
                        thread::sleep(duration);
                    }
                }
            });

            loop {
                select! {
                    recv(control_rx) -> control_msg => {
                        match control_msg {
                            Ok(ControlMessage::SendText(text)) => {
                                let mut sock = match socket.lock() {
                                    Ok(sock) => sock,
                                    Err(e) => {
                                        send_or_log!(event_tx, "Mutex::Lock on ControlMessage::SendText", WebSocketEvent::Error(format!("Failed to acquire lock for socket send: {}", e)));
                                        break;
                                    }
                                };
                                if let Err(e) = send_text_message_to_websocket(&mut sock, &text, "S9NonBlockingWebSocketClient") {
                                    send_or_break!(event_tx, "WebSocketEvent::Error on ControlMessage::SendText", WebSocketEvent::Error(format!("Error sending text: {}", e)));
                                }
                            },
                            Ok(ControlMessage::Close()) => {
                                let mut sock = match socket.lock() {
                                    Ok(sock) => sock,
                                    Err(e) => {
                                        send_or_log!(event_tx, "Mutex::Lock on ControlMessage::Close", WebSocketEvent::Error(format!("Failed to acquire lock for socket send: {}", e)));
                                        break;
                                    }
                                };
                                close_websocket(&mut sock, "S9NonBlockingWebSocketClient");
                            },
                            Ok(ControlMessage::ForceQuit()) => {
                                if tracing::enabled!(tracing::Level::TRACE) {
                                    tracing::trace!("S9NonBlockingWebSocketClient forcibly quitting message loop");
                                }
                                send_or_break!(event_tx, "WebSocketEvent::Quit on ControlMessage::ForceQuit", WebSocketEvent::Quit);
                                break;
                            },
                            Err(e) => {
                                // Channel dropped, try notify event channel
                                send_or_log!(event_tx, "Read from ControlMessage", WebSocketEvent::Error(format!("Failed read from control channel: {}", e)));
                                break;
                            }
                        }
                    },
                    recv(socket_rx) -> socket_msg => {
                        match socket_msg {
                            Ok(Ok(msg)) => {
                                match msg {
                                    Message::Text(message) => {
                                        trace_on_text_message(&message);
                                        send_or_break!(event_tx, "WebSocketEvent::TextMessage on Message::Text", WebSocketEvent::TextMessage(message.as_bytes().to_vec()));
                                    },
                                    Message::Binary(bytes) => {
                                        trace_on_binary_message(&bytes);
                                        send_or_break!(event_tx, "WebSocketEvent::BinaryMessage on Message::Binary", WebSocketEvent::BinaryMessage(bytes.to_vec()));
                                    },
                                    Message::Close(close_frame) => {
                                        trace_on_close(&close_frame);
                                        let reason = close_frame.map(|cf| cf.to_string());
                                        send_or_log!(event_tx, "WebSocketEvent::ConnectionClosed on Message::Close", WebSocketEvent::ConnectionClosed(reason));
                                        send_or_log!(event_tx, "WebSocketEvent::Quit on Message::Close", WebSocketEvent::Quit);
                                        break;
                                    },
                                    Message::Ping(bytes) => {
                                        if tracing::enabled!(tracing::Level::TRACE) {
                                            tracing::trace!("S9NonBlockingWebSocketClient received ping frame: {}", String::from_utf8_lossy(&bytes));
                                        }
                                        send_or_break!(event_tx, "WebSocketEvent::Ping on Message::Ping", WebSocketEvent::Ping(bytes.to_vec()));
                                    },
                                    Message::Pong(bytes) => {
                                        if tracing::enabled!(tracing::Level::TRACE) {
                                            tracing::trace!("S9NonBlockingWebSocketClient received pong frame: {}", String::from_utf8_lossy(&bytes));
                                        }
                                        send_or_break!(event_tx, "WebSocketEvent::Pong on Message::Pong", WebSocketEvent::Pong(bytes.to_vec()));
                                    },
                                    Message::Frame(_) => {
                                        if tracing::enabled!(tracing::Level::TRACE) {
                                            tracing::trace!("S9NonBlockingWebSocketClient received frame from server");
                                        }
                                        // No handling for frames until use case needs it
                                    }
                                }
                            },
                            Ok(Err(e)) => {
                                match e {
                                    S9WebSocketError::WebSocket(WebSocketError::ConnectionClosed(_)) => {
                                        send_or_log!(event_tx, "WebSocketEvent::ConnectionClosed on Error::ConnectionClosed", WebSocketEvent::ConnectionClosed(Some("S9NonBlockingWebSocketClient connection closed normally".to_string())));
                                    },
                                    _ => {
                                        send_or_log!(event_tx, "WebSocketEvent::Error on Tungsenite Error", WebSocketEvent::Error(format!("S9NonBlockingWebSocketClient error reading message: {}", e)));
                                    }
                                }
                                send_or_break!(event_tx, "WebSocketEvent::Quit on Tungsenite::Error", WebSocketEvent::Quit);
                                break;
                            },
                            Err(_) => {
                                // Socket thread has closed
                                send_or_log!(event_tx, "WebSocketEvent::Error on socket thread closed", WebSocketEvent::Error("Socket reader thread closed unexpectedly".to_string()));
                                send_or_break!(event_tx, "WebSocketEvent::Quit on socket thread closed", WebSocketEvent::Quit);
                                break;
                            }
                        }
                    }
                }
            }
        });
        Ok(())
    }

    fn set_non_blocking(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> S9Result<()> {
        // Set underlying streams to pure non-blocking or fake non-blocking with timeout
        match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => {
                stream.set_nonblocking(true)?;
                stream.set_nodelay(true)?;
            },
            MaybeTlsStream::NativeTls(stream) => {
                stream.get_mut().set_nonblocking(true)?;
                stream.get_mut().set_nodelay(true)?;
            },
            _ => {}
        }
        Ok(())
    }

    #[inline]
    pub fn send_text_message(&mut self, s: &str) -> S9Result<()> {
        self.control_tx.send(ControlMessage::SendText(s.to_string()))
            .map_err(|e| {
                tracing::error!("S9NonBlockingWebSocketClient failed to send context {} through crossbeam channel: {}", "ControlMessage::SendText", e);
                ControlChannelError::from(e).into()
            })
    }
}

#[inline]
fn send_text_message_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, text: &str, client_name: &str) -> S9Result<()> {
    socket.send(Message::text(text))
        .map(|_| {
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("{} sent text message: {}", client_name, text);
            }
        })
        .map_err(|e| {
            tracing::error!("{} error sending text message: {}", client_name, e);
            WebSocketError::from(e).into()
        })
}

fn close_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, client_name: &str) {
    close_websocket_with_logging(socket, client_name, "on close");
}

impl S9BlockingWebSocketClient{
    pub fn connect(uri: &str) -> S9Result<S9BlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new())
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> S9Result<S9BlockingWebSocketClient> {
        let uri = get_uri(uri)?;

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let (sock, response) = tungstenite::connect(builder)?;
        trace_on_connected(response);

        Ok(S9BlockingWebSocketClient {
            socket: sock,
        })
    }

    #[inline]
    pub fn run_blocking<HANDLER>(&mut self, handler: &mut HANDLER, control_rx: Receiver<ControlMessage>)
    where
        HANDLER: S9WebSocketClientHandler,
    {
        loop {
            if let Ok(control_message) = control_rx.try_recv() {
                match control_message {
                    ControlMessage::SendText(text) => {
                        if let Err(e) = self.send_text_message_to_websocket(&text) {
                            handler.on_error(format!("Error sending text: {}", e));
                        }
                    },
                    ControlMessage::Close() => {
                        close_websocket_with_logging(&mut self.socket, "S9BlockingWebSocketClient", "on close");
                    },
                    ControlMessage::ForceQuit() => {
                        if tracing::enabled!(tracing::Level::TRACE) {
                            tracing::trace!("S9BlockingWebSocketClient forcibly quitting message loop");
                        }
                        handler.on_quit();
                        break;
                    }
                }
            }

            let msg = match self.socket.read() {
                Ok(msg) => msg,
                Err(e) => {
                    match e {
                        Error::ConnectionClosed => {
                            handler.on_connection_closed(Some("Connection closed".to_string()));
                        },
                        _ => {
                            handler.on_error(format!("S9BlockingWebSocketClient error reading message: {}", e));
                        }
                    }
                    handler.on_quit();
                    break;
                }
            };
            match msg {
                Message::Text(message) => {
                    trace_on_text_message(&message);
                    handler.on_text_message(message.as_bytes());
                },
                Message::Binary(bytes) => {
                    trace_on_binary_message(&bytes);
                    handler.on_binary_message(&bytes);
                },
                Message::Close(close_frame) => {
                    trace_on_close(&close_frame);
                    let reason = close_frame.map(|cf| cf.to_string());
                    handler.on_connection_closed(reason);
                    handler.on_quit();
                    break;
                },
                Message::Ping(bytes) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9BlockingWebSocketClient received ping frame: {}", String::from_utf8_lossy(&bytes));
                    }
                    handler.on_ping(&bytes);
                },
                Message::Pong(bytes) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9BlockingWebSocketClient received pong frame: {}", String::from_utf8_lossy(&bytes));
                    }
                    handler.on_pong(&bytes);
                },
                Message::Frame(_) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9BlockingWebSocketClient received frame from server");
                    }
                    // No handling for frames until use case needs it
                }
            }
        }
    }

    pub fn send_text_message(&mut self, text: &str) -> S9Result<()> {
        self.send_text_message_to_websocket(text)
    }

    fn send_text_message_to_websocket(&mut self, text: &str) -> S9Result<()> {
        self.socket.send(Message::text(text))
            .map(|_| {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9BlockingWebSocketClient sent text message: {}", text);
                }
            })
            .map_err(|e| {
                tracing::error!("S9BlockingWebSocketClient error sending text message: {}", e);
                WebSocketError::from(e).into()
            })
    }
}

fn get_uri(uri: &str) -> S9Result<Uri> {
    Uri::from_str(uri).map_err(|e| {
        tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
        WebSocketError::InvalidUri(e.to_string()).into()
    })
}

fn trace_on_connected(response: Response) {
    if tracing::enabled!(tracing::Level::TRACE) {
        tracing::trace!("S9WebSocketClient connected to the server");
        tracing::trace!("S9WebSocketClient response HTTP code: {}", response.status());
        tracing::trace!("S9WebSocketClient response contains the following headers:");
        for (header, _value) in response.headers() {
            tracing::trace!("* {header}");
        }
    }
}

fn trace_on_text_message(message: &Utf8Bytes) {
    if tracing::enabled!(tracing::Level::TRACE) {
        tracing::trace!("S9WebSocketClient received text message: {}", message);
    }
}

fn trace_on_binary_message(bytes: &Bytes) {
    if tracing::enabled!(tracing::Level::TRACE) {
        tracing::trace!("S9WebSocketClient received binary message: {:?}", bytes);
    }
}

fn trace_on_close(close_frame: &Option<CloseFrame>) {
    if tracing::enabled!(tracing::Level::TRACE) {
        match close_frame {
            Some(reason) => {
                tracing::trace!("S9WebSocketClient connection closed with reason: {}", reason)
            },
            None => {
                tracing::trace!("S9WebSocketClient connection closed without reason")
            },
        }
    }
}

fn close_websocket_with_logging(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, client_name: &str, context: &str) {
    socket.close(None)
        .map(|_| {
            tracing::trace!("{} connection close successfully requested {}", client_name, context);
        })
        .unwrap_or_else(|e| {
            tracing::error!("{} error on connection close request {}: {}", client_name, context, e);
        });
}

impl Drop for S9NonBlockingWebSocketClient {
    fn drop(&mut self) {
        if let Some(socket) = &mut self.socket {
            close_websocket_with_logging(socket, "S9NonBlockingWebSocketClient", "on Drop");
        }
    }
}

impl Drop for S9BlockingWebSocketClient {
    fn drop(&mut self) {
        close_websocket_with_logging(&mut self.socket, "S9BlockingWebSocketClient", "on Drop");
    }
}