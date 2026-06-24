use crate::sql::ast::*;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Keyword(String),
    Ident(String),
    Number(String),
    String(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Semicolon,
    Colon,
    Eq,
    Neq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    Bang,
    Question,
    LtLt,
    GtGt,
    Eof,
}

impl Parser {
    pub fn new(sql: &str) -> Self {
        let tokens = tokenize(sql);
        Self { tokens, pos: 0 }
    }

    pub fn parse(sql: &str) -> anyhow::Result<Statement> {
        let mut parser = Self::new(sql);
        parser.parse_statement()
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, expected: &Token) -> anyhow::Result<()> {
        let token = self.advance();
        if &token != expected {
            anyhow::bail!("expected {:?}, got {:?}", expected, token);
        }
        Ok(())
    }

    fn expect_keyword(&mut self, keyword: &str) -> anyhow::Result<()> {
        match self.advance() {
            Token::Keyword(k) if k.to_uppercase() == keyword.to_uppercase() => Ok(()),
            other => anyhow::bail!("expected keyword '{}', got {:?}", keyword, other),
        }
    }

    fn expect_ident(&mut self) -> anyhow::Result<String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            Token::Keyword(k) => Ok(k),
            other => anyhow::bail!("expected identifier, got {:?}", other),
        }
    }

    fn parse_statement(&mut self) -> anyhow::Result<Statement> {
        let first = self.peek().clone();
        match first {
            Token::Keyword(k) => match k.to_uppercase().as_str() {
                "SELECT" => Ok(Statement::Select(self.parse_select()?)),
                "WITH" => Ok(Statement::Select(self.parse_with_select()?)),
                "INSERT" => Ok(Statement::Insert(self.parse_insert()?)),
                "UPDATE" => Ok(Statement::Update(self.parse_update()?)),
                "DELETE" => Ok(Statement::Delete(self.parse_delete()?)),
                "CREATE" => self.parse_create(),
                "ALTER" => self.parse_alter(),
                "DROP" => self.parse_drop(),
                "BEGIN" | "START" => Ok(Statement::Begin(self.parse_begin()?)),
                "COMMIT" => { self.advance(); Ok(Statement::Commit) }
                "ROLLBACK" | "ABORT" => { self.advance(); Ok(Statement::Rollback) }
                "EXPLAIN" => {
                    self.advance();
                    Ok(Statement::Explain(Box::new(self.parse_statement()?)))
                }
                _ => anyhow::bail!("unexpected keyword: {}", k),
            },
            Token::LParen => {
                let stmt = self.parse_statement()?;
                Ok(stmt)
            }
            _ => anyhow::bail!("unexpected token: {:?}", first),
        }
    }

    fn parse_select(&mut self) -> anyhow::Result<SelectStatement> {
        self.expect_keyword("SELECT")?;

        let distinct = match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "DISTINCT" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ON") {
                    self.advance();
                    self.expect(&Token::LParen)?;
                    let mut exprs = Vec::new();
                    loop {
                        exprs.push(self.parse_expr()?);
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(&Token::RParen)?;
                    DistinctClause::DistinctOn(exprs)
                } else {
                    DistinctClause::Distinct
                }
            }
            _ => DistinctClause::All,
        };

        let select_list = self.parse_select_list()?;

        let from = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FROM") {
            self.advance();
            Some(self.parse_from_clause()?)
        } else {
            None
        };

        let where_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHERE") {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let mut group_by = Vec::new();
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "GROUP") {
            self.advance();
            self.expect_keyword("BY")?;
            loop {
                group_by.push(self.parse_expr()?);
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let having = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "HAVING") {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let mut order_by = Vec::new();
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ORDER") {
            self.advance();
            self.expect_keyword("BY")?;
            loop {
                let expr = self.parse_expr()?;
                let direction = match self.peek() {
                    Token::Keyword(k) if k.to_uppercase() == "ASC" => {
                        self.advance();
                        SortDirection::Asc
                    }
                    Token::Keyword(k) if k.to_uppercase() == "DESC" => {
                        self.advance();
                        SortDirection::Desc
                    }
                    _ => SortDirection::Default,
                };
                let nulls = match self.peek() {
                    Token::Keyword(k) if k.to_uppercase() == "NULLS" => {
                        self.advance();
                        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FIRST") {
                            self.advance();
                            NullsOrder::First
                        } else {
                            self.expect_keyword("LAST")?;
                            NullsOrder::Last
                        }
                    }
                    _ => NullsOrder::Default,
                };
                order_by.push(OrderByItem { expr, direction, nulls });
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let limit = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "LIMIT") {
            self.advance();
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ALL") {
                self.advance();
                Some(LimitClause::All)
            } else {
                Some(LimitClause::Expr(self.parse_expr()?))
            }
        } else {
            None
        };

        let mut set_operations = Vec::new();
        loop {
            match self.peek() {
                Token::Keyword(k) if k.to_uppercase() == "UNION" => {
                    self.advance();
                    let all = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ALL");
                    if all {
                        self.advance();
                    }
                    let right = self.parse_select_body()?;
                    set_operations.push(SetOperation {
                        operator: if all { SetOperator::UnionAll } else { SetOperator::Union },
                        select: Box::new(right),
                    });
                }
                Token::Keyword(k) if k.to_uppercase() == "INTERSECT" => {
                    self.advance();
                    let all = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ALL");
                    if all {
                        self.advance();
                    }
                    let right = self.parse_select_body()?;
                    set_operations.push(SetOperation {
                        operator: if all { SetOperator::IntersectAll } else { SetOperator::Intersect },
                        select: Box::new(right),
                    });
                }
                Token::Keyword(k) if k.to_uppercase() == "EXCEPT" => {
                    self.advance();
                    let all = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ALL");
                    if all {
                        self.advance();
                    }
                    let right = self.parse_select_body()?;
                    set_operations.push(SetOperation {
                        operator: if all { SetOperator::ExceptAll } else { SetOperator::Except },
                        select: Box::new(right),
                    });
                }
                _ => break,
            }
        }

        Ok(SelectStatement {
            with: None,
            distinct,
            select_list,
            from,
            where_clause,
            group_by,
            having,
            order_by,
            limit,
            set_operations,
        })
    }

    fn parse_select_body(&mut self) -> anyhow::Result<SelectStatement> {
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            let stmt = self.parse_select()?;
            self.expect(&Token::RParen)?;
            Ok(stmt)
        } else {
            self.parse_select()
        }
    }

    fn parse_with_select(&mut self) -> anyhow::Result<SelectStatement> {
        self.expect_keyword("WITH")?;
        let recursive = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "RECURSIVE") {
            self.advance();
            true
        } else {
            false
        };
        
        let mut ctes = Vec::new();
        loop {
            ctes.push(self.parse_cte()?);
            if !matches!(self.peek(), Token::Comma) {
                break;
            }
            self.advance();
        }
        
        let mut select = self.parse_select()?;
        select.with = Some(WithClause { recursive, ctes });
        Ok(select)
    }

    fn parse_cte(&mut self) -> anyhow::Result<CommonTableExpr> {
        let name = self.expect_ident()?;
        
        let columns = if matches!(self.peek(), Token::LParen) {
            self.advance();
            let mut cols = Vec::new();
            loop {
                cols.push(self.expect_ident()?);
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect(&Token::RParen)?;
            Some(cols)
        } else {
            None
        };
        
        self.expect_keyword("AS")?;
        
        // Optional MATERIALIZED / NOT MATERIALIZED hint (after AS)
        let materialized = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "MATERIALIZED") {
            self.advance();
            Some(true)
        } else if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NOT") {
            // Check if next token is MATERIALIZED
            let saved_pos = self.pos;
            self.advance();
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "MATERIALIZED") {
                self.advance();
                Some(false)
            } else {
                // Not followed by MATERIALIZED, restore position
                self.pos = saved_pos;
                None
            }
        } else {
            None
        };
        
        let query = if matches!(self.peek(), Token::LParen) {
            self.advance();
            let stmt = self.parse_statement()?;
            self.expect(&Token::RParen)?;
            stmt
        } else {
            self.parse_statement()?
        };
        
        Ok(CommonTableExpr { name, columns, materialized, query })
    }

    fn parse_select_list(&mut self) -> anyhow::Result<Vec<SelectItem>> {
        let mut items = Vec::new();
        loop {
            if matches!(self.peek(), Token::Star) {
                self.advance();
                if matches!(self.peek(), Token::Dot) {
                    self.advance();
                    let table = self.expect_ident()?;
                    items.push(SelectItem::TableStar { table });
                } else {
                    items.push(SelectItem::Star);
                }
            } else {
                let expr = self.parse_expr()?;
                let item = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AS") {
                    self.advance();
                    let alias = self.expect_ident()?;
                    SelectItem::ExprAs { expr, alias }
                } else if matches!(self.peek(), Token::Ident(_)) {
                    let alias = match self.peek().clone() {
                        Token::Ident(s) => { self.advance(); s }
                        _ => unreachable!(),
                    };
                    SelectItem::ExprAs { expr, alias }
                } else {
                    SelectItem::Expr(expr)
                };
                items.push(item);
            }
            if !matches!(self.peek(), Token::Comma) {
                break;
            }
            self.advance();
        }
        Ok(items)
    }

    fn parse_from_clause(&mut self) -> anyhow::Result<FromClause> {
        let mut joins = Vec::new();

        let table = self.parse_table_ref()?;
        
        // Parse alias for base table
        let alias = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AS") {
            self.advance();
            Some(self.expect_ident()?)
        } else if matches!(self.peek(), Token::Ident(_)) {
            match self.peek().clone() {
                Token::Ident(s) => { self.advance(); Some(s) }
                _ => None,
            }
        } else {
            None
        };
        
        joins.push(Join {
            table,
            alias,
            join_type: JoinType::Inner,
            constraint: JoinConstraint::None,
        });

        while matches!(
            self.peek(),
            Token::Keyword(k) if matches!(k.to_uppercase().as_str(), "JOIN" | "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" | "NATURAL" | "LATERAL")
        ) {
            let join_type = self.parse_join_type()?;
            let table = self.parse_table_ref()?;

            let alias = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AS") {
                self.advance();
                Some(self.expect_ident()?)
            } else if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ON" || k.to_uppercase() == "USING" || k.to_uppercase() == "JOIN" || k.to_uppercase() == "WHERE" || k.to_uppercase() == "GROUP" || k.to_uppercase() == "ORDER" || k.to_uppercase() == "LIMIT" || k.to_uppercase() == "HAVING") {
                None
            } else if matches!(self.peek(), Token::Ident(_)) {
                match self.peek().clone() {
                    Token::Ident(s) => { self.advance(); Some(s) }
                    _ => None,
                }
            } else {
                None
            };

            let constraint = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ON") {
                self.advance();
                JoinConstraint::On(Box::new(self.parse_expr()?))
            } else if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "USING") {
                self.advance();
                self.expect(&Token::LParen)?;
                let mut columns = Vec::new();
                loop {
                    columns.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                JoinConstraint::Using(columns)
            } else {
                JoinConstraint::None
            };

            joins.push(Join {
                table,
                alias,
                join_type,
                constraint,
            });
        }

        Ok(FromClause { joins })
    }

    fn parse_table_ref(&mut self) -> anyhow::Result<TableRef> {
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SELECT") {
                let query = self.parse_select()?;
                self.expect(&Token::RParen)?;
                Ok(TableRef::Subquery(Box::new(query)))
            } else {
                let name = self.expect_ident()?;
                self.expect(&Token::RParen)?;
                Ok(TableRef::Table(ObjectName::single(name)))
            }
        } else {
            let mut parts = vec![self.expect_ident()?];
            while matches!(self.peek(), Token::Dot) {
                self.advance();
                parts.push(self.expect_ident()?);
            }
            Ok(TableRef::Table(ObjectName::new(parts)))
        }
    }

    fn parse_join_type(&mut self) -> anyhow::Result<JoinType> {
        let mut join_type = JoinType::Inner;

        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "CROSS" => {
                self.advance();
                join_type = JoinType::Cross;
            }
            Token::Keyword(k) if k.to_uppercase() == "INNER" => {
                self.advance();
            }
            Token::Keyword(k) if k.to_uppercase() == "LEFT" => {
                self.advance();
                join_type = JoinType::Left;
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OUTER") {
                    self.advance();
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "RIGHT" => {
                self.advance();
                join_type = JoinType::Right;
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OUTER") {
                    self.advance();
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "FULL" => {
                self.advance();
                join_type = JoinType::Full;
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OUTER") {
                    self.advance();
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "NATURAL" => {
                self.advance();
                join_type = JoinType::Inner;
            }
            Token::Keyword(k) if k.to_uppercase() == "LATERAL" => {
                self.advance();
                join_type = JoinType::Lateral;
            }
            _ => {
                // Default to INNER JOIN - JOIN keyword consumed below
            }
        }

        if !matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "JOIN") {
            self.expect_keyword("JOIN")?;
        } else {
            self.advance();
        }

        Ok(join_type)
    }

    fn parse_insert(&mut self) -> anyhow::Result<InsertStatement> {
        self.expect_keyword("INSERT")?;
        self.expect_keyword("INTO")?;

        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        let table = ObjectName::new(parts);

        let columns = if matches!(self.peek(), Token::LParen) {
            self.advance();
            let mut cols = Vec::new();
            loop {
                cols.push(self.expect_ident()?);
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect(&Token::RParen)?;
            Some(cols)
        } else {
            None
        };

        let source = match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "VALUES" => {
                self.advance();
                let mut values = Vec::new();
                loop {
                    self.expect(&Token::LParen)?;
                    let mut row = Vec::new();
                    loop {
                        row.push(self.parse_expr()?);
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(&Token::RParen)?;
                    values.push(row);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                InsertSource::Values(values)
            }
            Token::Keyword(k) if k.to_uppercase() == "SELECT" => {
                InsertSource::Select(Box::new(self.parse_select()?))
            }
            _ => anyhow::bail!("expected VALUES or SELECT after INSERT INTO"),
        };

        let on_conflict = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ON") {
            self.advance();
            self.expect_keyword("CONFLICT")?;
            
            // Optional conflict target: (col1, col2, ...) before DO
            let conflict_target = if matches!(self.peek(), Token::LParen) {
                self.advance();
                let mut cols = Vec::new();
                loop {
                    cols.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                if cols.is_empty() {
                    None
                } else {
                    Some(cols)
                }
            } else {
                None
            };
            
            self.expect_keyword("DO")?;
            
            match self.peek() {
                Token::Keyword(k) if k.to_uppercase() == "NOTHING" => {
                    self.advance();
                    Some(OnConflict::DoNothing)
                }
                Token::Keyword(k) if k.to_uppercase() == "UPDATE" => {
                    self.advance();
                    self.expect_keyword("SET")?;
                    
                    let mut set_clauses = Vec::new();
                    loop {
                        let column = self.expect_ident()?;
                        self.expect(&Token::Eq)?;
                        let value = self.parse_expr()?;
                        set_clauses.push(SetClause {
                            column,
                            value: Box::new(value),
                        });
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    
                    let where_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHERE") {
                        self.advance();
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    
                    Some(OnConflict::DoUpdate {
                        target_columns: conflict_target,
                        where_clause,
                        set_clauses,
                    })
                }
                _ => anyhow::bail!("expected NOTHING or UPDATE after DO"),
            }
        } else {
            None
        };

        let returning = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "RETURNING") {
            self.advance();
            self.parse_select_list()?
        } else {
            Vec::new()
        };

        Ok(InsertStatement {
            table,
            columns,
            source,
            on_conflict,
            returning,
        })
    }

    fn parse_update(&mut self) -> anyhow::Result<UpdateStatement> {
        self.expect_keyword("UPDATE")?;

        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        let table = ObjectName::new(parts);

        let from = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FROM") {
            self.advance();
            Some(self.parse_from_clause()?)
        } else {
            None
        };

        self.expect_keyword("SET")?;
        let mut set_clauses = Vec::new();
        loop {
            let column = self.expect_ident()?;
            self.expect(&Token::Eq)?;
            let value = self.parse_expr()?;
            set_clauses.push(SetClause {
                column,
                value: Box::new(value),
            });
            if !matches!(self.peek(), Token::Comma) {
                break;
            }
            self.advance();
        }

        let where_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHERE") {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let returning = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "RETURNING") {
            self.advance();
            self.parse_select_list()?
        } else {
            Vec::new()
        };

        Ok(UpdateStatement {
            table,
            from,
            set_clauses,
            where_clause,
            returning,
        })
    }

    fn parse_delete(&mut self) -> anyhow::Result<DeleteStatement> {
        self.expect_keyword("DELETE")?;
        self.expect_keyword("FROM")?;

        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        let table = ObjectName::new(parts);

        let using = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "USING") {
            self.advance();
            Some(self.parse_from_clause()?)
        } else {
            None
        };

        let where_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHERE") {
            self.advance();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let returning = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "RETURNING") {
            self.advance();
            self.parse_select_list()?
        } else {
            Vec::new()
        };

        Ok(DeleteStatement {
            table,
            using,
            where_clause,
            returning,
        })
    }

    fn parse_create(&mut self) -> anyhow::Result<Statement> {
        self.expect_keyword("CREATE")?;

        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "TABLE" => {
                self.advance();
                let if_not_exists = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF") {
                    self.advance();
                    self.expect_keyword("NOT")?;
                    self.expect_keyword("EXISTS")?;
                    true
                } else {
                    false
                };

                let mut parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                let table = ObjectName::new(parts);

                self.expect(&Token::LParen)?;
                let mut columns = Vec::new();
                let mut constraints = Vec::new();

                loop {
                    if matches!(self.peek(), Token::RParen) {
                        break;
                    }

                    match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "PRIMARY" => {
                            self.advance();
                            self.expect_keyword("KEY")?;
                            self.expect(&Token::LParen)?;
                            let mut cols = Vec::new();
                            loop {
                                cols.push(self.expect_ident()?);
                                if !matches!(self.peek(), Token::Comma) {
                                    break;
                                }
                                self.advance();
                            }
                            self.expect(&Token::RParen)?;
                            constraints.push(TableConstraint::PrimaryKey(cols));
                        }
                        Token::Keyword(k) if k.to_uppercase() == "UNIQUE" => {
                            self.advance();
                            self.expect(&Token::LParen)?;
                            let mut cols = Vec::new();
                            loop {
                                cols.push(self.expect_ident()?);
                                if !matches!(self.peek(), Token::Comma) {
                                    break;
                                }
                                self.advance();
                            }
                            self.expect(&Token::RParen)?;
                            constraints.push(TableConstraint::Unique(cols));
                        }
                        Token::Keyword(k) if k.to_uppercase() == "CONSTRAINT" => {
                            self.advance();
                            let _name = self.expect_ident()?;
                            self.parse_table_constraint(&mut constraints)?;
                        }
                        _ => {
                            let col = self.parse_column_def()?;
                            columns.push(col);
                        }
                    }

                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }

                self.expect(&Token::RParen)?;

                Ok(Statement::CreateTable(CreateTableStatement {
                    table,
                    columns,
                    constraints,
                    if_not_exists,
                }))
            }
            Token::Keyword(k) if k.to_uppercase() == "INDEX" => {
                self.advance();
                let unique = false;
                let if_not_exists = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF") {
                    self.advance();
                    self.expect_keyword("NOT")?;
                    self.expect_keyword("EXISTS")?;
                    true
                } else {
                    false
                };

                let mut parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                let name = ObjectName::new(parts);

                self.expect_keyword("ON")?;

                let mut table_parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    table_parts.push(self.expect_ident()?);
                }
                let table = ObjectName::new(table_parts);

                self.expect(&Token::LParen)?;
                let mut columns = Vec::new();
                loop {
                    let expr = self.parse_expr()?;
                    let direction = match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "ASC" => {
                            self.advance();
                            SortDirection::Asc
                        }
                        Token::Keyword(k) if k.to_uppercase() == "DESC" => {
                            self.advance();
                            SortDirection::Desc
                        }
                        _ => SortDirection::Default,
                    };
                    let nulls = match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "NULLS" => {
                            self.advance();
                            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FIRST") {
                                self.advance();
                                NullsOrder::First
                            } else {
                                self.expect_keyword("LAST")?;
                                NullsOrder::Last
                            }
                        }
                        _ => NullsOrder::Default,
                    };
                    columns.push(IndexColumn {
                        expr,
                        direction,
                        nulls,
                    });
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;

                Ok(Statement::CreateIndex(CreateIndexStatement {
                    name,
                    table,
                    columns,
                    unique,
                    if_not_exists,
                }))
            }
            Token::Keyword(k) if k.to_uppercase() == "VIEW" => {
                self.advance();
                let or_replace = false;
                let mut parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                let name = ObjectName::new(parts);

                let columns = if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let mut cols = Vec::new();
                    loop {
                        cols.push(self.expect_ident()?);
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(&Token::RParen)?;
                    Some(cols)
                } else {
                    None
                };

                self.expect_keyword("AS")?;
                let query = self.parse_select()?;

                Ok(Statement::CreateView(CreateViewStatement {
                    name,
                    columns,
                    query: Box::new(query),
                    or_replace,
                }))
            }
            _ => anyhow::bail!("expected TABLE, INDEX, or VIEW after CREATE"),
        }
    }

    fn parse_column_def(&mut self) -> anyhow::Result<ColumnDef> {
        let name = self.expect_ident()?;
        let data_type = self.parse_data_type()?;
        let mut constraints = Vec::new();

        loop {
            match self.peek() {
                Token::Keyword(k) => match k.to_uppercase().as_str() {
                    "NOT" => {
                        self.advance();
                        self.expect_keyword("NULL")?;
                        constraints.push(ColumnConstraint::NotNull);
                    }
                    "NULL" => {
                        self.advance();
                        constraints.push(ColumnConstraint::Null);
                    }
                    "PRIMARY" => {
                        self.advance();
                        self.expect_keyword("KEY")?;
                        constraints.push(ColumnConstraint::PrimaryKey);
                    }
                    "UNIQUE" => {
                        self.advance();
                        constraints.push(ColumnConstraint::Unique);
                    }
                    "DEFAULT" => {
                        self.advance();
                        let expr = self.parse_expr()?;
                        constraints.push(ColumnConstraint::Default(expr));
                    }
                    "REFERENCES" => {
                        self.advance();
                        let mut table_parts = vec![self.expect_ident()?];
                        while matches!(self.peek(), Token::Dot) {
                            self.advance();
                            table_parts.push(self.expect_ident()?);
                        }
                        let table = ObjectName::new(table_parts);
                        self.expect(&Token::LParen)?;
                        let column = self.expect_ident()?;
                        self.expect(&Token::RParen)?;
                        constraints.push(ColumnConstraint::References {
                            table,
                            column,
                            on_delete: None,
                            on_update: None,
                        });
                    }
                    "CHECK" => {
                        self.advance();
                        self.expect(&Token::LParen)?;
                        let expr = self.parse_expr()?;
                        self.expect(&Token::RParen)?;
                        constraints.push(ColumnConstraint::Check(expr));
                    }
                    _ => break,
                },
                Token::RParen | Token::Comma | Token::Eof => break,
                _ => anyhow::bail!("unexpected token in column definition: {:?}", self.peek()),
            }
        }

        Ok(ColumnDef {
            name,
            data_type,
            constraints,
        })
    }

    fn parse_table_constraint(&mut self, constraints: &mut Vec<TableConstraint>) -> anyhow::Result<()> {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "PRIMARY" => {
                self.advance();
                self.expect_keyword("KEY")?;
                self.expect(&Token::LParen)?;
                let mut cols = Vec::new();
                loop {
                    cols.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                constraints.push(TableConstraint::PrimaryKey(cols));
            }
            Token::Keyword(k) if k.to_uppercase() == "UNIQUE" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let mut cols = Vec::new();
                loop {
                    cols.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                constraints.push(TableConstraint::Unique(cols));
            }
            Token::Keyword(k) if k.to_uppercase() == "CHECK" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                constraints.push(TableConstraint::Check(expr));
            }
            Token::Keyword(k) if k.to_uppercase() == "FOREIGN" => {
                self.advance();
                self.expect_keyword("KEY")?;
                self.expect(&Token::LParen)?;
                let mut columns = Vec::new();
                loop {
                    columns.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;

                self.expect_keyword("REFERENCES")?;

                let mut table_parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    table_parts.push(self.expect_ident()?);
                }
                let ref_table = ObjectName::new(table_parts);

                self.expect(&Token::LParen)?;
                let mut ref_columns = Vec::new();
                loop {
                    ref_columns.push(self.expect_ident()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;

                constraints.push(TableConstraint::ForeignKey {
                    columns,
                    ref_table,
                    ref_columns,
                    on_delete: None,
                    on_update: None,
                });
            }
            _ => anyhow::bail!("unexpected token in table constraint"),
        }
        Ok(())
    }

    fn parse_alter(&mut self) -> anyhow::Result<Statement> {
        self.expect_keyword("ALTER")?;
        self.expect_keyword("TABLE")?;

        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        let table = ObjectName::new(parts);

        let action = match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "ADD" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "COLUMN") {
                    self.advance();
                }
                let col = self.parse_column_def()?;
                AlterTableAction::AddColumn(col)
            }
            Token::Keyword(k) if k.to_uppercase() == "DROP" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "COLUMN") {
                    self.advance();
                }
                let name = self.expect_ident()?;
                let if_exists = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF");
                if if_exists {
                    self.advance();
                    self.expect_keyword("EXISTS")?;
                }
                let cascade = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "CASCADE");
                if cascade {
                    self.advance();
                }
                AlterTableAction::DropColumn {
                    name,
                    if_exists,
                    cascade,
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "RENAME" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "COLUMN") {
                    self.advance();
                    let old_name = self.expect_ident()?;
                    self.expect_keyword("TO")?;
                    let new_name = self.expect_ident()?;
                    AlterTableAction::RenameColumn { old_name, new_name }
                } else {
                    self.expect_keyword("TO")?;
                    let mut new_parts = vec![self.expect_ident()?];
                    while matches!(self.peek(), Token::Dot) {
                        self.advance();
                        new_parts.push(self.expect_ident()?);
                    }
                    AlterTableAction::RenameTable {
                        new_name: ObjectName::new(new_parts),
                    }
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "ALTER" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "COLUMN") {
                    self.advance();
                }
                let name = self.expect_ident()?;
                let action = match self.peek() {
                    Token::Keyword(k) if k.to_uppercase() == "SET" => {
                        self.advance();
                        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "DEFAULT") {
                            self.advance();
                            AlterColumnAction::SetDefault(self.parse_expr()?)
                        } else if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NOT") {
                            self.advance();
                            self.expect_keyword("NULL")?;
                            AlterColumnAction::SetNotNull
                        } else {
                            self.expect_keyword("DATA")?;
                            self.expect_keyword("TYPE")?;
                            AlterColumnAction::SetDataType(self.parse_data_type()?)
                        }
                    }
                    Token::Keyword(k) if k.to_uppercase() == "DROP" => {
                        self.advance();
                        match self.peek() {
                            Token::Keyword(k) if k.to_uppercase() == "DEFAULT" => {
                                self.advance();
                                AlterColumnAction::DropDefault
                            }
                            Token::Keyword(k) if k.to_uppercase() == "NOT" => {
                                self.advance();
                                self.expect_keyword("NULL")?;
                                AlterColumnAction::DropNotNull
                            }
                            _ => anyhow::bail!("expected DEFAULT or NOT NULL after DROP"),
                        }
                    }
                    _ => anyhow::bail!("expected SET or DROP after ALTER COLUMN"),
                };
                AlterTableAction::AlterColumn { name, action }
            }
            _ => anyhow::bail!("expected ADD, DROP, RENAME, or ALTER after ALTER TABLE"),
        };

        Ok(Statement::AlterTable(AlterTableStatement { table, action }))
    }

    fn parse_drop(&mut self) -> anyhow::Result<Statement> {
        self.expect_keyword("DROP")?;

        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "TABLE" => {
                self.advance();
                let if_exists = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF");
                if if_exists {
                    self.advance();
                    self.expect_keyword("EXISTS")?;
                }
                let mut parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                let table = ObjectName::new(parts);
                let cascade = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "CASCADE");
                if cascade {
                    self.advance();
                }
                Ok(Statement::DropTable(DropTableStatement {
                    table,
                    if_exists,
                    cascade,
                }))
            }
            Token::Keyword(k) if k.to_uppercase() == "INDEX" => {
                self.advance();
                let if_exists = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF");
                if if_exists {
                    self.advance();
                    self.expect_keyword("EXISTS")?;
                }
                let mut parts = vec![self.expect_ident()?];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                let name = ObjectName::new(parts);
                let cascade = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "CASCADE");
                if cascade {
                    self.advance();
                }
                Ok(Statement::DropIndex(DropIndexStatement {
                    name,
                    if_exists,
                    cascade,
                }))
            }
            _ => anyhow::bail!("expected TABLE or INDEX after DROP"),
        }
    }

    fn parse_begin(&mut self) -> anyhow::Result<BeginStatement> {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "BEGIN" => {
                self.advance();
            }
            Token::Keyword(k) if k.to_uppercase() == "START" => {
                self.advance();
                self.expect_keyword("TRANSACTION")?;
            }
            _ => anyhow::bail!("expected BEGIN or START TRANSACTION"),
        }

        let mut isolation_level = None;
        let mut read_only = false;
        let mut deferrable = false;

        loop {
            match self.peek() {
                Token::Keyword(k) if k.to_uppercase() == "ISOLATION" => {
                    self.advance();
                    self.expect_keyword("LEVEL")?;
                    isolation_level = Some(match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "SERIALIZABLE" => {
                            self.advance();
                            IsolationLevel::Serializable
                        }
                        Token::Keyword(k) if k.to_uppercase() == "REPEATABLE" => {
                            self.advance();
                            self.expect_keyword("READ")?;
                            IsolationLevel::RepeatableRead
                        }
                        Token::Keyword(k) if k.to_uppercase() == "READ" => {
                            self.advance();
                            match self.peek() {
                                Token::Keyword(k) if k.to_uppercase() == "COMMITTED" => {
                                    self.advance();
                                    IsolationLevel::ReadCommitted
                                }
                                Token::Keyword(k) if k.to_uppercase() == "UNCOMMITTED" => {
                                    self.advance();
                                    IsolationLevel::ReadUncommitted
                                }
                                _ => anyhow::bail!("expected COMMITTED or UNCOMMITTED"),
                            }
                        }
                        _ => anyhow::bail!("expected isolation level"),
                    });
                }
                Token::Keyword(k) if k.to_uppercase() == "READ" => {
                    self.advance();
                    self.expect_keyword("WRITE")?;
                    read_only = false;
                }
                Token::Keyword(k) if k.to_uppercase() == "DEFERRABLE" => {
                    self.advance();
                    deferrable = true;
                }
                Token::Keyword(k) if k.to_uppercase() == "NOT" => {
                    self.advance();
                    self.expect_keyword("DEFERRABLE")?;
                    deferrable = false;
                }
                _ => break,
            }
        }

        Ok(BeginStatement {
            isolation_level,
            read_only,
            deferrable,
        })
    }

    fn parse_expr(&mut self) -> anyhow::Result<Expr> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_and_expr()?;
        while matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OR") {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_not_expr()?;
        while matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AND") {
            self.advance();
            let right = self.parse_not_expr()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_not_expr(&mut self) -> anyhow::Result<Expr> {
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NOT") {
            self.advance();
            let expr = self.parse_comparison()?;
            Ok(Expr::UnaryOp {
                op: UnaryOperator::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> anyhow::Result<Expr> {
        let left = self.parse_bitwise_or()?;
        match self.peek() {
            Token::Eq => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::Equals,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::Equals,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Equals,
                        right: Box::new(right),
                    })
                }
            }
            Token::Neq => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::NotEquals,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::NotEquals,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::NotEquals,
                        right: Box::new(right),
                    })
                }
            }
            Token::Lt => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::LessThan,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::LessThan,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::LessThan,
                        right: Box::new(right),
                    })
                }
            }
            Token::Gt => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::GreaterThan,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::GreaterThan,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::GreaterThan,
                        right: Box::new(right),
                    })
                }
            }
            Token::LtEq => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::LessOrEqual,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::LessOrEqual,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::LessOrEqual,
                        right: Box::new(right),
                    })
                }
            }
            Token::GtEq => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME") {
                    let is_some = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
                    self.advance();
                    let right = self.parse_primary()?;
                    if is_some {
                        Ok(Expr::SomeComparison {
                            op: BinaryOperator::GreaterOrEqual,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    } else {
                        Ok(Expr::AnyComparison {
                            op: BinaryOperator::GreaterOrEqual,
                            left: Box::new(left),
                            right: Box::new(right),
                        })
                    }
                } else {
                    let right = self.parse_bitwise_or()?;
                    Ok(Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::GreaterOrEqual,
                        right: Box::new(right),
                    })
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "IN" => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SELECT") {
                        let query = self.parse_select()?;
                        self.expect(&Token::RParen)?;
                        Ok(Expr::InSubquery {
                            expr: Box::new(left),
                            negated: false,
                            subquery: Box::new(query),
                        })
                    } else {
                        let mut list = Vec::new();
                        loop {
                            list.push(self.parse_expr()?);
                            if !matches!(self.peek(), Token::Comma) {
                                break;
                            }
                            self.advance();
                        }
                        self.expect(&Token::RParen)?;
                        Ok(Expr::InList {
                            expr: Box::new(left),
                            negated: false,
                            list,
                        })
                    }
                } else {
                    anyhow::bail!("expected ( after IN");
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "BETWEEN" => {
                self.advance();
                let low = self.parse_bitwise_or()?;
                self.expect_keyword("AND")?;
                let high = self.parse_bitwise_or()?;
                Ok(Expr::Between {
                    expr: Box::new(left),
                    negated: false,
                    low: Box::new(low),
                    high: Box::new(high),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "LIKE" => {
                self.advance();
                let right = self.parse_bitwise_or()?;
                Ok(Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::Like,
                    right: Box::new(right),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "ILIKE" => {
                self.advance();
                let right = self.parse_bitwise_or()?;
                Ok(Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::ILike,
                    right: Box::new(right),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "IS" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NULL") {
                    self.advance();
                    Ok(Expr::IsNull(Box::new(left)))
                } else {
                    self.expect_keyword("NOT")?;
                    self.expect_keyword("NULL")?;
                    Ok(Expr::IsNotNull(Box::new(left)))
                }
            }
            _ => Ok(left),
        }
    }

    fn parse_bitwise_or(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_bitwise_xor()?;
        while matches!(self.peek(), Token::Pipe) {
            self.advance();
            let right = self.parse_bitwise_xor()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::BitwiseOr,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_bitwise_and()?;
        while matches!(self.peek(), Token::Caret) {
            self.advance();
            let right = self.parse_bitwise_and()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::BitwiseXor,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_shift()?;
        while matches!(self.peek(), Token::Ampersand) {
            self.advance();
            let right = self.parse_shift()?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::BitwiseAnd,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            match self.peek() {
                Token::LtLt => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::BitwiseShiftLeft,
                        right: Box::new(right),
                    };
                }
                Token::GtGt => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::BitwiseShiftRight,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            match self.peek() {
                Token::Plus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Plus,
                        right: Box::new(right),
                    };
                }
                Token::Minus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Minus,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> anyhow::Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Multiply,
                        right: Box::new(right),
                    };
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Divide,
                        right: Box::new(right),
                    };
                }
                Token::Percent => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Modulo,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> anyhow::Result<Expr> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_postfix()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOperator::Minus,
                    expr: Box::new(expr),
                })
            }
            Token::Plus => {
                self.advance();
                let expr = self.parse_postfix()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOperator::Plus,
                    expr: Box::new(expr),
                })
            }
            Token::Tilde => {
                self.advance();
                let expr = self.parse_postfix()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOperator::BitwiseNot,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> anyhow::Result<Expr> {
        let mut expr = self.parse_primary()?;
        
        // Handle :: type cast (PostgreSQL shorthand)
        while matches!(self.peek(), Token::Colon) {
            // Check if next is another colon (::)
            let saved_pos = self.pos;
            self.advance();
            if matches!(self.peek(), Token::Colon) {
                self.advance();
                let data_type = self.parse_data_type()?;
                expr = Expr::TypeCast {
                    expr: Box::new(expr),
                    data_type,
                };
            } else {
                // Not ::, restore position
                self.pos = saved_pos;
                break;
            }
        }
        
        // Handle AT TIME ZONE
        while matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AT") {
            let saved_pos = self.pos;
            self.advance();
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "TIME") {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ZONE") {
                    self.advance();
                    let zone = self.parse_bitwise_or()?;
                    expr = Expr::AtTimeZone {
                        expr: Box::new(expr),
                        zone: Box::new(zone),
                    };
                } else {
                    self.pos = saved_pos;
                    break;
                }
            } else {
                self.pos = saved_pos;
                break;
            }
        }
        
        Ok(expr)
    }

    fn parse_primary(&mut self) -> anyhow::Result<Expr> {
        match self.peek().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Number(n)))
            }
            Token::String(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            Token::Keyword(k) if k.to_uppercase() == "TRUE" => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            Token::Keyword(k) if k.to_uppercase() == "FALSE" => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            Token::Keyword(k) if k.to_uppercase() == "NULL" => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            Token::LParen => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SELECT") {
                    let query = self.parse_select()?;
                    self.expect(&Token::RParen)?;
                    Ok(Expr::NestedSelect(Box::new(query)))
                } else {
                    let expr = self.parse_expr()?;
                    self.expect(&Token::RParen)?;
                    Ok(expr)
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "CASE" => {
                self.advance();
                let operand = if !matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHEN") {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };

                let mut when_clauses = Vec::new();
                loop {
                    if !matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHEN") {
                        break;
                    }
                    self.advance();
                    let when = self.parse_expr()?;
                    self.expect_keyword("THEN")?;
                    let then = self.parse_expr()?;
                    when_clauses.push(WhenClause {
                        when: Box::new(when),
                        then: Box::new(then),
                    });
                }

                let else_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ELSE") {
                    self.advance();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };

                self.expect_keyword("END")?;

                Ok(Expr::Case {
                    operand,
                    when_clauses,
                    else_clause,
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "CAST" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let expr = self.parse_expr()?;
                self.expect_keyword("AS")?;
                let data_type = self.parse_data_type()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::TypeCast {
                    expr: Box::new(expr),
                    data_type,
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "EXTRACT" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let field_str = match self.peek() {
                    Token::Ident(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::Keyword(k) => {
                        let s = k.clone();
                        self.advance();
                        s
                    }
                    _ => anyhow::bail!("expected date part in EXTRACT"),
                };
                let field = DatePart::from_str(&field_str)
                    .ok_or_else(|| anyhow::anyhow!("unknown date part: {}", field_str))?;
                self.expect_keyword("FROM")?;
                let from = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Extract {
                    field,
                    from: Box::new(from),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "DATE_TRUNC" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let field_str = match self.peek() {
                    Token::Ident(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::Keyword(k) => {
                        let s = k.clone();
                        self.advance();
                        s
                    }
                    _ => anyhow::bail!("expected date part in DATE_TRUNC"),
                };
                let field = DatePart::from_str(&field_str)
                    .ok_or_else(|| anyhow::anyhow!("unknown date part: {}", field_str))?;
                self.expect(&Token::Comma)?;
                let source = self.parse_expr()?;
                let zone = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AT") {
                    self.advance();
                    self.expect_keyword("TIME")?;
                    self.expect_keyword("ZONE")?;
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                self.expect(&Token::RParen)?;
                Ok(Expr::DateTrunc {
                    field,
                    source: Box::new(source),
                    zone,
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "DATE_PART" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let field_str = match self.peek() {
                    Token::Ident(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    Token::Keyword(k) => {
                        let s = k.clone();
                        self.advance();
                        s
                    }
                    _ => anyhow::bail!("expected date part in DATE_PART"),
                };
                let field = DatePart::from_str(&field_str)
                    .ok_or_else(|| anyhow::anyhow!("unknown date part: {}", field_str))?;
                self.expect(&Token::Comma)?;
                let from = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Extract {
                    field,
                    from: Box::new(from),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "NOW" => {
                self.advance();
                self.expect(&Token::LParen)?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("NOW".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Keyword(k) if k.to_uppercase() == "CURRENT_DATE" => {
                self.advance();
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("CURRENT_DATE".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Keyword(k) if k.to_uppercase() == "CURRENT_TIME" => {
                self.advance();
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("CURRENT_TIME".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Keyword(k) if k.to_uppercase() == "CURRENT_TIMESTAMP" => {
                self.advance();
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("CURRENT_TIMESTAMP".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Keyword(k) if k.to_uppercase() == "LOCALTIME" => {
                self.advance();
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("LOCALTIME".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Keyword(k) if k.to_uppercase() == "LOCALTIMESTAMP" => {
                self.advance();
                Ok(Expr::Function(Box::new(FunctionCall {
                    name: ObjectName::single("LOCALTIMESTAMP".to_string()),
                    args: Vec::new(),
                    filter: None,
                    over: None,
                })))
            }
            Token::Ident(s) | Token::Keyword(s) => {
                self.advance();
                let name = s;

                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let mut args = Vec::new();

                    if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "DISTINCT") {
                        self.advance();
                    }

                    if matches!(self.peek(), Token::Star) {
                        self.advance();
                        args.push(FunctionArg::Star);
                    } else {
                        loop {
                            args.push(FunctionArg::Expr(self.parse_expr()?));
                            if !matches!(self.peek(), Token::Comma) {
                                break;
                            }
                            self.advance();
                        }
                    }

                    self.expect(&Token::RParen)?;

                    let filter = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FILTER") {
                        self.advance();
                        self.expect(&Token::LParen)?;
                        self.expect_keyword("WHERE")?;
                        let expr = self.parse_expr()?;
                        self.expect(&Token::RParen)?;
                        Some(Box::new(expr))
                    } else {
                        None
                    };

                    let over = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OVER") {
                        self.advance();
                        Some(Box::new(self.parse_window_spec()?))
                    } else {
                        None
                    };

                    Ok(Expr::Function(Box::new(FunctionCall {
                        name: ObjectName::single(name),
                        args,
                        filter,
                        over,
                    })))
                } else if name.to_uppercase() == "ARRAY" && matches!(self.peek(), Token::LBracket) {
                    // ARRAY[1,2,3] syntax
                    self.advance();
                    let mut elements = Vec::new();
                    loop {
                        elements.push(self.parse_expr()?);
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(&Token::RBracket)?;
                    Ok(Expr::Array(elements))
                } else if matches!(self.peek(), Token::Dot) {
                    self.advance();
                    let column = self.expect_ident()?;
                    Ok(Expr::QualifiedIdentifier { table: name, column })
                } else {
                    Ok(Expr::Identifier(name))
                }
            }
            Token::Question => {
                self.advance();
                let mut param_num = 1;
                if let Token::Number(n) = self.peek().clone() {
                    param_num = n.parse().unwrap_or(1);
                    self.advance();
                }
                Ok(Expr::Parameter(param_num))
            }
            Token::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                loop {
                    elements.push(self.parse_expr()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Array(elements))
            }
            _ => anyhow::bail!("unexpected token in expression: {:?}", self.peek()),
        }
    }

    fn parse_window_spec(&mut self) -> anyhow::Result<WindowSpec> {
        self.expect(&Token::LParen)?;

        let mut partition_by = Vec::new();
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "PARTITION") {
            self.advance();
            self.expect_keyword("BY")?;
            loop {
                partition_by.push(self.parse_expr()?);
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let mut order_by = Vec::new();
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ORDER") {
            self.advance();
            self.expect_keyword("BY")?;
            loop {
                let expr = self.parse_expr()?;
                let direction = match self.peek() {
                    Token::Keyword(k) if k.to_uppercase() == "ASC" => {
                        self.advance();
                        SortDirection::Asc
                    }
                    Token::Keyword(k) if k.to_uppercase() == "DESC" => {
                        self.advance();
                        SortDirection::Desc
                    }
                    _ => SortDirection::Default,
                };
                let nulls = match self.peek() {
                    Token::Keyword(k) if k.to_uppercase() == "NULLS" => {
                        self.advance();
                        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FIRST") {
                            self.advance();
                            NullsOrder::First
                        } else {
                            self.expect_keyword("LAST")?;
                            NullsOrder::Last
                        }
                    }
                    _ => NullsOrder::Default,
                };
                order_by.push(OrderByItem { expr, direction, nulls });
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        self.expect(&Token::RParen)?;

        Ok(WindowSpec {
            partition_by,
            order_by,
            frame: None,
        })
    }

    fn parse_data_type(&mut self) -> anyhow::Result<DataType> {
        let base = self.parse_data_type_base()?;
        // Handle [] suffix for array types (e.g., INTEGER[], VARCHAR(100)[])
        if matches!(self.peek(), Token::LBracket) {
            self.advance();
            self.expect(&Token::RBracket)?;
            Ok(DataType::Array(Box::new(base)))
        } else {
            Ok(base)
        }
    }

    fn parse_data_type_base(&mut self) -> anyhow::Result<DataType> {
        match self.peek() {
            Token::Keyword(k) => match k.to_uppercase().as_str() {
                "INT" | "INTEGER" => { self.advance(); Ok(DataType::Int) }
                "BIGINT" => { self.advance(); Ok(DataType::BigInt) }
                "SMALLINT" => { self.advance(); Ok(DataType::SmallInt) }
                "FLOAT" => { self.advance(); Ok(DataType::Float) }
                "DOUBLE" => {
                    self.advance();
                    self.expect_keyword("PRECISION")?;
                    Ok(DataType::Double)
                }
                "REAL" => { self.advance(); Ok(DataType::Float) }
                "NUMERIC" | "DECIMAL" => {
                    self.advance();
                    if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let precision = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(10);
                                self.advance();
                                v
                            }
                            _ => self.expect_ident()?.parse().unwrap_or(10),
                        };
                        self.expect(&Token::Comma)?;
                        let scale = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(0);
                                self.advance();
                                v
                            }
                            _ => self.expect_ident()?.parse().unwrap_or(0),
                        };
                        self.expect(&Token::RParen)?;
                        Ok(DataType::Numeric(precision, scale))
                    } else {
                        Ok(DataType::Numeric(10, 0))
                    }
                }
                "VARCHAR" => {
                    self.advance();
                    if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let len = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(255);
                                self.advance();
                                v
                            }
                            _ => self.expect_ident()?.parse().unwrap_or(255),
                        };
                        self.expect(&Token::RParen)?;
                        Ok(DataType::Varchar(len))
                    } else {
                        Ok(DataType::Varchar(255))
                    }
                }
                "CHAR" => {
                    self.advance();
                    if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let len = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(1);
                                self.advance();
                                v
                            }
                            _ => self.expect_ident()?.parse().unwrap_or(1),
                        };
                        self.expect(&Token::RParen)?;
                        Ok(DataType::Char(len))
                    } else {
                        Ok(DataType::Char(1))
                    }
                }
                "TEXT" => { self.advance(); Ok(DataType::Text) }
                "BOOLEAN" | "BOOL" => { self.advance(); Ok(DataType::Boolean) }
                "DATE" => { self.advance(); Ok(DataType::Date) }
                "TIME" => {
                    self.advance();
                    if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WITH") {
                        self.advance();
                        self.expect_keyword("TIME")?;
                        self.expect_keyword("ZONE")?;
                        Ok(DataType::TimeTz)
                    } else {
                        Ok(DataType::Time)
                    }
                }
                "TIMETZ" => { self.advance(); Ok(DataType::TimeTz) }
                "TIMESTAMP" => {
                    self.advance();
                    if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WITH") {
                        self.advance();
                        self.expect_keyword("TIME")?;
                        self.expect_keyword("ZONE")?;
                        Ok(DataType::TimestampTz)
                    } else {
                        Ok(DataType::Timestamp)
                    }
                }
                "TIMESTAMPTZ" => { self.advance(); Ok(DataType::TimestampTz) }
                "INTERVAL" => { self.advance(); Ok(DataType::Interval) }
                "JSON" => { self.advance(); Ok(DataType::Json) }
                "JSONB" => { self.advance(); Ok(DataType::JsonB) }
                "UUID" => { self.advance(); Ok(DataType::Uuid) }
                "SERIAL" => { self.advance(); Ok(DataType::Serial) }
                "BIGSERIAL" => { self.advance(); Ok(DataType::BigSerial) }
                "SMALLSERIAL" => { self.advance(); Ok(DataType::SmallSerial) }
                "MONEY" => { self.advance(); Ok(DataType::Money) }
                "INET" => { self.advance(); Ok(DataType::Inet) }
                "CIDR" => { self.advance(); Ok(DataType::Cidr) }
                "MACADDR" | "MACADDR8" => { self.advance(); Ok(DataType::MacAddr) }
                "BIT" => {
                    self.advance();
                    if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "VARYING") {
                        self.advance();
                        if matches!(self.peek(), Token::LParen) {
                            self.advance();
                            let len = match self.peek() {
                                Token::Number(n) => {
                                    let v = n.parse().unwrap_or(1);
                                    self.advance();
                                    v
                                }
                                _ => 1,
                            };
                            self.expect(&Token::RParen)?;
                            Ok(DataType::BitVarying(len))
                        } else {
                            Ok(DataType::BitVarying(1))
                        }
                    } else if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let len = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(1);
                                self.advance();
                                v
                            }
                            _ => 1,
                        };
                        self.expect(&Token::RParen)?;
                        Ok(DataType::Bit(len))
                    } else {
                        Ok(DataType::Bit(1))
                    }
                }
                "VARYING" => {
                    // BIT VARYING without preceding BIT - treat as error or parse as BIT VARYING
                    self.advance();
                    if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let len = match self.peek() {
                            Token::Number(n) => {
                                let v = n.parse().unwrap_or(1);
                                self.advance();
                                v
                            }
                            _ => 1,
                        };
                        self.expect(&Token::RParen)?;
                        Ok(DataType::BitVarying(len))
                    } else {
                        Ok(DataType::BitVarying(1))
                    }
                }
                "TSVECTOR" => { self.advance(); Ok(DataType::TsVector) }
                "TSQUERY" => { self.advance(); Ok(DataType::TsQuery) }
                "ARRAY" => {
                    self.advance();
                    self.expect(&Token::LBracket)?;
                    let elem_type = self.parse_data_type()?;
                    self.expect(&Token::RBracket)?;
                    Ok(DataType::Array(Box::new(elem_type)))
                }
                _ => {
                    let name = self.expect_ident()?;
                    let mut parts = vec![name];
                    while matches!(self.peek(), Token::Dot) {
                        self.advance();
                        parts.push(self.expect_ident()?);
                    }
                    Ok(DataType::Custom(parts))
                }
            },
            Token::Ident(s) => {
                let name = self.advance_ident(s.clone());
                let mut parts = vec![name];
                while matches!(self.peek(), Token::Dot) {
                    self.advance();
                    parts.push(self.expect_ident()?);
                }
                Ok(DataType::Custom(parts))
            }
            _ => anyhow::bail!("expected data type, got {:?}", self.peek()),
        }
    }

    fn advance_ident(&mut self, s: String) -> String {
        self.advance();
        s
    }
}

fn tokenize(sql: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            '(' => { tokens.push(Token::LParen); i += 1; }
            ')' => { tokens.push(Token::RParen); i += 1; }
            '[' => { tokens.push(Token::LBracket); i += 1; }
            ']' => { tokens.push(Token::RBracket); i += 1; }
            ',' => { tokens.push(Token::Comma); i += 1; }
            '.' => { tokens.push(Token::Dot); i += 1; }
            ';' => { tokens.push(Token::Semicolon); i += 1; }
            ':' => { tokens.push(Token::Colon); i += 1; }
            '+' => { tokens.push(Token::Plus); i += 1; }
            '*' => { tokens.push(Token::Star); i += 1; }
            '/' => { tokens.push(Token::Slash); i += 1; }
            '%' => { tokens.push(Token::Percent); i += 1; }
            '&' => { tokens.push(Token::Ampersand); i += 1; }
            '|' => { tokens.push(Token::Pipe); i += 1; }
            '^' => { tokens.push(Token::Caret); i += 1; }
            '~' => { tokens.push(Token::Tilde); i += 1; }
            '?' => { tokens.push(Token::Question); i += 1; }
            '-' => {
                if i + 1 < chars.len() && chars[i + 1] == '-' {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '<' => {
                if i + 1 < chars.len() {
                    match chars[i + 1] {
                        '>' => { tokens.push(Token::Neq); i += 2; }
                        '=' => { tokens.push(Token::LtEq); i += 2; }
                        '<' => { tokens.push(Token::LtLt); i += 2; }
                        _ => { tokens.push(Token::Lt); i += 1; }
                    }
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() {
                    match chars[i + 1] {
                        '=' => { tokens.push(Token::GtEq); i += 2; }
                        '>' => { tokens.push(Token::GtGt); i += 2; }
                        _ => { tokens.push(Token::Gt); i += 1; }
                    }
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Neq);
                    i += 2;
                } else {
                    tokens.push(Token::Bang);
                    i += 1;
                }
            }
            '\'' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\'' && i + 1 < chars.len() && chars[i + 1] == '\'' {
                        s.push('\'');
                        i += 2;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token::String(s));
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    s.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token::Ident(s));
            }
            _ if chars[i].is_ascii_digit() => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    s.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Number(s));
            }
            _ if chars[i].is_ascii_alphabetic() || chars[i] == '_' => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    s.push(chars[i]);
                    i += 1;
                }
                let upper = s.to_uppercase();
                match upper.as_str() {
                    "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "DROP" | "ALTER" |
                    "TABLE" | "INDEX" | "VIEW" | "PRIMARY" | "KEY" | "FOREIGN" | "REFERENCES" |
                    "CONSTRAINT" | "NOT" | "NULL" | "DEFAULT" | "CHECK" | "UNIQUE" |
                    "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "FLOAT" | "DOUBLE" | "PRECISION" |
                    "REAL" | "NUMERIC" | "DECIMAL" | "VARCHAR" | "CHAR" | "TEXT" | "BOOLEAN" | "BOOL" |
                    "DATE" | "TIME" | "TIMETZ" | "TIMESTAMP" | "TIMESTAMPTZ" | "INTERVAL" |
                    "JSON" | "JSONB" | "UUID" | "ARRAY" |
                    "SERIAL" | "BIGSERIAL" | "SMALLSERIAL" | "MONEY" |
                    "INET" | "CIDR" | "MACADDR" | "MACADDR8" |
                    "BIT" | "VARYING" | "TSVECTOR" | "TSQUERY" |
                    "FROM" | "WHERE" | "JOIN" | "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" |
                    "ON" | "USING" | "AS" | "AND" | "OR" | "IN" | "BETWEEN" | "LIKE" | "ILIKE" | "INTO" |
                    "IS" | "EXISTS" | "ANY" | "ALL" | "SOME" |
                    "ORDER" | "BY" | "ASC" | "DESC" | "NULLS" | "FIRST" | "LAST" |
                    "GROUP" | "HAVING" | "LIMIT" | "OFFSET" |
                    "DISTINCT" | "RETURNING" | "VALUES" | "SET" |
                    "UNION" | "INTERSECT" | "EXCEPT" | "CONFLICT" | "NOTHING" | "DO" | "EXCLUDED" |
                    "BEGIN" | "START" | "TRANSACTION" | "COMMIT" | "ROLLBACK" | "ABORT" |
                    "ISOLATION" | "LEVEL" | "READ" | "WRITE" | "SERIALIZABLE" |
                    "REPEATABLE" | "COMMITTED" | "UNCOMMITTED" |
                    "DEFERRABLE" | "EXPLAIN" |
                    "CASCADE" | "RESTRICT" | "IF" |
                    "TRUE" | "FALSE" | "CASE" | "WHEN" | "THEN" | "ELSE" | "END" |
                    "OVER" | "PARTITION" | "FILTER" | "ROWS" | "RANGE" | "GROUPS" |
                    "CURRENT" | "ROW" | "PRECEDING" | "FOLLOWING" | "UNBOUNDED" |
                    "ADD" | "COLUMN" | "RENAME" | "TO" | "DATA" | "TYPE" |
                    "WINDOW" | "FETCH" | "NEXT" | "ONLY" |
                    "WITH" | "RECURSIVE" | "MATERIALIZED" | "UNMATERIALIZED" |
                    "FORCE" | "ENABLE" | "DISABLE" | "SECURITY" |
                    "GRANT" | "REVOKE" | "ROLE" | "ADMIN" | "OPTION" | "PUBLIC" |
                    "TRIGGER" | "FUNCTION" | "PROCEDURE" | "RETURNS" | "RETURN" |
                    "DECLARE" | "LOOP" | "WHILE" | "FOR" | "REVERSE" |
                    "EXIT" | "CONTINUE" | "ELSIF" | "PERFORM" | "EXECUTE" |
                    "RAISE" | "NOTICE" | "EXCEPTION" | "WARNING" | "INFO" | "DEBUG" |
                    "LOG" | "ASSERT" | "FOUND" | "NEW" | "OLD" | "TG_OP" | "TG_TABLE_NAME" |
                    "TG_TABLE_SCHEMA" | "TG_WHEN" | "TG_LEVEL" | "TG_ARGV" |
                    "CAST" |
                    "NOW" | "CURRENT_DATE" | "CURRENT_TIME" | "CURRENT_TIMESTAMP" |
                    "EXTRACT" | "DATE_TRUNC" | "DATE_PART" | "AT" | "ZONE" |
                    "YEAR" | "MONTH" | "DAY" | "HOUR" | "MINUTE" | "SECOND" |
                    "MILLISECOND" | "MICROSECOND" | "DOW" | "DOY" | "ISODOW" |
                    "WEEK" | "QUARTER" | "EPOCH" | "ISOYEAR" |
                    "TIMEZONE" | "TIMEZONE_HOUR" | "TIMEZONE_MINUTE" |
                    "LOCALTIME" | "LOCALTIMESTAMP" |
                    "GREATEST" | "LEAST" | "COALESCE" | "NULLIF" |
                    "ABS" | "CEIL" | "FLOOR" | "ROUND" | "TRUNC" | "SIGN" | "POWER" | "SQRT" | "LN" | "EXP" |
                    "LENGTH" | "UPPER" | "LOWER" | "TRIM" | "LTRIM" | "RTRIM" |
                    "SUBSTRING" | "REPLACE" | "CONCAT" | "CONCAT_WS" |
                    "INITCAP" | "POSITION" | "STRPOS" | "OVERLAY" |
                    "LPAD" | "RPAD" | "REGEXP_REPLACE" | "REGEXP_MATCHES" => {
                        tokens.push(Token::Keyword(s));
                    }
                    _ => {
                        tokens.push(Token::Ident(s));
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    tokens.push(Token::Eof);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
