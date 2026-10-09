#[derive(Debug, Clone, Copy)]
pub struct NodeId(usize);

pub struct Node {
    pub data: String,
    pub children: Vec<NodeId>,
}

pub struct GraphArena {
    nodes: Vec<Node>,
}

impl GraphArena {
    pub fn new() -> Self { Self { nodes: Vec::new() } }

    pub fn add_node(&mut self, data: &str) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node { data: data.to_string(), children: Vec::new() });
        id
    }

    pub fn add_edge(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[parent.0].children.push(child);
    }
}