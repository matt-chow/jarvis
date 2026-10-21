use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// Drive a future that never actually waits (in-memory test doubles).
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}
