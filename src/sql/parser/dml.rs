use crate::sql::ast::*;
use super::{Parser, Token};

impl Parser {
    pub(crate) fn parse_insert(&mut self) -> anyhow::Result<InsertStatement> {
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

    pub(crate) fn parse_update(&mut self) -> anyhow::Result<UpdateStatement> {
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

    pub(crate) fn parse_delete(&mut self) -> anyhow::Result<DeleteStatement> {
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
}
