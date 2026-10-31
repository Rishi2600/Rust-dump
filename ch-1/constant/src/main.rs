use std::sync::atomic::{AtomicPtr, Ordering};

pub struct HazardPointer {
    ptr: AtomicPtr<()>,
}