use crate::sql::ast::*;
use crate::types::*;
use crate::executor::select::Row;
use std::collections::HashMap;

pub fn hash_join(
    left: &[(ItemPointerData, Row)],
    right: &[(ItemPointerData, Row)],
    on_expr: &Expr,
    join_type: JoinType,
    left_desc: &Option<TupleDesc>,
    col_names: &[String],
) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
    let mut result = Vec::new();
    let right_col_count = right.first().map(|(_, r)| r.len()).unwrap_or(0);

    // Build hash table from right side
    let mut hash_table: HashMap<String, Vec<&Row>> = HashMap::new();

    for (_rtid, rrow) in right {
        // For equi-join on expression like "a.id = b.id", extract the key
        if let Expr::BinaryOp { left, op: BinaryOperator::Equals, right: right_expr } = on_expr {
            if let Some(key_val) = crate::server::evaluate_expr(right_expr, rrow, left_desc.as_ref()) {
                hash_table.entry(key_val).or_default().push(rrow);
            }
        }
    }

    // Probe hash table with left side
    for (ltid, lrow) in left {
        if let Expr::BinaryOp { left: left_expr, op: BinaryOperator::Equals, .. } = on_expr {
            if let Some(key_val) = crate::server::evaluate_expr(left_expr, lrow, left_desc.as_ref()) {
                if let Some(matching_rows) = hash_table.get(&key_val) {
                    for rrow in matching_rows {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.iter().cloned());
                        result.push((ltid.clone(), combined));
                    }
                } else if matches!(join_type, JoinType::Left) {
                    let mut combined = lrow.clone();
                    combined.extend(vec!["".to_string(); right_col_count]);
                    result.push((ltid.clone(), combined));
                }
            }
        }
    }

    // Handle RIGHT JOIN unmatched right rows
    if matches!(join_type, JoinType::Right | JoinType::Full) {
        let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
        for (_rtid, rrow) in right {
            // Check if this right row was matched
            let mut matched = false;
            if let Expr::BinaryOp { right: right_expr, .. } = on_expr {
                if let Some(key_val) = crate::server::evaluate_expr(right_expr, rrow, left_desc.as_ref()) {
                    // Check if any left row had this key
                    for (_ltid, lrow) in left {
                        if let Some(lkey) = crate::server::evaluate_expr(
                            if let Expr::BinaryOp { left: l, .. } = on_expr { l } else { return Ok(result); },
                            lrow, left_desc.as_ref()
                        ) {
                            if lkey == key_val {
                                matched = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !matched {
                let mut combined = vec!["".to_string(); left_col_count];
                combined.extend(rrow.clone());
                result.push((ItemPointerData { page_id: PageId(0), offset: 0 }, combined));
            }
        }
    }

    // Handle FULL JOIN unmatched left rows
    if matches!(join_type, JoinType::Full) {
        let right_col_count_inner = right.first().map(|(_, r)| r.len()).unwrap_or(0);
        for (ltid, lrow) in left {
            let mut matched = false;
            if let Expr::BinaryOp { left: left_expr, .. } = on_expr {
                if let Some(key_val) = crate::server::evaluate_expr(left_expr, lrow, left_desc.as_ref()) {
                    for (_rtid, rrow) in right {
                        if let Some(rkey) = crate::server::evaluate_expr(
                            if let Expr::BinaryOp { right: r, .. } = on_expr { r } else { return Ok(result); },
                            rrow, left_desc.as_ref()
                        ) {
                            if rkey == key_val {
                                matched = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !matched {
                let mut combined = lrow.clone();
                combined.extend(vec!["".to_string(); right_col_count_inner]);
                result.push((ltid.clone(), combined));
            }
        }
    }

    Ok(result)
}

pub fn merge_join(
    left: &[(ItemPointerData, Row)],
    right: &[(ItemPointerData, Row)],
    on_expr: &Expr,
    join_type: JoinType,
    left_desc: &Option<TupleDesc>,
    col_names: &[String],
) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
    let mut result = Vec::new();
    let right_col_count = right.first().map(|(_, r)| r.len()).unwrap_or(0);

    if left.is_empty() || right.is_empty() {
        // Handle empty inputs for LEFT/RIGHT/FULL joins
        match join_type {
            JoinType::Left => {
                for (ltid, lrow) in left {
                    let mut combined = lrow.clone();
                    combined.extend(vec!["".to_string(); right_col_count]);
                    result.push((ltid.clone(), combined));
                }
            }
            JoinType::Right => {
                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                for (rtid, rrow) in right {
                    let mut combined = vec!["".to_string(); left_col_count];
                    combined.extend(rrow.clone());
                    result.push((rtid.clone(), combined));
                }
            }
            JoinType::Full => {
                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                for (ltid, lrow) in left {
                    let mut combined = lrow.clone();
                    combined.extend(vec!["".to_string(); right_col_count]);
                    result.push((ltid.clone(), combined));
                }
                for (rtid, rrow) in right {
                    let mut combined = vec!["".to_string(); left_col_count];
                    combined.extend(rrow.clone());
                    result.push((rtid.clone(), combined));
                }
            }
            _ => {}
        }
        return Ok(result);
    }

    // Extract join keys and sort
    let mut left_with_keys: Vec<(&ItemPointerData, &Row, String)> = left.iter().map(|(tid, row)| {
        let key = if let Expr::BinaryOp { left: left_expr, .. } = on_expr {
            crate::server::evaluate_expr(left_expr, row, left_desc.as_ref())
                .unwrap_or_default()
        } else {
            String::new()
        };
        (tid, row, key)
    }).collect();

    let mut right_with_keys: Vec<(&ItemPointerData, &Row, String)> = right.iter().map(|(tid, row)| {
        let key = if let Expr::BinaryOp { right: right_expr, .. } = on_expr {
            crate::server::evaluate_expr(right_expr, row, left_desc.as_ref())
                .unwrap_or_default()
        } else {
            String::new()
        };
        (tid, row, key)
    }).collect();

    left_with_keys.sort_by(|a, b| a.2.cmp(&b.2));
    right_with_keys.sort_by(|a, b| a.2.cmp(&b.2));

    // Merge join
    let mut i = 0;
    let mut j = 0;
    let mut left_matched = vec![false; left_with_keys.len()];
    let mut right_matched = vec![false; right_with_keys.len()];

    while i < left_with_keys.len() && j < right_with_keys.len() {
        let left_key = &left_with_keys[i].2;
        let right_key = &right_with_keys[j].2;

        if left_key == right_key {
            // Found matching keys - collect all matches
            let start_i = i;
            let start_j = j;

            // Find all left rows with this key
            while i < left_with_keys.len() && left_with_keys[i].2 == *left_key {
                let (ltid, lrow, _) = left_with_keys[i];

                // Find all right rows with this key
                let mut k = start_j;
                while k < right_with_keys.len() && right_with_keys[k].2 == *left_key {
                    let (rtid, rrow, _) = right_with_keys[k];
                    let mut combined = lrow.clone();
                    combined.extend(rrow.clone());
                    result.push((ltid.clone(), combined));
                    right_matched[k] = true;
                    k += 1;
                }
                left_matched[i] = true;
                i += 1;
            }

            // Skip remaining right rows with same key
            while j < right_with_keys.len() && right_with_keys[j].2 == *left_key {
                j += 1;
            }
        } else if left_key < right_key {
            // Left key is smaller, advance left
            if matches!(join_type, JoinType::Left) && !left_matched[i] {
                let (ltid, lrow, _) = left_with_keys[i];
                let mut combined = lrow.clone();
                combined.extend(vec!["".to_string(); right_col_count]);
                result.push((ltid.clone(), combined));
            }
            left_matched[i] = true;
            i += 1;
        } else {
            // Right key is smaller, advance right
            if matches!(join_type, JoinType::Right) && !right_matched[j] {
                let (rtid, rrow, _) = right_with_keys[j];
                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                let mut combined = vec!["".to_string(); left_col_count];
                combined.extend(rrow.clone());
                result.push((rtid.clone(), combined));
            }
            right_matched[j] = true;
            j += 1;
        }
    }

    // Handle remaining unmatched rows for LEFT/FULL joins
    if matches!(join_type, JoinType::Left | JoinType::Full) {
        while i < left_with_keys.len() {
            if !left_matched[i] {
                let (ltid, lrow, _) = left_with_keys[i];
                let mut combined = lrow.clone();
                combined.extend(vec!["".to_string(); right_col_count]);
                result.push((ltid.clone(), combined));
            }
            i += 1;
        }
    }

    // Handle remaining unmatched rows for RIGHT/FULL joins
    if matches!(join_type, JoinType::Right | JoinType::Full) {
        while j < right_with_keys.len() {
            if !right_matched[j] {
                let (rtid, rrow, _) = right_with_keys[j];
                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                let mut combined = vec!["".to_string(); left_col_count];
                combined.extend(rrow.clone());
                result.push((rtid.clone(), combined));
            }
            j += 1;
        }
    }

    Ok(result)
}

pub fn choose_join_algorithm(
    left_size: usize,
    right_size: usize,
    join_type: JoinType,
) -> JoinAlgorithm {
    // Use hash join for equi-joins when one side is small enough
    if left_size <= 1000 || right_size <= 1000 {
        JoinAlgorithm::Hash
    } else {
        JoinAlgorithm::Merge
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinAlgorithm {
    NestedLoop,
    Hash,
    Merge,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TupleDesc, Attribute, Oid};

    fn make_row(values: &[&str]) -> Row {
        values.iter().map(|s| s.to_string()).collect()
    }

    /// Build a minimal TupleDesc with ["id", "val"] columns so that
    /// `Identifier("id")` resolves to column 0 in evaluate_expr.
    fn make_desc() -> TupleDesc {
        TupleDesc {
            fields: vec![
                Attribute { name: "id".to_string(), type_oid: Oid(23), attnum: 1, typmod: -1 },
                Attribute { name: "val".to_string(), type_oid: Oid(25), attnum: 2, typmod: -1 },
            ],
        }
    }

    #[test]
    fn test_hash_join_inner() {
        let left = vec![
            (ItemPointerData { page_id: PageId(1), offset: 0 }, make_row(&["1", "Alice"])),
            (ItemPointerData { page_id: PageId(1), offset: 1 }, make_row(&["2", "Bob"])),
        ];
        let right = vec![
            (ItemPointerData { page_id: PageId(2), offset: 0 }, make_row(&["1", "100"])),
            (ItemPointerData { page_id: PageId(2), offset: 1 }, make_row(&["3", "300"])),
        ];

        let on_expr = Expr::BinaryOp {
            left: Box::new(Expr::Identifier("id".to_string())),
            op: BinaryOperator::Equals,
            right: Box::new(Expr::Identifier("id".to_string())),
        };

        let desc = Some(make_desc());
        let result = hash_join(&left, &right, &on_expr, JoinType::Inner, &desc, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1[0], "1");   // left id
        assert_eq!(result[0].1[3], "100"); // right val (idx 3 after combine)
    }

    #[test]
    fn test_merge_join_inner() {
        let left = vec![
            (ItemPointerData { page_id: PageId(1), offset: 0 }, make_row(&["1", "Alice"])),
            (ItemPointerData { page_id: PageId(1), offset: 1 }, make_row(&["2", "Bob"])),
        ];
        let right = vec![
            (ItemPointerData { page_id: PageId(2), offset: 0 }, make_row(&["1", "100"])),
            (ItemPointerData { page_id: PageId(2), offset: 1 }, make_row(&["3", "300"])),
        ];

        let on_expr = Expr::BinaryOp {
            left: Box::new(Expr::Identifier("id".to_string())),
            op: BinaryOperator::Equals,
            right: Box::new(Expr::Identifier("id".to_string())),
        };

        let desc = Some(make_desc());
        let result = merge_join(&left, &right, &on_expr, JoinType::Inner, &desc, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1[0], "1");   // left id
        assert_eq!(result[0].1[3], "100"); // right val (idx 3 after combine)
    }

    #[test]
    fn test_choose_join_algorithm() {
        assert_eq!(choose_join_algorithm(100, 10000, JoinType::Inner), JoinAlgorithm::Hash);
        assert_eq!(choose_join_algorithm(10000, 10000, JoinType::Inner), JoinAlgorithm::Merge);
    }
}