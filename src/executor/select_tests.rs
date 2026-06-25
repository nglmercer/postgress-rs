#[cfg(test)]
mod tests {
    use crate::buffer_cache::SharedBufferCache;
    use crate::catalog::Catalog;
    use crate::executor::select::execute_select;
    use crate::sql::ast::*;
    use crate::storage::ephemeral::EphemeralStorage;
    use std::sync::Arc;

    async fn setup() -> (Arc<SharedBufferCache>, Arc<Catalog>) {
        let storage: Arc<dyn crate::storage::StorageTrait> = Arc::new(EphemeralStorage::new());
        let cache = Arc::new(SharedBufferCache::new(storage.clone()));
        let catalog = Arc::new(Catalog::new(storage.clone()));
        catalog.register_cache(cache.clone());
        catalog.bootstrap().await.unwrap();
        cache.sync_from_catalog(&catalog);
        (cache, catalog)
    }

    #[tokio::test]
    async fn test_select_empty_from() {
        let (cache, catalog) = setup().await;
        let select = SelectStatement {
            with: None,
            distinct: DistinctClause::All,
            select_list: vec![SelectItem::Star],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: None,
            offset: None,
            locking: vec![],
            set_operations: vec![],
        };
        let result = execute_select(&select, &cache, &catalog).await.unwrap();
        assert!(result.rows.is_empty());
        assert!(result.columns.is_empty());
    }

    #[tokio::test]
    async fn test_select_limit_empty() {
        let (cache, catalog) = setup().await;
        let select = SelectStatement {
            with: None,
            distinct: DistinctClause::All,
            select_list: vec![SelectItem::Star],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: Some(LimitClause::Expr(Expr::Literal(Literal::Number(
                "5".to_string(),
            )))),
            offset: None,
            locking: vec![],
            set_operations: vec![],
        };
        let result = execute_select(&select, &cache, &catalog).await.unwrap();
        assert!(result.rows.is_empty());
    }

    #[tokio::test]
    async fn test_select_limit_all() {
        let (cache, catalog) = setup().await;
        let select = SelectStatement {
            with: None,
            distinct: DistinctClause::All,
            select_list: vec![SelectItem::Star],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: Some(LimitClause::All),
            offset: None,
            locking: vec![],
            set_operations: vec![],
        };
        let result = execute_select(&select, &cache, &catalog).await.unwrap();
        assert!(result.rows.is_empty());
    }

    #[test]
    fn test_build_select_messages_empty() {
        let result = crate::executor::SelectResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![],
        };
        let messages = crate::server::build_select_messages(&result);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_build_select_messages_with_rows() {
        let result = crate::executor::SelectResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec!["1".to_string(), "Alice".to_string()],
                vec!["2".to_string(), "Bob".to_string()],
            ],
        };
        let messages = crate::server::build_select_messages(&result);
        assert_eq!(messages.len(), 4);
    }

    #[test]
    fn test_case_when_no_operand() {
        let row: Vec<String> = vec![];
        let expr = Expr::Case {
            operand: None,
            when_clauses: vec![WhenClause {
                when: Box::new(Expr::Literal(Literal::Bool(true))),
                then: Box::new(Expr::Literal(Literal::String("yes".to_string()))),
            }],
            else_clause: Some(Box::new(Expr::Literal(Literal::String("no".to_string())))),
        };
        let result = crate::server::evaluate_expr(&expr, &row, None);
        assert_eq!(result, Some("yes".to_string()));
    }

    #[test]
    fn test_case_when_with_operand() {
        let row = vec!["2".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "x".to_string(),
                type_oid: crate::types::Oid(23),
                attnum: 1,
                typmod: -1,
            }],
        };
        let expr = Expr::Case {
            operand: Some(Box::new(Expr::Identifier("x".to_string()))),
            when_clauses: vec![
                WhenClause {
                    when: Box::new(Expr::Literal(Literal::Number("1".to_string()))),
                    then: Box::new(Expr::Literal(Literal::String("one".to_string()))),
                },
                WhenClause {
                    when: Box::new(Expr::Literal(Literal::Number("2".to_string()))),
                    then: Box::new(Expr::Literal(Literal::String("two".to_string()))),
                },
            ],
            else_clause: Some(Box::new(Expr::Literal(Literal::String(
                "other".to_string(),
            )))),
        };
        let result = crate::server::evaluate_expr(&expr, &row, Some(&desc));
        assert_eq!(result, Some("two".to_string()));
    }

    #[test]
    fn test_qualified_identifier() {
        let row = vec!["Alice".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "name".to_string(),
                type_oid: crate::types::Oid(25),
                attnum: 1,
                typmod: -1,
            }],
        };
        let expr = Expr::QualifiedIdentifier {
            table: "users".to_string(),
            column: "name".to_string(),
        };
        let result = crate::server::evaluate_expr(&expr, &row, Some(&desc));
        assert_eq!(result, Some("Alice".to_string()));
    }

    #[test]
    fn test_string_functions() {
        let row = vec!["hello".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "x".to_string(),
                type_oid: crate::types::Oid(25),
                attnum: 1,
                typmod: -1,
            }],
        };

        // LENGTH
        let expr = Expr::Function(Box::new(FunctionCall {
            name: ObjectName::new(vec!["LENGTH".to_string()]),
            args: vec![FunctionArg::Expr(Expr::Identifier("x".to_string()))],
            filter: None,
            over: None,
        }));
        assert_eq!(
            crate::server::evaluate_expr(&expr, &row, Some(&desc)),
            Some("5".to_string())
        );

        // UPPER
        let expr = Expr::Function(Box::new(FunctionCall {
            name: ObjectName::new(vec!["UPPER".to_string()]),
            args: vec![FunctionArg::Expr(Expr::Identifier("x".to_string()))],
            filter: None,
            over: None,
        }));
        assert_eq!(
            crate::server::evaluate_expr(&expr, &row, Some(&desc)),
            Some("HELLO".to_string())
        );

        // LOWER
        let expr = Expr::Function(Box::new(FunctionCall {
            name: ObjectName::new(vec!["LOWER".to_string()]),
            args: vec![FunctionArg::Expr(Expr::Identifier("x".to_string()))],
            filter: None,
            over: None,
        }));
        assert_eq!(
            crate::server::evaluate_expr(&expr, &row, Some(&desc)),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_in_list() {
        let row = vec!["2".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "x".to_string(),
                type_oid: crate::types::Oid(23),
                attnum: 1,
                typmod: -1,
            }],
        };
        let expr = Expr::InList {
            expr: Box::new(Expr::Identifier("x".to_string())),
            negated: false,
            list: vec![
                Expr::Literal(Literal::Number("1".to_string())),
                Expr::Literal(Literal::Number("2".to_string())),
                Expr::Literal(Literal::Number("3".to_string())),
            ],
        };
        let result = crate::server::evaluate_expr(&expr, &row, Some(&desc));
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_between() {
        let row = vec!["5".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "x".to_string(),
                type_oid: crate::types::Oid(23),
                attnum: 1,
                typmod: -1,
            }],
        };
        let expr = Expr::Between {
            expr: Box::new(Expr::Identifier("x".to_string())),
            negated: false,
            low: Box::new(Expr::Literal(Literal::Number("1".to_string()))),
            high: Box::new(Expr::Literal(Literal::Number("10".to_string()))),
        };
        let result = crate::server::evaluate_expr(&expr, &row, Some(&desc));
        assert_eq!(result, Some("true".to_string()));
    }

    #[tokio::test]
    async fn test_select_distinct_on() {
        let (cache, catalog) = setup().await;
        let select = SelectStatement {
            with: None,
            distinct: DistinctClause::DistinctOn(vec![Expr::Identifier("x".to_string())]),
            select_list: vec![SelectItem::Star],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: None,
            offset: None,
            locking: vec![],
            set_operations: vec![],
        };
        let result = execute_select(&select, &cache, &catalog).await.unwrap();
        assert!(result.rows.is_empty());
    }

    #[tokio::test]
    async fn test_select_with_offset() {
        let (cache, catalog) = setup().await;
        let select = SelectStatement {
            with: None,
            distinct: DistinctClause::All,
            select_list: vec![SelectItem::Star],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: None,
            offset: Some(Expr::Literal(Literal::Number("5".to_string()))),
            locking: vec![],
            set_operations: vec![],
        };
        let result = execute_select(&select, &cache, &catalog).await.unwrap();
        assert!(result.rows.is_empty());
    }

    #[test]
    fn test_qualified_identifier_with_table() {
        let row = vec!["100".to_string()];
        let desc = crate::types::TupleDesc {
            fields: vec![crate::types::Attribute {
                name: "id".to_string(),
                type_oid: crate::types::Oid(23),
                attnum: 1,
                typmod: -1,
            }],
        };
        let expr = Expr::QualifiedIdentifier {
            table: "users".to_string(),
            column: "id".to_string(),
        };
        let result = crate::server::evaluate_expr(&expr, &row, Some(&desc));
        assert_eq!(result, Some("100".to_string()));
    }

    #[test]
    fn test_case_when_else() {
        let row: Vec<String> = vec![];
        let expr = Expr::Case {
            operand: None,
            when_clauses: vec![WhenClause {
                when: Box::new(Expr::Literal(Literal::Bool(false))),
                then: Box::new(Expr::Literal(Literal::String("no".to_string()))),
            }],
            else_clause: Some(Box::new(Expr::Literal(Literal::String(
                "other".to_string(),
            )))),
        };
        let result = crate::server::evaluate_expr(&expr, &row, None);
        assert_eq!(result, Some("other".to_string()));
    }
}
