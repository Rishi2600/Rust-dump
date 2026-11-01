pub struct StateMachine<const STATE: usize>;

impl StateMachine<0> {
    pub fn next(self) -> StateMachine<1> { StateMachine::<1> }
}