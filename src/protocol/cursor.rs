use crate::executor::select::Row;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Cursor {
    pub name: String,
    pub rows: Vec<Row>,
    pub position: usize,
    pub fetch_size: usize,
    pub columns: Vec<String>,
}

impl Cursor {
    pub fn new(name: &str, rows: Vec<Row>, columns: Vec<String>, fetch_size: usize) -> Self {
        Self {
            name: name.to_string(),
            rows,
            position: 0,
            fetch_size,
            columns,
        }
    }

    pub fn fetch(&mut self, count: i32) -> Vec<Row> {
        let count = if count < 0 {
            self.fetch_size as i32
        } else {
            count
        } as usize;

        let end = std::cmp::min(self.position + count, self.rows.len());
        let result: Vec<Row> = self.rows[self.position..end].to_vec();
        self.position = end;
        result
    }

    pub fn move_to(&mut self, position: i64) -> anyhow::Result<()> {
        if position < 0 {
            self.position = 0;
        } else if position as usize >= self.rows.len() {
            self.position = self.rows.len();
        } else {
            self.position = position as usize;
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.rows.clear();
        self.position = 0;
    }

    pub fn is_eof(&self) -> bool {
        self.position >= self.rows.len()
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }
}

pub struct CursorManager {
    cursors: HashMap<String, Cursor>,
}

impl CursorManager {
    pub fn new() -> Self {
        Self {
            cursors: HashMap::new(),
        }
    }

    pub fn declare(&mut self, name: &str, cursor: Cursor) {
        self.cursors.insert(name.to_string(), cursor);
    }

    pub fn get_cursor(&mut self, name: &str) -> Option<&mut Cursor> {
        self.cursors.get_mut(name)
    }

    pub fn close(&mut self, name: &str) -> bool {
        self.cursors.remove(name).is_some()
    }

    pub fn close_all(&mut self) {
        self.cursors.clear();
    }

    pub fn list_cursors(&self) -> Vec<&str> {
        self.cursors.keys().map(|s| s.as_str()).collect()
    }

    pub fn cursor_count(&self) -> usize {
        self.cursors.len()
    }
}

impl Default for CursorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_fetch() {
        let rows: Vec<Row> = (0..10).map(|i| vec![format!("row_{}", i)]).collect();
        let mut cursor = Cursor::new("test", rows, vec!["col".to_string()], 3);

        let batch1 = cursor.fetch(3);
        assert_eq!(batch1.len(), 3);
        assert_eq!(batch1[0][0], "row_0");

        let batch2 = cursor.fetch(3);
        assert_eq!(batch2.len(), 3);
        assert_eq!(batch2[0][0], "row_3");

        let batch3 = cursor.fetch(3);
        assert_eq!(batch3.len(), 3);

        let batch4 = cursor.fetch(3);
        assert_eq!(batch4.len(), 1);
        assert!(cursor.is_eof());
    }

    #[test]
    fn test_cursor_move_to() {
        let rows: Vec<Row> = (0..10).map(|i| vec![format!("row_{}", i)]).collect();
        let mut cursor = Cursor::new("test", rows, vec!["col".to_string()], 3);

        cursor.move_to(5).unwrap();
        let batch = cursor.fetch(3);
        assert_eq!(batch[0][0], "row_5");
    }

    #[test]
    fn test_cursor_close() {
        let rows: Vec<Row> = (0..10).map(|i| vec![format!("row_{}", i)]).collect();
        let mut cursor = Cursor::new("test", rows, vec!["col".to_string()], 3);
        cursor.close();
        assert_eq!(cursor.total_rows(), 0);
    }

    #[test]
    fn test_cursor_manager() {
        let mut mgr = CursorManager::new();
        let rows: Vec<Row> = (0..5).map(|i| vec![format!("row_{}", i)]).collect();
        let cursor = Cursor::new("c1", rows, vec!["col".to_string()], 10);
        mgr.declare("c1", cursor);
        assert_eq!(mgr.cursor_count(), 1);
        assert!(mgr.get_cursor("c1").is_some());

        mgr.close("c1");
        assert_eq!(mgr.cursor_count(), 0);
    }
}
