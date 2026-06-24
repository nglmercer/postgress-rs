use crate::sql::ast::*;
use crate::types::*;
use crate::executor::select::Row;
use super::context::ExecContext;

impl<'a> ExecContext<'a> {
    pub fn execute_join(
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
