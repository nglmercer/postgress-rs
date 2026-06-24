use postgress_rs::executor::heap::TupleInsert;
use postgress_rs::types::{Oid, SlotId};

#[test]
fn test_tuple_insert_struct() {
    let op = TupleInsert {
        rel_oid: Oid(1),
        values: vec![SlotId(10), SlotId(20)],
    };
    assert_eq!(op.rel_oid, Oid(1));
    assert_eq!(op.values.len(), 2);
}

#[test]
fn test_planner_seq_scan() {
    use postgress_rs::executor::planner::Planner;
    use postgress_rs::protocol::codes::Query;
    use postgress_rs::types::Oid;
    
    let query = Query::Select { table: Oid(5), r#where: Some("id = 1".to_string()), columns: vec![] };
    let plan = Planner::plan(&query);
    if let postgress_rs::executor::planner::Plan::SeqScan(scan) = plan {
        assert_eq!(scan.rel_oid, 5);
    } else {
        panic!("Expected SeqScan");
    }
}

#[test]
fn test_parser_empty_input() {
    use postgress_rs::protocol::parser::Parser;
    let mut parser = Parser::new();
    assert!(parser.feed(b"").is_none());
    assert!(parser.feed(b"   ").is_none());
}

#[test]
fn test_parser_select() {
    use postgress_rs::protocol::parser::Parser;
    use postgress_rs::types::Oid;
    let mut parser = Parser::new();
    let q = parser.feed(b"SELECT * FROM users WHERE id = 1");
    assert!(q.is_some());
    if let Some(postgress_rs::protocol::codes::Query::Select { table, r#where, .. }) = q {
        // Should parse to some Oid
        assert!(matches!(table, Oid(_)));
        assert!(r#where.is_some());
    } else {
        panic!("Expected Select query");
    }
}
