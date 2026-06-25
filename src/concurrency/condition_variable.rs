use tokio::sync::Notify;

use super::Latch;

pub struct ConditionVariable {
    notify: Notify,
}

impl ConditionVariable {
    pub fn new() -> Self {
        Self {
            notify: Notify::new(),
        }
    }

    pub async fn wait(&self, latch: &Latch) {
        while !latch.is_set() {
            self.notify.notified().await;
        }
    }

    pub fn notify_one(&self) {
        self.notify.notify_one();
    }

    pub fn notify_all(&self) {
        self.notify.notify_waiters();
    }
}

impl Default for ConditionVariable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_condition_variable_notify_one() {
        let cv = Arc::new(ConditionVariable::new());
        let cv_clone = Arc::clone(&cv);

        let handle = tokio::spawn(async move {
            let latch = Latch::new();
            cv_clone.wait(&latch).await;
            true
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        cv.notify_one();

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_condition_variable_notify_all() {
        let cv = Arc::new(ConditionVariable::new());
        let cv1 = Arc::clone(&cv);
        let cv2 = Arc::clone(&cv);

        let handle1 = tokio::spawn(async move {
            let latch = Latch::new();
            cv1.wait(&latch).await;
            true
        });

        let handle2 = tokio::spawn(async move {
            let latch = Latch::new();
            cv2.wait(&latch).await;
            true
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        cv.notify_all();

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
