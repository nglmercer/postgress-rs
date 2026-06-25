use super::{Parser, Token};
use crate::sql::ast::*;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> anyhow::Result<Expr> {
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ANY" || k.to_uppercase() == "SOME")
                {
                    let is_some =
                        matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "SOME");
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

        while matches!(self.peek(), Token::Colon) {
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
                self.pos = saved_pos;
                break;
            }
        }

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
                let operand = if !matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "WHEN")
                {
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

                let else_clause = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "ELSE")
                {
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
                let field = field_str
                    .parse::<DatePart>()
                    .map_err(|_| anyhow::anyhow!("unknown date part: {}", field_str))?;
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
                let field = field_str
                    .parse::<DatePart>()
                    .map_err(|_| anyhow::anyhow!("unknown date part: {}", field_str))?;
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
                let field = field_str
                    .parse::<DatePart>()
                    .map_err(|_| anyhow::anyhow!("unknown date part: {}", field_str))?;
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

                    let filter = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "FILTER")
                    {
                        self.advance();
                        self.expect(&Token::LParen)?;
                        self.expect_keyword("WHERE")?;
                        let expr = self.parse_expr()?;
                        self.expect(&Token::RParen)?;
                        Some(Box::new(expr))
                    } else {
                        None
                    };

                    let over = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "OVER")
                    {
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
                    Ok(Expr::QualifiedIdentifier {
                        table: name,
                        column,
                    })
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

    pub(crate) fn parse_window_spec(&mut self) -> anyhow::Result<WindowSpec> {
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
                order_by.push(OrderByItem {
                    expr,
                    direction,
                    nulls,
                });
                if !matches!(self.peek(), Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        // Parse frame clause: ROWS|RANGE|GROUPS ...
        let frame = if matches!(
            self.peek(),
            Token::Keyword(k)
                if k.to_uppercase() == "ROWS"
                    || k.to_uppercase() == "RANGE"
                    || k.to_uppercase() == "GROUPS"
        ) {
            let _frame_type = self.advance();
            let is_between =
                matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "BETWEEN");
            if is_between {
                self.advance();
                let start = self.parse_frame_bound()?;
                self.expect_keyword("AND")?;
                let end = Some(self.parse_frame_bound()?);
                Some(Box::new(FrameClause { start, end }))
            } else {
                let start = self.parse_frame_bound()?;
                // Single bound: end defaults to CURRENT ROW
                let end = Some(Box::new(FrameBound::CurrentRow));
                Some(Box::new(FrameClause { start, end }))
            }
        } else {
            None
        };

        self.expect(&Token::RParen)?;

        Ok(WindowSpec {
            partition_by,
            order_by,
            frame,
        })
    }

    fn parse_frame_bound(&mut self) -> anyhow::Result<Box<FrameBound>> {
        match self.peek() {
            Token::Keyword(k) if k.to_uppercase() == "UNBOUNDED" => {
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "PRECEDING") {
                    self.advance();
                    Ok(Box::new(FrameBound::UnboundedPreceding))
                } else {
                    self.expect_keyword("FOLLOWING")?;
                    Ok(Box::new(FrameBound::UnboundedFollowing))
                }
            }
            Token::Keyword(k) if k.to_uppercase() == "CURRENT" => {
                self.advance();
                self.expect_keyword("ROW")?;
                Ok(Box::new(FrameBound::CurrentRow))
            }
            _ => {
                let expr = self.parse_expr()?;
                if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "PRECEDING") {
                    self.advance();
                    Ok(Box::new(FrameBound::Preceding(Box::new(expr))))
                } else {
                    self.expect_keyword("FOLLOWING")?;
                    Ok(Box::new(FrameBound::Following(Box::new(expr))))
                }
            }
        }
    }
}
