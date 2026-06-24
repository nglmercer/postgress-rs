use crate::types::Oid;

#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    Select { 
        table: Oid, 
        where_clause: Option<String>, 
        columns: Vec<String> 
    },
    Insert { 
        table: Oid, 
        values: Vec<Vec<u8>> 
    },
    CreateTable {
        name: String,
        columns: Vec<(String, Oid)>,
    },
    DropTable {
        name: String,
    },
    Update {
        table: Oid,
        column: String,
        value: Vec<u8>,
        where_clause: Option<String>,
    },
    Delete {
        table: Oid,
        where_clause: Option<String>,
    },
    Begin {
        mode: Option<String>,
    },
    Commit,
    Rollback,
}

#[derive(Debug)]
pub enum Response {
    Row { data: Vec<String> },
    Rows { rows: Vec<Vec<String>> },
    Complete { rows: usize },
    Error(String),
}
