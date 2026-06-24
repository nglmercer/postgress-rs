use crate::types::Oid;
use crate::protocol::codes::Query;

#[derive(Debug, Clone)]
pub enum RowData {
    Text(Vec<u8>),
}

pub struct Parser {
    buffer: Vec<u8>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    pub fn feed(&mut self, data: &[u8]) -> Option<Query> {
        self.buffer.extend_from_slice(data);
        if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n' || b == b';') {
            let line = self.buffer.drain(..=pos).collect::<Vec<u8>>();
            let s = String::from_utf8_lossy(&line).trim().to_uppercase();
            if s.is_empty() {
                return None;
            }
            self.buffer.clear();
            self.buffer.extend_from_slice(&line);
            return parse_query(&s);
        }
        None
    }
}

fn parse_query(s: &str) -> Option<Query> {
    let s = s.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("BEGIN") || s.starts_with("START TRANSACTION") {
        return Some(Query::Begin { mode: None });
    } else if s.starts_with("COMMIT") {
        return Some(Query::Commit);
    } else if s.starts_with("ROLLBACK") || s.starts_with("ABORT") {
        return Some(Query::Rollback);
    } else if s.starts_with("SELECT") {
        parse_select(s)
    } else if s.starts_with("INSERT") {
        parse_insert(s)
    } else if s.starts_with("UPDATE") {
        parse_update(s)
    } else if s.starts_with("DELETE") {
        parse_delete(s)
    } else if s.starts_with("CREATE TABLE") {
        parse_create_table(s)
    } else if s.starts_with("DROP TABLE") {
        parse_drop_table(s)
    } else if s.starts_with("CREATE INDEX") {
        parse_create_index(s)
    } else {
        None
    }
}

fn parse_select(s: &str) -> Option<Query> {
    let after_select = s.strip_prefix("SELECT")?.trim();
    
    let (columns, table_and_where) = if after_select.starts_with('*') {
        let after_star = after_select[1..].trim();
        let after_from = after_star.strip_prefix("FROM")?;
        (vec![], after_from.trim())
    } else {
        let parts: Vec<&str> = after_select.splitn(2, "FROM").collect();
        if parts.len() < 2 {
            return None;
        }
        let cols = parts[0].trim().split(',').map(|s| s.trim().to_string()).collect();
        (cols, parts[1].trim())
    };

    let (table_name, where_clause) = if let Some(idx) = table_and_where.find("WHERE") {
        let table = table_and_where[..idx].trim().to_string();
        let where_part = table_and_where[idx..].trim().to_string();
        (table, Some(where_part))
    } else {
        (table_and_where.trim().to_string(), None)
    };

    let table_oid = Oid(fnv_hash(&table_name));
    Some(Query::Select {
        table: table_oid,
        where_clause,
        columns,
    })
}

fn parse_insert(s: &str) -> Option<Query> {
    let after_insert = s.strip_prefix("INSERT")?.trim();
    let after_into = after_insert.strip_prefix("INTO")?.trim();
    let parts: Vec<&str> = after_into.splitn(2, "VALUES").collect();
    if parts.len() < 2 {
        return None;
    }
    let table_name = parts[0].trim();
    let values_part = parts[1].trim().trim_start_matches('(').trim_end_matches(')');
    let values: Vec<Vec<u8>> = values_part
        .split(',')
        .map(|v| v.trim().as_bytes().to_vec())
        .collect();
    
    let table_oid = Oid(fnv_hash(table_name));
    Some(Query::Insert { table: table_oid, values })
}

fn parse_create_table(s: &str) -> Option<Query> {
    let after_create = s.strip_prefix("CREATE TABLE")?.trim();
    let name_end = after_create.find('(')?;
    let table_name = after_create[..name_end].trim().to_string();
    let cols_str = &after_create[name_end..];
    let cols_str = cols_str.trim_start_matches('(').trim_end_matches(')').trim();
    
    let mut columns = Vec::new();
    for col_def in cols_str.split(',') {
        let parts: Vec<&str> = col_def.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            let col_name = parts[0].to_string();
            let col_type = parts[1];
            let type_oid = match col_type.to_uppercase().as_str() {
                "INT" | "INTEGER" => Oid(23),
                "TEXT" | "VARCHAR" => Oid(25),
                "BOOL" => Oid(16),
                _ => Oid(0),
            };
            columns.push((col_name, type_oid));
        }
    }
    
    Some(Query::CreateTable { name: table_name, columns })
}

fn parse_drop_table(s: &str) -> Option<Query> {
    let after_drop = s.strip_prefix("DROP TABLE")?.trim();
    let table_name = after_drop.trim().trim_end_matches(';').trim().to_string();
    Some(Query::DropTable { name: table_name })
}

fn parse_create_index(s: &str) -> Option<Query> {
    // CREATE INDEX index_name ON table_name (column_name)
    let after_index = s.strip_prefix("CREATE INDEX")?.trim();
    let parts: Vec<&str> = after_index.splitn(2, "ON").collect();
    if parts.len() < 2 {
        return None;
    }
    let index_name = parts[0].trim().to_string();
    let after_on = parts[1].trim();
    let paren_pos = after_on.find('(')?;
    let table_name = after_on[..paren_pos].trim().trim_end_matches(';').trim().to_string();
    let col_str = after_on[paren_pos..].trim_start_matches('(').trim_end_matches(')').trim_end_matches(';').trim();
    let column_name = col_str.to_string();

    Some(Query::CreateIndex {
        name: index_name,
        table: table_name,
        column: column_name,
    })
}

fn parse_update(s: &str) -> Option<Query> {
    // UPDATE table SET col = val WHERE cond
    let after_update = s.strip_prefix("UPDATE")?.trim();
    let parts: Vec<&str> = after_update.splitn(2, "SET").collect();
    if parts.len() < 2 {
        return None;
    }
    let table_name = parts[0].trim();
    let after_set = parts[1].trim();
    
    let (set_part, where_part) = if let Some(idx) = after_set.find("WHERE") {
        (&after_set[..idx], Some(after_set[idx..].to_string()))
    } else {
        (after_set, None)
    };
    
    // Parse SET column = value
    let set_parts: Vec<&str> = set_part.splitn(2, '=').collect();
    if set_parts.len() != 2 {
        return None;
    }
    let column = set_parts[0].trim();
    let value = set_parts[1].trim().trim_start_matches('\'').trim_end_matches('\'').to_string();
    
    Some(Query::Update {
        table: Oid(fnv_hash(table_name)),
        column: column.to_string(),
        value: value.as_bytes().to_vec(),
        where_clause: where_part,
    })
}

fn parse_delete(s: &str) -> Option<Query> {
    // DELETE FROM table WHERE cond
    let after_delete = s.strip_prefix("DELETE")?.trim();
    let after_from = after_delete.strip_prefix("FROM")?.trim();
    let (table_name, where_part) = if let Some(idx) = after_from.find("WHERE") {
        (after_from[..idx].trim(), Some(after_from[idx..].trim().to_string()))
    } else {
        (after_from.trim(), None)
    };
    
    Some(Query::Delete {
        table: Oid(fnv_hash(table_name)),
        where_clause: where_part,
    })
}

fn fnv_hash(data: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for b in data.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}
