use crate::sql::ast::*;
use crate::types::*;
use crate::buffer_cache::SharedBufferCache;
use crate::executor::heap::heap_scan;
use std::collections::HashMap;

pub type Row = Vec<String>;

pub struct SelectResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

pub async fn execute_select(
    select: &SelectStatement,
    cache: &SharedBufferCache,
    catalog: &crate::catalog::Catalog,
) -> anyhow::Result<SelectResult> {
    let mut ctx = ExecContext::new(cache, catalog);

    let base_rows = if let Some(ref from) = select.from {
        ctx.execute_from(from).await?
    } else {
        ctx.execute_select_without_from(&select.select_list)
    };

    let filtered = if let Some(ref where_clause) = select.where_clause {
        ctx.apply_where(base_rows, where_clause)
    } else {
        base_rows
    };

    let grouped = if !select.group_by.is_empty() || ctx.has_aggregates(&select.select_list) {
        ctx.apply_group_by(filtered, &select.group_by, &select.select_list)
    } else {
        filtered
    };

    let having_filtered = if let Some(ref having) = select.having {
        ctx.apply_having(grouped, having)
    } else {
        grouped
    };

    let distinct_rows = match &select.distinct {
        DistinctClause::Distinct => ctx.apply_distinct(having_filtered),
        DistinctClause::DistinctOn(exprs) => ctx.apply_distinct_on(having_filtered, exprs),
        DistinctClause::All => having_filtered,
    };

    let ordered = if !select.order_by.is_empty() {
        ctx.apply_order_by(distinct_rows, &select.order_by)
    } else {
        distinct_rows
    };

    let offset_rows = if let Some(ref offset_expr) = select.offset {
        ctx.apply_offset(ordered, offset_expr)
    } else {
        ordered
    };

    let final_rows = if let Some(ref limit) = select.limit {
        ctx.apply_limit_offset(offset_rows, limit)
    } else {
        offset_rows
    };

    let mut result_rows: Vec<Row> = final_rows.into_iter().map(|(_tid, row)| row).collect();

    // Apply window functions if any
    ctx.apply_window_functions(&mut result_rows, &select.select_list)?;

    let columns = ctx.resolve_columns(&select.select_list)?;

    // Handle set operations (UNION, INTERSECT, EXCEPT)
    if !select.set_operations.is_empty() {
        let mut result = SelectResult { columns, rows: result_rows };
        for set_op in &select.set_operations {
                let right_result = Box::pin(execute_select(&set_op.select, cache, catalog)).await?;
            result = ctx.apply_set_operation(result, right_result, &set_op.operator)?;
        }
        return Ok(result);
    }

    Ok(SelectResult { columns, rows: result_rows })
}

struct ExecContext<'a> {
    cache: &'a SharedBufferCache,
    catalog: &'a crate::catalog::Catalog,
    col_names: Vec<String>,
    tuple_desc: Option<TupleDesc>,
}

impl<'a> ExecContext<'a> {
    fn new(cache: &'a SharedBufferCache, catalog: &'a crate::catalog::Catalog) -> Self {
        Self {
            cache,
            catalog,
            col_names: vec![],
            tuple_desc: None,
        }
    }

    async fn execute_from(&mut self, from: &FromClause) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        if from.joins.is_empty() {
            return Ok(vec![]);
        }

        let first = &from.joins[0];
        let mut result = self.resolve_table_ref(&first.table).await?;

        for join in &from.joins[1..] {
            let right_rows = self.resolve_table_ref(&join.table).await?;
            let left_desc = self.tuple_desc.clone();
            result = self.execute_join(result, right_rows, join, &left_desc)?;
        }

        Ok(result)
    }

    fn execute_select_without_from(&self, select_list: &[SelectItem]) -> Vec<(ItemPointerData, Row)> {
        let has_star = select_list.iter().any(|item| matches!(item, SelectItem::Star | SelectItem::TableStar { .. }));
        if has_star {
            return vec![];
        }
        let row: Row = select_list.iter().map(|item| {
            match item {
                SelectItem::Expr(e) | SelectItem::ExprAs { expr: e, .. } => {
                    crate::server::evaluate_expr(e, &[], None).unwrap_or_default()
                }
                SelectItem::Star | SelectItem::TableStar { .. } => unreachable!(),
            }
        }).collect();
        vec![(ItemPointerData { page_id: PageId(0), offset: 0 }, row)]
    }

    fn apply_offset(&self, rows: Vec<(ItemPointerData, Row)>, offset_expr: &Expr) -> Vec<(ItemPointerData, Row)> {
        let offset_val = crate::server::evaluate_expr(offset_expr, &[], self.tuple_desc.as_ref())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        rows.into_iter().skip(offset_val).collect()
    }

    fn apply_window_functions(&self, rows: &mut Vec<Row>, select_list: &[SelectItem]) -> anyhow::Result<()> {
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

    fn apply_set_operation(
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

        Ok(SelectResult { columns, rows: result_rows })
    }

    async fn resolve_table_ref(&mut self, table_ref: &TableRef) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        match table_ref {
            TableRef::Table(name) => {
                let table_str = name.parts.join(".");
                let rels = self.catalog.list_relations();
                let rel = rels.iter()
                    .find(|r| r.name.to_uppercase() == table_str.to_uppercase())
                    .ok_or_else(|| anyhow::anyhow!("relation \"{}\" does not exist", table_str))?;
                let rel_oid = rel.rel_oid.0;
                let desc = rel.tuple_desc.clone();
                self.col_names = desc.fields.iter().map(|a| a.name.clone()).collect();
                self.tuple_desc = Some(desc);
                let rows = heap_scan(self.cache, rel_oid).await?;
                Ok(rows)
            }
            TableRef::Subquery(sub) => {
                let sub_select = sub.as_ref().clone();
                let result = Box::pin(execute_select(&sub_select, self.cache, self.catalog)).await?;
                let rows: Vec<(ItemPointerData, Row)> = result.rows.into_iter().enumerate().map(|(i, row)| {
                    (ItemPointerData { page_id: PageId(i as u32), offset: 0 }, row)
                }).collect();
                self.col_names = result.columns;
                Ok(rows)
            }
            TableRef::Function(_) => {
                anyhow::bail!("function calls in FROM not yet supported");
            }
        }
    }

    fn execute_join(
        &self,
        left: Vec<(ItemPointerData, Row)>,
        right: Vec<(ItemPointerData, Row)>,
        join: &Join,
        left_desc: &Option<TupleDesc>,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        let mut result = Vec::new();
        let right_col_count = right.first().map(|(_, r)| r.len()).unwrap_or(0);

        match &join.constraint {
            JoinConstraint::On(on_expr) => {
                match join.join_type {
                    JoinType::Inner => {
                        for (ltid, lrow) in &left {
                            for (_rtid, rrow) in &right {
                                let mut combined = lrow.clone();
                                combined.extend(rrow.clone());
                                if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                                    result.push((ltid.clone(), combined));
                                }
                            }
                        }
                    }
                    JoinType::Left => {
                        for (ltid, lrow) in &left {
                            let mut found_match = false;
                            for (_rtid, rrow) in &right {
                                let mut combined = lrow.clone();
                                combined.extend(rrow.clone());
                                if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                                    found_match = true;
                                    result.push((ltid.clone(), combined));
                                }
                            }
                            if !found_match {
                                let mut combined = lrow.clone();
                                combined.extend(vec!["".to_string(); right_col_count]);
                                result.push((ltid.clone(), combined));
                            }
                        }
                    }
                    JoinType::Right => {
                        for (rtid, rrow) in &right {
                            let mut found_match = false;
                            for (_ltid, lrow) in &left {
                                let mut combined = lrow.clone();
                                combined.extend(rrow.clone());
                                if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                                    found_match = true;
                                    result.push((rtid.clone(), combined));
                                }
                            }
                            if !found_match {
                                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                                let mut combined = vec!["".to_string(); left_col_count];
                                combined.extend(rrow.clone());
                                result.push((rtid.clone(), combined));
                            }
                        }
                    }
                    JoinType::Full => {
                        let mut right_matched = vec![false; right.len()];
                        for (ltid, lrow) in &left {
                            let mut found_match = false;
                            for (j, (_rtid, rrow)) in right.iter().enumerate() {
                                let mut combined = lrow.clone();
                                combined.extend(rrow.clone());
                                if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                                    found_match = true;
                                    right_matched[j] = true;
                                    result.push((ltid.clone(), combined));
                                }
                            }
                            if !found_match {
                                let mut combined = lrow.clone();
                                combined.extend(vec!["".to_string(); right_col_count]);
                                result.push((ltid.clone(), combined));
                            }
                        }
                        for (j, (rtid, rrow)) in right.iter().enumerate() {
                            if !right_matched[j] {
                                let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                                let mut combined = vec!["".to_string(); left_col_count];
                                combined.extend(rrow.clone());
                                result.push((rtid.clone(), combined));
                            }
                        }
                    }
                    JoinType::Cross | JoinType::Lateral => {
                        for (ltid, lrow) in &left {
                            for (_rtid, rrow) in &right {
                                let mut combined = lrow.clone();
                                combined.extend(rrow.clone());
                                result.push((ltid.clone(), combined));
                            }
                        }
                    }
                }
            }
            JoinConstraint::None => {
                for (ltid, lrow) in &left {
                    for (_rtid, rrow) in &right {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        result.push((ltid.clone(), combined));
                    }
                }
            }
            JoinConstraint::Using(cols) => {
                let right_start = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                for (ltid, lrow) in &left {
                    for (_rtid, rrow) in &right {
                        let mut match_all = true;
                        for col in cols {
                            let left_idx = self.col_names.iter().position(|n| n.eq_ignore_ascii_case(col));
                            let right_idx = self.col_names.iter().position(|n| n.eq_ignore_ascii_case(col));
                            if let (Some(li), Some(ri)) = (left_idx, right_idx) {
                                if li < lrow.len() && (right_start + ri) < (right_start + rrow.len()) {
                                    if lrow[li] != rrow[ri] {
                                        match_all = false;
                                        break;
                                    }
                                }
                            }
                        }
                        if match_all {
                            let mut combined = lrow.clone();
                            combined.extend(rrow.clone());
                            result.push((ltid.clone(), combined));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    fn evaluate_join_condition(
        &self,
        expr: &Expr,
        row: &Row,
        tuple_desc: &Option<TupleDesc>,
    ) -> bool {
        match crate::server::evaluate_expr(expr, row, tuple_desc.as_ref()) {
            Some(val) => val == "true" || val == "t",
            None => false,
        }
    }

    fn apply_where(&self, rows: Vec<(ItemPointerData, Row)>, where_clause: &Expr) -> Vec<(ItemPointerData, Row)> {
        rows.into_iter().filter(|(_tid, row)| {
            crate::server::evaluate_where(where_clause, row, self.tuple_desc.as_ref())
        }).collect()
    }

    fn has_aggregates(&self, select_list: &[SelectItem]) -> bool {
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

    fn apply_group_by(
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

    fn apply_having(&self, rows: Vec<(ItemPointerData, Row)>, having: &Expr) -> Vec<(ItemPointerData, Row)> {
        rows.into_iter().filter(|(_tid, row)| {
            crate::server::evaluate_where(having, row, self.tuple_desc.as_ref())
        }).collect()
    }

    fn apply_distinct(&self, rows: Vec<(ItemPointerData, Row)>) -> Vec<(ItemPointerData, Row)> {
        let mut seen = std::collections::HashSet::new();
        rows.into_iter().filter(|(_tid, row)| {
            seen.insert(row.clone())
        }).collect()
    }

    fn apply_distinct_on(&self, rows: Vec<(ItemPointerData, Row)>, on_exprs: &[Expr]) -> Vec<(ItemPointerData, Row)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (tid, row) in rows {
            let key: Vec<String> = on_exprs.iter().map(|e| {
                crate::server::evaluate_expr(e, &row, self.tuple_desc.as_ref())
                    .unwrap_or_default()
            }).collect();
            if seen.insert(key) {
                result.push((tid, row));
            }
        }
        result
    }

    fn apply_order_by(&self, mut rows: Vec<(ItemPointerData, Row)>, order_by: &[OrderByItem]) -> Vec<(ItemPointerData, Row)> {
        rows.sort_by(|a, b| {
            for item in order_by {
                let a_val = crate::server::evaluate_expr(&item.expr, &a.1, self.tuple_desc.as_ref())
                    .unwrap_or_default();
                let b_val = crate::server::evaluate_expr(&item.expr, &b.1, self.tuple_desc.as_ref())
                    .unwrap_or_default();

                let nulls_first = match item.nulls {
                    NullsOrder::First => true,
                    NullsOrder::Last | NullsOrder::Default => false,
                };

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

                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        });
        rows
    }

    fn apply_limit_offset(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        limit: &LimitClause,
    ) -> Vec<(ItemPointerData, Row)> {
        match limit {
            LimitClause::All => rows,
            LimitClause::Expr(expr) => {
                let limit_val = crate::server::evaluate_expr(expr, &[], self.tuple_desc.as_ref())
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                rows.into_iter().take(limit_val).collect()
            }
        }
    }

    fn resolve_columns(&self, select_list: &[SelectItem]) -> anyhow::Result<Vec<String>> {
        let mut cols = Vec::new();
        for item in select_list {
            match item {
                SelectItem::Star => {
                    if let Some(desc) = &self.tuple_desc {
                        for attr in &desc.fields {
                            cols.push(attr.name.clone());
                        }
                    }
                }
                SelectItem::TableStar { .. } => {
                    if let Some(desc) = &self.tuple_desc {
                        for attr in &desc.fields {
                            cols.push(attr.name.clone());
                        }
                    }
                }
                SelectItem::ExprAs { alias, .. } => {
                    cols.push(alias.clone());
                }
                SelectItem::Expr(expr) => {
                    let name = match expr {
                        Expr::Identifier(col) => col.clone(),
                        Expr::Function(f) => {
                            let fname = f.name.parts.last().cloned().unwrap_or_default();
                            format!("{}", fname)
                        }
                        Expr::Literal(lit) => match lit {
                            Literal::Number(n) => n.clone(),
                            Literal::String(s) => format!("'{}'", s),
                            Literal::Bool(b) => b.to_string(),
                            Literal::Null => "NULL".to_string(),
                            _ => "?".to_string(),
                        },
                        _ => "expr".to_string(),
                    };
                    cols.push(name);
                }
            }
        }
        Ok(cols)
    }
}
