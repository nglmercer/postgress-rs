mod token;
mod select;
mod dml;
mod ddl;
mod expr;
mod data_type;
mod transaction;

use crate::sql::ast::*;
pub(crate) use token::{Token, tokenize};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
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

    pub(crate) fn expect_keyword(&mut self, keyword: &str) -> anyhow::Result<()> {
        match self.advance() {
            Token::Keyword(k) if k.to_uppercase() == keyword.to_uppercase() => Ok(()),
            other => anyhow::bail!("expected keyword '{}', got {:?}", keyword, other),
        }
    }

    pub(crate) fn expect_ident(&mut self) -> anyhow::Result<String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            Token::Keyword(k) => Ok(k),
            other => anyhow::bail!("expected identifier, got {:?}", other),
        }
    }

    pub(crate) fn expect_string(&mut self) -> anyhow::Result<String> {
        match self.advance() {
            Token::String(s) => Ok(s),
            other => anyhow::bail!("expected string literal, got {:?}", other),
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
                "MERGE" => Ok(Statement::Merge(self.parse_merge()?)),
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

    pub(crate) fn advance_ident(&mut self, s: String) -> String {
        self.advance();
        s
    }
}

#[cfg(test)]
mod tests;
