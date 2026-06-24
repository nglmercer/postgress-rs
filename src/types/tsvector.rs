use serde::{Deserialize, Serialize};
use super::tsquery::TsQuery;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsVector {
    pub lexemes: Vec<TsLexeme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsLexeme {
    pub text: String,
    pub positions: Vec<TsPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsPosition {
    pub word: u32,
    pub weight: TsWeight,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TsWeight {
    A = 1,
    B = 2,
    C = 3,
    D = 4,
}

impl TsWeight {
    pub fn from_char(c: char) -> Self {
        match c {
            'A' | 'a' => TsWeight::A,
            'B' | 'b' => TsWeight::B,
            'C' | 'c' => TsWeight::C,
            'D' | 'd' => TsWeight::D,
            _ => TsWeight::D,
        }
    }
}

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "in", "on", "at",
    "to", "for", "of", "and", "or", "but", "not",
];

pub fn to_tsvector(text: &str) -> TsVector {
    let mut lexemes: Vec<TsLexeme> = Vec::new();
    let mut word_pos = 1u32;

    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.is_empty() || STOP_WORDS.contains(&word.as_str()) {
            word_pos += 1;
            continue;
        }

        let position = TsPosition {
            word: word_pos,
            weight: TsWeight::D,
        };

        if let Some(existing) = lexemes.iter_mut().find(|l| l.text == word) {
            existing.positions.push(position);
        } else {
            lexemes.push(TsLexeme {
                text: word,
                positions: vec![position],
            });
        }
        word_pos += 1;
    }

    lexemes.sort_by(|a, b| a.text.cmp(&b.text));
    TsVector { lexemes }
}

pub fn ts_match(tsvector: &TsVector, query: &TsQuery) -> bool {
    match query {
        TsQuery::Lexeme(lexeme) => {
            let lexeme_lower = lexeme.to_lowercase();
            tsvector.lexemes.iter().any(|l| l.text == lexeme_lower)
        }
        TsQuery::And(left, right) => {
            ts_match(tsvector, left) && ts_match(tsvector, right)
        }
        TsQuery::Or(left, right) => {
            ts_match(tsvector, left) || ts_match(tsvector, right)
        }
        TsQuery::Not(inner) => {
            !ts_match(tsvector, inner)
        }
        TsQuery::Phrase(terms, _distance) => {
            terms.iter().all(|t| ts_match(tsvector, t))
        }
    }
}

pub fn ts_rank(tsvector: &TsVector, query: &TsQuery) -> f32 {
    match query {
        TsQuery::Lexeme(lexeme) => {
            let lexeme_lower = lexeme.to_lowercase();
            tsvector.lexemes.iter()
                .filter(|l| l.text == lexeme_lower)
                .map(|l| {
                    l.positions.iter().map(|p| {
                        match p.weight {
                            TsWeight::A => 1.0,
                            TsWeight::B => 0.4,
                            TsWeight::C => 0.2,
                            TsWeight::D => 0.1,
                        }
                    }).sum::<f32>()
                })
                .sum()
        }
        TsQuery::And(left, right) => {
            ts_rank(tsvector, left).min(ts_rank(tsvector, right))
        }
        TsQuery::Or(left, right) => {
            ts_rank(tsvector, left).max(ts_rank(tsvector, right))
        }
        TsQuery::Not(_) => 0.0,
        TsQuery::Phrase(terms, _) => {
            terms.iter().map(|t| ts_rank(tsvector, t)).sum::<f32>() / terms.len() as f32
        }
    }
}

pub fn ts_rank_cd(tsvector: &TsVector, query: &TsQuery) -> f32 {
    ts_rank(tsvector, query) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_tsvector() {
        let tv = to_tsvector("hello world test");
        assert!(tv.lexemes.iter().any(|l| l.text == "hello"));
        assert!(tv.lexemes.iter().any(|l| l.text == "world"));
        assert!(tv.lexemes.iter().any(|l| l.text == "test"));
    }

    #[test]
    fn test_to_tsvector_stop_words() {
        let tv = to_tsvector("the cat is on the mat");
        assert!(!tv.lexemes.iter().any(|l| l.text == "the"));
        assert!(!tv.lexemes.iter().any(|l| l.text == "is"));
        assert!(!tv.lexemes.iter().any(|l| l.text == "on"));
        assert!(tv.lexemes.iter().any(|l| l.text == "cat"));
        assert!(tv.lexemes.iter().any(|l| l.text == "mat"));
    }

    #[test]
    fn test_ts_match() {
        let tv = to_tsvector("hello world");
        let query = TsQuery::Lexeme("hello".to_string());
        assert!(ts_match(&tv, &query));

        let query = TsQuery::Lexeme("missing".to_string());
        assert!(!ts_match(&tv, &query));
    }

    #[test]
    fn test_ts_match_and() {
        let tv = to_tsvector("hello world");
        let query = TsQuery::And(
            Box::new(TsQuery::Lexeme("hello".to_string())),
            Box::new(TsQuery::Lexeme("world".to_string())),
        );
        assert!(ts_match(&tv, &query));

        let query = TsQuery::And(
            Box::new(TsQuery::Lexeme("hello".to_string())),
            Box::new(TsQuery::Lexeme("missing".to_string())),
        );
        assert!(!ts_match(&tv, &query));
    }

    #[test]
    fn test_ts_rank() {
        let tv = to_tsvector("hello world");
        let query = TsQuery::Lexeme("hello".to_string());
        let rank = ts_rank(&tv, &query);
        assert!(rank > 0.0);
    }

    #[test]
    fn test_ts_rank_cd() {
        let tv = to_tsvector("hello world");
        let query = TsQuery::Lexeme("hello".to_string());
        let rank = ts_rank_cd(&tv, &query);
        let full_rank = ts_rank(&tv, &query);
        assert!((rank - full_rank * 0.5).abs() < 0.001);
    }

    #[test]
    fn test_positions() {
        let tv = to_tsvector("cat dog cat");
        let cat_lexeme = tv.lexemes.iter().find(|l| l.text == "cat").unwrap();
        assert_eq!(cat_lexeme.positions.len(), 2);
        assert_eq!(cat_lexeme.positions[0].word, 1);
        assert_eq!(cat_lexeme.positions[1].word, 3);
    }
}
