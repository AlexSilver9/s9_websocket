//! Simple echo client example using the pure non-blocking WebSocket client.
//! This is the zero overhead version to process incoming messages.
//!
//! This example also demonstrates how to use a signal channel to control the client
//! from external threads (e.g., CTRL-C handler, timeout threads) using on_idle().

use std::time::Duration;
use s9_websocket::{S9NonBlockingWebSocketClient, NonBlockingOptions, S9WebSocketClientHandler};
use crossbeam_channel::{unbounded, Receiver};

/// External signals that can be sent to the client from other threads
enum ExternalSignal {
    Close,
    ForceQuit,
}

struct EchoHandler {
    message_count: usize,
    signal_rx: Receiver<ExternalSignal>,
    closing: bool,
}

impl S9WebSocketClientHandler<S9NonBlockingWebSocketClient> for EchoHandler {
    fn on_activated(&mut self, _client: &mut S9NonBlockingWebSocketClient) {
        println!("WebSocket client activated");
    }

    fn on_idle(&mut self, client: &mut S9NonBlockingWebSocketClient) {
        // Check for external signals from other threads when no data is available (WouldBlock/TimedOut)
        if let Ok(signal) = self.signal_rx.try_recv() {
            match signal {
                ExternalSignal::Close => {
                    if !self.closing {
                        println!("Received close signal from external thread - sending CloseFrame");
                        client.close();
                        self.closing = true;
                    }
                },
                ExternalSignal::ForceQuit => {
                    println!("Received force quit signal from external thread - breaking event loop immediately");
                    client.force_quit();
                }
            }
        }
    }

    fn on_text_message(&mut self, client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
        // Normal text message processing, continues even after Close is sent
        let text = String::from_utf8_lossy(data);
        if self.closing {
            println!("Received (while closing): {}", text);
        } else {
            println!("Received: {}", text);
        }
        self.message_count += 1;

        if !self.closing && self.message_count <= 2 {
            // Send a message after receiving one echo
            println!("Sending Echo!");
            client.send_text_message(&format!("Echoed: {}", text)).ok();
        }
    }

    fn on_binary_message(&mut self, _client: &mut S9NonBlockingWebSocketClient, data: &[u8]) {
        println!("Received binary message: {} bytes", data.len());
    }

    fn on_connection_closed(&mut self, _client: &mut S9NonBlockingWebSocketClient, reason: Option<String>) {
        println!("Connection closed: {:?}", reason);
    }

    fn on_error(&mut self, _client: &mut S9NonBlockingWebSocketClient, error: String) {
        eprintln!("Error: {}", error);
    }

    fn on_quit(&mut self, _client: &mut S9NonBlockingWebSocketClient) {
        println!("Client quit");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Configure a hot loop with no spin-waiting and zero delay for writing to the socket
    // Alternatively some spin-wait to reduce CPU usage while still being responsive
    let options = NonBlockingOptions::new()
        .spin_wait_duration(None)?
        .nodelay(true);

    // Connect to the WebSocket echo server
    println!("Connecting to echo.websocket.org...");
    let mut client = S9NonBlockingWebSocketClient::connect("wss://echo.websocket.org", options)?;

    // Send initial message
    println!("Sending: Hello from s9_websocket #1!");
    client.send_text_message("Hello from s9_websocket #1!")?;

    // Create signal channel for external control
    let (signal_tx, signal_rx) = unbounded::<ExternalSignal>();

    // Create handler with signal receiver
    let mut handler = EchoHandler {
        message_count: 0,
        signal_rx,
        closing: false,
    };

    // Spawn a thread to simulate CTRL-C handler or timeout
    // This thread will send a Close signal after 5 seconds
    let signal_tx_clone = signal_tx.clone();
    let _close_thread = std::thread::spawn(move || {
        println!("External thread: Waiting 5 seconds before sending close signal...");
        std::thread::sleep(Duration::from_secs(5));
        println!("External thread: Sending Close signal");
        signal_tx_clone.send(ExternalSignal::Close).ok();

        // If server doesn't respond with CloseFrame within 2 seconds, force quit
        std::thread::sleep(Duration::from_secs(2));
        println!("External thread: Timeout reached, sending ForceQuit signal");
        signal_tx_clone.send(ExternalSignal::ForceQuit).ok();
    });

    // Start the event loop, which will run until the connection is closed or forcibly quit
    client.run(&mut handler);

    println!("Example completed successfully");
    Ok(())
}