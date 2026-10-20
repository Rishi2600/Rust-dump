use std::ptr::NonNull;

pub struct IntrusiveNode<T> {
    pub next: Option<NonNull<IntrusiveNode<T>>>,
    pub prev: Option<NonNull<IntrusiveNode<T>>>,
    pub value: T,
}

pub struct IntrusiveList<T> {
    head: Option<NonNull<IntrusiveNode<T>>>,
    tail: Option<NonNull<IntrusiveNode<T>>>,
}

impl<T> IntrusiveList<T> {
    pub fn new() -> Self {
        Self { head: None, tail: None }
    }

    pub fn push_front(&mut self, node: &mut IntrusiveNode<T>) {
        let node_ptr = NonNull::from(node);
        unsafe {
            (*node_ptr.as_ptr()).next = self.head;
            (*node_ptr.as_ptr()).prev = None;

            if let Some(head_ptr) = self.head {
                (*head_ptr.as_ptr()).prev = Some(node_ptr);
            } else {
                self.tail = Some(node_ptr);
            }
            self.head = Some(node_ptr);
        }
    }
}