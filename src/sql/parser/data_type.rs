use super::{Parser, Token};
use crate::sql::ast::*;

impl Parser {
    pub(crate) fn parse_data_type(&mut self) -> anyhow::Result<DataType> {
        let base = self.parse_data_type_base()?;
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
                "INT" | "INTEGER" => {
                    self.advance();
                    Ok(DataType::Int)
                }
                "BIGINT" => {
                    self.advance();
                    Ok(DataType::BigInt)
                }
                "SMALLINT" => {
                    self.advance();
                    Ok(DataType::SmallInt)
                }
                "FLOAT" => {
                    self.advance();
                    Ok(DataType::Float)
                }
                "DOUBLE" => {
                    self.advance();
                    self.expect_keyword("PRECISION")?;
                    Ok(DataType::Double)
                }
                "REAL" => {
                    self.advance();
                    Ok(DataType::Float)
                }
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
                "TEXT" => {
                    self.advance();
                    Ok(DataType::Text)
                }
                "BOOLEAN" | "BOOL" => {
                    self.advance();
                    Ok(DataType::Boolean)
                }
                "DATE" => {
                    self.advance();
                    Ok(DataType::Date)
                }
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
                "TIMETZ" => {
                    self.advance();
                    Ok(DataType::TimeTz)
                }
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
                "TIMESTAMPTZ" => {
                    self.advance();
                    Ok(DataType::TimestampTz)
                }
                "INTERVAL" => {
                    self.advance();
                    Ok(DataType::Interval)
                }
                "JSON" => {
                    self.advance();
                    Ok(DataType::Json)
                }
                "JSONB" => {
                    self.advance();
                    Ok(DataType::JsonB)
                }
                "UUID" => {
                    self.advance();
                    Ok(DataType::Uuid)
                }
                "SERIAL" => {
                    self.advance();
                    Ok(DataType::Serial)
                }
                "BIGSERIAL" => {
                    self.advance();
                    Ok(DataType::BigSerial)
                }
                "SMALLSERIAL" => {
                    self.advance();
                    Ok(DataType::SmallSerial)
                }
                "MONEY" => {
                    self.advance();
                    Ok(DataType::Money)
                }
                "INET" => {
                    self.advance();
                    Ok(DataType::Inet)
                }
                "CIDR" => {
                    self.advance();
                    Ok(DataType::Cidr)
                }
                "MACADDR" | "MACADDR8" => {
                    self.advance();
                    Ok(DataType::MacAddr)
                }
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
                "TSVECTOR" => {
                    self.advance();
                    Ok(DataType::TsVector)
                }
                "TSQUERY" => {
                    self.advance();
                    Ok(DataType::TsQuery)
                }
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
}
