pub struct ArenaTree<T> {
    pub nodes: Vec<TreeNode<T>>,
}

pub struct TreeNode<T> {
    pub data: T,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

impl<T> ArenaTree<T> {
    pub fn new(root_data: T) -> Self {
        Self {
            nodes: vec![TreeNode { data: root_data, parent: None, children: Vec::new() }],
        }
    }
}