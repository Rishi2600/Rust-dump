/// A collection that always hands back its largest item first.
///
/// The pool needs "the highest priority transaction" over and over while
/// new ones keep arriving. Sorting the whole pool each time would be
/// wasteful. A heap keeps the items in a looser arrangement that is cheap
/// to maintain: it promises only that every item is at least as large as
/// the two below it.
///
/// ```text
///            90                 stored as a flat list:
///          /    \               [90, 70, 80, 10, 60, 30]
///        70      80
///       /  \    /               the children of index i are at
///     10   60  30               2i + 1 and 2i + 2
/// ```
///
/// The largest item is therefore always at the top. Adding or removing one
/// only disturbs a single path between the top and the bottom, which for a
/// million items is about twenty steps.
#[derive(Debug, Clone)]
pub struct MaxHeap<T> {
    items: Vec<T>,
}

impl<T: Ord> MaxHeap<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The largest item, without removing it.
    pub fn peek(&self) -> Option<&T> {
        self.items.first()
    }

    /// Add an item: put it at the bottom, then let it rise.
    pub fn push(&mut self, item: T) {
        self.items.push(item);
        self.sift_up(self.items.len() - 1);
    }

    /// Remove and return the largest item.
    pub fn pop(&mut self) -> Option<T> {
        let last = self.items.len().checked_sub(1)?;
        // Move the top to the end, where it can be removed cheaply. The item
        // that took its place is probably too small to be there, so it sinks.
        self.items.swap(0, last);
        let top = self.items.pop();
        if !self.items.is_empty() {
            self.sift_down(0);
        }
        top
    }

    /// Swap an item with its parent for as long as it is the larger.
    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if self.items[index] <= self.items[parent] {
                break;
            }
            self.items.swap(index, parent);
            index = parent;
        }
    }

    /// Swap an item with its larger child for as long as a child is larger.
    fn sift_down(&mut self, mut index: usize) {
        loop {
            let (left, right) = (2 * index + 1, 2 * index + 2);
            let mut largest = index;
            if left < self.items.len() && self.items[left] > self.items[largest] {
                largest = left;
            }
            if right < self.items.len() && self.items[right] > self.items[largest] {
                largest = right;
            }
            if largest == index {
                break;
            }
            self.items.swap(index, largest);
            index = largest;
        }
    }
}

impl<T: Ord> Default for MaxHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(mut heap: MaxHeap<u64>) -> Vec<u64> {
        let mut out = Vec::new();
        while let Some(item) = heap.pop() {
            out.push(item);
        }
        out
    }

    #[test]
    fn items_come_out_largest_first_whatever_order_they_went_in() {
        let mut heap = MaxHeap::new();
        for item in [30, 90, 10, 70, 60, 80] {
            heap.push(item);
        }
        assert_eq!(heap.len(), 6);
        assert_eq!(heap.peek(), Some(&90));
        assert_eq!(drain(heap), [90, 80, 70, 60, 30, 10]);
    }

    #[test]
    fn an_empty_heap_has_nothing_to_give() {
        let mut heap: MaxHeap<u64> = MaxHeap::default();
        assert!(heap.is_empty());
        assert_eq!(heap.peek(), None);
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn equal_items_are_all_kept() {
        let mut heap = MaxHeap::new();
        for item in [5, 5, 9, 5, 9] {
            heap.push(item);
        }
        assert_eq!(drain(heap), [9, 9, 5, 5, 5]);
    }

    #[test]
    fn pushes_and_pops_can_be_interleaved() {
        let mut heap = MaxHeap::new();
        heap.push(3);
        heap.push(8);
        assert_eq!(heap.pop(), Some(8));
        heap.push(5);
        heap.push(1);
        assert_eq!(heap.pop(), Some(5));
        heap.push(9);
        assert_eq!(drain(heap), [9, 3, 1]);
    }

    #[test]
    fn a_thousand_scrambled_numbers_come_out_sorted() {
        // A simple pseudo-random sequence, the same on every run.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut numbers = Vec::new();
        let mut heap = MaxHeap::new();
        for _ in 0..1000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let number = state >> 40;
            numbers.push(number);
            heap.push(number);
        }
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(drain(heap), numbers);
    }
}
