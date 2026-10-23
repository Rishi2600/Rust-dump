use std::sync::{Mutex, Condvar};
use std::collections::VecDeque;

pub struct BoundedChannel<T> {
    queue: Mutex<VecDeque<T>>,
    space_available: Condvar,
    item_available: Condvar,
    capacity: usize,
}