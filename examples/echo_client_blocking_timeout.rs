//! Simple echo client example using the blocking WebSocket client.
//! The blocking client with options set to timeout on read and write operations simulates
//! non-blocking behavior.
//! For more complex use-cases, consider using the non-blocking client
//!
//! This example connects to a WebSocket echo server, sends some messages
//! and prints the echoed responses.

use std::time::Duration;
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
        if self.message_count <= 2 {
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

    // Default blocking options for infinite read/write blocking.
    let options = s9_websocket::BlockingOptions::new()
        .read_timeout(Some(Duration::from_micros(100)))?
        .write_timeout(Some(Duration::from_micros(100)))?;

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Create control channel
    let (control_tx, control_rx) = unbounded::<ControlMessage>();
    let control_tx_for_thread = control_tx.clone();

    // Create handler
    let mut handler = EchoHandler { control_tx, message_count: 0 };

    let close_thread = std::thread::spawn(move || {
        // Wait some time to ensure the client event loop is running and send a close message
        std::thread::sleep(Duration::from_secs(3));
        println!("Sending Close");
        control_tx_for_thread.send(ControlMessage::Close()).ok();
    });

    // Run blocking loop which will run until the connection is closed
    let client_thread = std::thread::spawn(move || {
        client.run(&mut handler, control_rx);
    });

    // Wait for threads to finish
    close_thread.join().ok();
    client_thread.join().ok();

    println!("Example completed successfully");
    Ok(())
}