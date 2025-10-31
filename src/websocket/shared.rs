use std::collections::HashMap;
use std::net::TcpStream;
use std::str::FromStr;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::Uri;
use tungstenite::protocol::CloseFrame;
use crate::error::{S9Result, S9WebSocketError};
use super::options::{NonBlockingOptions, BlockingOptions};
use super::types::ControlMessage;

// ============================================================================
// Shared Internal Helpers
// ============================================================================

/// Control flow indicator for message handling loops
pub(crate) enum ControlFlow {
    Continue,
    Break,
}

/// Establishes WebSocket connection with optional custom headers
pub(crate) fn connect_socket(uri: &str, headers: &HashMap<String, String>) -> S9Result<(WebSocket<MaybeTlsStream<TcpStream>>, Response)> {
    let uri = Uri::from_str(uri).map_err(|e| {
        tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
        S9WebSocketError::InvalidUri(e.to_string())
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
        ControlMessage::SendBinary(data) => {
            if let Err(e) = send_binary_message_to_websocket(socket, data) {
                return Err(format!("Error sending binary: {}", e));
            }
            Ok(ControlFlow::Continue)
        },
        ControlMessage::SendPing(data) => {
            if let Err(e) = send_ping_to_websocket(socket, data) {
                return Err(format!("Error sending ping: {}", e));
            }
            Ok(ControlFlow::Continue)
        },
        ControlMessage::SendPong(data) => {
            if let Err(e) = send_pong_to_websocket(socket, data) {
                return Err(format!("Error sending pong: {}", e));
            }
            Ok(ControlFlow::Continue)
        },
        ControlMessage::Close() => {
            close_websocket_with_logging(socket, "ControlMessage::Close");
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
            S9WebSocketError::from(e).into()
        })
}

/// Sends binary message to WebSocket
#[inline]
pub(crate) fn send_binary_message_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, data: Vec<u8>) -> S9Result<()> {
    socket.send(Message::Binary(data.into()))
        .map(|_| {
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("Sent binary message");
            }
        })
        .map_err(|e| {
            tracing::error!("Error sending binary message: {}", e);
            S9WebSocketError::from(e).into()
        })
}

/// Sends ping to WebSocket
#[inline]
pub(crate) fn send_ping_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, data: Vec<u8>) -> S9Result<()> {
    socket.send(Message::Ping(data.into()))
        .map(|_| {
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("Sent ping");
            }
        })
        .map_err(|e| {
            tracing::error!("Error sending ping: {}", e);
            S9WebSocketError::from(e).into()
        })
}

/// Sends pong to WebSocket
#[inline]
pub(crate) fn send_pong_to_websocket(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, data: Vec<u8>) -> S9Result<()> {
    socket.send(Message::Pong(data.into()))
        .map(|_| {
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!("Sent pong");
            }
        })
        .map_err(|e| {
            tracing::error!("Error sending pong: {}", e);
            S9WebSocketError::from(e).into()
        })
}

/// Determines if an error message indicates a connection closure
#[inline]
pub(crate) fn is_connection_closed_error(error_msg: &str) -> bool {
    // TODO: Find a type safe and reliable way to detect connection closure errors
    error_msg.contains("Connection closed") || error_msg.contains("closed")
}

/// Closes WebSocket connection with context logging
pub(crate) fn close_websocket_with_logging(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, context: &str) {
    if socket.can_write() {
        socket.close(None)
            .map(|_| {
                tracing::trace!("Connection close successfully requested for context: {}", context);
            })
            .unwrap_or_else(|e| {
                tracing::error!("Error on connection close request for context {}: {}", context, e);
            });
    }
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

/// Traces pong message receipt
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
