use tokio::sync::Mutex;

/// Factory stages and legacy issue operations share one execution slot.
/// Automatically followed issues wait before claiming a Worker job; manual requests fail visibly.
pub async fn reserve(running: &Mutex<bool>, wait: bool) -> bool {
    loop {
        let mut busy = running.lock().await;
        if !*busy {
            *busy = true;
            return true;
        }
        if !wait {
            return false;
        }
        drop(busy);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
