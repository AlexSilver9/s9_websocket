use std::collections::HashMap;
use std::net::TcpStream;
use std::str;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;

// TODO: add non-blocking, idea: https://github.com/haxpor/bybit-shiprekt/blob/6c3c5693d675fc997ce5e76df27e571f2aaaf291/src/main.rs

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
}

pub enum ControlMessage {
    SendText(String),
    Close(),
    ForceQuit(),
}

pub struct S9WebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl S9WebSocketClient {
    pub fn connect(uri: &str) -> Result<S9WebSocketClient, Error> {
        let uri = match Self::get_uri(uri) {
            Ok(value) => value,
            Err(value) => return value,
        };

        let result = tungstenite::connect(uri);
        match result {
            Ok((sock, response)) => {
                Self::trace_on_connected(response);

                Ok(S9WebSocketClient {
                    socket: sock,
                })
            },
            Err(e) => Err(e)
        }
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>) -> Result<S9WebSocketClient, Error> {
        let uri = match Self::get_uri(uri) {
            Ok(value) => value,
            Err(value) => return value,
        };

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let result = tungstenite::connect(builder);
        match result {
            Ok((sock, response)) => {
                Self::trace_on_connected(response);

                Ok(S9WebSocketClient {
                    socket: sock,
                })
            },
            Err(e) => Err(e)
        }
    }

    #[inline]
    pub fn run<HANDLER>(&mut self, handler: &mut HANDLER, control_rx: mpsc::Receiver<ControlMessage>)
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
                            handler.on_error(format!("S9WebSocketClient error reading message: {}", e));
                        }
                    }
                    break;
                }
            };
            match msg {
                Message::Text(message) => {
                    Self::trace_on_text_message(&message);
                    handler.on_text_message(message.as_bytes());
                },
                Message::Binary(bytes) => {
                    Self::trace_on_binary_message(&bytes);
                    handler.on_binary_message(&bytes);
                },
                Message::Close(close_frame) => {
                    self.trace_on_close(&close_frame);
                    let reason = close_frame.map(|cf| cf.to_string());
                    handler.on_connection_closed(reason);
                    break;
                },
                Message::Ping(bytes) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9WebSocketClient received ping frame: {}", String::from_utf8_lossy(&bytes));
                    }
                    handler.on_ping(&bytes);
                },
                Message::Pong(bytes) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9WebSocketClient received pong frame: {}", String::from_utf8_lossy(&bytes));
                    }
                    handler.on_pong(&bytes);
                },
                Message::Frame(_) => {
                    if tracing::enabled!(tracing::Level::TRACE) {
                        tracing::trace!("S9WebSocketClient received frame from server");
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
                    tracing::trace!("S9WebSocketClient sent text message: {}", String::from(s));
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("S9WebSocketClient error sending text message: {}", e);
                Err(e)
            }
        }
    }

    pub fn close(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9WebSocketClient connection close requested");
                },
                Err(e) => {
                    tracing::error!("S9WebSocketClient error on connection close request: {}", e);
                }
            }
        }
    }

    fn get_uri(uri: &str) -> Result<Uri, Result<S9WebSocketClient, Error>> {
        let uri: Uri = match Uri::from_str(uri) {
            Ok(uri) => uri,
            Err(e) => {
                tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
                return Err(Err(Error::from(e)));
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

    fn trace_on_close(&mut self, close_frame: &Option<CloseFrame>) {
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
}

impl Drop for S9WebSocketClient {
    fn drop(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9WebSocketClient connection close successfully requested on Drop");
                },
                Err(e) => {
                    tracing::error!("S9WebSocketClient error on connection close request on Drop: {}", e);
                }
            }
        }
    }
}
