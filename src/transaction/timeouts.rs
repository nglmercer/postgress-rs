use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct TimeoutConfig {
    pub statement_timeout: Option<Duration>,
    pub lock_timeout: Option<Duration>,
}

impl TimeoutConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = Some(timeout);
        self
    }

    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = Some(timeout);
        self
    }
}

#[derive(Debug)]
pub struct StatementTimeoutError;

impl std::fmt::Display for StatementTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statement canceled due to statement timeout")
    }
}

#[derive(Debug)]
pub struct LockTimeoutError;

impl std::fmt::Display for LockTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lock timeout exceeded")
    }
}

impl std::error::Error for StatementTimeoutError {}
impl std::error::Error for LockTimeoutError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert!(config.statement_timeout.is_none());
        assert!(config.lock_timeout.is_none());
    }

    #[test]
    fn test_timeout_config_builder() {
        let config = TimeoutConfig::new()
            .with_statement_timeout(Duration::from_secs(30))
            .with_lock_timeout(Duration::from_secs(10));
        assert_eq!(config.statement_timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.lock_timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            StatementTimeoutError.to_string(),
            "statement canceled due to statement timeout"
        );
        assert_eq!(LockTimeoutError.to_string(), "lock timeout exceeded");
    }
}
