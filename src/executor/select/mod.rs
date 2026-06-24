pub mod context;
pub mod joins;
pub mod group_by;
pub mod window;
pub mod set_ops;
pub mod filter_limit;

use crate::sql::ast::*;
use crate::buffer_cache::SharedBufferCache;

pub type Row = Vec<String>;

pub struct SelectResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

pub use context::ExecContext;

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
