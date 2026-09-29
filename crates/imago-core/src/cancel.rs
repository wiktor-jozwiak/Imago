//! Cooperative cancellation.
//!
//! A [`CancelToken`] is cheap to clone and check. Tokens form a tree: a file's
//! token is cancelled when its job is cancelled, but cancelling one file
//! ("skip") leaves the rest of the job running.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::errors::{ImagoError, Result};

/// A cancellation flag with an optional parent.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    parent: Option<Arc<CancelToken>>,
}

impl CancelToken {
    /// A new, independent token.
    pub fn new() -> Self {
        Self::default()
    }

    /// A child token: cancelled when either it or `self` is cancelled.
    pub fn child(&self) -> Self {
        CancelToken {
            flag: Arc::new(AtomicBool::new(false)),
            parent: Some(Arc::new(self.clone())),
        }
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Whether this token or any ancestor was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_cancelled())
    }

    /// Returns [`ImagoError::Cancelled`] if cancellation was requested.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ImagoError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());
    }

    #[test]
    fn clones_share_the_flag() {
        let token = CancelToken::new();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
        assert!(token.check().unwrap_err().is_cancelled());
    }

    #[test]
    fn parent_cancels_children_but_not_the_other_way_round() {
        let job = CancelToken::new();
        let file_a = job.child();
        let file_b = job.child();

        file_a.cancel();
        assert!(file_a.is_cancelled());
        assert!(!file_b.is_cancelled());
        assert!(!job.is_cancelled());

        job.cancel();
        assert!(file_b.is_cancelled());
    }

    #[test]
    fn tokens_work_across_threads() {
        let token = CancelToken::new();
        let child = token.child();
        let handle = std::thread::spawn(move || {
            while !child.is_cancelled() {
                std::thread::yield_now();
            }
            true
        });
        token.cancel();
        assert!(handle.join().unwrap());
    }
}
