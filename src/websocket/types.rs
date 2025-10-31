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

pub(crate) use send_or_break;
pub(crate) use send_or_log;

// ============================================================================
// Public API Types
// ============================================================================

pub trait S9WebSocketClientHandler<C> {
    fn on_activated(&mut self, client: &mut C) {
        // Default: noop
        let _ = client; // Suppress unused variable warning
    }
    fn on_poll(&mut self, client: &mut C) {
        // Default: noop
        let _ = client;
    }
    fn on_idle(&mut self, client: &mut C) {
        // Default: noop
        let _ = client;
    }
    fn on_text_message(&mut self, client: &mut C, data: &[u8]);
    fn on_binary_message(&mut self, client: &mut C, data: &[u8]);
    fn on_connection_closed(&mut self, client: &mut C, reason: Option<String>);
    fn on_ping(&mut self, client: &mut C, _data: &[u8]) {
        // Default: noop
        let _ = client;
    }
    fn on_pong(&mut self, client: &mut C, _data: &[u8]) {
        // Default: noop
        let _ = client;
    }
    fn on_error(&mut self, client: &mut C, error: String);
    fn on_quit(&mut self, client: &mut C) {
        // Default: noop
        let _ = client; //
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
    SendBinary(Vec<u8>),
    SendPing(Vec<u8>),
    SendPong(Vec<u8>),
    Close(),
    ForceQuit(),
}
