use crate::types::Oid;
use crate::sql::ast::Expr;
use super::{PartitionManager, PartitionEntry};

pub struct PartitionPruner;

impl PartitionPruner {
    pub fn prune_partitions(
        where_clause: &Option<Expr>,
        partition_manager: &PartitionManager,
    ) -> Vec<Oid> {
        if let Some(where_clause) = where_clause {
            if let Some((column, value)) = Self::extract_equality_predicate(where_clause) {
                if partition_manager.get_columns().contains(&column) {
                    if let Some(entry) = partition_manager.find_partition_for_value(&value) {
                        return vec![entry.partition_oid];
                    }
                }
            }
        }

        partition_manager.get_partitions().iter()
            .map(|p| p.partition_oid)
            .collect()
    }

    fn extract_equality_predicate(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::BinaryOp { left, op, right } => {
                if let crate::sql::ast::BinaryOperator::Equals = op {
                    if let Expr::Identifier(col) = left.as_ref() {
                        if let Expr::Literal(lit) = right.as_ref() {
                            let value = match lit {
                                crate::sql::ast::Literal::String(s) => s.clone(),
                                crate::sql::ast::Literal::Number(n) => n.clone(),
                                _ => return None,
                            };
                            return Some((col.clone(), value));
                        }
                    }
                }
                None
            }
            Expr::BinaryOp { left, op, right } => {
                if let crate::sql::ast::BinaryOperator::And = op {
                    if let Some(result) = Self::extract_equality_predicate(left.as_ref()) {
                        return Some(result);
                    }
                    return Self::extract_equality_predicate(right.as_ref());
                }
                None
            }
            _ => None,
        }
    }

    pub fn estimate_partition_selectivity(
        where_clause: &Option<Expr>,
        partition_manager: &PartitionManager,
    ) -> f64 {
        let total_partitions = partition_manager.partition_count() as f64;
        if total_partitions == 0.0 {
            return 1.0;
        }

        if let Some(entry) = Self::prune_partitions(where_clause, partition_manager).first() {
            if Self::prune_partitions(where_clause, partition_manager).len() == 1 {
                return 1.0 / total_partitions;
            }
        }

        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::{Literal, BinaryOperator};
    use crate::executor::partition::{PartitionStrategy, PartitionEntry, PartitionBound, ListBound};

    #[test]
    fn test_prune_partitions_no_where() {
        let mut pm = PartitionManager::new(Oid(1), PartitionStrategy::List, vec!["region".to_string()]);
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(2),
            bound: PartitionBound::List(vec![ListBound { values: vec!["us-east".to_string()] }]),
        });
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(3),
            bound: PartitionBound::List(vec![ListBound { values: vec!["eu-west".to_string()] }]),
        });

        let result = PartitionPruner::prune_partitions(&None, &pm);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_prune_partitions_with_equality() {
        let mut pm = PartitionManager::new(Oid(1), PartitionStrategy::List, vec!["region".to_string()]);
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(2),
            bound: PartitionBound::List(vec![ListBound { values: vec!["us-east".to_string()] }]),
        });
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(3),
            bound: PartitionBound::List(vec![ListBound { values: vec!["eu-west".to_string()] }]),
        });

        let where_clause = Expr::BinaryOp {
            left: Box::new(Expr::Identifier("region".to_string())),
            op: BinaryOperator::Equals,
            right: Box::new(Expr::Literal(Literal::String("us-east".to_string()))),
        };

        let result = PartitionPruner::prune_partitions(&Some(where_clause), &pm);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], Oid(2));
    }
}
