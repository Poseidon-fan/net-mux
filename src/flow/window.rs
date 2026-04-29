//! Credit-based send / receive windows.
//!
//! These are intentionally lock-free on the hot path: the number of bytes
//! "owed" is a single `AtomicU32`, and a parking slot from
//! [`futures_util::task::AtomicWaker`] handles wake-ups when credit becomes
//! available again.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::task::Context;

use futures_util::task::AtomicWaker;

/// Result of attempting to acquire credit from a [`SendWindow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AcquireOutcome {
    /// Acquired `n` bytes of credit.
    Acquired(u32),
    /// Window is closed (stream gone) — caller should propagate an error.
    Closed,
    /// No credit currently available; waker has been registered.
    Pending,
}

/// Outbound credit available to transmit on a stream.
///
/// Credit is granted by the peer via `WindowUpdate` frames and consumed when
/// we transmit `Data` frames. The hot path is a single CAS; waiters block on
/// an [`AtomicWaker`] and are woken whenever credit increases or the window
/// is closed.
#[derive(Debug)]
pub(crate) struct SendWindow {
    available: AtomicU32,
    closed: AtomicBool,
    waker: AtomicWaker,
}

impl SendWindow {
    pub(crate) fn new(initial: u32) -> Self {
        Self {
            available: AtomicU32::new(initial),
            closed: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }

    /// Attempt to consume up to `desired` bytes of credit.
    ///
    /// On `Pending`, the supplied waker is registered for wake-up.
    pub(crate) fn poll_acquire(&self, cx: &mut Context<'_>, desired: u32) -> AcquireOutcome {
        if self.closed.load(Ordering::Acquire) {
            return AcquireOutcome::Closed;
        }
        // Fast path
        if let Some(n) = self.try_take(desired) {
            return AcquireOutcome::Acquired(n);
        }

        // Register, then re-check — see `AtomicWaker` docs.
        self.waker.register(cx.waker());
        if self.closed.load(Ordering::Acquire) {
            return AcquireOutcome::Closed;
        }
        if let Some(n) = self.try_take(desired) {
            return AcquireOutcome::Acquired(n);
        }
        AcquireOutcome::Pending
    }

    fn try_take(&self, desired: u32) -> Option<u32> {
        loop {
            let cur = self.available.load(Ordering::Acquire);
            if cur == 0 {
                return None;
            }
            let take = cur.min(desired);
            match self.available.compare_exchange_weak(
                cur,
                cur - take,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(take),
                Err(_) => continue,
            }
        }
    }

    /// Grant additional credit. Wakes a pending waiter, if any.
    pub(crate) fn grant(&self, delta: u32) {
        if delta == 0 {
            return;
        }
        // Saturating so a hostile peer cannot wrap us around.
        let mut cur = self.available.load(Ordering::Acquire);
        loop {
            let new = cur.saturating_add(delta);
            match self.available.compare_exchange_weak(
                cur,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
        self.waker.wake();
    }

    /// Close the window: future `poll_acquire` calls return `Closed` and any
    /// pending waiter is woken.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.waker.wake();
    }
}

/// Inbound credit accounting for a stream.
///
/// Tracks the running tally of bytes consumed by the application but not
/// yet acknowledged to the peer. When the tally crosses
/// `initial_window / 2` the session emits a `WindowUpdate`, returning the
/// accumulated delta.
#[derive(Debug)]
pub(crate) struct RecvWindow {
    initial: u32,
    pending_credit: AtomicU32,
}

impl RecvWindow {
    pub(crate) fn new(initial: u32) -> Self {
        Self {
            initial,
            pending_credit: AtomicU32::new(0),
        }
    }

    /// Record that the application consumed `bytes` bytes.
    ///
    /// Returns `Some(delta)` when the accumulated delta is large enough to
    /// emit a `WindowUpdate`, having atomically reset the counter.
    pub(crate) fn on_consume(&self, bytes: u32) -> Option<u32> {
        if bytes == 0 {
            return None;
        }
        let threshold = (self.initial / 2).max(1);
        let prev = self.pending_credit.fetch_add(bytes, Ordering::AcqRel);
        let new = prev.saturating_add(bytes);
        if new >= threshold {
            // Reset to 0 atomically and emit the entire accumulated value;
            // multiple racing producers may all observe that they crossed
            // the threshold, but only one CAS will succeed.
            let mut cur = new;
            loop {
                match self.pending_credit.compare_exchange_weak(
                    cur,
                    0,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return Some(cur),
                    Err(actual) => {
                        if actual < threshold {
                            return None;
                        }
                        cur = actual;
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::poll_fn;
    use std::task::Poll;

    #[tokio::test]
    async fn send_window_basic() {
        let w = SendWindow::new(100);

        let n = poll_fn(|cx| match w.poll_acquire(cx, 30) {
            AcquireOutcome::Acquired(n) => Poll::Ready(n),
            _ => unreachable!(),
        })
        .await;
        assert_eq!(n, 30);

        let n = poll_fn(|cx| match w.poll_acquire(cx, 200) {
            AcquireOutcome::Acquired(n) => Poll::Ready(n),
            _ => unreachable!(),
        })
        .await;
        assert_eq!(n, 70);
    }

    #[tokio::test]
    async fn send_window_pending_then_grant() {
        use std::sync::Arc;
        use tokio::time::{Duration, sleep};

        let w = Arc::new(SendWindow::new(0));
        let w2 = w.clone();
        let task = tokio::spawn(async move {
            poll_fn(|cx| match w2.poll_acquire(cx, 16) {
                AcquireOutcome::Acquired(n) => Poll::Ready(n),
                AcquireOutcome::Pending => Poll::Pending,
                AcquireOutcome::Closed => panic!("unexpected"),
            })
            .await
        });

        sleep(Duration::from_millis(20)).await;
        w.grant(8);
        let got = task.await.unwrap();
        assert_eq!(got, 8);
    }

    #[tokio::test]
    async fn send_window_close_wakes() {
        use std::sync::Arc;
        use tokio::time::{Duration, sleep};

        let w = Arc::new(SendWindow::new(0));
        let w2 = w.clone();
        let task = tokio::spawn(async move {
            poll_fn(|cx| match w2.poll_acquire(cx, 16) {
                AcquireOutcome::Closed => Poll::Ready(()),
                AcquireOutcome::Pending => Poll::Pending,
                AcquireOutcome::Acquired(_) => panic!("unexpected"),
            })
            .await
        });
        sleep(Duration::from_millis(20)).await;
        w.close();
        task.await.unwrap();
    }

    #[test]
    fn recv_window_threshold() {
        let w = RecvWindow::new(100);
        assert_eq!(w.on_consume(10), None);
        assert_eq!(w.on_consume(10), None);
        // 20 + 30 = 50 >= 50 (threshold)
        assert_eq!(w.on_consume(30), Some(50));
        // counter reset
        assert_eq!(w.on_consume(10), None);
    }

    #[test]
    fn recv_window_zero_bytes_noop() {
        let w = RecvWindow::new(100);
        assert_eq!(w.on_consume(0), None);
    }
}
