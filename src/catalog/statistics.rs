use crate::types::Oid;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ColumnStatistics {
    pub table_oid: Oid,
    pub column_name: String,
    pub null_fraction: f64,
    pub average_width: f32,
    pub distinct_values: f32,
    pub mcv_frequency: Vec<f32>,
    pub histogram_bounds: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TableStatistics {
    pub table_oid: Oid,
    pub row_count: u64,
    pub page_count: u64,
    pub columns: HashMap<String, ColumnStatistics>,
}

pub struct StatisticsCollector {
    tables: HashMap<Oid, TableStatistics>,
}

impl StatisticsCollector {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn collect_table_stats(
        &mut self,
        table_oid: Oid,
        rows: &[(Vec<String>, bool)],
        column_names: &[String],
    ) {
        let visible_rows = rows.iter().filter(|(_, v)| *v).count() as f64;
        let mut columns = HashMap::new();

        for (col_idx, col_name) in column_names.iter().enumerate() {
            let mut null_count = 0;
            let mut distinct_values = std::collections::HashSet::new();
            let mut total_width = 0;
            let mut value_counts = HashMap::new();

            for (values, is_visible) in rows {
                if !is_visible {
                    continue;
                }
                if let Some(val) = values.get(col_idx) {
                    if val.is_empty() || val == "NULL" {
                        null_count += 1;
                    } else {
                        total_width += val.len();
                        distinct_values.insert(val.clone());
                        *value_counts.entry(val.clone()).or_insert(0) += 1;
                    }
                }
            }

            let null_fraction = if visible_rows > 0.0 {
                null_count as f64 / visible_rows
            } else {
                0.0
            };

            let average_width = if !distinct_values.is_empty() {
                total_width as f32 / distinct_values.len() as f32
            } else {
                0.0
            };

            let distinct = if visible_rows > 0.0 {
                distinct_values.len() as f32
            } else {
                0.0
            };

            let mut mcv_freq: Vec<f32> = value_counts
                .values()
                .map(|&count| count as f32 / visible_rows as f32)
                .filter(|&f| f > 0.01)
                .collect();
            mcv_freq.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            mcv_freq.truncate(10);

            let mut bounds: Vec<String> = distinct_values.into_iter().collect();
            bounds.sort();
            let step = if bounds.len() > 1 {
                bounds.len() / 10
            } else {
                1
            };
            let histogram: Vec<String> = bounds.into_iter().step_by(step.max(1)).collect();

            columns.insert(
                col_name.clone(),
                ColumnStatistics {
                    table_oid,
                    column_name: col_name.clone(),
                    null_fraction,
                    average_width,
                    distinct_values: distinct,
                    mcv_frequency: mcv_freq,
                    histogram_bounds: histogram,
                },
            );
        }

        self.tables.insert(
            table_oid,
            TableStatistics {
                table_oid,
                row_count: visible_rows as u64,
                page_count: 0,
                columns,
            },
        );
    }

    pub fn get_table_stats(&self, table_oid: Oid) -> Option<&TableStatistics> {
        self.tables.get(&table_oid)
    }

    pub fn get_column_stats(&self, table_oid: Oid, column_name: &str) -> Option<&ColumnStatistics> {
        self.tables.get(&table_oid)?.columns.get(column_name)
    }

    pub fn estimate_selectivity(
        &self,
        table_oid: Oid,
        column_name: &str,
        op: &str,
        _value: &str,
    ) -> f64 {
        if let Some(stats) = self.get_column_stats(table_oid, column_name) {
            match op {
                "=" | "==" => {
                    if stats.distinct_values > 0.0 {
                        1.0 / stats.distinct_values as f64
                    } else {
                        0.1
                    }
                }
                "<" | "<=" => 0.3,
                ">" | ">=" => 0.3,
                "<>" | "!=" => 0.9,
                "LIKE" => 0.1,
                _ => 0.1,
            }
        } else {
            0.1
        }
    }

    pub fn estimate_join_cost(
        &self,
        left_oid: Oid,
        right_oid: Oid,
        left_rows: f64,
        right_rows: f64,
    ) -> f64 {
        let left_stats = self.get_table_stats(left_oid);
        let right_stats = self.get_table_stats(right_oid);

        let left_distinct = left_stats
            .and_then(|s| s.columns.values().next())
            .map(|c| c.distinct_values as f64)
            .unwrap_or(1000.0);

        let right_distinct = right_stats
            .and_then(|s| s.columns.values().next())
            .map(|c| c.distinct_values as f64)
            .unwrap_or(1000.0);

        left_rows * right_rows / left_distinct.max(right_distinct)
    }
}

impl Default for StatisticsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rows() -> Vec<(Vec<String>, bool)> {
        vec![
            (vec!["1".to_string(), "Alice".to_string()], true),
            (vec!["2".to_string(), "Bob".to_string()], true),
            (vec!["3".to_string(), "Charlie".to_string()], true),
            (vec!["4".to_string(), "Alice".to_string()], true),
            (vec!["5".to_string(), "Bob".to_string()], true),
        ]
    }

    #[test]
    fn test_collect_table_stats() {
        let mut collector = StatisticsCollector::new();
        let rows = make_rows();
        let cols = vec!["id".to_string(), "name".to_string()];

        collector.collect_table_stats(Oid(1), &rows, &cols);

        let stats = collector.get_table_stats(Oid(1)).unwrap();
        assert_eq!(stats.row_count, 5);
        assert!(stats.columns.contains_key("id"));
        assert!(stats.columns.contains_key("name"));
    }

    #[test]
    fn test_column_statistics() {
        let mut collector = StatisticsCollector::new();
        let rows = make_rows();
        let cols = vec!["id".to_string(), "name".to_string()];

        collector.collect_table_stats(Oid(1), &rows, &cols);

        let name_stats = collector.get_column_stats(Oid(1), "name").unwrap();
        assert_eq!(name_stats.null_fraction, 0.0);
        assert_eq!(name_stats.distinct_values, 3.0);
    }

    #[test]
    fn test_estimate_selectivity() {
        let mut collector = StatisticsCollector::new();
        let rows = make_rows();
        let cols = vec!["id".to_string(), "name".to_string()];

        collector.collect_table_stats(Oid(1), &rows, &cols);

        let sel = collector.estimate_selectivity(Oid(1), "name", "=", "Alice");
        assert!(sel > 0.0 && sel <= 1.0);
    }

    #[test]
    fn test_join_cost_estimation() {
        let mut collector = StatisticsCollector::new();
        let rows = make_rows();
        let cols = vec!["id".to_string(), "name".to_string()];

        collector.collect_table_stats(Oid(1), &rows, &cols);
        collector.collect_table_stats(Oid(2), &rows, &cols);

        let cost = collector.estimate_join_cost(Oid(1), Oid(2), 5.0, 5.0);
        assert!(cost > 0.0);
    }

    #[test]
    fn test_null_statistics() {
        let mut collector = StatisticsCollector::new();
        let rows = vec![
            (vec!["1".to_string(), "Alice".to_string()], true),
            (vec!["2".to_string(), "".to_string()], true),
            (vec!["3".to_string(), "NULL".to_string()], true),
            (vec!["4".to_string(), "Charlie".to_string()], true),
        ];
        let cols = vec!["id".to_string(), "name".to_string()];

        collector.collect_table_stats(Oid(1), &rows, &cols);

        let name_stats = collector.get_column_stats(Oid(1), "name").unwrap();
        assert!(name_stats.null_fraction > 0.0);
        assert_eq!(name_stats.distinct_values, 2.0);
    }
}
