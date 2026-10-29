use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

pub fn block_on<F: Future>(mut future: F) -> F::Output {
    let mut boxed = Box::pin(future);
    let waker = dummy_waker();
    let mut cx = Context::from_waker(&waker);

    loop {
        match boxed.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn dummy_waker() -> Waker {
    unsafe fn no_op(_: *const ()) {}
    unsafe fn clone(_: *const ()) -> RawWaker { raw_waker() }
    unsafe fn raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, no_op, no_op, no_op))
    }
    unsafe { Waker::from_raw(raw_waker()) }
}