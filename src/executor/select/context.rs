use crate::buffer_cache::SharedBufferCache;
use crate::executor::btree::{btree_scan, BTreeScan};
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
        where_clause: Option<&Expr>,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        if from.joins.is_empty() {
            return Ok(vec![]);
        }

        let first = &from.joins[0];
        let mut result = self.resolve_table_ref(&first.table, where_clause).await?;

        for join in &from.joins[1..] {
            let right_rows = self.resolve_table_ref(&join.table, None).await?;
            let left_desc = self.tuple_desc.clone();
            result = self.execute_join(result, right_rows, join, &left_desc)?;
        }

        Ok(result)
    }

    pub async fn resolve_table_ref(
        &mut self,
        table_ref: &TableRef,
        where_clause: Option<&Expr>,
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

                // Try index scan for simple equality: column = value
                if let Some(where_expr) = where_clause {
                    if let Some((col_name, value_str)) = Self::extract_equality(where_expr) {
                        let col_upper = col_uppercase(&col_name);
                        if let Some(idx) = self.catalog.find_index(
                            crate::types::Oid(rel_oid),
                            &col_upper,
                        ) {
                            if idx.root_page != PageId::default() {
                                return self.index_scan_lookup(
                                    rel_oid,
                                    &idx,
                                    &col_name,
                                    &value_str,
                                ).await;
                            }
                        }
                    }
                }

                // Fallback to full heap scan
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

    /// Extract a simple `column = literal_value` from a WHERE expression.
    fn extract_equality(expr: &Expr) -> Option<(String, String)> {
        match expr {
            Expr::BinaryOp { left, op, right } if *op == BinaryOperator::Equals => {
                match (left.as_ref(), right.as_ref()) {
                    (Expr::Identifier(col), Expr::Literal(lit)) => {
                        let val = literal_to_string(lit);
                        Some((col.clone(), val))
                    }
                    (Expr::Literal(lit), Expr::Identifier(col)) => {
                        let val = literal_to_string(lit);
                        Some((col.clone(), val))
                    }
                    _ => None,
                }
            }
            Expr::BinaryOp { left, op, right } if *op == BinaryOperator::And => {
                Self::extract_equality(left).or_else(|| Self::extract_equality(right))
            }
            _ => None,
        }
    }

    /// Fetch a single tuple by (page_id, offset) from the heap.
    async fn fetch_tuple_by_pointer(
        &self,
        rel_oid: u32,
        page_id: PageId,
        offset: u16,
    ) -> anyhow::Result<Option<(ItemPointerData, Row)>> {
        let state = self.cache
            .get_relation_state(Oid(rel_oid))
            .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
        let rel_state = state.lock();
        let rel = &rel_state.relation;

        let page_data = self.cache.fetch_page(page_id)?;
        let page = page_data.lock();
        let heap_page = crate::storage::heap_page::HeapPage::deserialize(&page.data);

        let snapshot = self.snapshot.clone().unwrap_or(crate::transaction::Snapshot {
            xid: crate::transaction::TransactionId(u32::MAX),
            active_xids: vec![],
        });

        if (offset as usize) < heap_page.tuples.len() {
            let tuple_data = &heap_page.tuples[offset as usize];
            if let Ok(tup) = bincode::deserialize::<Tuple>(tuple_data) {
                if super::super::heap::is_visible(&tup, &snapshot) {
                    let values = super::super::heap::decode_tuple_values(&tup, &rel.tuple_desc);
                    let tid = ItemPointerData { page_id, offset };
                    return Ok(Some((tid, values)));
                }
            }
        }
        Ok(None)
    }

    /// Use a B-tree index to look up a tuple, then fetch it from the heap.
    async fn index_scan_lookup(
        &self,
        rel_oid: u32,
        idx: &crate::catalog::IndexInfo,
        col_name: &str,
        value_str: &str,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        // Encode the search key the same way the index was built
        let scan_key = crate::executor::btree::encode_index_key(
            &[value_str.to_string()],
            &[crate::btree::IndexColumn {
                name: col_name.to_string(),
                direction: crate::btree::SortDirection::Asc,
                nulls_first: false,
            }],
        );

        let op = BTreeScan {
            index_oid: idx.index_oid,
            scan_from: scan_key,
        };

        let storage = &*self.cache.storage;
        let page_size: usize = 8192;
        let hits = btree_scan(storage, &op, idx.root_page, page_size)?;

        let mut results = Vec::with_capacity(hits.len());
        for (_key, (page_id, offset)) in hits {
            if let Some(row) = self.fetch_tuple_by_pointer(rel_oid, page_id, offset).await? {
                results.push(row);
            }
        }
        Ok(results)
    }
}

fn col_uppercase(name: &str) -> String {
    name.to_uppercase()
}

fn literal_to_string(lit: &Literal) -> String {
    match lit {
        Literal::Number(n) => n.clone(),
        Literal::String(s) => s.clone(),
        Literal::Bool(b) => b.to_string(),
        Literal::Null => "NULL".to_string(),
        _ => format!("{:?}", lit),
    }
}
