pub struct RunQueue<const N: usize> {
    tasks: [usize; N],
    head: usize,
    len: usize,
}

impl<const N: usize> RunQueue<N> {
    pub fn enqueue(&mut self, tid: usize) -> bool {
        if self.len == N {
            return false;
        }
        self.tasks[(self.head + self.len) % N] = tid;
        self.len += 1;
        true
    }

    pub fn pick_next(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let tid = self.tasks[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(tid)
    }
}