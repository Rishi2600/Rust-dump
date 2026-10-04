// Note: Requires an async runtime like Tokio
async fn fetch_data() -> String {
    // Simulated async network call
    String::from("Data received")
}

async fn process() {
    let data = fetch_data().await; // Non-blocking wait
    println!("{}", data);
}