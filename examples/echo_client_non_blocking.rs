//! Simple echo client example using the pure non-blocking WebSocket client.
//! This is the zero overhead version to process incoming messages.
//!
//! This example connects to a WebSocket echo server, sends some messages
//! and prints the echoed responses.

use s9_websocket::{S9NonBlockingWebSocketClient, NonBlockingOptions, ControlMessage, S9WebSocketClientHandler};
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

        if self.message_count >= 2 {
            println!("Closing connection...");
            self.control_tx.send(ControlMessage::Close()).ok();
        } else {
            // Send a message after receiving one echo via ControlMessage.
            // The echo for this will be received after our connection close attempt.
            println!("Sending Echo!");
            self.control_tx.send(ControlMessage::SendText(format!("Echoed: {}", text))).ok();
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

    // Configure a hot spinning loop with no spin-waiting and zero delay for writing to the socket
    let options = NonBlockingOptions::new()
        .spin_wait_duration(None)?
        .nodelay(true);

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Create control channel
    let (control_tx, control_rx) = unbounded::<ControlMessage>();

    // Send a text message via control channel
    println!("Sending: Hello from s9_websocket #1!");
    control_tx.send(ControlMessage::SendText("Hello from s9_websocket #1!".to_string()))?;

    // Create handler
    let mut handler = EchoHandler { control_tx, message_count: 0 };

    // Start the event loop, which will run until the connection is closed
    // You may offload this to a separate thread and wait for the client to quit
    client.run(&mut handler, control_rx);

    println!("Example completed successfully");
    Ok(())
}