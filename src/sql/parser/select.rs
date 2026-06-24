use crate::sql::ast::*;
use super::{Parser, Token};

impl Parser {
    pub(crate) fn parse_select(&mut self) -> anyhow::Result<SelectStatement> {
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

    pub(crate) fn parse_select_body(&mut self) -> anyhow::Result<SelectStatement> {
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            let stmt = self.parse_select()?;
            self.expect(&Token::RParen)?;
            Ok(stmt)
        } else {
            self.parse_select()
        }
    }

    pub(crate) fn parse_with_select(&mut self) -> anyhow::Result<SelectStatement> {
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

    pub(crate) fn parse_cte(&mut self) -> anyhow::Result<CommonTableExpr> {
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

        let materialized = if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "MATERIALIZED") {
            self.advance();
            Some(true)
        } else if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "NOT") {
            let saved_pos = self.pos;
            self.advance();
            if matches!(self.peek(), Token::Keyword(k) if k.to_uppercase() == "MATERIALIZED") {
                self.advance();
                Some(false)
            } else {
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

    pub(crate) fn parse_select_list(&mut self) -> anyhow::Result<Vec<SelectItem>> {
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

    pub(crate) fn parse_from_clause(&mut self) -> anyhow::Result<FromClause> {
        let mut joins = Vec::new();

        let table = self.parse_table_ref()?;

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

    pub(crate) fn parse_table_ref(&mut self) -> anyhow::Result<TableRef> {
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

    pub(crate) fn parse_join_type(&mut self) -> anyhow::Result<JoinType> {
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
}
