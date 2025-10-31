use std::collections::HashMap;
use std::net::TcpStream;
use std::thread::{self, JoinHandle};
use crossbeam_channel::{unbounded, Receiver, Sender};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use crate::error::{S9Result, S9WebSocketError};
use super::options::NonBlockingOptions;
use super::types::{WebSocketEvent, ControlMessage};
use super::types::{send_or_break, send_or_log};
use super::shared;

// ============================================================================
// S9AsyncNonBlockingWebSocketClient - Async client with channels
// ============================================================================

pub struct S9AsyncNonBlockingWebSocketClient {
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    options: NonBlockingOptions,
    pub control_tx: Sender<ControlMessage>,
    control_rx: Receiver<ControlMessage>,
    event_tx: Sender<WebSocketEvent>,
    pub event_rx: Receiver<WebSocketEvent>,
}

impl S9AsyncNonBlockingWebSocketClient {
    pub fn connect(uri: &str, options: NonBlockingOptions)-> S9Result<S9AsyncNonBlockingWebSocketClient> {
        Self::connect_with_headers(uri, &HashMap::new(), options)
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>, options: NonBlockingOptions) -> S9Result<S9AsyncNonBlockingWebSocketClient> {
        let (mut socket, _response) = shared::connect_socket(uri, headers)?;

        shared::configure_non_blocking(&mut socket, &options)?;

        let (control_tx, control_rx) = unbounded::<ControlMessage>();
        let (event_tx, event_rx) = unbounded::<WebSocketEvent>();

        Ok(S9AsyncNonBlockingWebSocketClient {
            socket: Some(socket),
            options,
            control_tx,
            control_rx,
            event_tx,
            event_rx
        })
    }

    /// Returns a reference to the underlying WebSocket if it hasn't been moved to the event loop thread yet.
    ///
    /// This provides low-level access to the tungstenite WebSocket for advanced use cases.
    /// Note: This will return `None` after `run()` has been called, as the socket is moved to the event loop thread.
    /// Use with caution as direct manipulation may interfere with the client's operation.
    #[inline]
    pub fn get_socket(&self) -> Option<&WebSocket<MaybeTlsStream<TcpStream>>> {
        self.socket.as_ref()
    }

    /// Returns a mutable reference to the underlying WebSocket if it hasn't been moved to the event loop thread yet.
    ///
    /// This provides low-level access to the tungstenite WebSocket for advanced use cases.
    /// Note: This will return `None` after `run()` has been called, as the socket is moved to the event loop thread.
    /// Use with caution as direct manipulation may interfere with the client's operation.
    #[inline]
    pub fn get_socket_mut(&mut self) -> Option<&mut WebSocket<MaybeTlsStream<TcpStream>>> {
        self.socket.as_mut()
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
                return Err(S9WebSocketError::SocketUnavailable.into());
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
                            Message::Ping(bytes) => {
                                shared::trace_on_ping_message(&bytes);
                                send_or_break!(event_tx, "WebSocketEvent::Ping on Message::Ping", WebSocketEvent::Ping(bytes.to_vec()));
                            },
                            Message::Pong(bytes) => {
                                shared::trace_on_pong_message(&bytes);
                                send_or_break!(event_tx, "WebSocketEvent::Pong on Message::Pong", WebSocketEvent::Pong(bytes.to_vec()));
                            },
                            Message::Close(close_frame) => {
                                shared::trace_on_close_frame(&close_frame);
                                let reason = close_frame.map(|cf| cf.to_string());
                                send_or_log!(event_tx, "WebSocketEvent::ConnectionClosed on Message::Close", WebSocketEvent::ConnectionClosed(reason));
                                send_or_log!(event_tx, "WebSocketEvent::Quit on Message::Close", WebSocketEvent::Quit);
                                break;
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

impl Drop for S9AsyncNonBlockingWebSocketClient {
    fn drop(&mut self) {
        if let Some(socket) = &mut self.socket {
            shared::close_websocket_with_logging(socket, "on Drop");
        }
    }
}
