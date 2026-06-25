use crate::protocol::codes::Query;
use crate::types::Oid;

#[derive(Debug, Clone)]
pub enum RowData {
    Text(Vec<u8>),
}

pub struct Parser {
    buffer: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn feed(&mut self, data: &[u8]) -> Option<Query> {
        self.buffer.extend_from_slice(data);
        if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n' || b == b';') {
            let line = self.buffer.drain(..=pos).collect::<Vec<u8>>();
            let s = String::from_utf8_lossy(&line).trim().to_string();
            if s.is_empty() {
                return None;
            }
            self.buffer.clear();
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

    // Try to parse with the new SQL parser first
    match crate::sql::parser::Parser::parse(s) {
        Ok(stmt) => Some(Query::from_statement(stmt)),
        Err(_) => {
            // Fall back to the old simple parser for backward compatibility
            parse_query_legacy(s)
        }
    }
}

fn parse_query_legacy(s: &str) -> Option<Query> {
    let s_upper = s.trim().trim_end_matches(';').trim().to_uppercase();
    if s_upper.starts_with("BEGIN") || s_upper.starts_with("START TRANSACTION") {
        Some(Query::Begin { mode: None })
    } else if s_upper.starts_with("COMMIT") {
        Some(Query::Commit)
    } else if s_upper.starts_with("ROLLBACK") || s_upper.starts_with("ABORT") {
        Some(Query::Rollback)
    } else if s_upper.starts_with("SELECT") {
        parse_select(s)
    } else if s_upper.starts_with("INSERT") {
        parse_insert(s)
    } else if s_upper.starts_with("UPDATE") {
        parse_update(s)
    } else if s_upper.starts_with("DELETE") {
        parse_delete(s)
    } else if s_upper.starts_with("CREATE TABLE") {
        parse_create_table(s)
    } else if s_upper.starts_with("DROP TABLE") {
        parse_drop_table(s)
    } else if s_upper.starts_with("CREATE INDEX") {
        parse_create_index(s)
    } else {
        None
    }
}

fn parse_select(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let select_pos = s_upper.find("SELECT")?;
    let after_select = s[select_pos + 6..].trim();

    let (columns, table_and_where) =
        if after_select.starts_with('*') || after_select.to_uppercase().starts_with("*") {
            let after_star = after_select[1..].trim();
            let after_star_upper = after_star.to_uppercase();
            let from_pos = after_star_upper.find("FROM")?;
            let after_from = after_star[from_pos + 4..].trim();
            (vec![], after_from)
        } else {
            let from_pos = s_upper.find("FROM")?;
            let cols_str = s[select_pos + 6..from_pos].trim();
            let cols = cols_str.split(',').map(|s| s.trim().to_string()).collect();
            (cols, s[from_pos + 4..].trim())
        };

    let table_and_where_upper = table_and_where.to_uppercase();
    let (table_name, where_clause) = if let Some(idx) = table_and_where_upper.find("WHERE") {
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
    let s_upper = s.to_uppercase();
    let insert_pos = s_upper.find("INSERT")?;
    let after_insert = s[insert_pos + 6..].trim();
    let after_insert_upper = after_insert.to_uppercase();
    let into_pos = after_insert_upper.find("INTO")?;
    let after_into = after_insert[into_pos + 4..].trim();
    let parts: Vec<&str> = after_into.splitn(2, "VALUES").collect();
    if parts.len() < 2 {
        return None;
    }
    let table_name = parts[0].trim();
    let values_part = parts[1]
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')');
    let values: Vec<Vec<u8>> = values_part
        .split(',')
        .map(|v| v.trim().as_bytes().to_vec())
        .collect();

    let table_oid = Oid(fnv_hash(table_name));
    Some(Query::Insert {
        table: table_oid,
        values,
    })
}

fn parse_create_table(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let after_create = s[s_upper.find("CREATE TABLE")? + 12..].trim();
    let name_end = after_create.find('(')?;
    let table_name = after_create[..name_end].trim().to_string();
    let cols_str = &after_create[name_end..];
    let cols_str = cols_str
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    let mut columns = Vec::new();
    for col_def in cols_str.split(',') {
        let parts: Vec<&str> = col_def.split_whitespace().collect();
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

    Some(Query::CreateTable {
        name: table_name,
        columns,
    })
}

fn parse_drop_table(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let after_drop = s[s_upper.find("DROP TABLE")? + 10..].trim();
    let table_name = after_drop.trim().trim_end_matches(';').trim().to_string();
    Some(Query::DropTable { name: table_name })
}

fn parse_create_index(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let after_index = s[s_upper.find("CREATE INDEX")? + 12..].trim();
    let parts: Vec<&str> = after_index.splitn(2, "ON").collect();
    if parts.len() < 2 {
        return None;
    }
    let index_name = parts[0].trim().to_string();
    let after_on = parts[1].trim();
    let paren_pos = after_on.find('(')?;
    let table_name = after_on[..paren_pos]
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();
    let col_str = after_on[paren_pos..]
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim_end_matches(';')
        .trim();
    let column_name = col_str.to_string();

    Some(Query::CreateIndex {
        name: index_name,
        table: table_name,
        column: column_name,
    })
}

fn parse_update(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let after_update = s[s_upper.find("UPDATE")? + 6..].trim();
    let parts: Vec<&str> = after_update.splitn(2, "SET").collect();
    if parts.len() < 2 {
        return None;
    }
    let table_name = parts[0].trim();
    let after_set = parts[1].trim();

    let (set_part, where_part) = if let Some(idx) = after_set.to_uppercase().find("WHERE") {
        (&after_set[..idx], Some(after_set[idx..].to_string()))
    } else {
        (after_set, None)
    };

    let set_parts: Vec<&str> = set_part.splitn(2, '=').collect();
    if set_parts.len() != 2 {
        return None;
    }
    let column = set_parts[0].trim();
    let value = set_parts[1]
        .trim()
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .to_string();

    Some(Query::Update {
        table: Oid(fnv_hash(table_name)),
        column: column.to_string(),
        value: value.as_bytes().to_vec(),
        where_clause: where_part,
    })
}

fn parse_delete(s: &str) -> Option<Query> {
    let s_upper = s.to_uppercase();
    let delete_pos = s_upper.find("DELETE")?;
    let after_delete = s[delete_pos + 6..].trim();
    let after_delete_upper = after_delete.to_uppercase();
    let from_pos = after_delete_upper.find("FROM")?;
    let after_from = after_delete[from_pos + 4..].trim();
    let after_from_upper = after_from.to_uppercase();
    let (table_name, where_part) = if let Some(idx) = after_from_upper.find("WHERE") {
        (
            after_from[..idx].trim(),
            Some(after_from[idx..].trim().to_string()),
        )
    } else {
        (after_from.trim(), None)
    };

    Some(Query::Delete {
        table: Oid(fnv_hash(table_name)),
        where_clause: where_part,
    })
}

pub fn fnv_hash(data: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for b in data.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}
