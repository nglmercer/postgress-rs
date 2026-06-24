use postgress_rs::executor::heap::TupleInsert;
use postgress_rs::types::Oid;

#[test]
fn test_tuple_insert_struct() {
    let op = TupleInsert {
        rel_oid: Oid(1),
        values: vec![b"10".to_vec(), b"20".to_vec()],
    };
    assert_eq!(op.rel_oid, Oid(1));
    assert_eq!(op.values.len(), 2);
}

#[test]
fn test_planner_seq_scan() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    use postgress_rs::types::Oid;
    
    let query = Query::Select { table: Oid(5), where_clause: Some("id = 1".to_string()), columns: vec![] };
    let plan = Planner::plan(&query, &[]);
    if let postgress_rs::executor::planner::Plan::SeqScan(scan) = plan {
        assert_eq!(scan.rel_oid, 5);
    } else {
        panic!("Expected SeqScan");
    }
}

#[test]
fn test_planner_insert_plan() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let query = Query::Insert { table: Oid(10), values: vec![b"1".to_vec()] };
    let plan = Planner::plan(&query, &[]);
    if let postgress_rs::executor::planner::Plan::SeqScan(scan) = plan {
        assert_eq!(scan.rel_oid, 10);
    } else {
        panic!("Expected SeqScan");
    }
}

#[test]
fn test_planner_create_table() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let query = Query::CreateTable { name: "test".to_string(), columns: vec![] };
    let plan = Planner::plan(&query, &[]);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_drop_table() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let query = Query::DropTable { name: "test".to_string() };
    let plan = Planner::plan(&query, &[]);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_begin() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let query = Query::Begin { mode: None };
    let plan = Planner::plan(&query, &[]);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_commit() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let plan = Planner::plan(&Query::Commit, &[]);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_rollback() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    
    let plan = Planner::plan(&Query::Rollback, &[]);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_index_scan_selection() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    use postgress_rs::types::{Oid, PageId};
    use postgress_rs::catalog::IndexInfo;

    let indexes = vec![IndexInfo {
        index_oid: Oid(100),
        rel_oid: Oid(5),
        column_name: "ID".to_string(),
        root_page: PageId(1),
    }];

    let query = Query::Select { table: Oid(5), where_clause: Some("id = 42".to_string()), columns: vec![] };
    let plan = Planner::plan(&query, &indexes);
    match plan {
        postgress_rs::executor::planner::Plan::IndexScan(scan) => {
            assert_eq!(scan.index_oid, 100);
            assert_eq!(scan.rel_oid, 5);
        }
        _ => panic!("Expected IndexScan"),
    }
}

#[test]
fn test_planner_index_scan_no_matching_index() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    use postgress_rs::types::{Oid, PageId};
    use postgress_rs::catalog::IndexInfo;

    let indexes = vec![IndexInfo {
        index_oid: Oid(100),
        rel_oid: Oid(5),
        column_name: "NAME".to_string(),
        root_page: PageId(1),
    }];

    let query = Query::Select { table: Oid(5), where_clause: Some("id = 42".to_string()), columns: vec![] };
    let plan = Planner::plan(&query, &indexes);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_index_scan_range_filter_falls_back() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    use postgress_rs::types::{Oid, PageId};
    use postgress_rs::catalog::IndexInfo;

    let indexes = vec![IndexInfo {
        index_oid: Oid(100),
        rel_oid: Oid(5),
        column_name: "ID".to_string(),
        root_page: PageId(1),
    }];

    let query = Query::Select { table: Oid(5), where_clause: Some("id > 42".to_string()), columns: vec![] };
    let plan = Planner::plan(&query, &indexes);
    assert!(matches!(plan, postgress_rs::executor::planner::Plan::SeqScan(_)));
}

#[test]
fn test_planner_seq_scan_cost() {
    use postgress_rs::executor::planner::Planner;
    let cost = Planner::seq_scan_cost(10, 1000);
    assert!((cost - 20.0).abs() < 0.01);
}

#[test]
fn test_planner_index_scan_cost() {
    use postgress_rs::executor::planner::Planner;
    let cost = Planner::index_scan_cost(5, 0.1, 1000);
    assert!((cost - 115.0).abs() < 0.01);
}

#[test]
fn test_planner_estimate_selectivity_equals() {
    use postgress_rs::executor::planner::Planner;
    let sel = Planner::estimate_selectivity("id = 1");
    assert!((sel - 0.1).abs() < 0.01);
}

#[test]
fn test_planner_estimate_selectivity_range() {
    use postgress_rs::executor::planner::Planner;
    let sel = Planner::estimate_selectivity("id > 100");
    assert!((sel - 0.3).abs() < 0.01);
}

#[test]
fn test_planner_estimate_selectivity_like() {
    use postgress_rs::executor::planner::Planner;
    let sel = Planner::estimate_selectivity("name LIKE '%test%'");
    assert!((sel - 0.5).abs() < 0.01);
}

#[test]
fn test_planner_estimate_selectivity_default() {
    use postgress_rs::executor::planner::Planner;
    let sel = Planner::estimate_selectivity("name");
    assert!((sel - 1.0).abs() < 0.01);
}

#[test]
fn test_parser_empty_input() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    assert!(parser.feed(b"").is_none());
    assert!(parser.feed(b"   ").is_none());
}

#[test]
fn test_parser_select_star() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"SELECT * FROM users;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Select { table, where_clause, columns }) = q {
        assert!(matches!(table, Oid(_)));
        assert!(where_clause.is_none());
        assert!(columns.is_empty());
    } else {
        panic!("Expected Select query");
    }
}

#[test]
fn test_parser_select_with_where() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"SELECT * FROM users WHERE id = 1;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Select { table, where_clause, .. }) = q {
        assert!(matches!(table, Oid(_)));
        assert!(where_clause.is_some());
    } else {
        panic!("Expected Select query");
    }
}

#[test]
fn test_parser_select_columns() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"SELECT name, age FROM users;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Select { columns, .. }) = q {
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0], "NAME");
        assert_eq!(columns[1], "AGE");
    } else {
        panic!("Expected Select query");
    }
}

#[test]
fn test_parser_insert() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"INSERT INTO users VALUES (1, 'alice');\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Insert { table, values }) = q {
        assert!(matches!(table, Oid(_)));
        assert_eq!(values.len(), 2);
    } else {
        panic!("Expected Insert query");
    }
}

#[test]
fn test_parser_create_table() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"CREATE TABLE users (id INT, name TEXT);\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::CreateTable { name, columns }) = q {
        assert_eq!(name, "USERS");
        assert_eq!(columns.len(), 2);
    } else {
        panic!("Expected CreateTable query");
    }
}

#[test]
fn test_parser_create_table_with_types() {
    use postgress_rs::protocol::parser::Parser;
    use postgress_rs::types::Oid;
    let mut parser = Parser::new();
    let q = parser.feed(b"CREATE TABLE t (a INT, b TEXT, c BOOL);\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::CreateTable { columns, .. }) = q {
        assert_eq!(columns[0].1, Oid(23)); // INT
        assert_eq!(columns[1].1, Oid(25)); // TEXT
        assert_eq!(columns[2].1, Oid(16)); // BOOL
    } else {
        panic!("Expected CreateTable query");
    }
}

#[test]
fn test_parser_drop_table() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"DROP TABLE users;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::DropTable { name }) = q {
        assert_eq!(name, "USERS");
    } else {
        panic!("Expected DropTable query");
    }
}

#[test]
fn test_parser_update() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"UPDATE users SET name = 'bob' WHERE id = 1;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Update { table, column, value, where_clause }) = q {
        assert!(matches!(table, Oid(_)));
        assert_eq!(column, "NAME");
        assert_eq!(value, b"BOB");
        assert!(where_clause.is_some());
    } else {
        panic!("Expected Update query");
    }
}

#[test]
fn test_parser_update_without_where() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"UPDATE users SET name = 'bob';\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Update { where_clause, .. }) = q {
        assert!(where_clause.is_none());
    } else {
        panic!("Expected Update query");
    }
}

#[test]
fn test_parser_delete() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"DELETE FROM users WHERE id = 1;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Delete { table, where_clause }) = q {
        assert!(matches!(table, Oid(_)));
        assert!(where_clause.is_some());
    } else {
        panic!("Expected Delete query");
    }
}

#[test]
fn test_parser_delete_without_where() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"DELETE FROM users;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Delete { where_clause, .. }) = q {
        assert!(where_clause.is_none());
    } else {
        panic!("Expected Delete query");
    }
}

#[test]
fn test_parser_begin() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"BEGIN;\n");
    assert!(q.is_some());
    assert!(matches!(q.unwrap(), postgress_rs::protocol::codes::Query::Begin { .. }));
}

#[test]
fn test_parser_commit() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"COMMIT;\n");
    assert!(q.is_some());
    assert!(matches!(q.unwrap(), postgress_rs::protocol::codes::Query::Commit));
}

#[test]
fn test_parser_rollback() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"ROLLBACK;\n");
    assert!(q.is_some());
    assert!(matches!(q.unwrap(), postgress_rs::protocol::codes::Query::Rollback));
}

#[test]
fn test_parser_multiple_statements() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q1 = parser.feed(b"SELECT * FROM a;\n");
    assert!(q1.is_some());
    let q2 = parser.feed(b"INSERT INTO b VALUES (1);\n");
    assert!(q2.is_some());
    let q3 = parser.feed(b"COMMIT;\n");
    assert!(q3.is_some());
}

#[test]
fn test_parser_unknown_statement() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"UNKNOWN COMMAND;\n");
    assert!(q.is_none());
}

#[test]
fn test_parser_select_with_integer_where() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"SELECT * FROM users WHERE id = 42;\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Select { where_clause, .. }) = q {
        assert!(where_clause.unwrap().contains("42"));
    } else {
        panic!("Expected Select query");
    }
}

#[test]
fn test_parser_insert_multiple_values() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"INSERT INTO users VALUES (1, 'alice', 'admin');\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Insert { values, .. }) = q {
        assert_eq!(values.len(), 3);
    } else {
        panic!("Expected Insert query");
    }
}

#[test]
fn test_parser_insert_numeric_values() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"INSERT INTO users VALUES (1, 2, 3);\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Insert { values, .. }) = q {
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], b"1");
        assert_eq!(values[1], b"2");
        assert_eq!(values[2], b"3");
    } else {
        panic!("Expected Insert query");
    }
}

#[test]
fn test_parser_create_index() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"CREATE INDEX idx_users_id ON users (id);\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::CreateIndex { name, table, column }) = q {
        assert_eq!(name, "IDX_USERS_ID");
        assert_eq!(table, "USERS");
        assert_eq!(column, "ID");
    } else {
        panic!("Expected CreateIndex query");
    }
}

#[test]
fn test_parser_create_index_multiple_words() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    let q = parser.feed(b"CREATE INDEX idx_orders_customer ON orders (customer_id);\n");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::CreateIndex { name, table, column }) = q {
        assert_eq!(name, "IDX_ORDERS_CUSTOMER");
        assert_eq!(table, "ORDERS");
        assert_eq!(column, "CUSTOMER_ID");
    } else {
        panic!("Expected CreateIndex query");
    }
}
