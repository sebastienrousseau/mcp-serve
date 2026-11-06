// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Sebastien Rousseau. All rights reserved.

//! The opt-in time limit on one tool call.
//!
//! Ported from noyalib-mcp, the one server that runs its tools off the
//! async workers. A server whose tools are quick and run inline on the
//! async worker does not call this, and nothing in the transports
//! imposes it.

use std::fmt;
use std::time::Duration;

/// How long one tool call may run before the client is answered with
/// an error, for a server that has no reason to choose otherwise.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Run a tool's blocking `work` on the blocking pool, off the async
/// workers, and answer with an error if it outlasts `limit`.
///
/// File I/O and parsing block. On an async worker thread a slow call
/// would stall every other request on that worker, `ping` included.
/// A call past its time limit is answered at once; its thread cannot
/// be interrupted and finishes in the background, its result
/// discarded. A panic in `work` is reported as an `internal:` error
/// rather than taking the session down.
///
/// Must be awaited inside a Tokio runtime, which every transport of
/// this crate provides.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let quick = mcp_serve::run_blocking(Duration::from_secs(1), || {
///     Ok::<_, String>(2 + 2)
/// });
/// assert_eq!(quick.await, Ok(4));
///
/// let slow = mcp_serve::run_blocking(Duration::from_millis(1), || {
///     std::thread::sleep(Duration::from_millis(200));
///     Ok::<_, String>(())
/// });
/// assert!(slow.await.unwrap_err().contains("1 ms time limit"));
/// # });
/// ```
///
/// # Errors
///
/// `work`'s own error; a message naming the limit when it ran past
/// it; or an `internal:` message when it panicked.
pub async fn run_blocking<T, F>(limit: Duration, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match tokio::time::timeout(limit, tokio::task::spawn_blocking(work)).await {
        Ok(joined) => joined.unwrap_or_else(|e| Err(internal(e))),
        Err(_) => Err(format!(
            "the call ran past the {} ms time limit and was abandoned",
            limit.as_millis()
        )),
    }
}

fn internal(e: impl fmt::Display) -> String {
    format!("internal: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn work_within_the_limit_returns_its_own_outcome() {
        let limit = Duration::from_secs(5);
        assert_eq!(run_blocking(limit, || Ok::<_, String>(7)).await, Ok(7));
        let failed = run_blocking(limit, || Err::<(), _>("bad".to_owned()));
        assert_eq!(failed.await, Err("bad".to_owned()));
    }

    #[tokio::test]
    async fn work_past_the_limit_is_abandoned_with_a_message() {
        let err = run_blocking(Duration::from_millis(1), || {
            std::thread::sleep(Duration::from_millis(300));
            Ok::<_, String>(())
        })
        .await
        .expect_err("timed out");
        assert!(err.contains("1 ms time limit"), "{err}");
    }

    #[tokio::test]
    async fn a_panic_is_an_internal_error() {
        let err = run_blocking(Duration::from_secs(5), || -> Result<(), _> {
            panic!("tool bug")
        })
        .await
        .expect_err("panicked");
        assert!(err.starts_with("internal: "), "{err}");
    }

    #[test]
    fn the_default_is_thirty_seconds() {
        assert_eq!(DEFAULT_CALL_TIMEOUT, Duration::from_secs(30));
    }
}
