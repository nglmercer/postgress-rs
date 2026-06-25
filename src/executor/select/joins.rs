use super::context::ExecContext;
use crate::executor::select::algorithms::{
    choose_join_algorithm, hash_join, merge_join, JoinAlgorithm,
};
use crate::executor::select::Row;
use crate::sql::ast::*;
use crate::types::*;

impl<'a> ExecContext<'a> {
    pub fn execute_join(
        &self,
        left: Vec<(ItemPointerData, Row)>,
        right: Vec<(ItemPointerData, Row)>,
        join: &Join,
        left_desc: &Option<TupleDesc>,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        let right_col_count = right.first().map(|(_, r)| r.len()).unwrap_or(0);

        match &join.constraint {
            JoinConstraint::On(on_expr) => {
                if self.is_equi_join(on_expr) {
                    let algo = choose_join_algorithm(left.len(), right.len(), join.join_type);
                    match algo {
                        JoinAlgorithm::Hash => {
                            return hash_join(
                                &left,
                                &right,
                                on_expr,
                                join.join_type,
                                left_desc,
                                &self.col_names,
                            );
                        }
                        JoinAlgorithm::Merge => {
                            return merge_join(
                                &left,
                                &right,
                                on_expr,
                                join.join_type,
                                left_desc,
                                &self.col_names,
                            );
                        }
                        JoinAlgorithm::NestedLoop => {}
                    }
                }
                self.nested_loop_join(
                    &left,
                    &right,
                    on_expr,
                    join.join_type,
                    left_desc,
                    right_col_count,
                )
            }
            JoinConstraint::None => {
                let mut result = Vec::new();
                for (ltid, lrow) in &left {
                    for (_rtid, rrow) in &right {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        result.push((*ltid, combined));
                    }
                }
                Ok(result)
            }
            JoinConstraint::Using(cols) => self.using_join(&left, &right, cols, right_col_count),
        }
    }

    fn is_equi_join(&self, expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::BinaryOp {
                op: BinaryOperator::Equals,
                ..
            }
        )
    }

    fn nested_loop_join(
        &self,
        left: &[(ItemPointerData, Row)],
        right: &[(ItemPointerData, Row)],
        on_expr: &Expr,
        join_type: JoinType,
        left_desc: &Option<TupleDesc>,
        right_col_count: usize,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        let mut result = Vec::new();

        match join_type {
            JoinType::Inner => {
                for (ltid, lrow) in left {
                    for (_rtid, rrow) in right {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                            result.push((*ltid, combined));
                        }
                    }
                }
            }
            JoinType::Left => {
                for (ltid, lrow) in left {
                    let mut found_match = false;
                    for (_rtid, rrow) in right {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                            found_match = true;
                            result.push((*ltid, combined));
                        }
                    }
                    if !found_match {
                        let mut combined = lrow.clone();
                        combined.extend(vec!["".to_string(); right_col_count]);
                        result.push((*ltid, combined));
                    }
                }
            }
            JoinType::Right => {
                for (rtid, rrow) in right {
                    let mut found_match = false;
                    for (_ltid, lrow) in left {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                            found_match = true;
                            result.push((*rtid, combined));
                        }
                    }
                    if !found_match {
                        let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                        let mut combined = vec!["".to_string(); left_col_count];
                        combined.extend(rrow.clone());
                        result.push((*rtid, combined));
                    }
                }
            }
            JoinType::Full => {
                let mut right_matched = vec![false; right.len()];
                for (ltid, lrow) in left {
                    let mut found_match = false;
                    for (j, (_rtid, rrow)) in right.iter().enumerate() {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        if self.evaluate_join_condition(on_expr, &combined, left_desc) {
                            found_match = true;
                            right_matched[j] = true;
                            result.push((*ltid, combined));
                        }
                    }
                    if !found_match {
                        let mut combined = lrow.clone();
                        combined.extend(vec!["".to_string(); right_col_count]);
                        result.push((*ltid, combined));
                    }
                }
                for (j, (rtid, rrow)) in right.iter().enumerate() {
                    if !right_matched[j] {
                        let left_col_count = left.first().map(|(_, l)| l.len()).unwrap_or(0);
                        let mut combined = vec!["".to_string(); left_col_count];
                        combined.extend(rrow.clone());
                        result.push((*rtid, combined));
                    }
                }
            }
            JoinType::Cross | JoinType::Lateral => {
                for (ltid, lrow) in left {
                    for (_rtid, rrow) in right {
                        let mut combined = lrow.clone();
                        combined.extend(rrow.clone());
                        result.push((*ltid, combined));
                    }
                }
            }
        }

        Ok(result)
    }

    fn using_join(
        &self,
        left: &[(ItemPointerData, Row)],
        right: &[(ItemPointerData, Row)],
        cols: &[String],
        _right_col_count: usize,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        let mut result = Vec::new();
        let right_start = left.first().map(|(_, l)| l.len()).unwrap_or(0);
        for (ltid, lrow) in left {
            for (_rtid, rrow) in right {
                let mut match_all = true;
                for col in cols {
                    let left_idx = self
                        .col_names
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(col));
                    let right_idx = self
                        .col_names
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(col));
                    if let (Some(li), Some(ri)) = (left_idx, right_idx) {
                        if li < lrow.len()
                            && (right_start + ri) < (right_start + rrow.len())
                            && lrow[li] != rrow[ri]
                        {
                            match_all = false;
                            break;
                        }
                    }
                }
                if match_all {
                    let mut combined = lrow.clone();
                    combined.extend(rrow.clone());
                    result.push((*ltid, combined));
                }
            }
        }
        Ok(result)
    }

    pub fn evaluate_join_condition(
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
}
