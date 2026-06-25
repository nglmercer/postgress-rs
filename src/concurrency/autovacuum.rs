use crate::types::Oid;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AutovacuumConfig {
    pub vacuum_threshold: f64,
    pub analyze_threshold: f64,
    pub cost_delay: Duration,
    pub cost_limit: usize,
    pub naptime: Duration,
}

impl Default for AutovacuumConfig {
    fn default() -> Self {
        Self {
            vacuum_threshold: 0.2,
            analyze_threshold: 0.1,
            cost_delay: Duration::from_millis(20),
            cost_limit: 200,
            naptime: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableVacuumStats {
    pub rel_oid: Oid,
    pub n_live_tup: u64,
    pub n_dead_tup: u64,
    pub last_vacuum: Option<Instant>,
    pub last_analyze: Option<Instant>,
}

impl TableVacuumStats {
    pub fn new(rel_oid: Oid) -> Self {
        Self {
            rel_oid,
            n_live_tup: 0,
            n_dead_tup: 0,
            last_vacuum: None,
            last_analyze: None,
        }
    }

    pub fn dead_ratio(&self) -> f64 {
        let total = self.n_live_tup + self.n_dead_tup;
        if total == 0 {
            0.0
        } else {
            self.n_dead_tup as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct VacuumDecision {
    pub should_vacuum: bool,
    pub should_analyze: bool,
    pub estimated_pages: u32,
}

pub struct AutovacuumDaemon {
    config: AutovacuumConfig,
    table_stats: HashMap<Oid, TableVacuumStats>,
}

impl AutovacuumDaemon {
    pub fn new(config: AutovacuumConfig) -> Self {
        Self {
            config,
            table_stats: HashMap::new(),
        }
    }

    pub fn update_stats(&mut self, rel_oid: Oid, n_live: u64, n_dead: u64) {
        let stats = self
            .table_stats
            .entry(rel_oid)
            .or_insert_with(|| TableVacuumStats::new(rel_oid));
        stats.n_live_tup = n_live;
        stats.n_dead_tup = n_dead;
    }

    pub fn record_vacuum(&mut self, rel_oid: Oid) {
        if let Some(stats) = self.table_stats.get_mut(&rel_oid) {
            stats.last_vacuum = Some(Instant::now());
        }
    }

    pub fn record_analyze(&mut self, rel_oid: Oid) {
        if let Some(stats) = self.table_stats.get_mut(&rel_oid) {
            stats.last_analyze = Some(Instant::now());
        }
    }

    pub fn compute_vacuum_decision(&self, rel_oid: Oid) -> VacuumDecision {
        let stats = self.table_stats.get(&rel_oid);
        let should_vacuum = stats.is_some_and(|s| {
            let dominated = s.dead_ratio() > self.config.vacuum_threshold;
            let timed_out = s
                .last_vacuum
                .is_none_or(|t| t.elapsed() > self.config.naptime);
            dominated && timed_out
        });

        let should_analyze = stats.is_some_and(|s| {
            let dominated = s.dead_ratio() > self.config.analyze_threshold;
            let timed_out = s
                .last_analyze
                .is_none_or(|t| t.elapsed() > self.config.naptime);
            dominated && timed_out
        });

        let estimated_pages = stats.map_or(0, |s| ((s.n_live_tup + s.n_dead_tup) / 1000) as u32);

        VacuumDecision {
            should_vacuum,
            should_analyze,
            estimated_pages,
        }
    }

    pub fn tables_needing_vacuum(&self) -> Vec<Oid> {
        self.table_stats
            .keys()
            .filter(|&&oid| self.compute_vacuum_decision(oid).should_vacuum)
            .cloned()
            .collect()
    }

    pub fn get_stats(&self, rel_oid: Oid) -> Option<&TableVacuumStats> {
        self.table_stats.get(&rel_oid)
    }

    pub fn all_stats(&self) -> &HashMap<Oid, TableVacuumStats> {
        &self.table_stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autovacuum_config_default() {
        let config = AutovacuumConfig::default();
        assert_eq!(config.vacuum_threshold, 0.2);
        assert_eq!(config.analyze_threshold, 0.1);
    }

    #[test]
    fn test_table_vacuum_stats_dead_ratio() {
        let mut stats = TableVacuumStats::new(Oid(1));
        stats.n_live_tup = 80;
        stats.n_dead_tup = 20;
        assert!((stats.dead_ratio() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_table_vacuum_stats_dead_ratio_empty() {
        let stats = TableVacuumStats::new(Oid(1));
        assert_eq!(stats.dead_ratio(), 0.0);
    }

    #[test]
    fn test_autovacuum_daemon_vacuum_decision() {
        let mut daemon = AutovacuumDaemon::new(AutovacuumConfig::default());
        daemon.update_stats(Oid(1), 70, 30);

        let decision = daemon.compute_vacuum_decision(Oid(1));
        assert!(decision.should_vacuum);
    }

    #[test]
    fn test_autovacuum_daemon_no_vacuum_needed() {
        let mut daemon = AutovacuumDaemon::new(AutovacuumConfig::default());
        daemon.update_stats(Oid(1), 90, 10);

        let decision = daemon.compute_vacuum_decision(Oid(1));
        assert!(!decision.should_vacuum);
    }

    #[test]
    fn test_autovacuum_daemon_tables_needing_vacuum() {
        let mut daemon = AutovacuumDaemon::new(AutovacuumConfig::default());
        daemon.update_stats(Oid(1), 70, 30);
        daemon.update_stats(Oid(2), 90, 10);

        let tables = daemon.tables_needing_vacuum();
        assert!(tables.contains(&Oid(1)));
        assert!(!tables.contains(&Oid(2)));
    }

    #[test]
    fn test_record_vacuum() {
        let mut daemon = AutovacuumDaemon::new(AutovacuumConfig::default());
        daemon.update_stats(Oid(1), 80, 20);
        daemon.record_vacuum(Oid(1));

        let stats = daemon.get_stats(Oid(1)).unwrap();
        assert!(stats.last_vacuum.is_some());
    }
}
