use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};

pub struct SharedCache<K, V> {
    store: Arc<RwLock<HashMap<K, V>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> SharedCache<K, V> {
    pub fn new() -> Self {
        Self { store: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn get_or_insert_with<F>(&self, key: K, init: F) -> V
    where
        F: FnOnce() -> V,
    {
        if let Some(val) = self.store.read().unwrap().get(&key) {
            return val.clone();
        }

        let mut lock = self.store.write().unwrap();
        lock.entry(key).or_insert_with(init).clone()
    }
}