pub struct Node<T> {
    pub value: T,
    pub next: Option<Box<Node<T>>>,
    pub prev: *mut Node<T>, // Raw pointer to avoid borrow checker loops
}

impl<T> Node<T> {
    pub fn new(value: T) -> Self {
        Node { value, next: None, prev: std::ptr::null_mut() }
    }
}