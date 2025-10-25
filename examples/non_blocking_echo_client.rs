//! Simple echo client example using the non-blocking WebSocket client.
//!
//! This example connects to a WebSocket echo server, sends a message,
//! and prints the echoed response.

use s9_websocket::{S9NonBlockingWebSocketClient, WebSocketEvent, NonBlockingOptions, ControlMessage};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org")?;

    // Configure with 10ms sleep between reads
    let options = NonBlockingOptions::new(Some(Duration::from_millis(10)))?;

    // Start the non-blocking event loop
    client.run_non_blocking(options)?;

    // Send a test message
    client.send_text_message("Hello from s9_websocket!")?;
    println!("Sent: Hello from s9_websocket!");

    // Handle events
    let mut message_count = 0;
    loop {
        match client.event_rx.recv() {
            Ok(WebSocketEvent::Activated) => {
                println!("WebSocket connection activated");
            }
            Ok(WebSocketEvent::TextMessage(data)) => {
                let text = String::from_utf8_lossy(&data);
                println!("Received: {}", text);
                message_count += 1;

                // After receiving one echo, we can break the loop immediately
                // or wait for a WebSocketEvent::ConnectionClosed + WebSocketEvent::Quit event
                if message_count >= 1 {
                    println!("Closing connection...");
                    client.control_tx.send(ControlMessage::Close())?;
                }
            }
            Ok(WebSocketEvent::ConnectionClosed(reason)) => {
                println!("Connection closed: {:?}", reason);
                break;
            }
            Ok(WebSocketEvent::Error(err)) => {
                eprintln!("Error: {}", err);
                break;
            }
            Ok(WebSocketEvent::Quit) => {
                println!("Client quit");
                break;
            }
            _ => {}
        }
    }

    println!("Example completed successfully");
    Ok(())
}