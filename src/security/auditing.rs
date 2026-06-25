use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AuditLog {
    pub timestamp: String,
    pub user: String,
    pub database: String,
    pub statement: String,
    pub duration: Duration,
    pub rows_affected: Option<u64>,
    pub connection_id: u32,
    pub success: bool,
    pub error_message: Option<String>,
}

impl AuditLog {
    pub fn new(user: &str, database: &str, statement: &str, connection_id: u32) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user: user.to_string(),
            database: database.to_string(),
            statement: statement.to_string(),
            duration: Duration::from_millis(0),
            rows_affected: None,
            connection_id,
            success: true,
            error_message: None,
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_rows(mut self, rows: u64) -> Self {
        self.rows_affected = Some(rows);
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.success = false;
        self.error_message = Some(error.to_string());
        self
    }
}

pub struct AuditLogger {
    logs: Mutex<Vec<AuditLog>>,
    enabled: bool,
}

impl AuditLogger {
    pub fn new() -> Self {
        Self {
            logs: Mutex::new(Vec::new()),
            enabled: true,
        }
    }

    pub fn with_enabled(enabled: bool) -> Self {
        Self {
            logs: Mutex::new(Vec::new()),
            enabled,
        }
    }

    pub fn log_statement(&self, entry: AuditLog) {
        if !self.enabled {
            return;
        }
        let mut logs = self.logs.lock().unwrap();
        logs.push(entry);
    }

    pub fn log_connection(&self, user: &str, addr: &str, connected: bool) {
        if !self.enabled {
            return;
        }
        let entry = AuditLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            user: user.to_string(),
            database: String::new(),
            statement: if connected {
                format!("connection from {}", addr)
            } else {
                format!("disconnection from {}", addr)
            },
            duration: Duration::from_millis(0),
            rows_affected: None,
            connection_id: 0,
            success: true,
            error_message: None,
        };
        let mut logs = self.logs.lock().unwrap();
        logs.push(entry);
    }

    pub fn get_logs(&self) -> Vec<AuditLog> {
        let logs = self.logs.lock().unwrap();
        logs.clone()
    }

    pub fn clear_logs(&self) {
        let mut logs = self.logs.lock().unwrap();
        logs.clear();
    }

    pub fn log_count(&self) -> usize {
        let logs = self.logs.lock().unwrap();
        logs.len()
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_new() {
        let log = AuditLog::new("user1", "testdb", "SELECT 1", 1);
        assert_eq!(log.user, "user1");
        assert_eq!(log.database, "testdb");
        assert!(log.success);
    }

    #[test]
    fn test_audit_log_with_error() {
        let log = AuditLog::new("user1", "testdb", "SELECT 1", 1).with_error("syntax error");
        assert!(!log.success);
        assert_eq!(log.error_message.unwrap(), "syntax error");
    }

    #[test]
    fn test_audit_logger() {
        let logger = AuditLogger::new();
        let log = AuditLog::new("user1", "testdb", "SELECT 1", 1);
        logger.log_statement(log);
        assert_eq!(logger.log_count(), 1);
    }

    #[test]
    fn test_audit_logger_disabled() {
        let logger = AuditLogger::with_enabled(false);
        let log = AuditLog::new("user1", "testdb", "SELECT 1", 1);
        logger.log_statement(log);
        assert_eq!(logger.log_count(), 0);
    }

    #[test]
    fn test_audit_logger_connection() {
        let logger = AuditLogger::new();
        logger.log_connection("user1", "127.0.0.1:5432", true);
        assert_eq!(logger.log_count(), 1);
    }

    #[test]
    fn test_audit_logger_clear() {
        let logger = AuditLogger::new();
        logger.log_statement(AuditLog::new("user1", "testdb", "SELECT 1", 1));
        logger.clear_logs();
        assert_eq!(logger.log_count(), 0);
    }
}
