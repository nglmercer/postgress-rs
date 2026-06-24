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
}
