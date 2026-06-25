pub mod pruning;

use crate::sql::ast::Expr;
use crate::types::Oid;

#[derive(Debug, Clone, PartialEq)]
pub enum PartitionStrategy {
    Range,
    List,
    Hash,
}

#[derive(Debug, Clone)]
pub struct PartitionSpec {
    pub strategy: PartitionStrategy,
    pub columns: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct RangeBound {
    pub inclusive: bool,
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ListBound {
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HashBound {
    pub modulus: u32,
    pub remainder: u32,
}

#[derive(Debug, Clone)]
pub enum PartitionBound {
    Range(Vec<RangeBound>),
    List(Vec<ListBound>),
    Hash(HashBound),
}

#[derive(Debug, Clone)]
pub struct PartitionEntry {
    pub partition_oid: Oid,
    pub bound: PartitionBound,
}

pub struct PartitionManager {
    parent_oid: Oid,
    strategy: PartitionStrategy,
    columns: Vec<String>,
    partitions: Vec<PartitionEntry>,
}

impl PartitionManager {
    pub fn new(parent_oid: Oid, strategy: PartitionStrategy, columns: Vec<String>) -> Self {
        Self {
            parent_oid,
            strategy,
            columns,
            partitions: Vec::new(),
        }
    }

    pub fn add_partition(&mut self, entry: PartitionEntry) {
        self.partitions.push(entry);
    }

    pub fn get_partitions(&self) -> &[PartitionEntry] {
        &self.partitions
    }

    pub fn get_parent_oid(&self) -> Oid {
        self.parent_oid
    }

    pub fn get_strategy(&self) -> &PartitionStrategy {
        &self.strategy
    }

    pub fn get_columns(&self) -> &[String] {
        &self.columns
    }

    pub fn partition_count(&self) -> usize {
        self.partitions.len()
    }

    pub fn find_partition_for_value(&self, column_value: &str) -> Option<&PartitionEntry> {
        let cv = column_value.to_string();
        match &self.strategy {
            PartitionStrategy::Range => self.partitions.iter().find(|p| {
                if let PartitionBound::Range(bounds) = &p.bound {
                    bounds.iter().any(|b| {
                        if b.inclusive {
                            cv <= *b.values.first().unwrap_or(&String::new())
                        } else {
                            cv < *b.values.first().unwrap_or(&String::new())
                        }
                    })
                } else {
                    false
                }
            }),
            PartitionStrategy::List => self.partitions.iter().find(|p| {
                if let PartitionBound::List(bounds) = &p.bound {
                    bounds
                        .iter()
                        .any(|b| b.values.contains(&column_value.to_string()))
                } else {
                    false
                }
            }),
            PartitionStrategy::Hash => {
                let hash = self.hash_value(column_value);
                self.partitions.iter().find(|p| {
                    if let PartitionBound::Hash(h) = &p.bound {
                        hash % h.modulus == h.remainder
                    } else {
                        false
                    }
                })
            }
        }
    }

    fn hash_value(&self, value: &str) -> u32 {
        let mut hash = 0u32;
        for byte in value.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_manager_new() {
        let pm = PartitionManager::new(
            Oid(1),
            PartitionStrategy::Range,
            vec!["created_at".to_string()],
        );
        assert_eq!(pm.get_parent_oid(), Oid(1));
        assert_eq!(pm.partition_count(), 0);
    }

    #[test]
    fn test_partition_manager_range() {
        let mut pm =
            PartitionManager::new(Oid(1), PartitionStrategy::Range, vec!["id".to_string()]);
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(2),
            bound: PartitionBound::Range(vec![RangeBound {
                inclusive: true,
                values: vec!["100".to_string()],
            }]),
        });
        assert_eq!(pm.partition_count(), 1);
    }

    #[test]
    fn test_partition_manager_list() {
        let mut pm =
            PartitionManager::new(Oid(1), PartitionStrategy::List, vec!["region".to_string()]);
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(2),
            bound: PartitionBound::List(vec![ListBound {
                values: vec!["us-east".to_string(), "us-west".to_string()],
            }]),
        });
        let found = pm.find_partition_for_value("us-east");
        assert!(found.is_some());
    }

    #[test]
    fn test_partition_manager_hash() {
        let mut pm = PartitionManager::new(Oid(1), PartitionStrategy::Hash, vec!["id".to_string()]);
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(2),
            bound: PartitionBound::Hash(HashBound {
                modulus: 4,
                remainder: 0,
            }),
        });
        pm.add_partition(PartitionEntry {
            partition_oid: Oid(3),
            bound: PartitionBound::Hash(HashBound {
                modulus: 4,
                remainder: 1,
            }),
        });
        assert_eq!(pm.partition_count(), 2);
    }
}
