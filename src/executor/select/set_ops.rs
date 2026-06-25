use super::context::ExecContext;
use crate::executor::select::{Row, SelectResult};
use crate::sql::ast::*;

impl<'a> ExecContext<'a> {
    pub fn apply_set_operation(
        &self,
        left: SelectResult,
        right: SelectResult,
        operator: &SetOperator,
    ) -> anyhow::Result<SelectResult> {
        let columns = left.columns.clone();
        let mut result_rows = left.rows;

        match operator {
            SetOperator::Union => {
                let mut seen = std::collections::HashSet::new();
                result_rows.retain(|row| seen.insert(row.clone()));
                for row in right.rows {
                    if seen.insert(row.clone()) {
                        result_rows.push(row);
                    }
                }
            }
            SetOperator::UnionAll => {
                result_rows.extend(right.rows);
            }
            SetOperator::Intersect => {
                let right_set: std::collections::HashSet<Row> = right.rows.into_iter().collect();
                result_rows.retain(|row| right_set.contains(row));
            }
            SetOperator::IntersectAll => {
                let mut right_count = std::collections::HashMap::new();
                for row in &right.rows {
                    *right_count.entry(row.clone()).or_insert(0) += 1;
                }
                let mut result = Vec::new();
                for row in &result_rows {
                    if let Some(count) = right_count.get_mut(row) {
                        if *count > 0 {
                            result.push(row.clone());
                            *count -= 1;
                        }
                    }
                }
                result_rows = result;
            }
            SetOperator::Except => {
                let right_set: std::collections::HashSet<Row> = right.rows.into_iter().collect();
                result_rows.retain(|row| !right_set.contains(row));
            }
            SetOperator::ExceptAll => {
                let mut right_count = std::collections::HashMap::new();
                for row in &right.rows {
                    *right_count.entry(row.clone()).or_insert(0) += 1;
                }
                let mut result = Vec::new();
                for row in &result_rows {
                    if let Some(count) = right_count.get_mut(row) {
                        if *count > 0 {
                            *count -= 1;
                            continue;
                        }
                    }
                    result.push(row.clone());
                }
                result_rows = result;
            }
        }

        Ok(SelectResult {
            columns,
            rows: result_rows,
        })
    }
}
