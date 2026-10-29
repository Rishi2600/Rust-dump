pub struct Idle;
pub struct Active;

pub struct Connection<State> {
    id: usize,
    _state: State,
}

pub struct Pool {
    conns: Vec<usize>,
}

impl Pool {
    pub fn acquire(&mut self) -> Option<Connection<Idle>> {
        self.conns.pop().map(|id| Connection { id, _state: Idle })
    }
}

impl Connection<Idle> {
    pub fn execute_query(self, _query: &str) -> Connection<Active> {
        println!("Executing query on connection {}", self.id);
        Connection { id: self.id, _state: Active }
    }
}