use std::ptr::NonNull;

pub struct Node<T> {
    pub next: Option<NonNull<Node<T>>>,
    pub value: T,
}

pub struct IntrusiveList<T> {
    pub head: Option<NonNull<Node<T>>>,
}

impl<T> IntrusiveList<T> {
    pub fn new() -> Self { Self { head: None } }
}