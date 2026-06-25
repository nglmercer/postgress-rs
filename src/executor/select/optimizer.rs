use crate::catalog::statistics::StatisticsCollector;
use crate::executor::select::Row;
use crate::sql::ast::*;
use crate::types::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub startup_cost: f64,
    pub total_cost: f64,
    pub estimated_rows: f64,
    pub width: usize,
}

#[derive(Debug, Clone)]
pub struct PlanNode {
    pub node_type: PlanNodeType,
    pub cost: CostEstimate,
    pub children: Vec<PlanNode>,
}

#[derive(Debug, Clone)]
pub enum PlanNodeType {
    SeqScan {
        table_oid: Oid,
        table_name: String,
    },
    IndexScan {
        table_oid: Oid,
        index_oid: Oid,
        table_name: String,
        index_name: String,
    },
    NestedLoopJoin {
        join_type: JoinType,
        condition: Option<Expr>,
    },
    HashJoin {
        join_type: JoinType,
        condition: Option<Expr>,
    },
    MergeJoin {
        join_type: JoinType,
        condition: Option<Expr>,
    },
    Sort {
        columns: Vec<String>,
        directions: Vec<SortDirection>,
    },
    HashAggregate {
        group_by: Vec<Expr>,
        aggregates: Vec<Expr>,
    },
    Limit {
        limit: usize,
        offset: usize,
    },
}

pub struct CostOptimizer {
    stats: StatisticsCollector,
    table_costs: HashMap<String, CostEstimate>,
}

impl CostOptimizer {
    pub fn new(stats: StatisticsCollector) -> Self {
        Self {
            stats,
            table_costs: HashMap::new(),
        }
    }

    pub fn estimate_seq_scan(
        &self,
        table_name: &str,
        table_oid: Oid,
        _selectivity: f64,
    ) -> CostEstimate {
        let rows = self
            .stats
            .get_table_stats(table_oid)
            .map(|s| s.row_count as f64)
            .unwrap_or(1000.0);

        let cpu_cost = rows * 0.01;
        let io_cost = rows * 0.1;

        CostEstimate {
            startup_cost: 0.0,
            total_cost: cpu_cost + io_cost,
            estimated_rows: rows,
            width: 100,
        }
    }

    pub fn estimate_index_scan(
        &self,
        table_name: &str,
        table_oid: Oid,
        index_oid: Oid,
        selectivity: f64,
    ) -> CostEstimate {
        let total_rows = self
            .stats
            .get_table_stats(table_oid)
            .map(|s| s.row_count as f64)
            .unwrap_or(1000.0);

        let estimated_rows = total_rows * selectivity;
        let index_cost = if estimated_rows > 1.0 {
            estimated_rows.ln() * 2.0
        } else {
            1.0
        };
        let heap_cost = estimated_rows * 0.1;

        CostEstimate {
            startup_cost: index_cost,
            total_cost: index_cost + heap_cost,
            estimated_rows,
            width: 100,
        }
    }

    pub fn estimate_nested_loop_join(
        &self,
        left_cost: &CostEstimate,
        right_cost: &CostEstimate,
        _selectivity: f64,
    ) -> CostEstimate {
        let outer_rows = left_cost.estimated_rows;
        let inner_rows = right_cost.estimated_rows;

        let cpu_cost = outer_rows * inner_rows * 0.001;
        let total_cost = left_cost.total_cost + right_cost.total_cost + cpu_cost;
        let estimated_rows = outer_rows * inner_rows * _selectivity;

        CostEstimate {
            startup_cost: left_cost.startup_cost + right_cost.startup_cost,
            total_cost,
            estimated_rows,
            width: left_cost.width + right_cost.width,
        }
    }

    pub fn estimate_hash_join(
        &self,
        left_cost: &CostEstimate,
        right_cost: &CostEstimate,
        _selectivity: f64,
    ) -> CostEstimate {
        let build_rows = right_cost.estimated_rows;
        let probe_rows = left_cost.estimated_rows;

        let build_cost = build_rows * 0.1;
        let probe_cost = probe_rows * 0.01;
        let total_cost = left_cost.total_cost + right_cost.total_cost + build_cost + probe_cost;
        let estimated_rows = probe_rows * _selectivity;

        CostEstimate {
            startup_cost: right_cost.startup_cost + build_cost,
            total_cost,
            estimated_rows,
            width: left_cost.width + right_cost.width,
        }
    }

    pub fn estimate_merge_join(
        &self,
        left_cost: &CostEstimate,
        right_cost: &CostEstimate,
        _selectivity: f64,
    ) -> CostEstimate {
        let left_rows = left_cost.estimated_rows;
        let right_rows = right_cost.estimated_rows;

        let sort_cost_left = left_rows * left_rows.ln() * 0.01;
        let sort_cost_right = right_rows * right_rows.ln() * 0.01;
        let merge_cost = (left_rows + right_rows) * 0.001;

        let total_cost = left_cost.total_cost
            + right_cost.total_cost
            + sort_cost_left
            + sort_cost_right
            + merge_cost;
        let estimated_rows = (left_rows + right_rows) / 2.0 * _selectivity;

        CostEstimate {
            startup_cost: sort_cost_left + sort_cost_right,
            total_cost,
            estimated_rows,
            width: left_cost.width + right_cost.width,
        }
    }

    pub fn choose_join_algorithm(
        &self,
        left_cost: &CostEstimate,
        right_cost: &CostEstimate,
        selectivity: f64,
    ) -> (PlanNodeType, CostEstimate) {
        let nested_loop = self.estimate_nested_loop_join(left_cost, right_cost, selectivity);
        let hash_join = self.estimate_hash_join(left_cost, right_cost, selectivity);
        let merge_join = self.estimate_merge_join(left_cost, right_cost, selectivity);

        if hash_join.total_cost <= nested_loop.total_cost
            && hash_join.total_cost <= merge_join.total_cost
        {
            (
                PlanNodeType::HashJoin {
                    join_type: JoinType::Inner,
                    condition: None,
                },
                hash_join,
            )
        } else if merge_join.total_cost <= nested_loop.total_cost {
            (
                PlanNodeType::MergeJoin {
                    join_type: JoinType::Inner,
                    condition: None,
                },
                merge_join,
            )
        } else {
            (
                PlanNodeType::NestedLoopJoin {
                    join_type: JoinType::Inner,
                    condition: None,
                },
                nested_loop,
            )
        }
    }

    pub fn optimize_join_order(
        &self,
        tables: &[(String, Oid, CostEstimate)],
    ) -> Vec<(String, Oid, CostEstimate)> {
        if tables.len() <= 1 {
            return tables.to_vec();
        }

        let mut sorted = tables.to_vec();
        sorted.sort_by(|a, b| {
            a.2.estimated_rows
                .partial_cmp(&b.2.estimated_rows)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted
    }

    pub fn estimate_selectivity(
        &self,
        table_oid: Oid,
        column_name: &str,
        op: &str,
        value: &str,
    ) -> f64 {
        self.stats
            .estimate_selectivity(table_oid, column_name, op, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::statistics::StatisticsCollector;

    fn make_stats() -> StatisticsCollector {
        let mut stats = StatisticsCollector::new();
        let rows = vec![
            (vec!["1".to_string(), "Alice".to_string()], true),
            (vec!["2".to_string(), "Bob".to_string()], true),
            (vec!["3".to_string(), "Charlie".to_string()], true),
            (vec!["4".to_string(), "Alice".to_string()], true),
            (vec!["5".to_string(), "Bob".to_string()], true),
        ];
        let cols = vec!["id".to_string(), "name".to_string()];
        stats.collect_table_stats(Oid(1), &rows, &cols);
        stats.collect_table_stats(Oid(2), &rows, &cols);
        stats
    }

    #[test]
    fn test_estimate_seq_scan() {
        let stats = make_stats();
        let optimizer = CostOptimizer::new(stats);

        let cost = optimizer.estimate_seq_scan("users", Oid(1), 0.1);
        assert!(cost.total_cost > 0.0);
        assert!(cost.estimated_rows > 0.0);
    }

    #[test]
    fn test_estimate_index_scan() {
        let stats = make_stats();
        let optimizer = CostOptimizer::new(stats);

        let cost = optimizer.estimate_index_scan("users", Oid(1), Oid(100), 0.1);
        assert!(cost.total_cost > 0.0);
        assert!(cost.estimated_rows > 0.0);
        assert!(cost.startup_cost > 0.0);
    }

    #[test]
    fn test_choose_join_algorithm() {
        let stats = make_stats();
        let optimizer = CostOptimizer::new(stats);

        let left_cost = CostEstimate {
            startup_cost: 0.0,
            total_cost: 10.0,
            estimated_rows: 100.0,
            width: 50,
        };
        let right_cost = CostEstimate {
            startup_cost: 0.0,
            total_cost: 10.0,
            estimated_rows: 100.0,
            width: 50,
        };

        let (algo, cost) = optimizer.choose_join_algorithm(&left_cost, &right_cost, 0.1);
        assert!(cost.total_cost > 0.0);
        match algo {
            PlanNodeType::HashJoin { .. }
            | PlanNodeType::MergeJoin { .. }
            | PlanNodeType::NestedLoopJoin { .. } => {}
            _ => panic!("Unexpected join algorithm"),
        }
    }

    #[test]
    fn test_optimize_join_order() {
        let stats = make_stats();
        let optimizer = CostOptimizer::new(stats);

        let tables = vec![
            (
                "large".to_string(),
                Oid(1),
                CostEstimate {
                    startup_cost: 0.0,
                    total_cost: 1000.0,
                    estimated_rows: 10000.0,
                    width: 100,
                },
            ),
            (
                "small".to_string(),
                Oid(2),
                CostEstimate {
                    startup_cost: 0.0,
                    total_cost: 10.0,
                    estimated_rows: 100.0,
                    width: 50,
                },
            ),
        ];

        let optimized = optimizer.optimize_join_order(&tables);
        assert_eq!(optimized[0].0, "small");
        assert_eq!(optimized[1].0, "large");
    }

    #[test]
    fn test_estimate_selectivity() {
        let stats = make_stats();
        let optimizer = CostOptimizer::new(stats);

        let sel = optimizer.estimate_selectivity(Oid(1), "name", "=", "Alice");
        assert!(sel > 0.0 && sel <= 1.0);
    }
}
