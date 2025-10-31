//! Simple echo client example using the async non-blocking WebSocket client.
//! Utilizes a dedicated thread for spinning and channels for async communication.
//!
//! This example connects to a WebSocket echo server, sends some messages
//! and prints the echoed responses.

use s9_websocket::{S9AsyncNonBlockingWebSocketClient, WebSocketEvent, NonBlockingOptions, ControlMessage};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Configure with 10ms sleep between reads, to save CPU cycles
    let options = NonBlockingOptions::new()
        .spin_wait_duration(Some(Duration::from_millis(10)))?;

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9AsyncNonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Start the event loop, which will start thread and return the handle immediately
    let client_thread = client.run()?;

    // Send a text message using control channel
    client.control_tx.send(ControlMessage::SendText("Hello from s9_websocket!".to_string()))?;
    println!("Sent: Hello from s9_websocket!");

    let control_tx = client.control_tx.clone();

    // Spawn a new thread for handling events
    let our_thread = std::thread::spawn(move || {
        // Handle events
        let mut message_count = 0;
        loop {
            match client.event_rx.recv() {
                Ok(WebSocketEvent::Activated) => {
                    println!("WebSocket read thread activated");
                }
                Ok(WebSocketEvent::TextMessage(data)) => {
                    let text = String::from_utf8_lossy(&data);
                    println!("Received: {}", text);
                    message_count += 1;

                    if message_count <= 2 {
                        // Echo the messages
                        println!("Sending Echo!");
                        client.control_tx.send(ControlMessage::SendText(format!("Echoed: {}", text))).ok();
                    }

                    // After closing, we can break our loop immediately
                    // or wait for a WebSocketEvent::ConnectionClosed + WebSocketEvent::Quit event
                    if message_count == 3 {
                        println!("Closing connection...");
                        client.control_tx.send(ControlMessage::Close()).ok();
                    }
                }
                Ok(WebSocketEvent::ConnectionClosed(reason)) => {
                    println!("Connection closed: {:?}", reason);
                }
                Ok(WebSocketEvent::Error(err)) => {
                    eprintln!("Error: {}", err);
                }
                Ok(WebSocketEvent::Quit) => {
                    println!("Client quit");
                    break;
                }
                _ => {}
            }
        }
    });

    // Outside our thread we can use message passing to send messages via control channel
    control_tx.send(ControlMessage::SendText("Hello from s9_websocket again!".to_string()))?;

    // Optionally wait for the event loops
    println!("Waiting for the event loops to finish...");
    client_thread.join().ok();
    our_thread.join().ok();

    println!("Example completed successfully");
    Ok(())
}