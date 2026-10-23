pub struct SparseSet<T> {
    sparse: Vec<usize>,
    dense: Vec<usize>,
    data: Vec<T>,
}

impl<T> SparseSet<T> {
    pub fn new() -> Self {
        Self { sparse: Vec::new(), dense: Vec::new(), data: Vec::new() }
    }

    pub fn insert(&mut self, index: usize, value: T) {
        if index >= self.sparse.len() {
            self.sparse.resize(index + 1, usize::MAX);
        }
        self.sparse[index] = self.dense.len();
        self.dense.push(index);
        self.data.push(value);
    }
}