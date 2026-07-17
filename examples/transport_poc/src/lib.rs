use std::time::Duration;

use boltffi::*;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct TransportConfig {
    pub baud_rate: u32,
    pub label: String,
}

#[error]
#[derive(Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum TransportError {
    NotConfigured = 0,
    Timeout = 1,
    Io(String) = 2,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "transport not configured"),
            Self::Timeout => write!(f, "transport operation timed out"),
            Self::Io(message) => write!(f, "transport I/O error: {message}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<UnexpectedFfiCallbackError> for TransportError {
    fn from(error: UnexpectedFfiCallbackError) -> Self {
        Self::Io(error.0)
    }
}

#[export]
#[allow(async_fn_in_trait)]
pub trait Transport: Send + Sync {
    async fn configure(&self, config: TransportConfig) -> Result<(), TransportError>;

    /// Reads at most `maximum_bytes`. Returning an empty vector is permitted
    /// for a non-blocking transport and is treated like no progress.
    async fn read(&self, maximum_bytes: u32, timeout: Duration) -> Result<Vec<u8>, TransportError>;

    /// Returns only after the complete buffer has been accepted by the
    /// underlying transport.
    async fn write_all(&self, data: Vec<u8>) -> Result<(), TransportError>;
}

/// Drives a full configure -> write -> read round trip through a
/// Dart/JS-implemented `Transport`, to exercise every method through one
/// exported call.
#[export]
pub async fn run_transport_roundtrip(
    transport: impl Transport,
    payload: Vec<u8>,
) -> Result<Vec<u8>, TransportError> {
    transport
        .configure(TransportConfig {
            baud_rate: 115_200,
            label: "poc".to_string(),
        })
        .await?;
    transport.write_all(payload.clone()).await?;
    transport
        .read(payload.len() as u32, Duration::from_millis(500))
        .await
}
