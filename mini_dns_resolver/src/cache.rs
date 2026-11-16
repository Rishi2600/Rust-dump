use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::name::Name;
use crate::record::Record;
use crate::rtype::RecordType;

/// Longest any record is kept, whatever its TTL claims.
const MAX_TTL: u32 = 86_400;

struct Entry {
    records: Vec<Record>,
    expires: Instant,
}

/// Remembers record sets until their time to live runs out.
///
/// A record set is every record with the same owner name and type. The
/// current time is passed in by the caller, so tests can move the clock.
#[derive(Default)]
pub struct Cache {
    entries: HashMap<(Name, RecordType), Entry>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store records, grouped into sets by owner and type. A set replaces any
    /// older one and lives for the shortest TTL among its records.
    pub fn insert(&mut self, records: &[Record], now: Instant) {
        let mut sets: HashMap<(Name, RecordType), Vec<Record>> = HashMap::new();
        for record in records {
            let key = (record.name.clone(), record.rtype);
            sets.entry(key).or_default().push(record.clone());
        }
        for (key, records) in sets {
            let ttl = records.iter().map(|record| record.ttl).min().unwrap_or(0);
            // A TTL of zero means "use this once and do not keep it".
            if ttl == 0 {
                continue;
            }
            let lifetime = Duration::from_secs(u64::from(ttl.min(MAX_TTL)));
            let expires = now + lifetime;
            self.entries.insert(key, Entry { records, expires });
        }
    }

    /// Records for this name and type that are still fresh, with their TTLs
    /// counted down to the time remaining.
    pub fn get(&self, name: &Name, rtype: RecordType, now: Instant) -> Option<Vec<Record>> {
        let entry = self.entries.get(&(name.clone(), rtype))?;
        let remaining = entry.expires.checked_duration_since(now)?;
        if remaining.is_zero() {
            return None;
        }
        let ttl = remaining.as_secs_f64().ceil() as u32;
        let records = entry.records.iter().cloned().map(|mut record| {
            record.ttl = ttl;
            record
        });
        Some(records.collect())
    }

    /// Drop everything that has expired.
    pub fn purge(&mut self, now: Instant) {
        self.entries.retain(|_, entry| entry.expires > now);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RData;
    use std::net::Ipv4Addr;

    fn name(text: &str) -> Name {
        Name::parse(text).expect("valid name")
    }

    fn a(owner: &str, ttl: u32, last: u8) -> Record {
        let address = RData::A(Ipv4Addr::new(192, 0, 2, last));
        Record::new(name(owner), RecordType::A, ttl, address)
    }

    fn after(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn returns_records_with_their_ttl_counted_down() {
        let (mut cache, start) = (Cache::new(), Instant::now());
        cache.insert(&[a("example.com", 300, 1)], start);
        let hit = cache.get(&name("example.com"), RecordType::A, after(start, 100));
        assert_eq!(hit, Some(vec![a("example.com", 200, 1)]));
        // Case does not matter, the record type does.
        assert!(
            cache
                .get(&name("EXAMPLE.com"), RecordType::A, start)
                .is_some()
        );
        assert!(
            cache
                .get(&name("example.com"), RecordType::Aaaa, start)
                .is_none()
        );
    }

    #[test]
    fn forgets_records_once_their_ttl_has_passed() {
        let (mut cache, start) = (Cache::new(), Instant::now());
        cache.insert(&[a("example.com", 300, 1)], start);
        assert!(
            cache
                .get(&name("example.com"), RecordType::A, after(start, 299))
                .is_some()
        );
        assert!(
            cache
                .get(&name("example.com"), RecordType::A, after(start, 300))
                .is_none()
        );
        assert_eq!(cache.len(), 1);
        cache.purge(after(start, 300));
        assert!(cache.is_empty());
    }

    #[test]
    fn a_set_expires_with_its_shortest_lived_record() {
        let (mut cache, start) = (Cache::new(), Instant::now());
        cache.insert(&[a("example.com", 300, 1), a("example.com", 60, 2)], start);
        let hit = cache
            .get(&name("example.com"), RecordType::A, start)
            .unwrap();
        assert_eq!(hit.len(), 2);
        assert!(hit.iter().all(|record| record.ttl == 60));
        assert!(
            cache
                .get(&name("example.com"), RecordType::A, after(start, 61))
                .is_none()
        );
    }

    #[test]
    fn zero_ttl_is_never_stored_and_new_sets_replace_old_ones() {
        let (mut cache, start) = (Cache::new(), Instant::now());
        cache.insert(&[a("once.example.com", 0, 1)], start);
        assert!(cache.is_empty());

        cache.insert(&[a("example.com", 300, 1)], start);
        cache.insert(&[a("example.com", 300, 2)], start);
        let hit = cache.get(&name("example.com"), RecordType::A, start);
        assert_eq!(hit, Some(vec![a("example.com", 300, 2)]));
    }
}
