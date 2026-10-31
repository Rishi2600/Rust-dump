pub struct FileState<State> {
    fd: i32,
    _state: State,
}