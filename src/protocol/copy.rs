use crate::buffer_cache::SharedBufferCache;
use crate::executor::heap::{tuple_insert, TupleInsert};
use crate::transaction::TransactionManager;
use crate::types::Oid;
use crate::wal::WAL;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CopyState {
    pub rel_oid: Oid,
    pub format: CopyFormat,
    pub direction: CopyDirection,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CopyFormat {
    Text,
    Csv,
    Binary,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CopyDirection {
    In,
    Out,
}

pub async fn handle_copy_in(
    state: &CopyState,
    data_rows: &[Vec<String>],
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    _txn_mgr: &Arc<TransactionManager>,
) -> anyhow::Result<u64> {
    let mut rows_copied = 0u64;

    let rel_state = cache
        .get_relation_state(state.rel_oid)
        .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
    let _tuple_desc = {
        let rel = rel_state.lock();
        rel.relation.tuple_desc.clone()
    };

    for row in data_rows {
        let values: Vec<u8> = row.join("\t").into_bytes();
        let tuple = crate::types::Tuple {
            slots: vec![],
            data: values,
            xmin: 0,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let _encoded = bincode::serialize(&tuple)?;

        let insert_op = TupleInsert {
            rel_oid: state.rel_oid,
            values: row.iter().map(|s| s.as_bytes().to_vec()).collect(),
        };

        tuple_insert(cache, &*wal.lock().await, &insert_op).await?;
        rows_copied += 1;
    }

    Ok(rows_copied)
}

pub async fn handle_copy_out(
    state: &CopyState,
    cache: &SharedBufferCache,
) -> anyhow::Result<Vec<Vec<String>>> {
    use crate::executor::heap::heap_scan;

    let rows = heap_scan(cache, state.rel_oid.0).await?;
    let result: Vec<Vec<String>> = rows.into_iter().map(|(_, values)| values).collect();
    Ok(result)
}

pub fn parse_copy_statement(sql: &str) -> anyhow::Result<CopyState> {
    let sql_upper = sql.to_uppercase();

    let direction = if sql_upper.contains("FROM") {
        CopyDirection::In
    } else if sql_upper.contains("TO") || sql_upper.contains("STDOUT") {
        CopyDirection::Out
    } else {
        anyhow::bail!("COPY requires FROM or TO");
    };

    let format = if sql_upper.contains("FORMAT CSV") {
        CopyFormat::Csv
    } else if sql_upper.contains("FORMAT BINARY") {
        CopyFormat::Binary
    } else {
        CopyFormat::Text
    };

    Ok(CopyState {
        rel_oid: Oid(0),
        format,
        direction,
        columns: vec![],
    })
}

pub fn parse_copy_data(data: &str, format: &CopyFormat) -> Vec<String> {
    match format {
        CopyFormat::Text => data
            .lines()
            .flat_map(|line| line.split('\t').map(|s| s.to_string()))
            .collect(),
        CopyFormat::Csv => csv_split(data),
        CopyFormat::Binary => data.split('\t').map(|s| s.to_string()).collect(),
    }
}

fn csv_split(data: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in data.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                result.push(current.trim().to_string());
                current = String::new();
            }
            '\n' if !in_quotes => {
                if !current.is_empty() {
                    result.push(current.trim().to_string());
                    current = String::new();
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_copy_in() {
        let state = parse_copy_statement("COPY users FROM STDIN").unwrap();
        assert_eq!(state.direction, CopyDirection::In);
        assert_eq!(state.format, CopyFormat::Text);
    }

    #[test]
    fn test_parse_copy_out() {
        let state = parse_copy_statement("COPY users TO STDOUT").unwrap();
        assert_eq!(state.direction, CopyDirection::Out);
    }

    #[test]
    fn test_parse_copy_csv() {
        let state = parse_copy_statement("COPY users FROM STDIN FORMAT CSV").unwrap();
        assert_eq!(state.format, CopyFormat::Csv);
    }

    #[test]
    fn test_parse_copy_binary() {
        let state = parse_copy_statement("COPY users FROM STDIN FORMAT BINARY").unwrap();
        assert_eq!(state.format, CopyFormat::Binary);
    }

    #[test]
    fn test_parse_copy_data_text() {
        let data = "1\tAlice\n2\tBob";
        let rows = parse_copy_data(data, &CopyFormat::Text);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], "1");
        assert_eq!(rows[1], "Alice");
    }

    #[test]
    fn test_parse_copy_data_csv() {
        let data = "1,\"Alice\",25\n2,\"Bob\",30";
        let rows = parse_copy_data(data, &CopyFormat::Csv);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0], "1");
        assert_eq!(rows[1], "Alice");
        assert_eq!(rows[2], "25");
    }

    #[test]
    fn test_csv_split() {
        let data = "a,b,c";
        let result = csv_split(data);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_csv_split_quoted() {
        let data = "a,\"hello, world\",c";
        let result = csv_split(data);
        assert_eq!(result, vec!["a", "hello, world", "c"]);
    }
}
