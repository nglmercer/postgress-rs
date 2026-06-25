use crate::sql::ast::{DataType, Expr, SelectStatement, Statement};
use crate::types::Oid;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionLanguage {
    Sql,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Volatility {
    Immutable,
    Stable,
    Volatile,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecurityDefiner {
    Invoker,
    Definer,
}

#[derive(Debug, Clone)]
pub struct FunctionArgDef {
    pub name: String,
    pub data_type: DataType,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    SqlBody(Statement),
    SqlQuery(SelectStatement),
    SqlCompound(Vec<Statement>),
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub args: Vec<FunctionArgDef>,
    pub returns: Option<DataType>,
    pub language: FunctionLanguage,
    pub body: FunctionBody,
    pub volatility: Volatility,
    pub security: SecurityDefiner,
    pub strict: bool,
}

impl FunctionDef {
    pub fn new(name: &str, body: FunctionBody) -> Self {
        Self {
            name: name.to_string(),
            args: Vec::new(),
            returns: None,
            language: FunctionLanguage::Sql,
            body,
            volatility: Volatility::Volatile,
            security: SecurityDefiner::Invoker,
            strict: false,
        }
    }

    pub fn with_args(mut self, args: Vec<FunctionArgDef>) -> Self {
        self.args = args;
        self
    }

    pub fn with_returns(mut self, returns: DataType) -> Self {
        self.returns = Some(returns);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Truncate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerLevel {
    Row,
    Statement,
}

#[derive(Debug, Clone)]
pub struct TriggerDef {
    pub name: String,
    pub table_oid: Oid,
    pub timing: TriggerTiming,
    pub events: Vec<TriggerEvent>,
    pub function_name: String,
    pub columns: Vec<String>,
    pub condition: Option<Expr>,
    pub level: TriggerLevel,
}

impl TriggerDef {
    pub fn new(
        name: &str,
        table_oid: Oid,
        timing: TriggerTiming,
        events: Vec<TriggerEvent>,
        function_name: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            table_oid,
            timing,
            events,
            function_name: function_name.to_string(),
            columns: Vec::new(),
            condition: None,
            level: TriggerLevel::Row,
        }
    }

    pub fn applies_to_event(&self, event: &TriggerEvent) -> bool {
        self.events.iter().any(|e| e == event)
    }
}

#[derive(Debug, Clone)]
pub struct TriggerContext {
    pub old: Option<HashMap<String, String>>,
    pub new: Option<HashMap<String, String>>,
    pub event: TriggerEvent,
    pub table_name: String,
}

pub struct FunctionRegistry {
    functions: HashMap<String, FunctionDef>,
    triggers: Vec<TriggerDef>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            triggers: Vec::new(),
        }
    }

    pub fn register_function(&mut self, func: FunctionDef) {
        self.functions.insert(func.name.clone(), func);
    }

    pub fn get_function(&self, name: &str) -> Option<&FunctionDef> {
        self.functions.get(name)
    }

    pub fn register_trigger(&mut self, trigger: TriggerDef) {
        self.triggers.push(trigger);
    }

    pub fn get_triggers_for_table(&self, table_oid: Oid, event: &TriggerEvent) -> Vec<&TriggerDef> {
        self.triggers
            .iter()
            .filter(|t| t.table_oid == table_oid && t.applies_to_event(event))
            .collect()
    }

    pub fn drop_function(&mut self, name: &str) -> bool {
        self.functions.remove(name).is_some()
    }

    pub fn drop_trigger(&mut self, name: &str) -> bool {
        let len_before = self.triggers.len();
        self.triggers.retain(|t| t.name != name);
        self.triggers.len() < len_before
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::{BinaryOperator, Literal};

    #[test]
    fn test_function_def() {
        let body = FunctionBody::SqlQuery(SelectStatement {
            with: None,
            distinct: crate::sql::ast::DistinctClause::All,
            select_list: vec![],
            from: None,
            where_clause: None,
            group_by: vec![],
            having: None,
            order_by: vec![],
            limit: None,
            offset: None,
            locking: vec![],
            set_operations: vec![],
        });
        let func = FunctionDef::new("get_count", body).with_returns(DataType::BigInt);
        assert_eq!(func.name, "get_count");
        assert!(func.returns.is_some());
    }

    #[test]
    fn test_trigger_def() {
        let trigger = TriggerDef::new(
            "tr_insert",
            Oid(1),
            TriggerTiming::Before,
            vec![TriggerEvent::Insert],
            "log_insert",
        );
        assert!(trigger.applies_to_event(&TriggerEvent::Insert));
        assert!(!trigger.applies_to_event(&TriggerEvent::Delete));
    }

    #[test]
    fn test_function_registry() {
        let mut registry = FunctionRegistry::new();
        let body = FunctionBody::SqlBody(Statement::Commit);
        let func = FunctionDef::new("test_func", body);
        registry.register_function(func);
        assert!(registry.get_function("test_func").is_some());
        assert!(registry.get_function("nonexistent").is_none());
    }

    #[test]
    fn test_trigger_registry() {
        let mut registry = FunctionRegistry::new();
        let trigger = TriggerDef::new(
            "tr1",
            Oid(1),
            TriggerTiming::Before,
            vec![TriggerEvent::Insert],
            "fn1",
        );
        registry.register_trigger(trigger);
        let triggers = registry.get_triggers_for_table(Oid(1), &TriggerEvent::Insert);
        assert_eq!(triggers.len(), 1);
    }

    #[test]
    fn test_drop_function() {
        let mut registry = FunctionRegistry::new();
        let body = FunctionBody::SqlBody(Statement::Commit);
        registry.register_function(FunctionDef::new("test_func", body));
        assert!(registry.drop_function("test_func"));
        assert!(!registry.drop_function("nonexistent"));
    }
}
