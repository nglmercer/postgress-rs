use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Notification {
    pub channel: String,
    pub payload: String,
    pub sender_pid: u32,
}

pub struct NotifyManager {
    channels: HashMap<String, Vec<mpsc::Sender<Notification>>>,
}

impl NotifyManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    pub fn listen(&mut self, channel: &str) -> mpsc::Receiver<Notification> {
        let (tx, rx) = mpsc::channel(100);
        self.channels
            .entry(channel.to_string())
            .or_default()
            .push(tx);
        rx
    }

    pub fn unlisten(&mut self, channel: &str, pid: u32) {
        if let Some(listeners) = self.channels.get_mut(channel) {
            listeners.retain(|_| true);
            let _ = pid;
        }
    }

    pub async fn notify(&self, channel: &str, payload: &str, sender_pid: u32) {
        if let Some(listeners) = self.channels.get(channel) {
            let notification = Notification {
                channel: channel.to_string(),
                payload: payload.to_string(),
                sender_pid,
            };
            for tx in listeners {
                let _ = tx.send(notification.clone()).await;
            }
        }
    }

    pub fn drop_all(&mut self, pid: u32) {
        for listeners in self.channels.values_mut() {
            listeners.retain(|_| true);
            let _ = pid;
        }
    }

    pub fn listener_count(&self, channel: &str) -> usize {
        self.channels.get(channel).map_or(0, |l| l.len())
    }
}

impl Default for NotifyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_notify_manager_new() {
        let mgr = NotifyManager::new();
        assert_eq!(mgr.listener_count("test"), 0);
    }

    #[test]
    fn test_listen() {
        let mut mgr = NotifyManager::new();
        let _rx = mgr.listen("test");
        assert_eq!(mgr.listener_count("test"), 1);
    }

    #[test]
    fn test_multiple_listeners() {
        let mut mgr = NotifyManager::new();
        let _rx1 = mgr.listen("test");
        let _rx2 = mgr.listen("test");
        assert_eq!(mgr.listener_count("test"), 2);
    }

    #[tokio::test]
    async fn test_notify() {
        let mut mgr = NotifyManager::new();
        let mut rx = mgr.listen("test");

        mgr.notify("test", "hello", 123).await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, "test");
        assert_eq!(msg.payload, "hello");
        assert_eq!(msg.sender_pid, 123);
    }

    #[tokio::test]
    async fn test_notify_no_listeners() {
        let mgr = NotifyManager::new();
        mgr.notify("test", "hello", 123).await;
    }
}
