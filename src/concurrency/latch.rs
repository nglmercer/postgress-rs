use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

pub struct Latch {
    state: AtomicI32,
    notify: Notify,
}

impl Latch {
    pub fn new() -> Self {
        Self {
            state: AtomicI32::new(0),
            notify: Notify::new(),
        }
    }

    pub fn set(&self) {
        self.state.store(1, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub fn unset(&self) {
        self.state.store(0, Ordering::Release);
    }

    pub fn wait(&self) {
        while self.state.load(Ordering::Acquire) == 0 {
            std::thread::yield_now();
        }
    }

    pub async fn async_wait(&self) {
        while self.state.load(Ordering::Acquire) == 0 {
            self.notify.notified().await;
        }
    }

    pub fn is_set(&self) -> bool {
        self.state.load(Ordering::Acquire) != 0
    }

    pub fn try_wait(&self) -> bool {
        self.state.load(Ordering::Acquire) != 0
    }
}

impl Default for Latch {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Latch {
    fn clone(&self) -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_latch_new() {
        let latch = Latch::new();
        assert!(!latch.is_set());
    }

    #[test]
    fn test_latch_set_unset() {
        let latch = Latch::new();
        latch.set();
        assert!(latch.is_set());
        latch.unset();
        assert!(!latch.is_set());
    }

    #[test]
    fn test_latch_try_wait() {
        let latch = Latch::new();
        assert!(!latch.try_wait());
        latch.set();
        assert!(latch.try_wait());
    }

    #[test]
    fn test_latch_wait_in_thread() {
        let latch = Arc::new(Latch::new());
        let latch_clone = Arc::clone(&latch);

        let handle = thread::spawn(move || {
            latch_clone.wait();
            true
        });

        thread::sleep(std::time::Duration::from_millis(10));
        latch.set();

        assert!(handle.join().unwrap());
    }

    #[tokio::test]
    async fn test_latch_async_wait() {
        let latch = Arc::new(Latch::new());
        let latch_clone = Arc::clone(&latch);

        let handle = tokio::spawn(async move {
            latch_clone.async_wait().await;
            true
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        latch.set();

        assert!(handle.await.unwrap());
    }
}
