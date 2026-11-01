pub struct ScopeGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.cleanup.take() {
            f();
        }
    }
}

pub fn with_rollback<F, R>(mut action: F) -> R 
where F: FnMut() -> R,
{
    let _guard = ScopeGuard { cleanup: Some(|| println!("Rolling back transaction due to panic!")) };
    action()
}