#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ProtocolState { Disconnected, Handshaking, Established, Error }

pub struct ProtocolSession<const S: ProtocolState>;

impl ProtocolSession<{ ProtocolState::Disconnected }> {
    pub fn start_handshake(self) -> ProtocolSession<{ ProtocolState::Handshaking }> {
        ProtocolSession
    }
}

impl ProtocolSession<{ ProtocolState::Handshaking }> {
    pub fn establish(self) -> ProtocolSession<{ ProtocolState::Established }> {
        ProtocolSession
    }
}