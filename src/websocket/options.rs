use std::time::Duration;
use crate::error::{S9Result, S9WebSocketError};

// ============================================================================
// Configuration options
// ============================================================================

#[derive(Debug, Clone, Default)]
pub(crate) struct SharedOptions {
    pub(crate) spin_wait_duration: Option<Duration>,
    pub(crate) nodelay: Option<bool>,
    pub(crate) ttl: Option<u32>,
}

/// Configuration options for the non-blocking WebSocket client.
#[derive(Debug, Clone, Default)]
pub struct NonBlockingOptions {
    pub(crate) shared: SharedOptions,
}

impl NonBlockingOptions {
    /// Creates a new `NonBlockingOptions` builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the duration to wait in the event loop after control message processing and socket reads.
    /// Must be None or greater than zero
    /// This prevents the event loop from consuming 100% CPU.
    pub fn spin_wait_duration(mut self, duration: Option<Duration>) -> S9Result<Self> {
        if let Some(duration) = duration {
            if duration.is_zero() {
                return Err(S9WebSocketError::InvalidConfiguration("Spin wait duration cannot be zero".to_string()).into());
            }
        }
        self.shared.spin_wait_duration = duration;
        Ok(self)
    }

    /// Enables or disables the `TCP_NODELAY` option for messages to be sent.
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.shared.nodelay = Some(nodelay);
        self
    }

    /// Sets the TTL (Time To Live, # of hops) for the socket.
    /// None for the system default
    pub fn ttl(mut self, ttl: Option<u32>) -> S9Result<Self> {
        self.shared.ttl = ttl;
        Ok(self)
    }
}

/// Configuration options for the blocking WebSocket client.
#[derive(Debug, Clone, Default)]
pub struct BlockingOptions {
    pub(crate) shared: SharedOptions,
    pub(crate) read_timeout: Option<Duration>,
    pub(crate) write_timeout: Option<Duration>,
}

impl BlockingOptions {
    /// Creates a new `BlockingOptions` builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the duration to wait in the event loop after control message processing and socket reads.
    /// Must be None or greater than zero
    /// This prevents the event loop from consuming 100% CPU.
    pub fn spin_wait_duration(mut self, duration: Option<Duration>) -> S9Result<Self> {
        if let Some(duration) = duration {
            if duration.is_zero() {
                return Err(S9WebSocketError::InvalidConfiguration("Spin wait duration cannot be zero".to_string()).into());
            }
        }
        self.shared.spin_wait_duration = duration;
        Ok(self)
    }

    /// Enables or disables the `TCP_NODELAY` option for messages to be sent.
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.shared.nodelay = Some(nodelay);
        self
    }

    /// Sets the TTL (Time To Live, # of hops) for the socket.
    /// None for the system default
    pub fn ttl(mut self, ttl: Option<u32>) -> S9Result<Self> {
        self.shared.ttl = ttl;
        Ok(self)
    }

    /// Sets the read timeout for the socket.
    /// Must be None for the indefinitely blocking of socket read or greater than zero
    pub fn read_timeout(mut self, timeout: Option<Duration>) -> S9Result<Self> {
        if let Some(timeout) = timeout {
            if timeout.is_zero() {
                return Err(S9WebSocketError::InvalidConfiguration("Read timeout duration cannot be zero".to_string()).into());
            }
        }
        self.read_timeout = timeout;
        Ok(self)
    }

    /// Sets the write timeout for the socket.
    /// /// Must be None for the indefinitely blocking of socket write or greater than zero
    pub fn write_timeout(mut self, timeout: Option<Duration>) -> S9Result<Self> {
        if let Some(timeout) = timeout {
            if timeout.is_zero() {
                return Err(S9WebSocketError::InvalidConfiguration("Write timeout duration cannot be zero".to_string()).into());
            }
        }
        self.write_timeout = timeout;
        Ok(self)
    }
}
