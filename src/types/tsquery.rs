use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TsQuery {
    Lexeme(String),
    And(Box<TsQuery>, Box<TsQuery>),
    Or(Box<TsQuery>, Box<TsQuery>),
    Not(Box<TsQuery>),
    Phrase(Vec<TsQuery>, i32),
}

impl TsQuery {
    pub fn lexeme(text: &str) -> Self {
        TsQuery::Lexeme(text.to_string())
    }

    pub fn and(left: TsQuery, right: TsQuery) -> Self {
        TsQuery::And(Box::new(left), Box::new(right))
    }

    pub fn or(left: TsQuery, right: TsQuery) -> Self {
        TsQuery::Or(Box::new(left), Box::new(right))
    }

    pub fn not(inner: TsQuery) -> Self {
        TsQuery::Not(Box::new(inner))
    }

    pub fn phrase(terms: Vec<TsQuery>, distance: i32) -> Self {
        TsQuery::Phrase(terms, distance)
    }
}

pub fn to_tsquery(query: &str) -> TsQuery {
    let query = query.trim();
    if query.is_empty() {
        return TsQuery::Lexeme(String::new());
    }

    if let Some(pos) = query.find(" & ") {
        let left = to_tsquery(&query[..pos]);
        let right = to_tsquery(&query[pos + 3..]);
        return TsQuery::and(left, right);
    }

    if let Some(pos) = query.find(" | ") {
        let left = to_tsquery(&query[..pos]);
        let right = to_tsquery(&query[pos + 3..]);
        return TsQuery::or(left, right);
    }

    if let Some(pos) = query.find(" <-> ") {
        let left = to_tsquery(&query[..pos]);
        let right = to_tsquery(&query[pos + 5..]);
        return TsQuery::phrase(vec![left, right], 1);
    }

    if let Some(stripped) = query.strip_prefix('!') {
        return TsQuery::not(to_tsquery(stripped));
    }

    TsQuery::Lexeme(query.to_string())
}

pub fn plainto_tsquery(text: &str) -> TsQuery {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return TsQuery::Lexeme(String::new());
    }

    let mut result = TsQuery::Lexeme(words[0].to_string());
    for word in &words[1..] {
        result = TsQuery::and(result, TsQuery::Lexeme(word.to_string()));
    }
    result
}

pub fn phraseto_tsquery(text: &str) -> TsQuery {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return TsQuery::Lexeme(String::new());
    }

    let queries: Vec<TsQuery> = words.iter().map(|w| TsQuery::Lexeme(w.to_string())).collect();
    TsQuery::phrase(queries, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_tsquery_simple() {
        let q = to_tsquery("hello");
        assert!(matches!(q, TsQuery::Lexeme(s) if s == "hello"));
    }

    #[test]
    fn test_to_tsquery_and() {
        let q = to_tsquery("hello & world");
        assert!(matches!(q, TsQuery::And(_, _)));
    }

    #[test]
    fn test_to_tsquery_or() {
        let q = to_tsquery("hello | world");
        assert!(matches!(q, TsQuery::Or(_, _)));
    }

    #[test]
    fn test_to_tsquery_not() {
        let q = to_tsquery("!hello");
        assert!(matches!(q, TsQuery::Not(_)));
    }

    #[test]
    fn test_to_tsquery_phrase() {
        let q = to_tsquery("hello <-> world");
        assert!(matches!(q, TsQuery::Phrase(_, 1)));
    }

    #[test]
    fn test_plainto_tsquery() {
        let q = plainto_tsquery("hello world");
        assert!(matches!(q, TsQuery::And(_, _)));
    }

    #[test]
    fn test_phraseto_tsquery() {
        let q = phraseto_tsquery("hello world");
        assert!(matches!(q, TsQuery::Phrase(_, 1)));
    }

    #[test]
    fn test_to_tsquery_complex() {
        let q = to_tsquery("(hello | world) & !test");
        assert!(matches!(q, TsQuery::And(_, _)));
    }
}
