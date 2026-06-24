#[cfg(test)]
mod tests {
    use crate::sql::ast::{Expr, Literal, BinaryOperator, UnaryOperator};
    use crate::server::evaluate_expr;

    #[test]
    fn test_evaluate_literal_number() {
        let result = evaluate_expr(&Expr::Literal(Literal::Number("42".to_string())), &[], None);
        assert_eq!(result, Some("42".to_string()));
    }

    #[test]
    fn test_evaluate_literal_string() {
        let result = evaluate_expr(&Expr::Literal(Literal::String("hello".to_string())), &[], None);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_evaluate_literal_null() {
        let result = evaluate_expr(&Expr::Literal(Literal::Null), &[], None);
        assert_eq!(result, Some("NULL".to_string()));
    }

    #[test]
    fn test_evaluate_binary_equals() {
        let left = Expr::Literal(Literal::Number("5".to_string()));
        let right = Expr::Literal(Literal::Number("5".to_string()));
        let result = evaluate_expr(&Expr::BinaryOp { left: Box::new(left), op: BinaryOperator::Equals, right: Box::new(right) }, &[], None);
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_evaluate_binary_plus() {
        let left = Expr::Literal(Literal::Number("2".to_string()));
        let right = Expr::Literal(Literal::Number("3".to_string()));
        let result = evaluate_expr(&Expr::BinaryOp { left: Box::new(left), op: BinaryOperator::Plus, right: Box::new(right) }, &[], None);
        assert_eq!(result, Some("5".to_string()));
    }

    #[test]
    fn test_evaluate_unary_not() {
        let inner = Expr::Literal(Literal::Bool(true));
        let result = evaluate_expr(&Expr::UnaryOp { op: UnaryOperator::Not, expr: Box::new(inner) }, &[], None);
        assert_eq!(result, Some("false".to_string()));
    }

    #[test]
    fn test_evaluate_unary_minus() {
        let inner = Expr::Literal(Literal::Number("5".to_string()));
        let result = evaluate_expr(&Expr::UnaryOp { op: UnaryOperator::Minus, expr: Box::new(inner) }, &[], None);
        assert_eq!(result, Some("-5".to_string()));
    }

    #[test]
    fn test_evaluate_is_null() {
        let inner = Expr::Literal(Literal::Null);
        let result = evaluate_expr(&Expr::IsNull(Box::new(inner)), &[], None);
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_evaluate_is_not_null() {
        let inner = Expr::Literal(Literal::String("hello".to_string()));
        let result = evaluate_expr(&Expr::IsNotNull(Box::new(inner)), &[], None);
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_evaluate_identifier_with_tuple_desc() {
        use crate::types::{TupleDesc, Attribute, Oid};
        let desc = TupleDesc {
            fields: vec![
                Attribute { name: "id".to_string(), type_oid: Oid(23), attnum: 0, typmod: -1 },
                Attribute { name: "name".to_string(), type_oid: Oid(25), attnum: 1, typmod: -1 },
            ],
        };
        let row = vec!["1".to_string(), "alice".to_string()];
        let result = evaluate_expr(&Expr::Identifier("name".to_string()), &row, Some(&desc));
        assert_eq!(result, Some("alice".to_string()));
    }

    #[test]
    fn test_evaluate_identifier_case_insensitive() {
        use crate::types::{TupleDesc, Attribute, Oid};
        let desc = TupleDesc {
            fields: vec![
                Attribute { name: "NAME".to_string(), type_oid: Oid(25), attnum: 0, typmod: -1 },
            ],
        };
        let row = vec!["alice".to_string()];
        let result = evaluate_expr(&Expr::Identifier("name".to_string()), &row, Some(&desc));
        assert_eq!(result, Some("alice".to_string()));
    }

    #[test]
    fn test_evaluate_function_upper() {
        let arg = Expr::Literal(Literal::String("hello".to_string()));
        let func = Expr::Function(Box::new(crate::sql::ast::FunctionCall {
            name: crate::sql::ast::ObjectName::new(vec!["UPPER".to_string()]),
            args: vec![crate::sql::ast::FunctionArg::Expr(arg)],
            filter: None,
            over: None,
        }));
        let result = evaluate_expr(&func, &[], None);
        assert_eq!(result, Some("HELLO".to_string()));
    }

    #[test]
    fn test_evaluate_function_length() {
        let arg = Expr::Literal(Literal::String("hello".to_string()));
        let func = Expr::Function(Box::new(crate::sql::ast::FunctionCall {
            name: crate::sql::ast::ObjectName::new(vec!["LENGTH".to_string()]),
            args: vec![crate::sql::ast::FunctionArg::Expr(arg)],
            filter: None,
            over: None,
        }));
        let result = evaluate_expr(&func, &[], None);
        assert_eq!(result, Some("5".to_string()));
    }

    #[test]
    fn test_evaluate_function_concat() {
        let arg1 = Expr::Literal(Literal::String("hello".to_string()));
        let arg2 = Expr::Literal(Literal::String("world".to_string()));
        let func = Expr::Function(Box::new(crate::sql::ast::FunctionCall {
            name: crate::sql::ast::ObjectName::new(vec!["CONCAT".to_string()]),
            args: vec![
                crate::sql::ast::FunctionArg::Expr(arg1),
                crate::sql::ast::FunctionArg::Expr(arg2),
            ],
            filter: None,
            over: None,
        }));
        let result = evaluate_expr(&func, &[], None);
        assert_eq!(result, Some("helloworld".to_string()));
    }

    #[test]
    fn test_evaluate_between() {
        let expr = Expr::Literal(Literal::Number("5".to_string()));
        let low = Expr::Literal(Literal::Number("1".to_string()));
        let high = Expr::Literal(Literal::Number("10".to_string()));
        let result = evaluate_expr(&Expr::Between {
            expr: Box::new(expr),
            negated: false,
            low: Box::new(low),
            high: Box::new(high),
        }, &[], None);
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_evaluate_in_list() {
        let expr = Expr::Literal(Literal::Number("2".to_string()));
        let list = vec![
            Expr::Literal(Literal::Number("1".to_string())),
            Expr::Literal(Literal::Number("2".to_string())),
            Expr::Literal(Literal::Number("3".to_string())),
        ];
        let result = evaluate_expr(&Expr::InList {
            expr: Box::new(expr),
            negated: false,
            list,
        }, &[], None);
        assert_eq!(result, Some("true".to_string()));
    }

    #[test]
    fn test_evaluate_type_cast() {
        let inner = Expr::Literal(Literal::String("123".to_string()));
        let result = evaluate_expr(&Expr::TypeCast {
            expr: Box::new(inner),
            data_type: crate::sql::ast::DataType::Int,
        }, &[], None);
        assert_eq!(result, Some("123".to_string()));
    }
}
