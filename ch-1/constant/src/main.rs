#[derive(Copy, Clone, PartialEq, Eq)]
pub enum State { Idle, Running, Stopped }

pub struct Fsm<const CURRENT: State>;

impl Fsm<{ State::Idle }> {
    pub fn start(self) -> Fsm<{ State::Running }> {
        println!("Transition: Idle -> Running");
        Fsm
    }
}

impl Fsm<{ State::Running }> {
    pub fn stop(self) -> Fsm<{ State::Stopped }> {
        println!("Transition: Running -> Stopped");
        Fsm
    }
}

fn execute_fsm() {
    let state = Fsm::<{ State::Idle }>;
    let running = state.start();
    let _stopped = running.stop();
    // running.stop(); // Error! `running` value moved.
}