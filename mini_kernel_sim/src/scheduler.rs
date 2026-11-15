use std::collections::VecDeque;

/// FIFO queue of runnable task ids: the heart of round-robin scheduling.
#[derive(Debug, Default)]
pub struct RunQueue {
    queue: VecDeque<usize>,
}

impl RunQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a task to the back of the queue.
    pub fn enqueue(&mut self, pid: usize) {
        self.queue.push_back(pid);
    }

    /// Take the task that has been waiting longest.
    pub fn pick_next(&mut self) -> Option<usize> {
        self.queue.pop_front()
    }

    /// Drop a task from the queue, wherever it is.
    pub fn remove(&mut self, pid: usize) {
        self.queue.retain(|&queued| queued != pid);
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_take_turns_in_arrival_order() {
        let mut queue = RunQueue::new();
        for pid in [1, 2, 3] {
            queue.enqueue(pid);
        }
        // A preempted task goes to the back and waits for everyone else.
        let first = queue.pick_next().unwrap();
        queue.enqueue(first);
        assert_eq!(queue.pick_next(), Some(2));
        assert_eq!(queue.pick_next(), Some(3));
        assert_eq!(queue.pick_next(), Some(1));
        assert_eq!(queue.pick_next(), None);
    }

    #[test]
    fn remove_drops_a_task_from_the_queue() {
        let mut queue = RunQueue::new();
        queue.enqueue(1);
        queue.enqueue(2);
        queue.remove(1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pick_next(), Some(2));
        assert!(queue.is_empty());
    }
}
