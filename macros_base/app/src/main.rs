#[derive(Copy, Clone)]
pub struct NodeId(pub usize);

pub struct TreeNode<T> {
    pub data: T,
    pub children: Vec<NodeId>,
}

pub struct TreeArena<T> {
    pub nodes: Vec<TreeNode<T>>,
}

impl<T> TreeArena<T> {
    pub fn new(root: T) -> Self {
        Self { nodes: vec![TreeNode { data: root, children: Vec::new() }] }
    }
}