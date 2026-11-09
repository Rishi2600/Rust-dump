pub async fn retry_async<F, Fut, T, E>(mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    loop {
        match op().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                // In production: add backoff sleep here
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}