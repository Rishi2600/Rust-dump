pub struct Disconnected;
pub struct Connected;
pub struct Authenticated;

pub struct Session<State> {
    token: String,
    _state: State,
}

impl Session<Disconnected> {
    pub fn new() -> Self {
        Self { token: String::new(), _state: Disconnected }
    }

    pub fn connect(self, endpoint: &str) -> Session<Connected> {
        println!("Connecting to {}", endpoint);
        Session { token: self.token, _state: Connected }
    }
}

impl Session<Connected> {
    pub fn authenticate(self, api_key: &str) -> Session<Authenticated> {
        Session { token: api_key.to_string(), _state: Authenticated }
    }
}