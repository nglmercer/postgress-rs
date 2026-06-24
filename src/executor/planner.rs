use crate::protocol::codes::Query;

pub struct SeqScan {
    pub rel_oid: u32,
    pub filter: Option<String>,
}

pub struct IndexScan {
    pub index_oid: u32,
    pub scan_from: Vec<u8>,
}

pub enum Plan {
    SeqScan(SeqScan),
    IndexScan(IndexScan),
}

pub struct Planner;

impl Planner {
    pub fn plan(query: &Query) -> Plan {
        match query {
            Query::Select { table, where_clause, .. } => {
                let rel_oid = table.0 .0;
                if let Some(filter) = where_clause {
                    Plan::SeqScan(SeqScan { rel_oid, filter: Some(filter.clone()) })
                } else {
                    Plan::SeqScan(SeqScan { rel_oid, filter: None })
                }
            }
            Query::Insert { table, .. } => {
                Plan::SeqScan(SeqScan { rel_oid: table.0 .0, filter: None })
            }
            Query::CreateTable { .. } => {
                Plan::SeqScan(SeqScan { rel_oid: 0, filter: None })
            }
            Query::DropTable { .. } => {
                Plan::SeqScan(SeqScan { rel_oid: 0, filter: None })
            }
            Query::Begin { .. } => {
                Plan::SeqScan(SeqScan { rel_oid: 0, filter: None })
            }
            Query::Commit => {
                Plan::SeqScan(SeqScan { rel_oid: 0, filter: None })
            }
            Query::Rollback => {
                Plan::SeqScan(SeqScan { rel_oid: 0, filter: None })
            }
        }
    }

    pub fn seq_scan_cost(pages: usize, tuples: usize) -> f64 {
        pages as f64 * 1.0 + tuples as f64 * 0.01
    }

    pub fn index_scan_cost(pages: usize, selectivity: f64, total_tuples: usize) -> f64 {
        pages as f64 * 1.0 + (total_tuples as f64 * selectivity) * 1.1
    }

    pub fn estimate_selectivity(filter: &str) -> f64 {
        if filter.contains('=') {
            0.1
        } else if filter.contains('>') || filter.contains('<') {
            0.3
        } else if filter.contains("LIKE") || filter.contains("like") {
            0.5
        } else {
            1.0
        }
    }
}
