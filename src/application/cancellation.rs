use std::sync::OnceLock;

use anyhow::Result;
use std::fmt;
use tokio_util::sync::CancellationToken;

/// Error returned when the active command was interrupted by SIGINT.
#[derive(Debug)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SIGINT")
    }
}

impl std::error::Error for Cancelled {}

/// Cooperative cancellation handle for one command invocation.
#[derive(Clone, Debug)]
pub struct Cancellation {
    token: CancellationToken,
}

impl Cancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    pub fn request(&self) {
        self.token.cancel();
    }

    pub fn is_requested(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn check(&self) -> Result<()> {
        if self.is_requested() {
            Err(Cancelled.into())
        } else {
            Ok(())
        }
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

// Compatibility access for the existing operation graph. New code should carry
// a cloned Cancellation handle instead of using these process-level helpers.
static CURRENT: OnceLock<Cancellation> = OnceLock::new();

pub fn current() -> Cancellation {
    CURRENT.get_or_init(Cancellation::new).clone()
}

pub fn request() {
    current().request();
}

pub fn is_requested() -> bool {
    current().is_requested()
}

pub fn check() -> Result<()> {
    current().check()
}

pub async fn cancelled() {
    current().cancelled().await;
}

#[cfg(test)]
mod tests {
    use super::{Cancellation, Cancelled};

    #[tokio::test]
    async fn request_wakes_waiters_and_returns_typed_error() {
        let cancellation = Cancellation::new();
        let waiter = cancellation.clone();
        let task = tokio::spawn(async move {
            waiter.cancelled().await;
            waiter.check().unwrap_err()
        });

        cancellation.request();
        let error = task.await.expect("cancellation waiter");
        assert!(error.downcast_ref::<Cancelled>().is_some());
    }

    #[test]
    fn ordinary_handle_starts_uncancelled() {
        let cancellation = Cancellation::new();
        assert!(!cancellation.is_requested());
        cancellation.check().expect("not cancelled");
    }
}
