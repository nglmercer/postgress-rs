use crate::sql::ast::{BinaryOperator, Expr, FunctionArg, Literal, UnaryOperator};
use crate::types::TupleDesc;

pub fn evaluate_expr(
    expr: &Expr,
    row: &[String],
    tuple_desc: Option<&TupleDesc>,
) -> Option<String> {
    match expr {
        Expr::Literal(lit) => Some(match lit {
            Literal::Number(n) => n.clone(),
            Literal::String(s) => s.clone(),
            Literal::Bool(b) => b.to_string(),
            Literal::Null => "NULL".to_string(),
            Literal::Blob(b) => format!("{:?}", b),
            Literal::Date(d) => d.clone(),
            Literal::Time(t) => t.clone(),
            Literal::Timestamp(t) => t.clone(),
            Literal::TimestampTz(t) => t.clone(),
            Literal::Interval(i) => i.clone(),
            Literal::Json(j) => j.clone(),
            Literal::JsonB(j) => j.clone(),
            Literal::Uuid(u) => u.clone(),
            Literal::Money(m) => m.clone(),
            Literal::Bit(b) => b.clone(),
            Literal::Hex(h) => h.clone(),
        }),
        Expr::Identifier(col) => {
            if let Some(desc) = tuple_desc {
                if let Some((idx, _)) = desc
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.name.eq_ignore_ascii_case(col))
                {
                    row.get(idx).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        }
        Expr::BinaryOp { left, op, right } => {
            let left_val = evaluate_expr(left, row, tuple_desc)?;
            let right_val = evaluate_expr(right, row, tuple_desc)?;
            let result: Option<String> = match op {
                BinaryOperator::Equals => Some((left_val == right_val).to_string()),
                BinaryOperator::NotEquals => Some((left_val != right_val).to_string()),
                BinaryOperator::LessThan => Some((left_val < right_val).to_string()),
                BinaryOperator::LessOrEqual => Some((left_val <= right_val).to_string()),
                BinaryOperator::GreaterThan => Some((left_val > right_val).to_string()),
                BinaryOperator::GreaterOrEqual => Some((left_val >= right_val).to_string()),
                BinaryOperator::And => {
                    let l = left_val.parse::<bool>().ok()?;
                    let r = right_val.parse::<bool>().ok()?;
                    Some((l && r).to_string())
                }
                BinaryOperator::Or => {
                    let l = left_val.parse::<bool>().ok()?;
                    let r = right_val.parse::<bool>().ok()?;
                    Some((l || r).to_string())
                }
                BinaryOperator::Plus => {
                    let l = left_val.parse::<f64>().ok()?;
                    let r = right_val.parse::<f64>().ok()?;
                    Some(format!("{}", l + r))
                }
                BinaryOperator::Minus => {
                    let l = left_val.parse::<f64>().ok()?;
                    let r = right_val.parse::<f64>().ok()?;
                    Some(format!("{}", l - r))
                }
                BinaryOperator::Multiply => {
                    let l = left_val.parse::<f64>().ok()?;
                    let r = right_val.parse::<f64>().ok()?;
                    Some(format!("{}", l * r))
                }
                BinaryOperator::Divide => {
                    let l = left_val.parse::<f64>().ok()?;
                    let r = right_val.parse::<f64>().ok()?;
                    if r == 0.0 {
                        return None;
                    }
                    Some(format!("{}", l / r))
                }
                BinaryOperator::Modulo => {
                    let l = left_val.parse::<f64>().ok()?;
                    let r = right_val.parse::<f64>().ok()?;
                    if r == 0.0 {
                        return None;
                    }
                    Some(format!("{}", l % r))
                }
                BinaryOperator::Like => {
                    let pattern = right_val.replace("%", "");
                    Some((left_val.contains(&pattern)).to_string())
                }
                BinaryOperator::ILike => {
                    let pattern = right_val.replace("%", "");
                    Some((left_val.to_lowercase().contains(&pattern.to_lowercase())).to_string())
                }
                _ => None,
            };
            result
        }
        Expr::UnaryOp { op, expr } => {
            let val = evaluate_expr(expr, row, tuple_desc)?;
            match op {
                UnaryOperator::Not => {
                    let b = val.parse::<bool>().ok()?;
                    Some((!b).to_string())
                }
                UnaryOperator::Minus => {
                    let n = val.parse::<f64>().ok()?;
                    Some((-n).to_string())
                }
                UnaryOperator::Plus => val.parse::<f64>().ok().map(|n| n.to_string()),
                UnaryOperator::BitwiseNot => {
                    let n = val.parse::<i64>().ok()?;
                    Some((!n).to_string())
                }
            }
        }
        Expr::IsNull(inner) => {
            let val = evaluate_expr(inner, row, tuple_desc)?.trim().to_string();
            Some((val.is_empty() || val.eq_ignore_ascii_case("NULL")).to_string())
        }
        Expr::IsNotNull(inner) => {
            let val = evaluate_expr(inner, row, tuple_desc)?.trim().to_string();
            Some((!val.is_empty() && !val.eq_ignore_ascii_case("NULL")).to_string())
        }
        Expr::TypeCast { expr, .. } => evaluate_expr(expr, row, tuple_desc),
        Expr::QualifiedIdentifier { table: _, column } => {
            if let Some(desc) = tuple_desc {
                if let Some((idx, _)) = desc
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.name.eq_ignore_ascii_case(column))
                {
                    row.get(idx).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        }
        Expr::Case {
            operand,
            when_clauses,
            else_clause,
        } => {
            let operand_val = operand
                .as_ref()
                .and_then(|e| evaluate_expr(e, row, tuple_desc));
            for when in when_clauses {
                let cond_val = evaluate_expr(&when.when, row, tuple_desc)?;
                let matches = match &operand_val {
                    Some(op) => cond_val == *op,
                    None => cond_val == "true" || cond_val == "t",
                };
                if matches {
                    return evaluate_expr(&when.then, row, tuple_desc);
                }
            }
            if let Some(else_expr) = else_clause {
                evaluate_expr(else_expr, row, tuple_desc)
            } else {
                Some("NULL".to_string())
            }
        }
        Expr::Function(func) => {
            let name = func
                .name
                .parts
                .last()
                .map(|s| s.to_uppercase())
                .unwrap_or_default();
            let arg0 = func.args.first().and_then(|a| match a {
                FunctionArg::Expr(e) => evaluate_expr(e, row, tuple_desc),
                FunctionArg::Star => Some("*".to_string()),
            });
            let arg1 = func.args.get(1).and_then(|a| match a {
                FunctionArg::Expr(e) => evaluate_expr(e, row, tuple_desc),
                FunctionArg::Star => Some("*".to_string()),
            });
            let arg2 = func.args.get(2).and_then(|a| match a {
                FunctionArg::Expr(e) => evaluate_expr(e, row, tuple_desc),
                FunctionArg::Star => Some("*".to_string()),
            });
            match name.as_str() {
                "COUNT" => Some("1".to_string()),
                "SUM" | "AVG" | "MIN" | "MAX" => arg0,
                "LENGTH" => arg0.map(|v| v.len().to_string()),
                "UPPER" => arg0.map(|s| s.to_uppercase()),
                "LOWER" => arg0.map(|s| s.to_lowercase()),
                "TRIM" => arg0.map(|s| s.trim().to_string()),
                "SUBSTRING" => {
                    let s = arg0.unwrap_or_default();
                    let start = arg1
                        .unwrap_or("1".to_string())
                        .parse::<usize>()
                        .unwrap_or(1);
                    match arg2 {
                        Some(len_str) => {
                            let len = len_str.parse::<usize>().unwrap_or(0);
                            Some(s.chars().skip(start.saturating_sub(1)).take(len).collect())
                        }
                        None => Some(s.chars().skip(start.saturating_sub(1)).collect()),
                    }
                }
                "CONCAT" => {
                    let mut parts = Vec::new();
                    parts.extend(func.args.iter().filter_map(|a| match a {
                        FunctionArg::Expr(e) => evaluate_expr(e, row, tuple_desc),
                        FunctionArg::Star => Some("*".to_string()),
                    }));
                    Some(parts.join(""))
                }
                _ => arg0,
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = evaluate_expr(expr, row, tuple_desc)?;
            let low_val = evaluate_expr(low, row, tuple_desc)?;
            let high_val = evaluate_expr(high, row, tuple_desc)?;
            let in_range = if let (Ok(v), Ok(l), Ok(h)) = (
                val.parse::<f64>(),
                low_val.parse::<f64>(),
                high_val.parse::<f64>(),
            ) {
                v >= l && v <= h
            } else {
                val >= low_val && val <= high_val
            };
            Some((if *negated { !in_range } else { in_range }).to_string())
        }
        Expr::InList {
            expr,
            negated,
            list,
        } => {
            let val = evaluate_expr(expr, row, tuple_desc)?;
            let matches = list
                .iter()
                .any(|e| evaluate_expr(e, row, tuple_desc) == Some(val.clone()));
            Some((if *negated { !matches } else { matches }).to_string())
        }
        _ => None,
    }
}

pub fn evaluate_where(where_clause: &Expr, row: &[String], tuple_desc: Option<&TupleDesc>) -> bool {
    match evaluate_expr(where_clause, row, tuple_desc) {
        Some(result) => result == "true" || result == "t",
        None => false,
    }
}
