pub struct FreeFrame {
    next: *mut FreeFrame,
}

pub struct FrameFreeList {
    head: *mut FreeFrame,
}

impl FrameFreeList {
    pub unsafe fn push(&mut self, frame: *mut FreeFrame) {
        (*frame).next = self.head;
        self.head = frame;
    }

    pub unsafe fn pop(&mut self) -> Option<*mut FreeFrame> {
        if self.head.is_null() {
            return None;
        }
        let frame = self.head;
        self.head = (*frame).next;
        Some(frame)
    }
}