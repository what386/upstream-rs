use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, anyhow};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Request cooperative cancellation of the active command.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

pub fn is_requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

pub fn check() -> Result<()> {
    if is_requested() {
        Err(anyhow!("Operation interrupted by CTRL-C"))
    } else {
        Ok(())
    }
}
