use crate::sql::ast::*;
use super::{Parser, Token};

impl Parser {
    pub(crate) fn parse_create(&mut self) -> anyhow::Result<Statement> {
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
                let or_replace = false;
                self.advance();
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
            Token::Keyword(k) if k.to_uppercase() == "MATERIALIZED" => {
                self.advance();
                self.expect_keyword("VIEW")?;
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

                Ok(Statement::CreateMaterializedView(CreateMaterializedViewStatement {
                    name,
                    columns,
                    query: Box::new(query),
                    or_replace,
                }))
            }
            Token::Keyword(k) if k.to_uppercase() == "SEQUENCE" => {
                self.advance();
                self.parse_create_sequence()
            }
            Token::Keyword(k) if k.to_uppercase() == "TYPE" => {
                self.advance();
                self.parse_create_type()
            }
            Token::Keyword(k) if k.to_uppercase() == "SCHEMA" => {
                self.advance();
                self.parse_create_schema()
            }
            _ => anyhow::bail!("expected TABLE, INDEX, VIEW, MATERIALIZED VIEW, SEQUENCE, TYPE, or SCHEMA after CREATE"),
        }
    }

    pub(crate) fn parse_column_def(&mut self) -> anyhow::Result<ColumnDef> {
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

    pub(crate) fn parse_table_constraint(&mut self, constraints: &mut Vec<TableConstraint>) -> anyhow::Result<()> {
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

    pub(crate) fn parse_alter(&mut self) -> anyhow::Result<Statement> {
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

    pub(crate) fn parse_drop(&mut self) -> anyhow::Result<Statement> {
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

    pub(crate) fn parse_merge(&mut self) -> anyhow::Result<MergeStatement> {
        self.expect_keyword("MERGE")?;
        self.expect_keyword("INTO")?;

        let mut target_parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            target_parts.push(self.expect_ident()?);
        }
        let target = ObjectName::new(target_parts);

        // Optional alias
        let _alias = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AS") {
            self.advance();
            Some(self.expect_ident()?)
        } else if matches!(self.peek(), Token::Ident(_)) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect_keyword("USING")?;

        // Source can be a table or subquery
        let source = if matches!(self.peek(), Token::LParen) {
            self.advance();
            let query = self.parse_select()?;
            self.expect(&Token::RParen)?;
            MergeSource::Subquery(Box::new(query))
        } else {
            let mut source_parts = vec![self.expect_ident()?];
            while matches!(self.peek(), Token::Dot) {
                self.advance();
                source_parts.push(self.expect_ident()?);
            }
            MergeSource::Table(ObjectName::new(source_parts))
        };

        // Optional source alias
        if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AS") {
            self.advance();
            self.advance();
        } else if matches!(self.peek(), Token::Ident(_)) {
            self.advance();
        }

        self.expect_keyword("ON")?;
        let join_condition = Box::new(self.parse_expr()?);

        let mut clauses = Vec::new();
        loop {
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHEN") {
                self.advance();
                let not_matched = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NOT");
                if not_matched {
                    self.advance();
                    self.expect_keyword("MATCHED")?;
                } else {
                    self.expect_keyword("MATCHED")?;
                }

                let condition = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AND") {
                    self.advance();
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };

                self.expect_keyword("THEN")?;

                let action = self.parse_merge_action(not_matched)?;

                if not_matched {
                    clauses.push(MergeClause::WhenNotMatched { condition, action });
                } else {
                    clauses.push(MergeClause::WhenMatched { condition, action });
                }
            } else {
                break;
            }
        }

        Ok(MergeStatement {
            target,
            source,
            join_condition,
            clauses,
        })
    }

    fn parse_merge_action(&mut self, _not_matched: bool) -> anyhow::Result<MergeAction> {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "UPDATE" => {
                self.advance();
                self.expect_keyword("SET")?;
                let mut set_clauses = Vec::new();
                loop {
                    let mut column = self.expect_ident()?;
                    while matches!(self.peek(), Token::Dot) {
                        self.advance();
                        column.push('.');
                        column.push_str(&self.expect_ident()?);
                    }
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
                Ok(MergeAction::Update { set_clauses })
            }
            Token::Keyword(k) if k.to_uppercase() == "DELETE" => {
                self.advance();
                Ok(MergeAction::Delete)
            }
            Token::Keyword(k) if k.to_uppercase() == "INSERT" => {
                self.advance();
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

                self.expect_keyword("VALUES")?;
                self.expect(&Token::LParen)?;
                let mut values = Vec::new();
                loop {
                    let mut row = Vec::new();
                    loop {
                        row.push(self.parse_expr()?);
                        if !matches!(self.peek(), Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                    values.push(row);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;

                Ok(MergeAction::Insert {
                    columns,
                    source: MergeInsertSource::Values(values),
                })
            }
            Token::Keyword(k) if k.to_uppercase() == "DO" => {
                self.advance();
                self.expect_keyword("NOTHING")?;
                Ok(MergeAction::DoNothing)
            }
            _ => anyhow::bail!("expected UPDATE, DELETE, INSERT, or DO NOTHING in MERGE action"),
        }
    }

    pub(crate) fn parse_create_sequence(&mut self) -> anyhow::Result<Statement> {
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

        let mut data_type = None;
        let mut increment = None;
        let mut min_value = None;
        let mut max_value = None;
        let mut start = None;
        let mut cache = None;
        let mut cycle = false;
        let mut owned_by = None;

        loop {
            match self.peek() {
                Token::Keyword(k) if k.to_uppercase() == "AS" => {
                    self.advance();
                    data_type = Some(self.parse_data_type()?);
                }
                Token::Keyword(k) if k.to_uppercase() == "INCREMENT" => {
                    self.advance();
                    self.expect_keyword("BY")?;
                    let negative = matches!(self.peek(), Token::Minus);
                    if negative {
                        self.advance();
                    }
                    let n: i64 = match &self.peek().clone() {
                        Token::Number(n) => {
                            let v: i64 = n.parse().unwrap_or(1);
                            self.advance();
                            v
                        }
                        _ => 1,
                    };
                    increment = Some(if negative { -n } else { n });
                }
                Token::Keyword(k) if k.to_uppercase() == "MINVALUE" || k.to_uppercase() == "MINVALUE" => {
                    self.advance();
                    let n: i64 = match &self.peek().clone() {
                        Token::Number(n) => {
                            let v: i64 = n.parse().unwrap_or(1);
                            self.advance();
                            v
                        }
                        _ => 1,
                    };
                    min_value = Some(n);
                }
                Token::Keyword(k) if k.to_uppercase() == "MAXVALUE" || k.to_uppercase() == "MAXVALUE" => {
                    self.advance();
                    let n: i64 = match &self.peek().clone() {
                        Token::Number(n) => {
                            let v: i64 = n.parse().unwrap_or(i64::MAX);
                            self.advance();
                            v
                        }
                        _ => i64::MAX,
                    };
                    max_value = Some(n);
                }
                Token::Keyword(k) if k.to_uppercase() == "START" => {
                    self.advance();
                    let with = matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WITH");
                    if with {
                        self.advance();
                    }
                    let n: i64 = match &self.peek().clone() {
                        Token::Number(n) => {
                            let v: i64 = n.parse().unwrap_or(1);
                            self.advance();
                            v
                        }
                        _ => 1,
                    };
                    start = Some(n);
                }
                Token::Keyword(k) if k.to_uppercase() == "CACHE" => {
                    self.advance();
                    let n: i64 = match &self.peek().clone() {
                        Token::Number(n) => {
                            let v: i64 = n.parse().unwrap_or(1);
                            self.advance();
                            v
                        }
                        _ => 1,
                    };
                    cache = Some(n);
                }
                Token::Keyword(k) if k.to_uppercase() == "CYCLE" => {
                    self.advance();
                    cycle = true;
                }
                Token::Keyword(k) if k.to_uppercase() == "NO" => {
                    self.advance();
                    // NO CYCLE, NO MINVALUE, NO MAXVALUE, NO CACHE
                    match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "CYCLE" => {
                            self.advance();
                            cycle = false;
                        }
                        Token::Keyword(k) if k.to_uppercase() == "MINVALUE" => {
                            self.advance();
                            min_value = None;
                        }
                        Token::Keyword(k) if k.to_uppercase() == "MAXVALUE" => {
                            self.advance();
                            max_value = None;
                        }
                        Token::Keyword(k) if k.to_uppercase() == "CACHE" => {
                            self.advance();
                            cache = None;
                        }
                        _ => {}
                    }
                }
                Token::Keyword(k) if k.to_uppercase() == "OWNED" => {
                    self.advance();
                    self.expect_keyword("BY")?;
                    let mut owned_parts = vec![self.expect_ident()?];
                    while matches!(self.peek(), Token::Dot) {
                        self.advance();
                        owned_parts.push(self.expect_ident()?);
                    }
                    owned_by = Some(ObjectName::new(owned_parts));
                }
                _ => break,
            }
        }

        Ok(Statement::CreateSequence(CreateSequenceStatement {
            name,
            if_not_exists,
            data_type,
            increment,
            min_value,
            max_value,
            start,
            cache,
            cycle,
            owned_by,
        }))
    }

    pub(crate) fn parse_create_type(&mut self) -> anyhow::Result<Statement> {
        let mut parts = vec![self.expect_ident()?];
        while matches!(self.peek(), Token::Dot) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        let name = ObjectName::new(parts);

        self.expect_keyword("AS")?;

        let definition = match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "ENUM" => {
                self.advance();
                self.expect(&Token::LParen)?;
                let mut values = Vec::new();
                loop {
                    values.push(self.expect_string()?);
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                TypeDefinition::Enum(values)
            }
            Token::Keyword(k) if k.to_uppercase() == "RANGE" => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    // SUBTYPE may be an ident or keyword
                    match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "SUBTYPE" => { self.advance(); }
                        Token::Ident(k) if k.to_uppercase() == "SUBTYPE" => { self.advance(); }
                        _ => anyhow::bail!("expected SUBTYPE"),
                    }
                    self.expect(&Token::Eq)?;
                    let subtype = self.parse_data_type()?;
                    self.expect(&Token::RParen)?;
                    TypeDefinition::Range(subtype)
                } else {
                    match self.peek() {
                        Token::Keyword(k) if k.to_uppercase() == "SUBTYPE" => { self.advance(); }
                        Token::Ident(k) if k.to_uppercase() == "SUBTYPE" => { self.advance(); }
                        _ => anyhow::bail!("expected SUBTYPE"),
                    }
                    self.expect(&Token::Eq)?;
                    let subtype = self.parse_data_type()?;
                    TypeDefinition::Range(subtype)
                }
            }
            Token::LParen => {
                // Composite type
                self.advance();
                let mut attrs = Vec::new();
                loop {
                    let attr_name = self.expect_ident()?;
                    let data_type = self.parse_data_type()?;
                    let collation = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "COLLATE") {
                        self.advance();
                        let mut collation_parts = vec![self.expect_ident()?];
                        while matches!(self.peek(), Token::Dot) {
                            self.advance();
                            collation_parts.push(self.expect_ident()?);
                        }
                        Some(ObjectName::new(collation_parts))
                    } else {
                        None
                    };
                    attrs.push(CompositeAttribute {
                        name: attr_name,
                        data_type,
                        collation,
                    });
                    if !matches!(self.peek(), Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RParen)?;
                TypeDefinition::Composite(attrs)
            }
            _ => anyhow::bail!("expected ENUM, RANGE, or ( after CREATE TYPE ... AS"),
        };

        Ok(Statement::CreateType(CreateTypeStatement {
            name,
            definition,
        }))
    }

    pub(crate) fn parse_create_schema(&mut self) -> anyhow::Result<Statement> {
        let if_not_exists = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "IF") {
            self.advance();
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            true
        } else {
            false
        };

        let mut name = None;
        let mut authorization = None;

        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "AUTHORIZATION" => {
                self.advance();
                authorization = Some(self.expect_ident()?);
            }
            Token::Ident(_) | Token::Keyword(_) => {
                name = Some(self.expect_ident()?);
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "AUTHORIZATION") {
                    self.advance();
                    authorization = Some(self.expect_ident()?);
                }
            }
            _ => {}
        }

        Ok(Statement::CreateSchema(CreateSchemaStatement {
            name,
            if_not_exists,
            authorization,
        }))
    }
}
