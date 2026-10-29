use std::any::{Any, TypeId};
use std::collections::HashMap;

pub struct ExtensionRegistry {
    extensions: HashMap<TypeId, Box<dyn Any>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self { extensions: HashMap::new() }
    }

    pub fn register<T: 'static>(&mut self, ext: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(ext));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.extensions.get(&TypeId::of::<T>()).and_then(|b| b.downcast_ref())
    }
}