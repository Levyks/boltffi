use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use boltffi::*;

/// Yields control back to the executor `ticks` times before resolving --
/// exercises a genuine poll -> Pending -> wake -> re-poll -> Ready cycle
/// without depending on any real timer or OS thread, since wasm32 has
/// neither.
struct Yield {
    remaining: u32,
}

impl Yield {
    fn new(ticks: u32) -> Self {
        Self { remaining: ticks }
    }
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.remaining == 0 {
            Poll::Ready(())
        } else {
            self.remaining -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub struct AsyncWorker {
    prefix: String,
}

#[export]
impl AsyncWorker {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    pub fn get_prefix(&self) -> String {
        self.prefix.clone()
    }

    pub async fn process(&self, input: String) -> String {
        format!("{}: {}", self.prefix, input)
    }

    // Unlike every other method here, this one genuinely suspends across
    // several polls instead of resolving on the first one -- every other
    // async method/function in this crate happens to resolve immediately,
    // which never exercises the real Pending/wake/re-poll cycle on an
    // async class method.
    pub async fn process_after_yielding(&self, input: String) -> String {
        Yield::new(3).await;
        format!("{}: {}", self.prefix, input)
    }

    pub async fn try_process(&self, input: String) -> Result<String, String> {
        if input.is_empty() {
            Err("input must not be empty".to_string())
        } else {
            Ok(format!("{}: {}", self.prefix, input))
        }
    }

    pub async fn find_item(&self, id: i32) -> Option<String> {
        if id > 0 {
            Some(format!("{}_{}", self.prefix, id))
        } else {
            None
        }
    }

    pub async fn process_batch(&self, inputs: Vec<String>) -> Vec<String> {
        inputs
            .into_iter()
            .map(|input| format!("{}: {}", self.prefix, input))
            .collect()
    }
}
