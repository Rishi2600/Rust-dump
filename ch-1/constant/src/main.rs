use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct ReadyFuture<T>(Option<T>);

impl<T: Unpin> Future for ReadyFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.take() {
            Some(val) => Poll::Ready(val),
            None => panic!("Future polled after completion"),
        }
    }
}