use std::collections::HashMap;
use std::net::TcpStream;
use std::{str, thread};
use std::str::FromStr;
use crossbeam_channel::{unbounded, Receiver, Sender};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;

// TODO: add non-blocking, idea: https://github.com/haxpor/bybit-shiprekt/blob/6c3c5693d675fc997ce5e76df27e571f2aaaf291/src/main.rs

macro_rules! send_or_break {
    ($sender:expr, $context:expr, $event:expr) => {
        if let Err(e) = $sender.send($event) {
            tracing::error!("S9WebSocketClient failed to send {} through crossbeam channel: {}", $context, e);
            break;
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
    Connected,
    TextMessage(Vec<u8>),
    BinaryMessage(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    ConnectionClosed(Option<String>),
    Error(String),
    ForceQuit,
}

pub enum ControlMessage {
    SendText(String),
    Close(),
    ForceQuit(),
}

pub struct S9NonBlockingWebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    pub control_tx: Sender<ControlMessage>,
    pub event_rx: Receiver<WebSocketEvent>
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

                let (control_tx, _) = unbounded::<ControlMessage>();
                let (_, event_rx) = unbounded::<WebSocketEvent>();

                Ok(S9NonBlockingWebSocketClient {
                    socket: sock,
                    control_tx,
                    event_rx
                })
            },
            Err(e) => Err(e)
        }
    }

    #[inline]
    pub fn run_non_blocking(mut self) -> (Sender<ControlMessage>, Receiver<WebSocketEvent>) {
        let (event_tx, event_rx) = unbounded::<WebSocketEvent>();
        let (control_tx, control_rx) = unbounded::<ControlMessage>();

        thread::spawn(move || {
            loop {
                if let Ok(control_message) = control_rx.try_recv() {
                    match control_message {
                        ControlMessage::SendText(text) => {
                            if let Err(e) = self.send_text_message(&text) {
                                send_or_break!(event_tx, "ControlMessage::SendText", WebSocketEvent::Error(format!("Error sending text: {}", e)));
                            }
                        },
                        ControlMessage::Close() => {
                            self.close();
                        },
                        ControlMessage::ForceQuit() => {
                            if tracing::enabled!(tracing::Level::TRACE) {
                                tracing::trace!("S9NonBlockingWebSocketClient forcibly quitting message loop");
                            }
                            send_or_break!(event_tx, "ControlMessage::ForceQuit", WebSocketEvent::ForceQuit);
                            break;
                        }
                    }
                }

                let msg = match self.socket.read() {
                    Ok(msg) => msg,
                    Err(e) => {
                        match e {
                            Error::ConnectionClosed => {
                                send_or_break!(event_tx, "Error::ConnectionClosed", WebSocketEvent::ConnectionClosed(Some("Connection closed".to_string())));
                            },
                            _ => {
                                send_or_break!(event_tx, "AllWebsocketReadError", WebSocketEvent::Error(format!("S9WebSocketClient error reading message: {}", e)));
                            }
                        }
                        break;
                    }
                };

                match msg {
                    Message::Text(message) => {
                        trace_on_text_message(&message);
                        send_or_break!(event_tx, "Message::Text", WebSocketEvent::TextMessage(message.as_bytes().to_vec()));
                    },
                    Message::Binary(bytes) => {
                        trace_on_binary_message(&bytes);
                        send_or_break!(event_tx, "Message::Binary", WebSocketEvent::BinaryMessage(bytes.to_vec()));
                    },
                    Message::Close(close_frame) => {
                        trace_on_close(&close_frame);
                        let reason = close_frame.map(|cf| cf.to_string());
                        send_or_break!(event_tx, "Message::Close", WebSocketEvent::ConnectionClosed(reason));
                        break;
                    },
                    Message::Ping(bytes) => {
                        if tracing::enabled!(tracing::Level::TRACE) {
                            tracing::trace!("S9NonBlockingWebSocketClient received ping frame: {}", String::from_utf8_lossy(&bytes));
                        }
                        send_or_break!(event_tx, "Message::Ping", WebSocketEvent::Ping(bytes.to_vec()));
                    },
                    Message::Pong(bytes) => {
                        if tracing::enabled!(tracing::Level::TRACE) {
                            tracing::trace!("S9NonBlockingWebSocketClient received pong frame: {}", String::from_utf8_lossy(&bytes));
                        }
                        send_or_break!(event_tx, "Message::Pong", WebSocketEvent::Pong(bytes.to_vec()));
                    },
                    Message::Frame(_) => {
                        if tracing::enabled!(tracing::Level::TRACE) {
                            tracing::trace!("S9NonBlockingWebSocketClient received frame from server");
                        }
                        // No handling for frames until use case needs it
                    }
                }
            }
        });

        (control_tx, event_rx)
    }

    fn send_text_message(&mut self, s: &str) -> Result<(), Error> {
        let msg = Message::text(s);
        let send_result = self.socket.send(msg);
        match send_result {
            Ok(()) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9NonBlockingWebSocketClient sent text message: {}", String::from(s));
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("S9NonBlockingWebSocketClient error sending text message: {}", e);
                Err(e)
            }
        }
    }

    fn close(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9NonBlockingWebSocketClient connection close requested");
                },
                Err(e) => {
                    tracing::error!("S9NonBlockingWebSocketClient error on connection close request: {}", e);
                }
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
                        if let Err(e) = self.send_text_message(&text) {
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
        self.send_text_message(pong.as_str())
    }

    #[inline]
    pub fn send_text_message(&mut self, s: &str) -> Result<(), Error> {
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
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
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
