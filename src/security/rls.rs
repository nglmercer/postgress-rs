use crate::sql::ast::Expr;
use crate::types::Oid;

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyCommand {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyPermissive {
    Permissive,
    Restrictive,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub policy_name: String,
    pub table_oid: Oid,
    pub command: PolicyCommand,
    pub permissive: PolicyPermissive,
    pub roles: Vec<String>,
    pub using_expr: Option<Expr>,
    pub check_expr: Option<Expr>,
}

impl Policy {
    pub fn new(name: &str, table_oid: Oid, command: PolicyCommand) -> Self {
        Self {
            policy_name: name.to_string(),
            table_oid,
            command,
            permissive: PolicyPermissive::Permissive,
            roles: vec![],
            using_expr: None,
            check_expr: None,
        }
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_using(mut self, expr: Expr) -> Self {
        self.using_expr = Some(expr);
        self
    }

    pub fn with_check(mut self, expr: Expr) -> Self {
        self.check_expr = Some(expr);
        self
    }

    pub fn applies_to_command(&self, cmd: &PolicyCommand) -> bool {
        self.command == PolicyCommand::All || self.command == *cmd
    }

    pub fn applies_to_role(&self, role: &str) -> bool {
        self.roles.is_empty() || self.roles.iter().any(|r| r == role)
    }
}

pub struct RlsFilter {
    pub policies: Vec<Policy>,
    pub command: PolicyCommand,
}

impl RlsFilter {
    pub fn new(command: PolicyCommand) -> Self {
        Self {
            policies: Vec::new(),
            command,
        }
    }

    pub fn add_policy(&mut self, policy: Policy) {
        if policy.applies_to_command(&self.command) {
            self.policies.push(policy);
        }
    }

    pub fn has_policies(&self) -> bool {
        !self.policies.is_empty()
    }

    pub fn get_using_expressions(&self) -> Vec<&Expr> {
        self.policies
            .iter()
            .filter_map(|p| p.using_expr.as_ref())
            .collect()
    }

    pub fn get_check_expressions(&self) -> Vec<&Expr> {
        self.policies
            .iter()
            .filter_map(|p| p.check_expr.as_ref())
            .collect()
    }

    pub fn combine_using_expressions(&self) -> Option<Expr> {
        let exprs = self.get_using_expressions();
        if exprs.is_empty() {
            return None;
        }
        if exprs.len() == 1 {
            return Some(exprs[0].clone());
        }

        let mut result = exprs[0].clone();
        for expr in &exprs[1..] {
            result = Expr::BinaryOp {
                left: Box::new(result),
                op: crate::sql::ast::BinaryOperator::And,
                right: Box::new((*expr).clone()),
            };
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::{BinaryOperator, Expr};

    #[test]
    fn test_policy_new() {
        let policy = Policy::new("test_policy", Oid(1), PolicyCommand::Select);
        assert_eq!(policy.policy_name, "test_policy");
        assert_eq!(policy.command, PolicyCommand::Select);
    }

    #[test]
    fn test_policy_applies_to_command() {
        let policy = Policy::new("all_policy", Oid(1), PolicyCommand::All);
        assert!(policy.applies_to_command(&PolicyCommand::Select));
        assert!(policy.applies_to_command(&PolicyCommand::Insert));

        let select_policy = Policy::new("select_policy", Oid(1), PolicyCommand::Select);
        assert!(select_policy.applies_to_command(&PolicyCommand::Select));
        assert!(!select_policy.applies_to_command(&PolicyCommand::Insert));
    }

    #[test]
    fn test_policy_applies_to_role() {
        let mut policy = Policy::new("role_policy", Oid(1), PolicyCommand::Select);
        assert!(policy.applies_to_role("anyone"));

        policy.roles = vec!["admin".to_string()];
        assert!(policy.applies_to_role("admin"));
        assert!(!policy.applies_to_role("user"));
    }

    #[test]
    fn test_rls_filter() {
        let mut filter = RlsFilter::new(PolicyCommand::Select);
        assert!(!filter.has_policies());

        let policy = Policy::new("p1", Oid(1), PolicyCommand::Select);
        filter.add_policy(policy);
        assert!(filter.has_policies());
    }

    #[test]
    fn test_rls_filter_combine() {
        let mut filter = RlsFilter::new(PolicyCommand::Select);
        let p1 = Policy::new("p1", Oid(1), PolicyCommand::Select)
            .with_using(Expr::Identifier("active = true".to_string()));
        let p2 = Policy::new("p2", Oid(1), PolicyCommand::Select)
            .with_using(Expr::Identifier("owner = current_user".to_string()));
        filter.add_policy(p1);
        filter.add_policy(p2);

        let combined = filter.combine_using_expressions();
        assert!(combined.is_some());
    }
}
