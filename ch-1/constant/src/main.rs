pub struct OnPanic<F: FnOnce()> {
    f: F,
}

impl<F: FnOnce()> Drop for OnPanic<F> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            (self.f)();
        }
    }
}