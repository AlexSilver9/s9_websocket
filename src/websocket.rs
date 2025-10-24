use std::collections::HashMap;
use std::net::TcpStream;
use std::{str, thread};
use std::str::FromStr;
use std::time::Duration;
use crossbeam_channel::{select, unbounded, Receiver, SendError, Sender};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;

// TODO: add non-blocking, idea: https://github.com/haxpor/bybit-shiprekt/blob/6c3c5693d675fc997ce5e76df27e571f2aaaf291/src/main.rs

macro_rules! send_or_break {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("S9WebSocketClient failed to send context {} through crossbeam channel: {}", $context, e);
            break;
        }
    };
}

macro_rules! send_or_log {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("S9WebSocketClient failed to send context {} through crossbeam channel: {}", $context, e);
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

pub enum NonBlockingStrategy {
    SpinNonBlocking(Option<Duration>),
    SpinBlockingWithTimeout(Duration, Option<Duration>),
}

impl NonBlockingStrategy {
    pub fn new_non_blocking_strategy() -> Self {
        NonBlockingStrategy::SpinNonBlocking(None)
    }

    pub fn new_non_blocking_strategy_with_spin_waiting(spin_wait_duration: Duration) -> Result<Self, String> {
        if spin_wait_duration.is_zero() {
            return Err("Spin wain duration cannot be zero".to_string());
        }
        Ok(NonBlockingStrategy::SpinNonBlocking(None))
    }

    pub fn new_timeout_strategy(timeout_duration: Duration, spin_wait_duration: Option<Duration>) -> Result<Self, String> {
        if timeout_duration.is_zero() {
            return Err("Timeout duration cannot be zero".to_string());
        }
        match spin_wait_duration {
            Some(duration) => {
                if duration.is_zero() {
                    return Err("Spin wain duration cannot be zero".to_string());
                }
                Ok(NonBlockingStrategy::SpinBlockingWithTimeout(duration, Some(duration)))
            },
            None => Ok(NonBlockingStrategy::SpinBlockingWithTimeout(timeout_duration, None)),
        }
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
    pub fn connect(uri: &str) -> Result<S9NonBlockingWebSocketClient, Error> {
        Self::connect_with_headers(uri, &HashMap::new())
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> Result<S9NonBlockingWebSocketClient, Error> {
        let uri = match get_uri(uri) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let result = tungstenite::connect(builder);
        match result {
            Ok((sock, response)) => {
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
            },
            Err(e) => Err(e)
        }
    }

    #[inline]
    pub fn run_non_blocking(&mut self, non_blocking_strategy: NonBlockingStrategy) {
        // Take ownership of the socket by replacing it with a dummy value
        // This is safe because we'll never use the original socket again after spawning
        let mut socket = self.socket.take().expect("Socket already moved to thread"); // TODO Throw Err
        let control_rx = self.control_rx.clone();
        let event_tx = self.event_tx.clone();

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting WebSocket non-blocking event loop thread...");
        }

        // Set underlying streams to pure non-blocking or fake non-blocking with timeout
        match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => {
                Self::set_non_blocking_mode(&non_blocking_strategy, stream);
            },
            MaybeTlsStream::NativeTls(stream) => {
                Self::set_non_blocking_mode(&non_blocking_strategy, stream.get_mut());
            },
            /*#[cfg(feature = "rustls")]
            MaybeTlsStream::Rustls(stream) => {
                Self::set_non_blocking_mode(&non_blocking_strategy, stream.get_mut());
            },*/
            _ => {}
        }

        thread::spawn(move || {
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::debug!("Starting WebSocket non-blocking event loop thread started");
            }
            send_or_log!(event_tx, "WebSocketEvent::Activated", WebSocketEvent::Activated);

            // Split socket into read and write halves would be ideal, but tungstenite doesn't support it
            // So we need to use Arc<Mutex<>> to share the socket
            let socket = std::sync::Arc::new(std::sync::Mutex::new(socket));
            let socket_reader = socket.clone();

            let (socket_tx, socket_rx) = unbounded::<Result<Message, Error>>();

            thread::spawn(move || {
                loop {
                    let msg = {
                        let mut sock = socket_reader.lock().unwrap(); // TODO: Handle error using crossbeam-channel or similar
                        sock.read()
                    };
                    let should_send = match &msg {
                        Err(Error::Io(io_err)) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
                            // This is expected for non-blocking sockets, don't send or break
                            false
                        },
                        Err(Error::Io(io_err)) if io_err.kind() == std::io::ErrorKind::TimedOut => {
                            // This is expected for non-blocking sockets, don't send or break
                            false
                        }
                        _ => true
                    };

                    if should_send {
                        let should_break = msg.is_err();
                        if socket_tx.send(msg).is_err() {
                            // Main thread has dropped, exit
                            // TODO: Maybe handle error using crossbeam-channel or similar
                            break;
                        }
                        if should_break {
                            break;
                        }
                    } else {
                        // Sleep for a while to avoid busy spin
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            });

            loop {
                select! {
                    recv(control_rx) -> control_msg => {
                        match control_msg {
                            Ok(ControlMessage::SendText(text)) => {
                                let mut sock = socket.lock().unwrap(); // TODO: Handle error using crossbeam-channel or similar
                                if let Err(e) = send_text_message_to_websocket(&mut sock, &text, "S9NonBlockingWebSocketClient") {
                                    send_or_break!(event_tx, "WebSocketEvent::Error on ControlMessage::SendText", WebSocketEvent::Error(format!("Error sending text: {}", e)));
                                }
                            },
                            Ok(ControlMessage::Close()) => {
                                let mut sock = socket.lock().unwrap(); // TODO: Handle error using crossbeam-channel or similar
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
                                // TODO: Handle error using crossbeam-channel or similar
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
                                    Error::ConnectionClosed => {
                                        send_or_log!(event_tx, "WebSocketEvent::ConnectionClosed on Error::ConnectionClosed", WebSocketEvent::ConnectionClosed(Some("Connection closed".to_string())));
                                    },
                                    _ => {
                                        send_or_log!(event_tx, "WebSocketEvent::Error on Tungsenite::AnyWebsocketReadError", WebSocketEvent::Error(format!("S9WebSocketClient error reading message: {}", e)));
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
    }

    fn set_non_blocking_mode(non_blocking_strategy: &NonBlockingStrategy, stream: &mut TcpStream) {
        match non_blocking_strategy {
            NonBlockingStrategy::SpinNonBlocking(_) => {
                stream.set_read_timeout(None).ok();// TODO: return Errors;
                stream.set_nonblocking(true).ok();
                stream.set_nodelay(true).ok();
            },
            NonBlockingStrategy::SpinBlockingWithTimeout(timeout_duration, _) => {
                stream.set_read_timeout(Some(*timeout_duration)).ok();
                stream.set_nonblocking(false).ok();
                stream.set_nodelay(true).ok();
            }
        }
    }

    pub fn send_text_message(&mut self, s: &str) -> Result<(), SendError<ControlMessage>> {
        let result = self.control_tx.send(ControlMessage::SendText(s.to_string()));
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::error!("S9WebSocketClient failed to send context {} through crossbeam channel: {}", "ControlMessage::SendText", e);
                Err(e)
            }
        }
    }
}

fn send_text_message_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, s: &str, client_name: &str) -> Result<(), Error> {
    let msg = Message::text(s);
    let send_result = socket.send(msg);
    match send_result {
        Ok(()) => {
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("{} sent text message: {}", client_name, String::from(s));
            }
            Ok(())
        },
        Err(e) => {
            tracing::error!("{} error sending text message: {}", client_name, e);
            Err(e)
        }
    }
}

fn close_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, client_name: &str) {
    if socket.can_read() {
        let close_result = socket.close(None);
        match close_result {
            Ok(_) => {
                tracing::trace!("{} connection close requested", client_name);
            },
            Err(e) => {
                tracing::error!("{} error on connection close request: {}", client_name, e);
            }
        }
    }
}

impl S9BlockingWebSocketClient{
    pub fn connect(uri: &str) -> Result<S9BlockingWebSocketClient, Error> {
        Self::connect_with_headers(uri, &HashMap::new())
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> Result<S9BlockingWebSocketClient, Error> {
        let uri = match get_uri(uri) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let result = tungstenite::connect(builder);
        match result {
            Ok((sock, response)) => {
                trace_on_connected(response);
                Ok(S9BlockingWebSocketClient {
                    socket: sock,
                })
            },
            Err(e) => Err(e)
        }
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
                        self.close();
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

    #[inline]
    pub fn send_text_pong_for_text_ping(&mut self, ping_message: &str) -> Result<(), Error> {
        let mut pong = String::with_capacity(ping_message.len());
        pong.push_str(&ping_message[..3]);
        pong.push('o');
        pong.push_str(&ping_message[4..]);
        self.send_text_message_to_websocket(pong.as_str())
    }

    #[inline]
    pub fn send_text_message(&mut self, s: &str) -> Result<(), Error> {
        self.send_text_message_to_websocket(s)
    }

    #[inline]
    fn send_text_message_to_websocket(&mut self, s: &str) -> Result<(), Error> {
        let msg = Message::text(s);
        let send_result = self.socket.send(msg);
        match send_result {
            Ok(()) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9BlockingWebSocketClient sent text message: {}", String::from(s));
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("S9BlockingWebSocketClient error sending text message: {}", e);
                Err(e)
            }
        }
    }

    pub fn close(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9BlockingWebSocketClient connection close requested");
                },
                Err(e) => {
                    tracing::error!("S9BlockingWebSocketClient error on connection close request: {}", e);
                }
            }
        }
    }
}

fn get_uri(uri: &str) -> Result<Uri, Error> {
    let uri: Uri = match Uri::from_str(uri) {
        Ok(uri) => uri,
        Err(e) => {
            tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
            return Err(Error::from(e));
        }
    };
    Ok(uri)
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

#[inline]
fn trace_on_text_message(message: &Utf8Bytes) {
    if tracing::enabled!(tracing::Level::TRACE) {
        tracing::trace!("S9WebSocketClient received text message: {}", message);
    }
}

#[inline]
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

impl Drop for S9NonBlockingWebSocketClient {
    fn drop(&mut self) {
        if let Some(socket) = &mut self.socket {
            if socket.can_read() {
                let close_result = socket.close(None);
                match close_result {
                    Ok(_) => {
                        tracing::trace!("S9NonBlockingWebSocketClient connection close successfully requested on Drop");
                    },
                    Err(e) => {
                        tracing::error!("S9NonBlockingWebSocketClient error on connection close request on Drop: {}", e);
                    }
                }
            }
        }
    }
}

impl Drop for S9BlockingWebSocketClient {
    fn drop(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9BlockingWebSocketClient connection close successfully requested on Drop");
                },
                Err(e) => {
                    tracing::error!("S9BlockingWebSocketClient error on connection close request on Drop: {}", e);
                }
            }
        }
    }
}
