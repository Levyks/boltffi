use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

use boltffi::*;

/// Resolves after `duration`, woken from a background thread rather than any
/// async runtime (this crate depends on none). Used by [`CancellableCounter`]
/// so that its progress only advances *after* being polled again past this
/// await point — which is exactly the point BoltFFI's generated future
/// cancellation stops happening.
struct Tick {
    state: Arc<TickState>,
}

struct TickState {
    ready: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl Tick {
    fn new(duration: Duration) -> Self {
        let state = Arc::new(TickState {
            ready: AtomicBool::new(false),
            waker: Mutex::new(None),
        });
        let background = Arc::clone(&state);
        #[cfg(not(target_arch = "wasm32"))]
        thread::spawn(move || {
            thread::sleep(duration);
            background.ready.store(true, Ordering::Release);
            if let Some(waker) = background.waker.lock().unwrap().take() {
                waker.wake();
            }
        });
        #[cfg(target_arch = "wasm32")]
        {
            background.ready.store(true, Ordering::Release);
            if let Some(waker) = background.waker.lock().unwrap().take() {
                waker.wake();
            }
        }
        Self { state }
    }
}

impl Future for Tick {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.state.ready.load(Ordering::Acquire) {
            Poll::Ready(())
        } else {
            *self.state.waker.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// Counts up one tick at a time, sleeping `tick_millis` between increments.
///
/// This exists to demonstrate — and let tests assert — that cancelling the
/// Dart-side `CancelableOperation` for an in-flight `count_to` call genuinely
/// stops the Rust future from making further progress. `progress()` only
/// increases from *inside* the polled future, past an await point; once
/// BoltFFI's generated binding cancels the underlying `RustFuture`, that
/// future is never polled again, so `progress()` provably stops climbing no
/// matter how long the caller waits afterward.
pub struct CancellableCounter {
    progress: Arc<AtomicI32>,
}

impl Default for CancellableCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl CancellableCounter {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(AtomicI32::new(0)),
        }
    }

    /// Ticks completed so far. Safe to call at any time, including after
    /// cancelling an in-flight `count_to`.
    pub fn progress(&self) -> i32 {
        self.progress.load(Ordering::Acquire)
    }

    pub async fn count_to(&self, target: i32, tick_millis: i32) -> i32 {
        let progress = Arc::clone(&self.progress);
        let tick_millis = tick_millis.max(0) as u64;
        loop {
            let current = progress.load(Ordering::Acquire);
            if current >= target {
                return current;
            }
            Tick::new(Duration::from_millis(tick_millis)).await;
            progress.fetch_add(1, Ordering::AcqRel);
        }
    }
}
