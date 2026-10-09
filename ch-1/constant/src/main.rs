use std::cell::RefCell;
use std::rc::Rc;

pub trait System {
    fn update(&mut self);
}

pub struct Engine {
    systems: Vec<Rc<RefCell<dyn System>>>,
}

impl Engine {
    pub fn register<S: System + 'static>(&mut self, system: S) {
        self.systems.push(Rc::new(RefCell::new(system)));
    }

    pub fn tick(&self) {
        for sys in &self.systems {
            sys.borrow_mut().update();
        }
    }
}