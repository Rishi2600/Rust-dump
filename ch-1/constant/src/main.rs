use std::ops::{Deref, DerefMut};

pub struct HardwareResource {
    pub id: u32,
}

pub struct ResourceGuard<'a> {
    resource: &'a mut HardwareResource,
}

impl<'a> ResourceGuard<'a> {
    pub fn lock(resource: &'a mut HardwareResource) -> Self {
        println!("Locking resource {}", resource.id);
        Self { resource }
    }
}

impl<'a> Deref for ResourceGuard<'a> {
    type Target = HardwareResource;
    fn deref(&self) -> &Self::Target { self.resource }
}

impl<'a> DerefMut for ResourceGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.resource }
}

impl<'a> Drop for ResourceGuard<'a> {
    fn drop(&mut self) {
        println!("Unlocking resource {}", self.resource.id);
    }
}