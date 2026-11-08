pub struct PanicGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> Drop for PanicGuard<F> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(f) = self.cleanup.take() {
                f();
            }
        }
    }
}