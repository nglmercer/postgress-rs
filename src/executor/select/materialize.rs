use crate::sql::ast::*;
use crate::types::*;
use crate::executor::select::Row;
use super::context::ExecContext;

impl<'a> ExecContext<'a> {
    pub fn materialize(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        Ok(rows)
    }

    pub fn materialize_with_limit(
        &self,
        rows: Vec<(ItemPointerData, Row)>,
        limit: usize,
    ) -> anyhow::Result<Vec<(ItemPointerData, Row)>> {
        Ok(rows.into_iter().take(limit).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ephemeral::EphemeralStorage;
    use crate::buffer_cache::SharedBufferCache;
    use crate::catalog::Catalog;
    use std::sync::Arc;

    fn make_row(values: &[&str]) -> Row {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn setup() -> (Arc<SharedBufferCache>, Arc<Catalog>) {
        let storage: Arc<dyn crate::storage::StorageTrait> = Arc::new(EphemeralStorage::new());
        let cache = Arc::new(SharedBufferCache::new(storage.clone()));
        let catalog = Arc::new(Catalog::new(storage.clone()));
        (cache, catalog)
    }

    #[test]
    fn test_materialize_preserves_all_rows() {
        let (cache, catalog) = setup();
        let ctx = ExecContext::new(&cache, &catalog);

        let rows = vec![
            (ItemPointerData { page_id: PageId(1), offset: 0 }, make_row(&["1", "Alice"])),
            (ItemPointerData { page_id: PageId(1), offset: 1 }, make_row(&["2", "Bob"])),
            (ItemPointerData { page_id: PageId(1), offset: 2 }, make_row(&["3", "Charlie"])),
        ];

        let result = ctx.materialize(rows.clone()).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].1, make_row(&["1", "Alice"]));
        assert_eq!(result[1].1, make_row(&["2", "Bob"]));
        assert_eq!(result[2].1, make_row(&["3", "Charlie"]));
    }

    #[test]
    fn test_materialize_empty_input() {
        let (cache, catalog) = setup();
        let ctx = ExecContext::new(&cache, &catalog);

        let rows: Vec<(ItemPointerData, Row)> = vec![];
        let result = ctx.materialize(rows).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_materialize_with_limit() {
        let (cache, catalog) = setup();
        let ctx = ExecContext::new(&cache, &catalog);

        let rows = vec![
            (ItemPointerData { page_id: PageId(1), offset: 0 }, make_row(&["1", "Alice"])),
            (ItemPointerData { page_id: PageId(1), offset: 1 }, make_row(&["2", "Bob"])),
            (ItemPointerData { page_id: PageId(1), offset: 2 }, make_row(&["3", "Charlie"])),
        ];

        let result = ctx.materialize_with_limit(rows, 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, make_row(&["1", "Alice"]));
        assert_eq!(result[1].1, make_row(&["2", "Bob"]));
    }

    #[test]
    fn test_materialize_with_limit_larger_than_input() {
        let (cache, catalog) = setup();
        let ctx = ExecContext::new(&cache, &catalog);

        let rows = vec![
            (ItemPointerData { page_id: PageId(1), offset: 0 }, make_row(&["1", "Alice"])),
        ];

        let result = ctx.materialize_with_limit(rows, 100).unwrap();
        assert_eq!(result.len(), 1);
    }
}
