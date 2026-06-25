use super::{Parser, Token};
use crate::sql::ast::*;

impl Parser {
    pub(crate) fn parse_begin(&mut self) -> anyhow::Result<BeginStatement> {
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
                Token::Keyword(k) if k.to_uppercase() == "WRITE" => {
                    self.advance();
                    read_only = true;
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
}
