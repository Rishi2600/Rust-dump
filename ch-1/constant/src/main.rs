pub struct Disconnected;
pub struct Authenticated;

pub struct Session<State> {
    token: String,
    _state: State,
}

impl Session<Disconnected> {
    pub fn new() -> Self { Self { token: String::new(), _state: Disconnected } }
    pub fn authenticate(self, token: &str) -> Session<Authenticated> {
        Session { token: token.to_string(), _state: Authenticated }
    }
}

impl Session<Authenticated> {
    pub fn query(&self, sql: &str) {
        println!("Executing '{}' with token {}", sql, self.token);
    }
}