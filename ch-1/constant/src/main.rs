pub struct Socket<const STATE: u8>;

impl Socket<0> {
    pub fn connect(self) -> Socket<1> {
        Socket::<1>
    }
}

impl Socket<1> {
    pub fn send_data(self) -> Socket<1> {
        self
    }
    pub fn disconnect(self) -> Socket<0> {
        Socket::<0>
    }
}