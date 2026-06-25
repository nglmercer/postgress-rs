use crate::buffer_cache::SharedBufferCache;
use crate::executor::heap::heap_scan_with_optional_snapshot;
use crate::executor::select::{execute_select_with_snapshot, Row};
use crate::sql::ast::*;
use crate::types::*;

pub struct ExecContext<'a> {
    pub(crate) cache: &'a SharedBufferCache,
    pub(crate) catalog: &'a crate::catalog::Catalog,
    pub(crate) col_names: Vec<String>,
    pub(crate) tuple_desc: Option<TupleDesc>,
    pub(crate) snapshot: Option<crate::transaction::Snapshot>,
}

impl<'a> ExecContext<'a> {
    pub fn new(cache: &'a SharedBufferCache, catalog: &'a crate::catalog::Catalog) -> Self {
        Self {
            cache,
            catalog,
            col_names: vec![],
            tuple_desc: None,
            snapshot: None,
        }
    }

    pub fn with_snapshot(mut self, snapshot: crate::transaction::Snapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    pub async fn execute_from(
        &mut self,
        from: &FromClause,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
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

    pub async fn resolve_table_ref(
        &mut self,
        table_ref: &TableRef,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        match table_ref {
            TableRef::Table(name) => {
                let table_str = name.parts.join(".");
                let rels = self.catalog.list_relations();
                let rel = rels
                    .iter()
                    .find(|r| r.name.to_uppercase() == table_str.to_uppercase())
                    .ok_or_else(|| anyhow::anyhow!("relation \"{}\" does not exist", table_str))?;
                let rel_oid = rel.rel_oid.0;
                let desc = rel.tuple_desc.clone();
                self.col_names = desc.fields.iter().map(|a| a.name.clone()).collect();
                self.tuple_desc = Some(desc);
                let rows =
                    heap_scan_with_optional_snapshot(self.cache, rel_oid, self.snapshot.as_ref())
                        .await?;
                Ok(rows)
            }
            TableRef::Subquery(sub) => {
                let sub_select = sub.as_ref().clone();
                let result = Box::pin(execute_select_with_snapshot(
                    &sub_select,
                    self.cache,
                    self.catalog,
                    self.snapshot.clone(),
                ))
                .await?;
                let rows: Vec<(ItemPointerData, Row)> = result
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| {
                        (
                            ItemPointerData {
                                page_id: PageId(i as u32),
                                offset: 0,
                            },
                            row,
                        )
                    })
                    .collect();
                self.col_names = result.columns;
                Ok(rows)
            }
            TableRef::Function(_) => {
                anyhow::bail!("function calls in FROM not yet supported");
            }
        }
    }

    pub fn resolve_columns(&self, select_list: &[SelectItem]) -> anyhow::Result<Vec<String>> {
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
                            fname.to_string()
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
