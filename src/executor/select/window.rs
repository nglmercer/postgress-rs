use crate::sql::ast::*;
use crate::executor::select::Row;
use super::context::ExecContext;

impl<'a> ExecContext<'a> {
    pub fn apply_window_functions(&self, rows: &mut Vec<Row>, select_list: &[SelectItem]) -> anyhow::Result<()> {
        for (i, item) in select_list.iter().enumerate() {
            if let SelectItem::Expr(Expr::Function(f)) | SelectItem::ExprAs { expr: Expr::Function(f), .. } = item {
                if let Some(ref over) = f.over {
                    let name = f.name.parts.last().map(|s| s.to_uppercase()).unwrap_or_default();
                    self.compute_window_function(rows, i, &name, f, over)?;
                }
            }
        }
        Ok(())
    }

    fn compute_window_function(
        &self,
        rows: &mut Vec<Row>,
        col_idx: usize,
        func_name: &str,
        func: &FunctionCall,
        over: &WindowSpec,
    ) -> anyhow::Result<()> {
        if over.order_by.is_empty() && over.partition_by.is_empty() {
            // Simple aggregate-like window
            let val = match func_name {
                "ROW_NUMBER" => (1..=rows.len()).map(|i| i.to_string()).collect::<Vec<_>>(),
                "RANK" | "DENSE_RANK" => (1..=rows.len()).map(|i| i.to_string()).collect::<Vec<_>>(),
                "NTILE" => {
                    let n = func.args.first()
                        .and_then(|a| match a {
                            FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                            _ => None,
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1);
                    (0..rows.len()).map(|i| ((i * n / rows.len()) + 1).to_string()).collect()
                }
                _ => return Ok(()),
            };
            for (i, v) in val.into_iter().enumerate() {
                if let Some(row) = rows.get_mut(i) {
                    if col_idx < row.len() {
                        row[col_idx] = v;
                    } else {
                        row.push(v);
                    }
                }
            }
            return Ok(());
        }

        // Sort by OVER ORDER BY if present
        let mut indexed_rows: Vec<(usize, Row)> = rows.iter().cloned().enumerate().collect();
        if !over.order_by.is_empty() {
            indexed_rows.sort_by(|a, b| {
                for item in &over.order_by {
                    let a_val = crate::server::evaluate_expr(&item.expr, &a.1, self.tuple_desc.as_ref()).unwrap_or_default();
                    let b_val = crate::server::evaluate_expr(&item.expr, &b.1, self.tuple_desc.as_ref()).unwrap_or_default();
                    let nulls_first = matches!(item.nulls, NullsOrder::First);
                    let a_null = a_val.is_empty() || a_val.eq_ignore_ascii_case("NULL");
                    let b_null = b_val.is_empty() || b_val.eq_ignore_ascii_case("NULL");
                    if a_null && b_null { continue; }
                    if a_null { return if nulls_first { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }; }
                    if b_null { return if nulls_first { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less }; }
                    let cmp = a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal);
                    let cmp = match item.direction {
                        SortDirection::Desc | SortDirection::Default => cmp.reverse(),
                        SortDirection::Asc => cmp,
                    };
                    if cmp != std::cmp::Ordering::Equal { return cmp; }
                }
                std::cmp::Ordering::Equal
            });
        }

        let window_vals: Vec<String> = match func_name {
            "ROW_NUMBER" => (0..indexed_rows.len()).map(|i| (i + 1).to_string()).collect(),
            "RANK" => {
                let mut vals = Vec::new();
                let mut i = 0;
                while i < indexed_rows.len() {
                    let mut j = i;
                    while j < indexed_rows.len() {
                        let same = indexed_rows[i].1.iter().zip(indexed_rows[j].1.iter()).all(|(a, b)| a == b);
                        if !same { break; }
                        j += 1;
                    }
                    let rank = (i + 1).to_string();
                    for _ in i..j {
                        vals.push(rank.clone());
                    }
                    i = j;
                }
                vals
            }
            "DENSE_RANK" => {
                let mut vals = Vec::new();
                let mut rank = 1;
                let mut i = 0;
                while i < indexed_rows.len() {
                    let mut j = i;
                    while j < indexed_rows.len() {
                        let same = indexed_rows[i].1.iter().zip(indexed_rows[j].1.iter()).all(|(a, b)| a == b);
                        if !same { break; }
                        j += 1;
                    }
                    for _ in i..j {
                        vals.push(rank.to_string());
                    }
                    rank += 1;
                    i = j;
                }
                vals
            }
            "NTILE" => {
                let n = func.args.first()
                    .and_then(|a| match a {
                        FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                        _ => None,
                    })
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1);
                (0..indexed_rows.len()).map(|i| ((i * n / indexed_rows.len()) + 1).to_string()).collect()
            }
            "LAG" => {
                let mut vals = Vec::new();
                let default = func.args.get(2)
                    .and_then(|a| match a {
                        FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let offset = func.args.get(1)
                    .and_then(|a| match a {
                        FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                        _ => None,
                    })
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1);
                for i in 0..indexed_rows.len() {
                    if i >= offset {
                        vals.push(indexed_rows[i - offset].1.get(col_idx).cloned().unwrap_or_default());
                    } else {
                        vals.push(default.clone());
                    }
                }
                vals
            }
            "LEAD" => {
                let mut vals = Vec::new();
                let default = func.args.get(2)
                    .and_then(|a| match a {
                        FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let offset = func.args.get(1)
                    .and_then(|a| match a {
                        FunctionArg::Expr(e) => crate::server::evaluate_expr(e, &[], self.tuple_desc.as_ref()),
                        _ => None,
                    })
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1);
                for i in 0..indexed_rows.len() {
                    if i + offset < indexed_rows.len() {
                        vals.push(indexed_rows[i + offset].1.get(col_idx).cloned().unwrap_or_default());
                    } else {
                        vals.push(default.clone());
                    }
                }
                vals
            }
            _ => return Ok(()),
        };

        // Map back to original order
        for (i, v) in window_vals.into_iter().enumerate() {
            let orig_idx = indexed_rows[i].0;
            if let Some(row) = rows.get_mut(orig_idx) {
                if col_idx < row.len() {
                    row[col_idx] = v;
                } else {
                    row.push(v);
                }
            }
        }

        Ok(())
    }
}
