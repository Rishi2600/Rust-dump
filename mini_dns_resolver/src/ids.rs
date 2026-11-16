use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// Source of query IDs.
///
/// A reply is only believed if it echoes the ID of the query, so an attacker
/// who cannot see our traffic has to guess it. That only helps if the IDs are
/// hard to predict, which rules out a simple counter.
#[derive(Debug, Clone)]
pub struct IdGenerator {
    state: u64,
}

impl IdGenerator {
    /// Start from a seed supplied by the operating system.
    pub fn new() -> Self {
        // The standard library seeds every `RandomState` randomly to protect
        // hash maps. Hashing nothing with it yields that randomness.
        Self::with_seed(RandomState::new().build_hasher().finish())
    }

    /// Start from a known seed, for reproducible tests.
    pub fn with_seed(seed: u64) -> Self {
        // The xorshift generator below must never start at zero.
        Self { state: seed | 1 }
    }

    pub fn next_id(&mut self) -> u16 {
        // xorshift64: three shift-and-xor steps scramble the state.
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state >> 24) as u16
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let (mut first, mut second) = (IdGenerator::with_seed(42), IdGenerator::with_seed(42));
        for _ in 0..100 {
            assert_eq!(first.next_id(), second.next_id());
        }
    }

    #[test]
    fn ids_are_spread_out_rather_than_counting_up() {
        let mut ids = IdGenerator::with_seed(0);
        let seen: HashSet<u16> = (0..1000).map(|_| ids.next_id()).collect();
        assert!(seen.len() > 950, "only {} distinct IDs", seen.len());
    }

    #[test]
    fn separate_generators_start_in_different_places() {
        let sequence = |mut ids: IdGenerator| [ids.next_id(), ids.next_id(), ids.next_id()];
        assert_ne!(sequence(IdGenerator::new()), sequence(IdGenerator::new()));
    }
}
