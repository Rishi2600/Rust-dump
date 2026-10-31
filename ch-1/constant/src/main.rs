use std::any::{Any, TypeId};
use std::collections::HashMap;

pub struct ServiceRegistry {
    services: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}