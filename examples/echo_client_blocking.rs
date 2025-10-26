//! Simple echo client example using the blocking WebSocket client.
//!
//! Please note that the blocking client with default options is for chat-like or
//! request-reply protocol applications because the underlying socket waits infinite
//! for incoming messages and won't send outgoing messages in the meantime.
//! For more complex use-cases, consider using the non-blocking client
//!
//! This example connects to a WebSocket echo server, sends some messages
//! and prints the echoed responses.

use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler, ControlMessage};
use crossbeam_channel::unbounded;

struct EchoHandler {
    control_tx: crossbeam_channel::Sender<ControlMessage>,
    message_count: usize,
}

impl S9WebSocketClientHandler for EchoHandler {
    fn on_text_message(&mut self, data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        println!("Received: {}", text);
        self.message_count += 1;

        // Send a message after receiving one echo via ControlMessage.
        // The echo for this will be received after our connection close attempt.
        println!("Sending Echo!");
        self.control_tx.send(ControlMessage::SendText(format!("Echoed: {}", text))).ok();

        if self.message_count == 2 {
            println!("Closing connection...");
            self.control_tx.send(ControlMessage::Close()).ok();
        }
    }

    fn on_binary_message(&mut self, data: &[u8]) {
        println!("Received binary message: {} bytes", data.len());
    }

    fn on_connection_closed(&mut self, reason: Option<String>) {
        println!("Connection closed: {:?}", reason);
    }

    fn on_error(&mut self, error: String) {
        eprintln!("Error: {}", error);
    }

    fn on_quit(&mut self) {
        println!("Client quit");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Default blocking options for infinite read/write blocking.
    let options = s9_websocket::BlockingOptions::new();

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Send a test message via client function, which is shorthand for ControlMessage::SendText
    println!("Sending: Hello from s9_websocket!");
    client.send_text_message("Hello from s9_websocket!")?;

    // Create control channel
    let (control_tx, control_rx) = unbounded::<ControlMessage>();

    // Create handler
    let mut handler = EchoHandler { control_tx, message_count: 0 };

    // Run blocking loop which will run until the connection is closed
    client.run(&mut handler, control_rx);

    println!("Example completed successfully");
    Ok(())
}