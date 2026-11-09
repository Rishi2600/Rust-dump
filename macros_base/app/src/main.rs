pub struct Locked;
pub struct Unlocked;

pub struct SecureVault<State> {
    code: u32,
    _state: State,
}

impl SecureVault<Locked> {
    pub fn new(code: u32) -> Self {
        Self { code, _state: Locked }
    }

    pub fn unlock(self, attempt: u32) -> Result<SecureVault<Unlocked>, Self> {
        if attempt == self.code {
            Ok(SecureVault { code: self.code, _state: Unlocked })
        } else {
            Err(self)
        }
    }
}