pub struct WaitNode {
    pub thread_id: usize,
    pub next: *mut WaitNode,
}