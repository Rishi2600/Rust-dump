use std::pin::Pin;
use std::task::{Context, Poll};

pub trait AsyncWindowStream {
    type Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}