pub trait EventHandler {
    fn handle(&self, event: &str);
}

pub struct EventDispatcher {
    handlers: Vec<Box<dyn EventHandler>>,
}

impl EventDispatcher {
    pub fn new() -> Self { Self { handlers: Vec::new() } }

    pub fn register(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
    }

    pub fn dispatch(&self, event: &str) {
        for handler in &self.handlers {
            handler.handle(event);
        }
    }
}