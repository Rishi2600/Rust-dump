use std::future::Future;
use std::pin::Pin;

type AsyncStep = Box<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), &'static str>> + Send>> + Send + Sync>;

pub struct SagaCoordinator {
    steps: Vec<AsyncStep>,
    compensations: Vec<AsyncStep>,
}

impl SagaCoordinator {
    pub fn new() -> Self {
        Self { steps: Vec::new(), compensations: Vec::new() }
    }

    pub fn add_step<F, C>(&mut self, step: F, compensation: C)
    where
        F: Fn() -> Pin<Box<dyn Future<Output = Result<(), &'static str>> + Send>> + Send + Sync + 'static,
        C: Fn() -> Pin<Box<dyn Future<Output = Result<(), &'static str>> + Send>> + Send + Sync + 'static,
    {
        self.steps.push(Box::new(step));
        self.compensations.push(Box::new(compensation));
    }

    pub async fn execute(&self) -> Result<(), &'static str> {
        let mut executed = 0;
        for step in &self.steps {
            if let Err(e) = step().await {
                // Rollback executed steps in reverse
                for rollback in self.compensations[..executed].iter().rev() {
                    let _ = rollback().await;
                }
                return Err(e);
            }
            executed += 1;
        }
        Ok(())
    }
}