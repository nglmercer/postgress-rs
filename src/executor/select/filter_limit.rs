use super::context::ExecContext;
use crate::executor::select::Row;
use crate::sql::ast::*;
use crate::types::*;

impl<'a> ExecContext<'a> {
    pub fn execute_select_without_from(
        &self,
        select_list: &[SelectItem],
    ) -> Vec<(ItemPointerData, Row)> {
        let has_star = select_list
            .iter()
            .any(|item| matches!(item, SelectItem::Star | SelectItem::TableStar { .. }));
        if has_star {
            return vec![];
        }
        let row: Row = select_list
            .iter()
            .map(|item| match item {
                SelectItem::Expr(e) | SelectItem::ExprAs { expr: e, .. } => {
                    crate::server::evaluate_expr(e, &[], None).unwrap_or_default()
                }
                SelectItem::Star | SelectItem::TableStar { .. } => unreachable!(),
            })
            .collect();
        vec![(
            ItemPointerData {
                page_id: PageId(0),
                offset: 0,
            },
            row,
        )]
    }

    pub fn apply_offset(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        offset_expr: &Expr,
    ) -> Vec<(ItemPointerData, Row)> {
        let offset_val = crate::server::evaluate_expr(offset_expr, &[], self.tuple_desc.as_ref())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        rows.into_iter().skip(offset_val).collect()
    }

    pub fn apply_where(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        where_clause: &Expr,
    ) -> Vec<(ItemPointerData, Row)> {
        rows.into_iter()
            .filter(|(_tid, row)| {
                crate::server::evaluate_where(where_clause, row, self.tuple_desc.as_ref())
            })
            .collect()
    }

    pub fn apply_having(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        having: &Expr,
    ) -> Vec<(ItemPointerData, Row)> {
        rows.into_iter()
            .filter(|(_tid, row)| {
                crate::server::evaluate_where(having, row, self.tuple_desc.as_ref())
            })
            .collect()
    }

    pub fn apply_distinct(&self, rows: Vec<(ItemPointerData, Row)>) -> Vec<(ItemPointerData, Row)> {
        let mut seen = std::collections::HashSet::new();
        rows.into_iter()
            .filter(|(_tid, row)| seen.insert(row.clone()))
            .collect()
    }

    pub fn apply_distinct_on(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        on_exprs: &[Expr],
    ) -> Vec<(ItemPointerData, Row)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (tid, row) in rows {
            let key: Vec<String> = on_exprs
                .iter()
                .map(|e| {
                    crate::server::evaluate_expr(e, &row, self.tuple_desc.as_ref())
                        .unwrap_or_default()
                })
                .collect();
            if seen.insert(key) {
                result.push((tid, row));
            }
        }
        result
    }

    pub fn apply_order_by(
        &self,
        mut rows: Vec<(ItemPointerData, Row)>,
        order_by: &[OrderByItem],
    ) -> Vec<(ItemPointerData, Row)> {
        rows.sort_by(|a, b| {
            for item in order_by {
                let a_val =
                    crate::server::evaluate_expr(&item.expr, &a.1, self.tuple_desc.as_ref())
                        .unwrap_or_default();
                let b_val =
                    crate::server::evaluate_expr(&item.expr, &b.1, self.tuple_desc.as_ref())
                        .unwrap_or_default();

                let nulls_first = match item.nulls {
                    NullsOrder::First => true,
                    NullsOrder::Last | NullsOrder::Default => false,
                };

                let a_null = a_val.is_empty() || a_val.eq_ignore_ascii_case("NULL");
                let b_null = b_val.is_empty() || b_val.eq_ignore_ascii_case("NULL");

                if a_null && b_null {
                    continue;
                }
                if a_null {
                    return if nulls_first {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }
                if b_null {
                    return if nulls_first {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    };
                }

                let cmp = a_val
                    .partial_cmp(&b_val)
                    .unwrap_or(std::cmp::Ordering::Equal);

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

    pub fn apply_limit_offset(
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
}
