use crate::protocol::codes::Query;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PreparedStatement {
    pub name: String,
    pub sql: String,
    pub query: Query,
    pub parameter_types: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Portal {
    pub name: String,
    pub statement_name: String,
    pub query: Query,
    pub parameter_values: Vec<Option<Vec<u8>>>,
    pub result_formats: Vec<i16>,
}

pub struct ExtendedQueryState {
    statements: HashMap<String, PreparedStatement>,
    portals: HashMap<String, Portal>,
    unnamed_statement: Option<PreparedStatement>,
    unnamed_portal: Option<Portal>,
}

impl ExtendedQueryState {
    pub fn new() -> Self {
        Self {
            statements: HashMap::new(),
            portals: HashMap::new(),
            unnamed_statement: None,
            unnamed_portal: None,
        }
    }

    pub fn prepare(
        &mut self,
        name: &str,
        sql: &str,
        parameter_types: Vec<u32>,
    ) -> anyhow::Result<()> {
        let mut parser = crate::protocol::parser::Parser::new();
        let query = parser.feed(sql.as_bytes())
            .ok_or_else(|| anyhow::anyhow!("Failed to parse SQL: {}", sql))?;

        let stmt = PreparedStatement {
            name: name.to_string(),
            sql: sql.to_string(),
            query,
            parameter_types,
        };

        if name.is_empty() {
            self.unnamed_statement = Some(stmt);
        } else {
            self.statements.insert(name.to_string(), stmt);
        }
        Ok(())
    }

    pub fn bind(
        &mut self,
        portal_name: &str,
        statement_name: &str,
        parameter_values: Vec<Option<Vec<u8>>>,
        result_formats: Vec<i16>,
    ) -> anyhow::Result<()> {
        let stmt = if statement_name.is_empty() {
            self.unnamed_statement.as_ref()
                .ok_or_else(|| anyhow::anyhow!("No unnamed prepared statement"))?
                .clone()
        } else {
            self.statements.get(statement_name)
                .ok_or_else(|| anyhow::anyhow!("Prepared statement \"{}\" not found", statement_name))?
                .clone()
        };

        let portal = Portal {
            name: portal_name.to_string(),
            statement_name: statement_name.to_string(),
            query: stmt.query.clone(),
            parameter_values,
            result_formats,
        };

        if portal_name.is_empty() {
            self.unnamed_portal = Some(portal);
        } else {
            self.portals.insert(portal_name.to_string(), portal);
        }
        Ok(())
    }

    pub fn get_portal(&self, name: &str) -> Option<&Portal> {
        if name.is_empty() {
            self.unnamed_portal.as_ref()
        } else {
            self.portals.get(name)
        }
    }

    pub fn close_statement(&mut self, name: &str) {
        if name.is_empty() {
            self.unnamed_statement = None;
        } else {
            self.statements.remove(name);
        }
    }

    pub fn close_portal(&mut self, name: &str) {
        if name.is_empty() {
            self.unnamed_portal = None;
        } else {
            self.portals.remove(name);
        }
    }

    pub fn close_all(&mut self) {
        self.statements.clear();
        self.portals.clear();
        self.unnamed_statement = None;
        self.unnamed_portal = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_and_bind() {
        let mut state = ExtendedQueryState::new();
        state.prepare("stmt1", "SELECT * FROM users WHERE id = 1;\n", vec![]).unwrap();
        state.bind("portal1", "stmt1", vec![], vec![]).unwrap();
        assert!(state.get_portal("portal1").is_some());
    }

    #[test]
    fn test_prepare_unnamed() {
        let mut state = ExtendedQueryState::new();
        state.prepare("", "INSERT INTO users VALUES (1);\n", vec![]).unwrap();
        state.bind("", "", vec![], vec![]).unwrap();
        assert!(state.get_portal("").is_some());
    }

    #[test]
    fn test_close_statement() {
        let mut state = ExtendedQueryState::new();
        state.prepare("stmt1", "SELECT * FROM users;\n", vec![]).unwrap();
        state.close_statement("stmt1");
        assert!(state.bind("p", "stmt1", vec![], vec![]).is_err());
    }

    #[test]
    fn test_close_portal() {
        let mut state = ExtendedQueryState::new();
        state.prepare("s", "SELECT * FROM users;\n", vec![]).unwrap();
        state.bind("p", "s", vec![], vec![]).unwrap();
        state.close_portal("p");
        assert!(state.get_portal("p").is_none());
    }

    #[test]
    fn test_close_all() {
        let mut state = ExtendedQueryState::new();
        state.prepare("s1", "SELECT * FROM users;\n", vec![]).unwrap();
        state.prepare("s2", "SELECT * FROM orders;\n", vec![]).unwrap();
        state.bind("p1", "s1", vec![], vec![]).unwrap();
        state.close_all();
        assert!(state.get_portal("p1").is_none());
    }

    #[test]
    fn test_bind_nonexistent_statement() {
        let mut state = ExtendedQueryState::new();
        assert!(state.bind("p", "no_such", vec![], vec![]).is_err());
    }

    #[test]
    fn test_get_portal_empty_name() {
        let mut state = ExtendedQueryState::new();
        assert!(state.get_portal("").is_none());
        state.prepare("", "SELECT * FROM users;\n", vec![]).unwrap();
        state.bind("", "", vec![], vec![]).unwrap();
        assert!(state.get_portal("").is_some());
    }

    #[test]
    fn test_multiple_portals() {
        let mut state = ExtendedQueryState::new();
        state.prepare("s1", "SELECT * FROM users;\n", vec![]).unwrap();
        state.prepare("s2", "SELECT * FROM orders;\n", vec![]).unwrap();
        state.bind("p1", "s1", vec![], vec![]).unwrap();
        state.bind("p2", "s2", vec![], vec![]).unwrap();
        assert!(state.get_portal("p1").is_some());
        assert!(state.get_portal("p2").is_some());
    }
}
