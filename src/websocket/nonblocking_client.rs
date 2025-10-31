use std::collections::HashMap;
use std::net::TcpStream;
use std::thread;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use crate::error::S9Result;
use super::options::NonBlockingOptions;
use super::types::S9WebSocketClientHandler;
use super::shared;

// ============================================================================
// S9NonBlockingWebSocketClient - Pure non-blocking client with handler callbacks
// ============================================================================

pub struct S9NonBlockingWebSocketClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    options: NonBlockingOptions,
    running: bool,
}

impl S9NonBlockingWebSocketClient {
    pub fn connect(uri: &str, options: NonBlockingOptions) -> S9Result<S9NonBlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new(), options)
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>, options: NonBlockingOptions) -> S9Result<S9NonBlockingWebSocketClient> {
        let (mut socket, _response) = shared::connect_socket(uri, headers)?;

        shared::configure_non_blocking(&mut socket, &options)?;

        Ok(S9NonBlockingWebSocketClient {
            socket,
            options,
            running: true,
        })
    }

    #[inline]
    pub fn run<HANDLER>(&mut self, handler: &mut HANDLER)
    where
        HANDLER: S9WebSocketClientHandler<Self>,
    {
        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!("Starting event loop");
        }

        // Notify activate before entering the main loop
        handler.on_activated(self);

        while self.running {
            handler.on_poll(self);

            match self.socket.read() {
                Ok(msg) => {
                    match msg {
                        Message::Text(message) => {
                            shared::trace_on_text_message(&message);
                            handler.on_text_message(self, message.as_bytes());
                        },
                        Message::Binary(bytes) => {
                            shared::trace_on_binary_message(&bytes);
                            handler.on_binary_message(self, &bytes);
                        },
                        Message::Close(close_frame) => {
                            shared::trace_on_close_frame(&close_frame);
                            let reason = close_frame.map(|cf| cf.to_string());
                            handler.on_connection_closed(self, reason);
                            handler.on_quit(self);
                            break;
                        },
                        Message::Ping(bytes) => {
                            shared::trace_on_ping_message(&bytes);
                            handler.on_ping(self, &bytes);
                        },
                        Message::Pong(bytes) => {
                            shared::trace_on_pong_message(&bytes);
                            handler.on_pong(self, &bytes);
                        },
                        Message::Frame(_) => {
                            shared::trace_on_frame();
                        }
                    }
                },
                Err(error) => {
                    let (reason, should_break) = shared::handle_read_error(error);
                    if let Some(error_msg) = reason {
                        if should_break {
                            if shared::is_connection_closed_error(&error_msg) {
                                handler.on_connection_closed(self, Some(error_msg));
                            } else {
                                handler.on_error(self, error_msg);
                            }
                            handler.on_quit(self);
                            break;
                        }
                    } else {
                        handler.on_idle(self);
                    }
                }
            };

            // Optionally sleep to reduce CPU usage
            if let Some(duration) = self.options.shared.spin_wait_duration {
                thread::sleep(duration);
            }
        }
    }

    #[inline]
    pub fn send_text_message(&mut self, text: &str) -> S9Result<()> {
        shared::send_text_message_to_websocket(&mut self.socket, text)
    }

    #[inline]
    pub fn send_binary_message(&mut self, data: Vec<u8>) -> S9Result<()> {
        shared::send_binary_message_to_websocket(&mut self.socket, data)
    }

    #[inline]
    pub fn send_ping(&mut self, data: Vec<u8>) -> S9Result<()> {
        shared::send_ping_to_websocket(&mut self.socket, data)
    }

    #[inline]
    pub fn send_pong(&mut self, data: Vec<u8>) -> S9Result<()> {
        shared::send_pong_to_websocket(&mut self.socket, data)
    }

    pub fn close(&mut self) {
        shared::close_websocket_with_logging(&mut self.socket, "on close");
    }

    pub fn force_quit(&mut self) {
        self.running = false;
    }
}

impl Drop for S9NonBlockingWebSocketClient {
    fn drop(&mut self) {
        shared::close_websocket_with_logging(&mut self.socket, "on Drop");
    }
}
