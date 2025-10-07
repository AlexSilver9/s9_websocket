use std::collections::{HashMap, VecDeque};
use std::net::TcpStream;
use std::{str, thread};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, ClientRequestBuilder, Error, Message, Utf8Bytes, WebSocket};
use tungstenite::handshake::client::Response;
use tungstenite::http::{Uri};
use tungstenite::protocol::CloseFrame;

// TODO: add non-blocking, idea: https://github.com/haxpor/bybit-shiprekt/blob/6c3c5693d675fc997ce5e76df27e571f2aaaf291/src/main.rs

pub trait S9WebSocketClientHandler {
    fn on_text_message(&mut self, data: &[u8]);
    fn on_binary_message(&mut self, data: &[u8]);
    fn on_connection_closed(&mut self, reason: Option<String>);
    fn on_error(&mut self, error: String);
    fn on_ping(&mut self, _data: &[u8]) {
        // Default: noop
    }
    fn on_pong(&mut self, _data: &[u8]) {
        // Default: noop
    }
    fn on_quit(&mut self) {
        // Default: noop
    }
}

pub enum ControlMessage {
    SendText(String),
    Close(),
    ForceQuit(),
}

pub struct MessageQueueStats {
    pub pending_messages: usize,
    pub max_queue_size: usize,
    pub average_processing_time: Duration,
    pub last_message_time: Option<Instant>,
    pub messages_dropped: u64, // TODO: Remove
    pub total_messages_processed: u64,
}

enum InternalMessage {
    WebSocketMessage(Message, Instant),
    ConnectionClosed(Option<String>),
    Error(String),
    Quit,
}


pub struct S9WebSocketClient {
    message_queue: Arc<Mutex<VecDeque<(InternalMessage)>>>,
    queue_stats: Arc<Mutex<MessageQueueStats>>,
    max_queue_size: usize,
    running: Arc<AtomicBool>,
    control_tx: mpsc::Sender<ControlMessage>,
}

impl S9WebSocketClient {
    pub fn connect(uri: &str, max_queue_size: usize) -> Result<S9WebSocketClient, Error> {
        Self::connect_with_headers(uri, &HashMap::new(), max_queue_size)
    }

    pub fn connect_with_headers(uri: &str, headers: &HashMap<String, String>, max_queue_size: usize) -> Result<S9WebSocketClient, Error> {
        let uri = match Self::get_uri(uri) {
            Ok(value) => value,
            Err(error) => return error,
        };

        let mut builder = ClientRequestBuilder::new(uri);
        for (key, value) in headers {
            builder = builder.with_header(key, value);
        }

        let result = tungstenite::connect(builder);
        match result {
            Ok((socket, response)) => {
                Self::trace_on_connected(response);

                let message_queue = Arc::new(Mutex::new(VecDeque::new()));
                let queue_stats = Arc::new(Mutex::new(MessageQueueStats {
                    pending_messages: 0,
                    max_queue_size,
                    average_processing_time: Duration::from_micros(0),
                    last_message_time: None,
                    messages_dropped: 0,
                    total_messages_processed: 0,
                }));

                let running = Arc::new(AtomicBool::new(true));
                let (control_tx, control_rx) = mpsc::channel();

                let reader_queue = message_queue.clone();
                let reader_stats = queue_stats.clone();
                let reader_running = running.clone();

                thread::spawn(move || {
                    Self::reader_thread(socket, reader_queue, reader_stats, reader_running, control_rx, max_queue_size);
                });

                Ok(S9WebSocketClient {
                    message_queue,
                    queue_stats,
                    max_queue_size,
                    running,
                    control_tx,
                })
            },
            Err(e) => Err(e)
        }
    }

    fn reader_thread(
        mut socket: WebSocket<MaybeTlsStream<TcpStream>>,
        message_queue: Arc<Mutex<VecDeque<InternalMessage>>>,
        queue_stats: Arc<Mutex<MessageQueueStats>>,
        running: Arc<AtomicBool>,
        control_rx: mpsc::Receiver<ControlMessage>,
        max_queue_size: usize,
    ) {
        while running.load(Ordering::Relaxed) {
            // Handle control messages
            if let Ok(control_message) = control_rx.try_recv() {
                match control_message {
                    ControlMessage::SendText(text) => {
                        if let Err(e) = Self::send_text_message_internal(&mut socket, &text) {
                            Self::enqueue_message(
                                &message_queue,
                                &queue_stats,
                                InternalMessage::Error(format!("Error sending text: {}", e)),
                                max_queue_size,
                            );
                        }
                    },
                    ControlMessage::Close() => {
                        Self::close_internal(&mut socket);
                    },
                    ControlMessage::ForceQuit() => {
                        if tracing::enabled!(tracing::Level::TRACE) {
                            tracing::trace!("S9WebSocketClient forcibly quitting message loop");
                        }
                        Self::enqueue_message(
                            &message_queue,
                            &queue_stats,
                            InternalMessage::Quit,
                            max_queue_size,
                        );
                        break;
                    }
                }
            }

            // Read WebSocket messages
            match socket.read() {
                Ok(msg) => {
                    let receive_time = Instant::now();
                    Self::enqueue_message(
                        &message_queue,
                        &queue_stats,
                        InternalMessage::WebSocketMessage(msg, receive_time),
                        max_queue_size,
                    );
                },
                Err(e) => {
                    match e {
                        Error::ConnectionClosed => {
                            Self::enqueue_message(
                                &message_queue,
                                &queue_stats,
                                InternalMessage::ConnectionClosed(Some("Connection closed".to_string())),
                                max_queue_size,
                            );
                        },
                        _ => {
                            Self::enqueue_message(
                                &message_queue,
                                &queue_stats,
                                InternalMessage::Error(format!("S9WebSocketClient error reading message: {}", e)),
                                max_queue_size,
                            );
                        }
                    }
                    Self::enqueue_message(
                        &message_queue,
                        &queue_stats,
                        InternalMessage::Quit,
                        max_queue_size,
                    );
                    break;
                }
            }
        }

        running.store(false, Ordering::Relaxed);
    }

    fn enqueue_message(
        message_queue: &Arc<Mutex<VecDeque<InternalMessage>>>,
        queue_stats: &Arc<Mutex<MessageQueueStats>>,
        message: InternalMessage,
        max_queue_size: usize,
    ) {
        let mut queue = message_queue.lock().unwrap();
        let mut stats = queue_stats.lock().unwrap();

        // Check if queue is full
        if queue.len() >= max_queue_size {
            if let Some(dropped) = queue.pop_front() {
                stats.messages_dropped += 1;
                if let InternalMessage::WebSocketMessage(msg, _) = dropped {
                    tracing::warn!("Dropped message due to queue overflow: {:?}", msg);
                }
            }
        }

        queue.push_back(message);
        stats.pending_messages = queue.len();
        stats.last_message_time = Some(Instant::now());

    }

    #[inline]
    pub fn run<HANDLER>(&mut self, handler: &mut HANDLER)
    where
        HANDLER: S9WebSocketClientHandler,
    {
        while self.running.load(Ordering::Relaxed) {
            let message = {
                let mut queue = self.message_queue.lock().unwrap();
                queue.pop_front()
            };

            if let Some(msg) = message {
                let process_start = Instant::now();

                match msg {
                    InternalMessage::WebSocketMessage(ws_msg, _receive_time) => {
                        self.process_websocket_message(ws_msg, handler);
                    },
                    InternalMessage::ConnectionClosed(reason) => {
                        handler.on_connection_closed(reason);
                        handler.on_quit();
                        break;
                    },
                    InternalMessage::Error(error) => {
                        handler.on_error(error);
                    },
                    InternalMessage::Quit => {
                        handler.on_quit();
                        break;
                    }
                }

                let process_duration = process_start.elapsed();

                // Update stats
                {
                    let mut stats = self.queue_stats.lock().unwrap();
                    let queue = self.message_queue.lock().unwrap();
                    stats.pending_messages = queue.len();
                    stats.total_messages_processed += 1;

                    // Simple exponential moving average
                    let alpha = 0.1;
                    let current_nanos = stats.average_processing_time.as_nanos() as f64;
                    let new_nanos = process_duration.as_nanos() as f64;
                    let updated_nanos = (alpha * new_nanos + (1.0 - alpha) * current_nanos) as u128;
                    stats.average_processing_time = Duration::from_nanos(updated_nanos as u64);
                }
            } else {
                // No messages available, wait for a message
                std::thread::sleep(Duration::from_micros(100));
            }
        }
    }

    fn process_websocket_message<HANDLER>(&self, msg: Message, handler: &mut HANDLER)
    where
        HANDLER: S9WebSocketClientHandler,
    {
        match msg {
            Message::Text(message) => {
                Self::trace_on_text_message(&message);
                handler.on_text_message(message.as_bytes());
            },
            Message::Binary(bytes) => {
                Self::trace_on_binary_message(&bytes);
                handler.on_binary_message(&bytes);
            },
            Message::Close(close_frame) => {
                Self::trace_on_close(&close_frame);
                let reason = close_frame.map(|cf| cf.to_string());
                handler.on_connection_closed(reason);
                handler.on_quit();
            },
            Message::Ping(bytes) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9WebSocketClient received ping frame: {}", String::from_utf8_lossy(&bytes));
                }
                handler.on_ping(&bytes);
            },
            Message::Pong(bytes) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9WebSocketClient received pong frame: {}", String::from_utf8_lossy(&bytes));
                }
                handler.on_pong(&bytes);
            },
            Message::Frame(_) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9WebSocketClient received frame from server");
                }
                // No handling for frames until use case needs it
            }
        }
    }

    pub fn get_queue_stats(&self) -> MessageQueueStats {
        let stats = self.queue_stats.lock().unwrap();
        MessageQueueStats {
            pending_messages: stats.pending_messages,
            max_queue_size: stats.max_queue_size,
            average_processing_time: stats.average_processing_time,
            last_message_time: stats.last_message_time,
            messages_dropped: stats.messages_dropped,
            total_messages_processed: stats.total_messages_processed,
        }
    }

    #[inline]
    pub fn send_text_pong_for_text_ping(&mut self, ping_message: &str) -> Result<(), mpsc::SendError<ControlMessage>> {
        let mut pong = String::with_capacity(ping_message.len());
        pong.push_str(&ping_message[..3]);
        pong.push('o');
        pong.push_str(&ping_message[4..]);
        self.send_text_message(pong.as_str())
    }

    #[inline]
    /*pub fn send_text_message(&mut self, s: &str) -> Result<(), Error> {
        let msg = Message::text(s);
        let send_result = self.socket.send(msg);
        match send_result {
            Ok(()) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9WebSocketClient sent text message: {}", String::from(s));
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("S9WebSocketClient error sending text message: {}", e);
                Err(e)
            }
        }
    }*/
    pub fn send_text_message(&self, text: &str) -> Result<(), mpsc::SendError<ControlMessage>> {
        self.control_tx.send(ControlMessage::SendText(text.to_string()))
    }

    /*pub fn close(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9WebSocketClient connection close requested");
                },
                Err(e) => {
                    tracing::error!("S9WebSocketClient error on connection close request: {}", e);
                }
            }
        }
    }*/
    pub fn close(&self) -> Result<(), mpsc::SendError<ControlMessage>> {
        self.control_tx.send(ControlMessage::Close())
    }

    pub fn force_quit(&self) -> Result<(), mpsc::SendError<ControlMessage>> {
        self.control_tx.send(ControlMessage::ForceQuit())
    }

    fn send_text_message_internal(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, text: &str) -> Result<(), Error> {
        let msg = Message::text(text);
        let send_result = socket.send(msg);
        match send_result {
            Ok(()) => {
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!("S9WebSocketClient sent text message: {}", text);
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("S9WebSocketClient error sending text message: {}", e);
                Err(e)
            }
        }
    }

    fn close_internal(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
        if socket.can_read() {
            let close_result = socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9WebSocketClient connection close requested");
                },
                Err(e) => {
                    tracing::error!("S9WebSocketClient error on connection close request: {}", e);
                }
            }
        }
    }

    fn get_uri(uri: &str) -> Result<Uri, Result<S9WebSocketClient, Error>> {
        let uri: Uri = match Uri::from_str(uri) {
            Ok(uri) => uri,
            Err(e) => {
                tracing::error!("S9WebSocketClient error connecting to invalid URI: {}", uri);
                return Err(Err(Error::from(e)));
            }
        };
        Ok(uri)
    }

    fn trace_on_connected(response: Response) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("S9WebSocketClient connected to the server");
            tracing::trace!("S9WebSocketClient response HTTP code: {}", response.status());
            tracing::trace!("S9WebSocketClient response contains the following headers:");
            for (header, _value) in response.headers() {
                tracing::trace!("* {header}");
            }
        }
    }

    #[inline]
    fn trace_on_text_message(message: &Utf8Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("S9WebSocketClient received text message: {}", message);
        }
    }

    #[inline]
    fn trace_on_binary_message(bytes: &Bytes) {
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!("S9WebSocketClient received binary message: {:?}", bytes);
        }
    }

    fn trace_on_close(close_frame: &Option<CloseFrame>) {
        if tracing::enabled!(tracing::Level::TRACE) {
            match close_frame {
                Some(reason) => {
                    tracing::trace!("S9WebSocketClient connection closed with reason: {}", reason)
                },
                None => {
                    tracing::trace!("S9WebSocketClient connection closed without reason")
                },
            }
        }
    }
}

impl Drop for S9WebSocketClient {
    /*fn drop(&mut self) {
        if self.socket.can_read() {
            let close_result = self.socket.close(None);
            match close_result {
                Ok(_) => {
                    tracing::trace!("S9WebSocketClient connection close successfully requested on Drop");
                },
                Err(e) => {
                    tracing::error!("S9WebSocketClient error on connection close request on Drop: {}", e);
                }
            }
        }
    }*/
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.force_quit(); // Signal the reader thread to quit
    }
}
