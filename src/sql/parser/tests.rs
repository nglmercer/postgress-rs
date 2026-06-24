use crate::sql::parser::Parser;
use crate::sql::ast::*;

    #[test]
    fn test_simple_select() {
        let stmt = Parser::parse("SELECT * FROM users").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                assert!(matches!(sel.select_list[0], SelectItem::Star));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_where() {
        let stmt = Parser::parse("SELECT id, name FROM users WHERE id = 1").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                assert!(sel.where_clause.is_some());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_insert() {
        let stmt = Parser::parse("INSERT INTO users (id, name) VALUES (1, 'Alice')").unwrap();
        match stmt {
            Statement::Insert(ins) => {
                assert_eq!(ins.table.parts, vec!["users"]);
                assert!(ins.columns.is_some());
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn test_create_table() {
        let stmt = Parser::parse("CREATE TABLE users (id INT PRIMARY KEY, name TEXT NOT NULL)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.table.parts, vec!["users"]);
                assert_eq!(create.columns.len(), 2);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_create_index() {
        let stmt = Parser::parse("CREATE INDEX idx_users_name ON users (name)").unwrap();
        match stmt {
            Statement::CreateIndex(create) => {
                assert_eq!(create.name.parts, vec!["idx_users_name"]);
                assert_eq!(create.table.parts, vec!["users"]);
                assert_eq!(create.columns.len(), 1);
            }
            _ => panic!("expected CreateIndex"),
        }
    }

    #[test]
    fn test_alter_table_add_column() {
        let stmt = Parser::parse("ALTER TABLE users ADD COLUMN email TEXT").unwrap();
        match stmt {
            Statement::AlterTable(alter) => {
                assert_eq!(alter.table.parts, vec!["users"]);
                match alter.action {
                    AlterTableAction::AddColumn(col) => {
                        assert_eq!(col.name, "email");
                    }
                    _ => panic!("expected AddColumn"),
                }
            }
            _ => panic!("expected AlterTable"),
        }
    }

    #[test]
    fn test_drop_table() {
        let stmt = Parser::parse("DROP TABLE IF EXISTS users CASCADE").unwrap();
        match stmt {
            Statement::DropTable(drop) => {
                assert_eq!(drop.table.parts, vec!["users"]);
                assert!(drop.if_exists);
                assert!(drop.cascade);
            }
            _ => panic!("expected DropTable"),
        }
    }

    #[test]
    fn test_select_with_join() {
        let stmt = Parser::parse("SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.from.is_some());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_group_by() {
        let stmt = Parser::parse("SELECT department, COUNT(*) FROM employees GROUP BY department HAVING COUNT(*) > 5").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.group_by.len(), 1);
                assert!(sel.having.is_some());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_order_limit() {
        let stmt = Parser::parse("SELECT * FROM users ORDER BY name ASC NULLS LAST LIMIT 10 OFFSET 20").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.order_by.len(), 1);
                assert!(sel.limit.is_some());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_subquery() {
        let stmt = Parser::parse("SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.where_clause.is_some());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_case() {
        let stmt = Parser::parse("SELECT CASE WHEN age > 18 THEN 'adult' ELSE 'minor' END FROM users").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_union() {
        let stmt = Parser::parse("SELECT id FROM users UNION SELECT id FROM admins").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.set_operations.len(), 1);
                assert!(matches!(sel.set_operations[0].operator, SetOperator::Union));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_union_all() {
        let stmt = Parser::parse("SELECT id FROM users UNION ALL SELECT id FROM admins").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.set_operations.len(), 1);
                assert!(matches!(sel.set_operations[0].operator, SetOperator::UnionAll));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_intersect() {
        let stmt = Parser::parse("SELECT id FROM users INTERSECT SELECT id FROM admins").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.set_operations.len(), 1);
                assert!(matches!(sel.set_operations[0].operator, SetOperator::Intersect));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_except() {
        let stmt = Parser::parse("SELECT id FROM users EXCEPT SELECT id FROM admins").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.set_operations.len(), 1);
                assert!(matches!(sel.set_operations[0].operator, SetOperator::Except));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_insert_on_conflict_do_nothing() {
        let stmt = Parser::parse("INSERT INTO users (id, name) VALUES (1, 'Alice') ON CONFLICT DO NOTHING").unwrap();
        match stmt {
            Statement::Insert(ins) => {
                assert!(ins.on_conflict.is_some());
                assert!(matches!(ins.on_conflict, Some(OnConflict::DoNothing)));
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn test_insert_on_conflict_do_update() {
        let stmt = Parser::parse("INSERT INTO users (id, name) VALUES (1, 'Alice') ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name").unwrap();
        match stmt {
            Statement::Insert(ins) => {
                assert!(ins.on_conflict.is_some());
                match ins.on_conflict {
                    Some(OnConflict::DoUpdate { target_columns, set_clauses, .. }) => {
                        assert!(target_columns.is_some());
                        assert_eq!(set_clauses.len(), 1);
                    }
                    _ => panic!("expected DoUpdate"),
                }
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn test_with_cte() {
        let stmt = Parser::parse("WITH active AS (SELECT id FROM users WHERE active = true) SELECT * FROM active").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.with.is_some());
                let with = sel.with.unwrap();
                assert!(!with.recursive);
                assert_eq!(with.ctes.len(), 1);
                assert_eq!(with.ctes[0].name, "active");
                assert!(with.ctes[0].columns.is_none());
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_with_recursive_cte() {
        let stmt = Parser::parse("WITH RECURSIVE tree AS (SELECT id FROM nodes UNION ALL SELECT n.id FROM nodes n JOIN tree t ON n.parent = t.id) SELECT * FROM tree").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.with.is_some());
                let with = sel.with.unwrap();
                assert!(with.recursive);
                assert_eq!(with.ctes.len(), 1);
                assert_eq!(with.ctes[0].name, "tree");
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_with_multiple_ctes() {
        let stmt = Parser::parse("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.with.is_some());
                let with = sel.with.unwrap();
                assert_eq!(with.ctes.len(), 2);
                assert_eq!(with.ctes[0].name, "a");
                assert_eq!(with.ctes[1].name, "b");
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_with_cte_with_columns() {
        let stmt = Parser::parse("WITH cte (x, y) AS (SELECT 1, 2) SELECT * FROM cte").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.with.is_some());
                let with = sel.with.unwrap();
                assert_eq!(with.ctes[0].columns, Some(vec!["x".to_string(), "y".to_string()]));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_with_cte_not_materialized() {
        let stmt = Parser::parse("WITH cte AS NOT MATERIALIZED (SELECT 1) SELECT * FROM cte").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.with.is_some());
                let with = sel.with.unwrap();
                assert_eq!(with.ctes[0].materialized, Some(false));
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_serial_types() {
        let stmt = Parser::parse("CREATE TABLE t (id SERIAL, bid BIGSERIAL, sid SMALLSERIAL)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Serial);
                assert_eq!(create.columns[1].data_type, DataType::BigSerial);
                assert_eq!(create.columns[2].data_type, DataType::SmallSerial);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_money_type() {
        let stmt = Parser::parse("CREATE TABLE t (price MONEY)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Money);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_cast_expression() {
        let stmt = Parser::parse("SELECT CAST(1 AS INTEGER)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Int);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_type_cast() {
        let stmt = Parser::parse("SELECT 1::INTEGER").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Int);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_type_cast_complex() {
        let stmt = Parser::parse("SELECT '123'::INTEGER + 1").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::BinaryOp { op, .. }) => {
                        assert!(matches!(op, BinaryOperator::Plus));
                    }
                    other => panic!("expected BinaryOp, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_text_type() {
        let stmt = Parser::parse("SELECT CAST(123 AS VARCHAR(50))").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Varchar(50));
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_any_comparison() {
        let stmt = Parser::parse("SELECT * FROM t WHERE x = ANY (SELECT y FROM t2)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.where_clause.is_some());
                match sel.where_clause.as_ref().unwrap().as_ref() {
                    Expr::AnyComparison { op, .. } => {
                        assert!(matches!(op, BinaryOperator::Equals));
                    }
                    other => panic!("expected AnyComparison, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_some_comparison() {
        let stmt = Parser::parse("SELECT * FROM t WHERE x > SOME (SELECT y FROM t2)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.where_clause.is_some());
                match sel.where_clause.as_ref().unwrap().as_ref() {
                    Expr::SomeComparison { op, .. } => {
                        assert!(matches!(op, BinaryOperator::GreaterThan));
                    }
                    other => panic!("expected SomeComparison, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // ========== Date/Time Type Tests ==========

    #[test]
    fn test_date_type() {
        let stmt = Parser::parse("CREATE TABLE t (d DATE)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Date);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_time_type() {
        let stmt = Parser::parse("CREATE TABLE t (t TIME)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Time);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_time_with_timezone_type() {
        let stmt = Parser::parse("CREATE TABLE t (t TIME WITH TIME ZONE)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TimeTz);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_timetz_type() {
        let stmt = Parser::parse("CREATE TABLE t (t TIMETZ)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TimeTz);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_timestamp_type() {
        let stmt = Parser::parse("CREATE TABLE t (ts TIMESTAMP)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Timestamp);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_timestamp_with_timezone_type() {
        let stmt = Parser::parse("CREATE TABLE t (ts TIMESTAMP WITH TIME ZONE)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TimestampTz);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_timestamptz_type() {
        let stmt = Parser::parse("CREATE TABLE t (ts TIMESTAMPTZ)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TimestampTz);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_interval_type() {
        let stmt = Parser::parse("CREATE TABLE t (i INTERVAL)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Interval);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    // ========== Date/Time Function Tests ==========

    #[test]
    fn test_now_function() {
        let stmt = Parser::parse("SELECT NOW()").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["NOW"]);
                        assert!(f.args.is_empty());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_current_date() {
        let stmt = Parser::parse("SELECT CURRENT_DATE").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["CURRENT_DATE"]);
                        assert!(f.args.is_empty());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_current_time() {
        let stmt = Parser::parse("SELECT CURRENT_TIME").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["CURRENT_TIME"]);
                        assert!(f.args.is_empty());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_current_timestamp() {
        let stmt = Parser::parse("SELECT CURRENT_TIMESTAMP").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["CURRENT_TIMESTAMP"]);
                        assert!(f.args.is_empty());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_localtime() {
        let stmt = Parser::parse("SELECT LOCALTIME").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["LOCALTIME"]);
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_localtimestamp() {
        let stmt = Parser::parse("SELECT LOCALTIMESTAMP").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Function(f)) => {
                        assert_eq!(f.name.parts, vec!["LOCALTIMESTAMP"]);
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_function() {
        let stmt = Parser::parse("SELECT EXTRACT(YEAR FROM created_at) FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, from }) => {
                        assert!(matches!(field, DatePart::Year));
                        assert!(matches!(from.as_ref(), Expr::Identifier(_)));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_month() {
        let stmt = Parser::parse("SELECT EXTRACT(MONTH FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Month));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_day() {
        let stmt = Parser::parse("SELECT EXTRACT(DAY FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Day));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_hour() {
        let stmt = Parser::parse("SELECT EXTRACT(HOUR FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Hour));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_minute() {
        let stmt = Parser::parse("SELECT EXTRACT(MINUTE FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Minute));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_second() {
        let stmt = Parser::parse("SELECT EXTRACT(SECOND FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Second));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_dow() {
        let stmt = Parser::parse("SELECT EXTRACT(DOW FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Dow));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_epoch() {
        let stmt = Parser::parse("SELECT EXTRACT(EPOCH FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Epoch));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_week() {
        let stmt = Parser::parse("SELECT EXTRACT(WEEK FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Week));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_quarter() {
        let stmt = Parser::parse("SELECT EXTRACT(QUARTER FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Quarter));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_timezone() {
        let stmt = Parser::parse("SELECT EXTRACT(TIMEZONE FROM ts)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Timezone));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_date_trunc() {
        let stmt = Parser::parse("SELECT DATE_TRUNC('hour', ts) FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::DateTrunc { field, source, zone }) => {
                        assert!(matches!(field, DatePart::Hour));
                        assert!(matches!(source.as_ref(), Expr::Identifier(_)));
                        assert!(zone.is_none());
                    }
                    other => panic!("expected DateTrunc, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_date_trunc_with_zone() {
        let stmt = Parser::parse("SELECT DATE_TRUNC('day', ts AT TIME ZONE 'UTC') FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::DateTrunc { field, source, zone }) => {
                        assert!(matches!(field, DatePart::Day));
                        assert!(matches!(source.as_ref(), Expr::AtTimeZone { .. }));
                        assert!(zone.is_none());
                    }
                    other => panic!("expected DateTrunc, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_date_part() {
        let stmt = Parser::parse("SELECT DATE_PART('year', ts) FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::Extract { field, .. }) => {
                        assert!(matches!(field, DatePart::Year));
                    }
                    other => panic!("expected Extract, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // ========== AT TIME ZONE Tests ==========

    #[test]
    fn test_at_time_zone() {
        let stmt = Parser::parse("SELECT ts AT TIME ZONE 'UTC' FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::AtTimeZone { expr, zone }) => {
                        assert!(matches!(expr.as_ref(), Expr::Identifier(_)));
                        assert!(matches!(zone.as_ref(), Expr::Literal(Literal::String(_))));
                    }
                    other => panic!("expected AtTimeZone, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_at_time_zone_with_cast() {
        let stmt = Parser::parse("SELECT ts::timestamptz AT TIME ZONE 'US/Eastern'").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::AtTimeZone { expr, zone }) => {
                        assert!(matches!(expr.as_ref(), Expr::TypeCast { .. }));
                        assert!(matches!(zone.as_ref(), Expr::Literal(Literal::String(s)) if s == "US/Eastern"));
                    }
                    other => panic!("expected AtTimeZone, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // ========== New Type Casting Tests ==========

    #[test]
    fn test_cast_to_boolean() {
        let stmt = Parser::parse("SELECT CAST(1 AS BOOLEAN)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Boolean);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_uuid() {
        let stmt = Parser::parse("SELECT '550e8400-e29b-41d4-a716-446655440000'::UUID").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Uuid);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_json() {
        let stmt = Parser::parse("SELECT '{\"a\":1}'::JSON").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Json);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_jsonb() {
        let stmt = Parser::parse("SELECT '{\"a\":1}'::JSONB").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::JsonB);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_array() {
        let stmt = Parser::parse("SELECT CAST(ARRAY[1,2,3] AS INTEGER[])").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Array(Box::new(DataType::Int)));
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_money() {
        let stmt = Parser::parse("SELECT CAST(123.45 AS MONEY)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Money);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_date() {
        let stmt = Parser::parse("SELECT CAST('2023-01-01' AS DATE)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Date);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_time() {
        let stmt = Parser::parse("SELECT CAST('12:30:00' AS TIME)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Time);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_timetz() {
        let stmt = Parser::parse("SELECT CAST('12:30:00+05:00' AS TIME WITH TIME ZONE)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::TimeTz);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_timestamp() {
        let stmt = Parser::parse("SELECT CAST('2023-01-01 12:00:00' AS TIMESTAMP)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Timestamp);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_timestamptz() {
        let stmt = Parser::parse("SELECT CAST('2023-01-01 12:00:00+05:00' AS TIMESTAMPTZ)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::TimestampTz);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_cast_to_interval() {
        let stmt = Parser::parse("SELECT CAST('1 day' AS INTERVAL)").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Interval);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_cast_to_date() {
        let stmt = Parser::parse("SELECT '2023-01-01'::DATE").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Date);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_cast_to_time() {
        let stmt = Parser::parse("SELECT '12:30:00'::TIME").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Time);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_cast_to_timetz() {
        let stmt = Parser::parse("SELECT '12:30:00+05:00'::TIMETZ").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::TimeTz);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_cast_to_timestamptz() {
        let stmt = Parser::parse("SELECT '2023-01-01'::TIMESTAMPTZ").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::TimestampTz);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_pg_cast_to_interval() {
        let stmt = Parser::parse("SELECT '1 day'::INTERVAL").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Interval);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // ========== New Data Type Tests ==========

    #[test]
    fn test_inet_type() {
        let stmt = Parser::parse("CREATE TABLE t (addr INET)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Inet);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_cidr_type() {
        let stmt = Parser::parse("CREATE TABLE t (net CIDR)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Cidr);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_macaddr_type() {
        let stmt = Parser::parse("CREATE TABLE t (mac MACADDR)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::MacAddr);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_bit_type() {
        let stmt = Parser::parse("CREATE TABLE t (b BIT)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Bit(1));
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_bit_with_length() {
        let stmt = Parser::parse("CREATE TABLE t (b BIT(8))").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::Bit(8));
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_bit_varying() {
        let stmt = Parser::parse("CREATE TABLE t (b BIT VARYING)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::BitVarying(1));
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_bit_varying_with_length() {
        let stmt = Parser::parse("CREATE TABLE t (b BIT VARYING(32))").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::BitVarying(32));
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_tsvector_type() {
        let stmt = Parser::parse("CREATE TABLE t (doc TSVECTOR)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TsVector);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_tsquery_type() {
        let stmt = Parser::parse("CREATE TABLE t (query TSQUERY)").unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns[0].data_type, DataType::TsQuery);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    // ========== Complex Date/Time Expression Tests ==========

    #[test]
    fn test_date_cast_with_pg_shorthand() {
        let stmt = Parser::parse("SELECT created_at::date FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Date);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_timestamp_cast_with_pg_shorthand() {
        let stmt = Parser::parse("SELECT created_at::timestamptz FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::TimestampTz);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_with_alias() {
        let stmt = Parser::parse("SELECT EXTRACT(YEAR FROM created_at) AS year FROM events").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::ExprAs { expr, alias } => {
                        assert!(matches!(expr, Expr::Extract { .. }));
                        assert_eq!(alias, "year");
                    }
                    other => panic!("expected ExprAs, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_now_with_alias() {
        let stmt = Parser::parse("SELECT NOW() AS current_time").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::ExprAs { expr, alias } => {
                        assert!(matches!(expr, Expr::Function(_)));
                        assert_eq!(alias, "current_time");
                    }
                    other => panic!("expected ExprAs, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_multiple_date_functions() {
        let stmt = Parser::parse("SELECT NOW(), CURRENT_DATE, CURRENT_TIMESTAMP, LOCALTIME, LOCALTIMESTAMP").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 5);
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_interval_literal_cast() {
        let stmt = Parser::parse("SELECT '1 day 2 hours'::interval").unwrap();
        match stmt {
            Statement::Select(sel) => {
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Interval);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_extract_all_date_parts() {
        let parts = vec![
            ("YEAR", DatePart::Year),
            ("MONTH", DatePart::Month),
            ("DAY", DatePart::Day),
            ("HOUR", DatePart::Hour),
            ("MINUTE", DatePart::Minute),
            ("SECOND", DatePart::Second),
            ("DOW", DatePart::Dow),
            ("DOY", DatePart::Doy),
            ("ISODOW", DatePart::IsoDow),
            ("WEEK", DatePart::Week),
            ("QUARTER", DatePart::Quarter),
            ("EPOCH", DatePart::Epoch),
            ("ISOYEAR", DatePart::IsoYear),
            ("TIMEZONE", DatePart::Timezone),
            ("TIMEZONE_HOUR", DatePart::TimezoneHour),
            ("TIMEZONE_MINUTE", DatePart::TimezoneMinute),
        ];

        for (part_str, expected) in parts {
            let sql = format!("SELECT EXTRACT({} FROM ts)", part_str);
            let stmt = Parser::parse(&sql).unwrap();
            match stmt {
                Statement::Select(sel) => {
                    match &sel.select_list[0] {
                        SelectItem::Expr(Expr::Extract { field, .. }) => {
                            assert_eq!(*field, expected, "Failed for part: {}", part_str);
                        }
                        other => panic!("expected Extract for part {}, got {:?}", part_str, other),
                    }
                }
                _ => panic!("expected Select for part {}", part_str),
            }
        }
    }

    #[test]
    fn test_date_part_all_parts() {
        let parts = vec![
            ("YEAR", DatePart::Year),
            ("MONTH", DatePart::Month),
            ("DAY", DatePart::Day),
            ("HOUR", DatePart::Hour),
            ("MINUTE", DatePart::Minute),
            ("SECOND", DatePart::Second),
        ];

        for (part_str, expected) in parts {
            let sql = format!("SELECT DATE_PART('{}', ts)", part_str);
            let stmt = Parser::parse(&sql).unwrap();
            match stmt {
                Statement::Select(sel) => {
                    match &sel.select_list[0] {
                        SelectItem::Expr(Expr::Extract { field, .. }) => {
                            assert_eq!(*field, expected, "Failed for part: {}", part_str);
                        }
                        other => panic!("expected Extract for part {}, got {:?}", part_str, other),
                    }
                }
                _ => panic!("expected Select for part {}", part_str),
            }
        }
    }

    #[test]
    fn test_date_trunc_parts() {
        let parts = vec![
            ("hour", DatePart::Hour),
            ("day", DatePart::Day),
            ("week", DatePart::Week),
            ("month", DatePart::Month),
            ("quarter", DatePart::Quarter),
            ("year", DatePart::Year),
        ];

        for (part_str, expected) in parts {
            let sql = format!("SELECT DATE_TRUNC('{}', ts)", part_str);
            let stmt = Parser::parse(&sql).unwrap();
            match stmt {
                Statement::Select(sel) => {
                    match &sel.select_list[0] {
                        SelectItem::Expr(Expr::DateTrunc { field, .. }) => {
                            assert_eq!(*field, expected, "Failed for part: {}", part_str);
                        }
                        other => panic!("expected DateTrunc for part {}, got {:?}", part_str, other),
                    }
                }
                _ => panic!("expected Select for part {}", part_str),
            }
        }
    }

    #[test]
    fn test_create_table_with_all_date_types() {
        let stmt = Parser::parse(
            "CREATE TABLE events (
                id SERIAL PRIMARY KEY,
                event_date DATE,
                event_time TIME,
                event_timetz TIMETZ,
                event_ts TIMESTAMP,
                event_tstz TIMESTAMPTZ,
                duration INTERVAL
            )"
        ).unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns.len(), 7);
                assert_eq!(create.columns[1].data_type, DataType::Date);
                assert_eq!(create.columns[2].data_type, DataType::Time);
                assert_eq!(create.columns[3].data_type, DataType::TimeTz);
                assert_eq!(create.columns[4].data_type, DataType::Timestamp);
                assert_eq!(create.columns[5].data_type, DataType::TimestampTz);
                assert_eq!(create.columns[6].data_type, DataType::Interval);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_create_table_with_network_types() {
        let stmt = Parser::parse(
            "CREATE TABLE hosts (
                ip INET,
                network CIDR,
                mac MACADDR
            )"
        ).unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns.len(), 3);
                assert_eq!(create.columns[0].data_type, DataType::Inet);
                assert_eq!(create.columns[1].data_type, DataType::Cidr);
                assert_eq!(create.columns[2].data_type, DataType::MacAddr);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_create_table_with_bit_types() {
        let stmt = Parser::parse(
            "CREATE TABLE bitfield (
                flag BIT(1),
                bits BIT(8),
                var_bits BIT VARYING(64)
            )"
        ).unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns.len(), 3);
                assert_eq!(create.columns[0].data_type, DataType::Bit(1));
                assert_eq!(create.columns[1].data_type, DataType::Bit(8));
                assert_eq!(create.columns[2].data_type, DataType::BitVarying(64));
            }
            _ => panic!("expected CreateTable"),
        }
    }

    #[test]
    fn test_create_table_with_fulltext_types() {
        let stmt = Parser::parse(
            "CREATE TABLE articles (
                content TSVECTOR,
                search_query TSQUERY
            )"
        ).unwrap();
        match stmt {
            Statement::CreateTable(create) => {
                assert_eq!(create.columns.len(), 2);
                assert_eq!(create.columns[0].data_type, DataType::TsVector);
                assert_eq!(create.columns[1].data_type, DataType::TsQuery);
            }
            _ => panic!("expected CreateTable"),
        }
    }

    // ========== Complex Combined Tests ==========

    #[test]
    fn test_complex_date_query() {
        let stmt = Parser::parse(
            "SELECT 
                id,
                created_at::date as date_only,
                EXTRACT(YEAR FROM created_at) as year,
                EXTRACT(MONTH FROM created_at) as month,
                DATE_TRUNC('day', created_at) as day_start
            FROM events 
            WHERE created_at > '2023-01-01'::date
            ORDER BY created_at"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 5);
                assert!(sel.where_clause.is_some());
                assert_eq!(sel.order_by.len(), 1);
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_at_time_zone_complex() {
        let stmt = Parser::parse(
            "SELECT 
                created_at AT TIME ZONE 'UTC' as utc_time,
                created_at AT TIME ZONE 'US/Eastern' as eastern_time
            FROM events"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                for item in &sel.select_list {
                    match item {
                        SelectItem::ExprAs { expr: Expr::AtTimeZone { .. }, .. } => {}
                        other => panic!("expected ExprAtTimeZone, got {:?}", other),
                    }
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_insert_with_date_cast() {
        let stmt = Parser::parse(
            "INSERT INTO events (name, event_date, event_time) 
            VALUES ('meeting', '2023-06-15'::date, '14:30:00'::time)"
        ).unwrap();
        match stmt {
            Statement::Insert(ins) => {
                assert_eq!(ins.table.parts, vec!["events"]);
                assert!(ins.columns.is_some());
                assert_eq!(ins.columns.as_ref().unwrap().len(), 3);
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn test_update_with_date_cast() {
        let stmt = Parser::parse(
            "UPDATE events SET event_date = '2023-12-25'::date WHERE id = 1"
        ).unwrap();
        match stmt {
            Statement::Update(upd) => {
                assert_eq!(upd.table.parts, vec!["events"]);
                assert_eq!(upd.set_clauses.len(), 1);
                assert!(upd.where_clause.is_some());
            }
            _ => panic!("expected Update"),
        }
    }


    // Window Frame tests

    #[test]
    fn test_window_with_rows_frame() {
        let stmt = Parser::parse(
            "SELECT id, SUM(amount) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM orders"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                match &sel.select_list[1] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert!(window.frame.is_some());
                        let frame = window.frame.as_ref().unwrap();
                        assert_eq!(*frame.start, FrameBound::UnboundedPreceding);
                        assert_eq!(**frame.end.as_ref().unwrap(), FrameBound::CurrentRow);
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_window_with_range_frame() {
        let stmt = Parser::parse(
            "SELECT id, AVG(price) OVER (ORDER BY created_at RANGE BETWEEN 1 PRECEDING AND CURRENT ROW) FROM products"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                match &sel.select_list[1] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert!(window.frame.is_some());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_window_with_groups_frame() {
        let stmt = Parser::parse(
            "SELECT id, COUNT(*) OVER (ORDER BY salary GROUPS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM employees"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                match &sel.select_list[1] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert!(window.frame.is_some());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_window_with_preceding_bound() {
        let stmt = Parser::parse(
            "SELECT id, SUM(amount) OVER (ORDER BY id ROWS 5 PRECEDING) FROM orders"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                match &sel.select_list[1] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert!(window.frame.is_some());
                        let frame = window.frame.as_ref().unwrap();
                        match &*frame.start {
                            FrameBound::Preceding(_) => {},
                            other => panic!("expected Preceding, got {:?}", other),
                        }
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_window_with_following_bound() {
        let stmt = Parser::parse(
            "SELECT id, SUM(amount) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 10 FOLLOWING) FROM orders"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 2);
                match &sel.select_list[1] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert!(window.frame.is_some());
                        let frame = window.frame.as_ref().unwrap();
                        assert_eq!(*frame.start, FrameBound::CurrentRow);
                        match &**frame.end.as_ref().unwrap() {
                            FrameBound::Following(_) => {},
                            other => panic!("expected Following, got {:?}", other),
                        }
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // CREATE SEQUENCE tests

    #[test]
    fn test_create_sequence_simple() {
        let stmt = Parser::parse("CREATE SEQUENCE order_seq").unwrap();
        match stmt {
            Statement::CreateSequence(seq) => {
                assert_eq!(seq.name.parts, vec!["order_seq"]);
                assert!(!seq.if_not_exists);
                assert!(seq.increment.is_none());
                assert!(seq.min_value.is_none());
                assert!(seq.max_value.is_none());
                assert!(seq.start.is_none());
                assert!(seq.cache.is_none());
                assert!(!seq.cycle);
            }
            _ => panic!("expected CreateSequence"),
        }
    }

    #[test]
    fn test_create_sequence_with_options() {
        let stmt = Parser::parse(
            "CREATE SEQUENCE IF NOT EXISTS my_seq INCREMENT BY 5 MINVALUE 1 MAXVALUE 999999 START WITH 100 CACHE 10 CYCLE"
        ).unwrap();
        match stmt {
            Statement::CreateSequence(seq) => {
                assert_eq!(seq.name.parts, vec!["my_seq"]);
                assert!(seq.if_not_exists);
                assert_eq!(seq.increment, Some(5));
                assert_eq!(seq.min_value, Some(1));
                assert_eq!(seq.max_value, Some(999999));
                assert_eq!(seq.start, Some(100));
                assert_eq!(seq.cache, Some(10));
                assert!(seq.cycle);
            }
            _ => panic!("expected CreateSequence"),
        }
    }

    #[test]
    fn test_create_sequence_with_no_cycle() {
        let stmt = Parser::parse(
            "CREATE SEQUENCE test_seq NO CYCLE NO MINVALUE NO MAXVALUE"
        ).unwrap();
        match stmt {
            Statement::CreateSequence(seq) => {
                assert_eq!(seq.name.parts, vec!["test_seq"]);
                assert!(!seq.cycle);
                assert!(seq.min_value.is_none());
                assert!(seq.max_value.is_none());
            }
            _ => panic!("expected CreateSequence"),
        }
    }

    #[test]
    fn test_create_sequence_with_schema() {
        let stmt = Parser::parse("CREATE SEQUENCE public.user_id_seq").unwrap();
        match stmt {
            Statement::CreateSequence(seq) => {
                assert_eq!(seq.name.parts, vec!["public", "user_id_seq"]);
            }
            _ => panic!("expected CreateSequence"),
        }
    }

    #[test]
    fn test_create_sequence_with_owned_by() {
        let stmt = Parser::parse("CREATE SEQUENCE test_seq OWNED BY users.id").unwrap();
        match stmt {
            Statement::CreateSequence(seq) => {
                assert_eq!(seq.name.parts, vec!["test_seq"]);
                assert!(seq.owned_by.is_some());
                assert_eq!(seq.owned_by.unwrap().parts, vec!["users", "id"]);
            }
            _ => panic!("expected CreateSequence"),
        }
    }

    // CREATE TYPE tests

    #[test]
    fn test_create_type_composite() {
        let stmt = Parser::parse(
            "CREATE TYPE address AS (street TEXT, city VARCHAR(100), zip_code CHAR(5))"
        ).unwrap();
        match stmt {
            Statement::CreateType(ct) => {
                assert_eq!(ct.name.parts, vec!["address"]);
                match &ct.definition {
                    TypeDefinition::Composite(attrs) => {
                        assert_eq!(attrs.len(), 3);
                        assert_eq!(attrs[0].name, "street");
                        assert_eq!(attrs[1].name, "city");
                        assert_eq!(attrs[2].name, "zip_code");
                    }
                    other => panic!("expected Composite, got {:?}", other),
                }
            }
            _ => panic!("expected CreateType"),
        }
    }

    #[test]
    fn test_create_type_enum() {
        let stmt = Parser::parse(
            "CREATE TYPE status AS ENUM ('active', 'inactive', 'pending')"
        ).unwrap();
        match stmt {
            Statement::CreateType(ct) => {
                assert_eq!(ct.name.parts, vec!["status"]);
                match &ct.definition {
                    TypeDefinition::Enum(values) => {
                        assert_eq!(values.len(), 3);
                        assert_eq!(values[0], "active");
                        assert_eq!(values[1], "inactive");
                        assert_eq!(values[2], "pending");
                    }
                    other => panic!("expected Enum, got {:?}", other),
                }
            }
            _ => panic!("expected CreateType"),
        }
    }

    #[test]
    fn test_create_type_range() {
        let stmt = Parser::parse(
            "CREATE TYPE float_range AS RANGE (SUBTYPE = DOUBLE PRECISION)"
        ).unwrap();
        match stmt {
            Statement::CreateType(ct) => {
                assert_eq!(ct.name.parts, vec!["float_range"]);
                match &ct.definition {
                    TypeDefinition::Range(subtype) => {
                        assert_eq!(*subtype, DataType::Double);
                    }
                    other => panic!("expected Range, got {:?}", other),
                }
            }
            _ => panic!("expected CreateType"),
        }
    }

    // MERGE tests

    #[test]
    fn test_merge_simple() {
        let stmt = Parser::parse(
            "MERGE INTO target t USING source s ON t.id = s.id WHEN MATCHED THEN UPDATE SET t.value = s.value"
        ).unwrap();
        match stmt {
            Statement::Merge(mg) => {
                assert_eq!(mg.target.parts, vec!["target"]);
                assert_eq!(mg.source, MergeSource::Table(ObjectName::new(vec!["source".to_string()])));
                assert_eq!(mg.clauses.len(), 1);
                match &mg.clauses[0] {
                    MergeClause::WhenMatched { condition, action } => {
                        assert!(condition.is_none());
                        match action {
                            MergeAction::Update { set_clauses } => {
                                assert_eq!(set_clauses.len(), 1);
                                assert_eq!(set_clauses[0].column, "t.value");
                            }
                            other => panic!("expected Update, got {:?}", other),
                        }
                    }
                    other => panic!("expected WhenMatched, got {:?}", other),
                }
            }
            _ => panic!("expected Merge"),
        }
    }

    #[test]
    fn test_merge_with_multiple_actions() {
        let stmt = Parser::parse(
            "MERGE INTO target t USING source s ON t.id = s.id WHEN MATCHED AND t.status = 'old' THEN DELETE WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)"
        ).unwrap();
        match stmt {
            Statement::Merge(mg) => {
                assert_eq!(mg.target.parts, vec!["target"]);
                assert_eq!(mg.clauses.len(), 2);
                match &mg.clauses[0] {
                    MergeClause::WhenMatched { condition, action } => {
                        assert!(condition.is_some());
                        match action {
                            MergeAction::Delete => {},
                            other => panic!("expected Delete, got {:?}", other),
                        }
                    }
                    other => panic!("expected WhenMatched, got {:?}", other),
                }
                match &mg.clauses[1] {
                    MergeClause::WhenNotMatched { condition, action } => {
                        assert!(condition.is_none());
                        match action {
                            MergeAction::Insert { columns, source } => {
                                assert_eq!(columns.as_ref().unwrap().len(), 2);
                                assert!(matches!(source, MergeInsertSource::Values(_)));
                            }
                            other => panic!("expected Insert, got {:?}", other),
                        }
                    }
                    other => panic!("expected WhenNotMatched, got {:?}", other),
                }
            }
            _ => panic!("expected Merge"),
        }
    }

    #[test]
    fn test_merge_with_subquery_source() {
        let stmt = Parser::parse(
            "MERGE INTO target t USING (SELECT id, name FROM source) s ON t.id = s.id WHEN MATCHED THEN UPDATE SET t.name = s.name"
        ).unwrap();
        match stmt {
            Statement::Merge(mg) => {
                assert_eq!(mg.target.parts, vec!["target"]);
                match &mg.source {
                    MergeSource::Subquery(_query) => {
                        // Subquery parsed successfully
                    }
                    other => panic!("expected Subquery, got {:?}", other),
                }
                assert_eq!(mg.clauses.len(), 1);
            }
            _ => panic!("expected Merge"),
        }
    }

    #[test]
    fn test_merge_with_condition() {
        let stmt = Parser::parse(
            "MERGE INTO target t USING source s ON t.id = s.id WHEN MATCHED AND s.amount > 100 THEN UPDATE SET t.status = 'high_value'"
        ).unwrap();
        match stmt {
            Statement::Merge(mg) => {
                assert_eq!(mg.clauses.len(), 1);
                match &mg.clauses[0] {
                    MergeClause::WhenMatched { condition, action } => {
                        assert!(condition.is_some());
                        match action {
                            MergeAction::Update { set_clauses } => {
                                assert_eq!(set_clauses.len(), 1);
                                assert_eq!(set_clauses[0].column, "t.status");
                            }
                            other => panic!("expected Update, got {:?}", other),
                        }
                    }
                    other => panic!("expected WhenMatched, got {:?}", other),
                }
            }
            _ => panic!("expected Merge"),
        }
    }

    // Type casting tests

    #[test]
    fn test_array_cast() {
        let stmt = Parser::parse("SELECT '{1,2,3}'::INTEGER[]").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        match data_type {
                            DataType::Array(inner) => {
                                assert_eq!(*inner, Box::new(DataType::Int));
                            }
                            other => panic!("expected Array, got {:?}", other),
                        }
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_json_cast() {
        let stmt = Parser::parse("SELECT '{\"key\": \"value\"}'::JSON").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Json);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_uuid_cast() {
        let stmt = Parser::parse("SELECT 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::UUID").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Uuid);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_boolean_cast() {
        let stmt = Parser::parse("SELECT 'true'::BOOLEAN").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Boolean);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_money_cast() {
        let stmt = Parser::parse("SELECT '123.45'::MONEY").unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 1);
                match &sel.select_list[0] {
                    SelectItem::Expr(Expr::TypeCast { data_type, .. }) => {
                        assert_eq!(*data_type, DataType::Money);
                    }
                    other => panic!("expected TypeCast, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    // Additional parser tests

    #[test]
    fn test_select_with_in_subquery() {
        let stmt = Parser::parse(
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.where_clause.is_some());
                let where_clause = sel.where_clause.as_ref().unwrap();
                match where_clause.as_ref() {
                    Expr::InSubquery { expr, negated, subquery: _ } => {
                        assert!(!negated);
                        match expr.as_ref() {
                            Expr::Identifier(name) => assert_eq!(name, "id"),
                            other => panic!("expected Identifier, got {:?}", other),
                        }
                    }
                    other => panic!("expected InSubquery, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_select_with_between() {
        let stmt = Parser::parse(
            "SELECT * FROM users WHERE age BETWEEN 18 AND 65"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert!(sel.where_clause.is_some());
                let where_clause = sel.where_clause.as_ref().unwrap();
                match where_clause.as_ref() {
                    Expr::Between { expr, low, high, .. } => {
                        match expr.as_ref() {
                            Expr::Identifier(name) => assert_eq!(name, "age"),
                            other => panic!("expected Identifier, got {:?}", other),
                        }
                        match low.as_ref() {
                            Expr::Literal(Literal::Number(n)) => assert_eq!(n, "18"),
                            other => panic!("expected Number, got {:?}", other),
                        }
                        match high.as_ref() {
                            Expr::Literal(Literal::Number(n)) => assert_eq!(n, "65"),
                            other => panic!("expected Number, got {:?}", other),
                        }
                    }
                    other => panic!("expected Between, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

    #[test]
    fn test_window_with_partition_and_order() {
        let stmt = Parser::parse(
            "SELECT id, department, SUM(amount) OVER (PARTITION BY department ORDER BY id) FROM employees"
        ).unwrap();
        match stmt {
            Statement::Select(sel) => {
                assert_eq!(sel.select_list.len(), 3);
                match &sel.select_list[2] {
                    SelectItem::Expr(Expr::Function(func)) => {
                        assert!(func.over.is_some());
                        let window = func.over.as_ref().unwrap();
                        assert_eq!(window.partition_by.len(), 1);
                        assert_eq!(window.order_by.len(), 1);
                        assert!(window.frame.is_none());
                    }
                    other => panic!("expected Function, got {:?}", other),
                }
            }
            _ => panic!("expected Select"),
        }
    }

