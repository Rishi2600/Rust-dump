enum Status {
    Pending,
    Processing(u8), // progress percentage
    Completed,
    Failed(String),
}

fn print_status(status: Status) {
    match status {
        Status::Pending => println!("Task queued."),
        Status::Processing(progress) => println!("In progress: {}%", progress),
        Status::Completed => println!("Task finished successfully!"),
        Status::Failed(reason) => println!("Task failed: {}", reason),
    }
}