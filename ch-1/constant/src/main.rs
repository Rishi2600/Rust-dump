pub struct NodeId(pub usize);

pub struct ArenaNode<T> {
    pub data: T,
    pub edges: Vec<NodeId>,
}

pub struct GraphArena<T> {
    pub nodes: Vec<ArenaNode<T>>,
}