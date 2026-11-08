use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct WindowAggregator<Fut> {
    future: Fut,
}

impl<Fut: Future> Future for WindowAggregator<Fut> {
    type Output = Option<Fut::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Safety: projecting pin field
        let fut = unsafe { Pin::map_unchecked_mut(self, |s| &mut s.future) };
        match fut.poll(cx) {
            Poll::Ready(val) => Poll::Ready(Some(val)),
            Poll::Pending => Poll::Pending,
        }
    }
}