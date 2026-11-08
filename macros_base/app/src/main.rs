pub struct Disconnected;
pub struct Connected;

pub struct Session<State> {
    id: u64,
    _state: State,
}

impl Session<Disconnected> {
    pub fn new(id: u64) -> Self {
        Self { id, _state: Disconnected }
    }

    pub fn connect(self) -> Session<Connected> {
        println!("Session {} connected.", self.id);
        Session { id: self.id, _state: Connected }
    }
}

impl Session<Connected> {
    pub fn query(&self, sql: &str) {
        println!("Executing query on session {}: {}", self.id, sql);
    }
}