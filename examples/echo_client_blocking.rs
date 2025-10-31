//! Simple echo client example using the blocking WebSocket client.
//!
//! Please note that the blocking client with default options is for chat-like or
//! request-reply protocol applications because the underlying socket waits indefinitely
//! for incoming messages and won't send outgoing messages in the meantime.
//! For more complex use-cases, consider using the non-blocking client
//!
//! This example connects to a WebSocket echo server, sends some messages
//! and prints the echoed responses.

use s9_websocket::{S9BlockingWebSocketClient, S9WebSocketClientHandler};

struct EchoHandler {
    message_count: usize,
}

impl S9WebSocketClientHandler<S9BlockingWebSocketClient> for EchoHandler {
    // Implement only what you need
    fn on_text_message(&mut self, client: &mut S9BlockingWebSocketClient, data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        println!("Received: {}", text);
        self.message_count += 1;

        // Send a message after receiving one echo
        if self.message_count == 1 {
            println!("Sending Echo!");
            client.send_text_message(&format!("Echoed: {}", text)).ok();
        }

        if self.message_count == 2 {
            println!("Closing connection...");
            client.close();
        }
    }

    fn on_binary_message(&mut self, _client: &mut S9BlockingWebSocketClient, data: &[u8]) {
        println!("Received binary message: {} bytes", data.len());
    }

    fn on_connection_closed(&mut self, _client: &mut S9BlockingWebSocketClient, reason: Option<String>) {
        println!("Connection closed: {:?}", reason);
    }

    fn on_error(&mut self, _client: &mut S9BlockingWebSocketClient, error: String) {
        eprintln!("Error: {}", error);
    }

    fn on_quit(&mut self, _client: &mut S9BlockingWebSocketClient) {
        println!("Client quit");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Default blocking options for indefinitely read/write blocking.
    let options = s9_websocket::BlockingOptions::new();

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9BlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Send a text message
    println!("Sending: Hello from s9_websocket!");
    client.send_text_message("Hello from s9_websocket!")?;

    // Create handler
    let mut handler = EchoHandler { message_count: 0 };

    // Run blocking loop which will run until the connection is closed
    client.run(&mut handler);

    println!("Example completed successfully");
    Ok(())
}