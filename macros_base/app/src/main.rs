use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

pub struct AuditedGuard<'a, T> {
    guard: std::sync::MutexGuard<'a, T>,
}

impl<'a, T> Deref for AuditedGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.guard }
}

impl<'a, T> DerefMut for AuditedGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.guard }
}

impl<'a, T> Drop for AuditedGuard<'a, T> {
    fn drop(&mut self) {
        println!("Audit: Shared resource modified and unlocked.");
    }
}