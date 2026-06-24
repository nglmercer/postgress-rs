use crate::sql::ast::*;
use crate::types::*;
use crate::executor::select::Row;
use super::context::ExecContext;
use std::collections::HashMap;

impl<'a> ExecContext<'a> {
    pub fn has_aggregates(&self, select_list: &[SelectItem]) -> bool {
        for item in select_list {
            if Self::item_has_agg(item) {
                return true;
            }
        }
        false
    }

    fn item_has_agg(item: &SelectItem) -> bool {
        match item {
            SelectItem::Expr(e) | SelectItem::ExprAs { expr: e, .. } => Self::expr_has_agg(e),
            SelectItem::Star | SelectItem::TableStar { .. } => false,
        }
    }

    fn expr_has_agg(expr: &Expr) -> bool {
        match expr {
            Expr::Function(f) => {
                let name = f.name.parts.last().map(|s| s.to_uppercase()).unwrap_or_default();
                matches!(name.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
            }
            Expr::BinaryOp { left, right, .. } => Self::expr_has_agg(left) || Self::expr_has_agg(right),
            Expr::UnaryOp { expr, .. } => Self::expr_has_agg(expr),
            Expr::Between { expr, low, high, .. } => Self::expr_has_agg(expr) || Self::expr_has_agg(low) || Self::expr_has_agg(high),
            Expr::InList { expr, list, .. } => Self::expr_has_agg(expr) || list.iter().any(|e| Self::expr_has_agg(e)),
            _ => false,
        }
    }

    pub fn apply_group_by(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        group_by: &[Expr],
        select_list: &[SelectItem],
    ) -> Vec<(ItemPointerData, Row)> {
        if group_by.is_empty() && rows.is_empty() {
            return vec![];
        }

        if group_by.is_empty() && !self.has_aggregates(select_list) {
            return rows;
        }

        let mut groups: HashMap<Vec<String>, Vec<Row>> = HashMap::new();

        for (_tid, row) in &rows {
            let key: Vec<String> = if group_by.is_empty() {
                vec!["__all__".to_string()]
            } else {
                group_by.iter().map(|gb| {
                    crate::server::evaluate_expr(gb, row, self.tuple_desc.as_ref())
                        .unwrap_or_default()
                }).collect()
            };
            groups.entry(key).or_default().push(row.clone());
        }

        let mut result = Vec::new();
        for (_group_key, group_rows) in groups {
            let mut result_row = Row::new();
            for item in select_list {
                let val = self.evaluate_select_item_with_group(item, &group_rows);
                result_row.push(val);
            }
            let tid = ItemPointerData { page_id: PageId(0), offset: 0 };
            result.push((tid, result_row));
        }

        result
    }

    fn evaluate_select_item_with_group(&self, item: &SelectItem, group_rows: &[Row]) -> String {
        match item {
            SelectItem::Expr(e) | SelectItem::ExprAs { expr: e, .. } => {
                self.evaluate_expr_with_group(e, group_rows)
            }
            SelectItem::Star => {
                if let Some(first) = group_rows.first() {
                    first.join(", ")
                } else {
                    String::new()
                }
            }
            SelectItem::TableStar { .. } => {
                if let Some(first) = group_rows.first() {
                    first.join(", ")
                } else {
                    String::new()
                }
            }
        }
    }

    fn evaluate_expr_with_group(&self, expr: &Expr, group_rows: &[Row]) -> String {
        match expr {
            Expr::Function(f) => {
                let name = f.name.parts.last().map(|s| s.to_uppercase()).unwrap_or_default();
                match name.as_str() {
                    "COUNT" => {
                        if let Some(FunctionArg::Star) = f.args.first() {
                            group_rows.len().to_string()
                        } else {
                            let arg = f.args.first().and_then(|a| match a {
                                FunctionArg::Expr(e) => Some(e),
                                _ => None,
                            });
                            if let Some(inner) = arg {
                                group_rows.iter().filter(|row| {
                                    crate::server::evaluate_expr(inner, row, self.tuple_desc.as_ref()).is_some()
                                }).count().to_string()
                            } else {
                                group_rows.len().to_string()
                            }
                        }
                    }
                    "SUM" => {
                        let arg = f.args.first().and_then(|a| match a {
                            FunctionArg::Expr(e) => Some(e),
                            _ => None,
                        });
                        if let Some(inner) = arg {
                            let sum: f64 = group_rows.iter().filter_map(|row| {
                                crate::server::evaluate_expr(inner, row, self.tuple_desc.as_ref())
                                    .and_then(|v| v.parse::<f64>().ok())
                            }).sum();
                            format!("{}", sum)
                        } else {
                            "0".to_string()
                        }
                    }
                    "AVG" => {
                        let arg = f.args.first().and_then(|a| match a {
                            FunctionArg::Expr(e) => Some(e),
                            _ => None,
                        });
                        if let Some(inner) = arg {
                            let vals: Vec<f64> = group_rows.iter().filter_map(|row| {
                                crate::server::evaluate_expr(inner, row, self.tuple_desc.as_ref())
                                    .and_then(|v| v.parse::<f64>().ok())
                            }).collect();
                            if vals.is_empty() {
                                "0".to_string()
                            } else {
                                format!("{}", vals.iter().sum::<f64>() / vals.len() as f64)
                            }
                        } else {
                            "0".to_string()
                        }
                    }
                    "MIN" => {
                        let arg = f.args.first().and_then(|a| match a {
                            FunctionArg::Expr(e) => Some(e),
                            _ => None,
                        });
                        if let Some(inner) = arg {
                            let mut vals: Vec<String> = group_rows.iter().filter_map(|row| {
                                crate::server::evaluate_expr(inner, row, self.tuple_desc.as_ref())
                            }).collect();
                            vals.sort();
                            vals.first().cloned().unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                    "MAX" => {
                        let arg = f.args.first().and_then(|a| match a {
                            FunctionArg::Expr(e) => Some(e),
                            _ => None,
                        });
                        if let Some(inner) = arg {
                            let mut vals: Vec<String> = group_rows.iter().filter_map(|row| {
                                crate::server::evaluate_expr(inner, row, self.tuple_desc.as_ref())
                            }).collect();
                            vals.sort();
                            vals.last().cloned().unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                    _ => {
                        if let Some(first) = group_rows.first() {
                            crate::server::evaluate_expr(&Expr::Function(f.clone()), first, self.tuple_desc.as_ref())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                }
            }
            Expr::Identifier(col) => {
                if let Some(desc) = &self.tuple_desc {
                    if let Some(idx) = desc.fields.iter().position(|a| a.name.eq_ignore_ascii_case(col)) {
                        if let Some(first) = group_rows.first() {
                            first.get(idx).cloned().unwrap_or_default()
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
            Expr::Literal(lit) => match lit {
                Literal::Number(n) => n.clone(),
                Literal::String(s) => s.clone(),
                Literal::Bool(b) => b.to_string(),
                Literal::Null => "NULL".to_string(),
                _ => String::new(),
            },
            Expr::BinaryOp { left, op, right } => {
                let l = self.evaluate_expr_with_group(left, group_rows);
                let r = self.evaluate_expr_with_group(right, group_rows);
                self.eval_binary_op(&l, op, &r)
            }
            _ => {
                if let Some(first) = group_rows.first() {
                    crate::server::evaluate_expr(expr, first, self.tuple_desc.as_ref())
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            }
        }
    }

    fn eval_binary_op(&self, left: &str, op: &BinaryOperator, right: &str) -> String {
        match op {
            BinaryOperator::Plus => {
                let l = left.parse::<f64>().unwrap_or(0.0);
                let r = right.parse::<f64>().unwrap_or(0.0);
                format!("{}", l + r)
            }
            BinaryOperator::Minus => {
                let l = left.parse::<f64>().unwrap_or(0.0);
                let r = right.parse::<f64>().unwrap_or(0.0);
                format!("{}", l - r)
            }
            BinaryOperator::Multiply => {
                let l = left.parse::<f64>().unwrap_or(0.0);
                let r = right.parse::<f64>().unwrap_or(0.0);
                format!("{}", l * r)
            }
            BinaryOperator::Divide => {
                let l = left.parse::<f64>().unwrap_or(0.0);
                let r = right.parse::<f64>().unwrap_or(1.0);
                if r == 0.0 { "NULL".to_string() } else { format!("{}", l / r) }
            }
            BinaryOperator::Modulo => {
                let l = left.parse::<f64>().unwrap_or(0.0);
                let r = right.parse::<f64>().unwrap_or(1.0);
                if r == 0.0 { "NULL".to_string() } else { format!("{}", l % r) }
            }
            BinaryOperator::Equals => (left == right).to_string(),
            BinaryOperator::NotEquals => (left != right).to_string(),
            BinaryOperator::LessThan => (left < right).to_string(),
            BinaryOperator::LessOrEqual => (left <= right).to_string(),
            BinaryOperator::GreaterThan => (left > right).to_string(),
            BinaryOperator::GreaterOrEqual => (left >= right).to_string(),
            BinaryOperator::And => {
                let l = left.parse::<bool>().unwrap_or(false);
                let r = right.parse::<bool>().unwrap_or(false);
                (l && r).to_string()
            }
            BinaryOperator::Or => {
                let l = left.parse::<bool>().unwrap_or(false);
                let r = right.parse::<bool>().unwrap_or(false);
                (l || r).to_string()
            }
            BinaryOperator::Like => {
                let pattern = right.replace('%', "");
                left.contains(&pattern).to_string()
            }
            BinaryOperator::ILike => {
                let pattern = right.replace('%', "").to_lowercase();
                left.to_lowercase().contains(&pattern).to_string()
            }
            _ => "NULL".to_string(),
        }
    }
}
