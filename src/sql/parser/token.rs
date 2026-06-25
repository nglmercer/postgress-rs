#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Keyword(String),
    Ident(String),
    Number(String),
    String(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Semicolon,
    Colon,
    Eq,
    Neq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    Bang,
    Question,
    LtLt,
    GtGt,
    Eof,
}

pub(crate) fn tokenize(sql: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Ampersand);
                i += 1;
            }
            '|' => {
                tokens.push(Token::Pipe);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            '~' => {
                tokens.push(Token::Tilde);
                i += 1;
            }
            '?' => {
                tokens.push(Token::Question);
                i += 1;
            }
            '-' => {
                if i + 1 < chars.len() && chars[i + 1] == '-' {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                } else {
                    tokens.push(Token::Minus);
                    i += 1;
                }
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '<' => {
                if i + 1 < chars.len() {
                    match chars[i + 1] {
                        '>' => {
                            tokens.push(Token::Neq);
                            i += 2;
                        }
                        '=' => {
                            tokens.push(Token::LtEq);
                            i += 2;
                        }
                        '<' => {
                            tokens.push(Token::LtLt);
                            i += 2;
                        }
                        _ => {
                            tokens.push(Token::Lt);
                            i += 1;
                        }
                    }
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < chars.len() {
                    match chars[i + 1] {
                        '=' => {
                            tokens.push(Token::GtEq);
                            i += 2;
                        }
                        '>' => {
                            tokens.push(Token::GtGt);
                            i += 2;
                        }
                        _ => {
                            tokens.push(Token::Gt);
                            i += 1;
                        }
                    }
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::Neq);
                    i += 2;
                } else {
                    tokens.push(Token::Bang);
                    i += 1;
                }
            }
            '\'' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\'' && i + 1 < chars.len() && chars[i + 1] == '\'' {
                        s.push('\'');
                        i += 2;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token::String(s));
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    s.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token::Ident(s));
            }
            _ if chars[i].is_ascii_digit() => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    s.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Number(s));
            }
            _ if chars[i].is_ascii_alphabetic() || chars[i] == '_' => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    s.push(chars[i]);
                    i += 1;
                }
                let upper = s.to_uppercase();
                match upper.as_str() {
                    "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "CREATE" | "DROP" | "ALTER"
                    | "TABLE" | "INDEX" | "VIEW" | "PRIMARY" | "KEY" | "FOREIGN" | "REFERENCES"
                    | "CONSTRAINT" | "NOT" | "NULL" | "DEFAULT" | "CHECK" | "UNIQUE" | "INT"
                    | "INTEGER" | "BIGINT" | "SMALLINT" | "FLOAT" | "DOUBLE" | "PRECISION"
                    | "REAL" | "NUMERIC" | "DECIMAL" | "VARCHAR" | "CHAR" | "TEXT" | "BOOLEAN"
                    | "BOOL" | "DATE" | "TIME" | "TIMETZ" | "TIMESTAMP" | "TIMESTAMPTZ"
                    | "INTERVAL" | "JSON" | "JSONB" | "UUID" | "ARRAY" | "SERIAL" | "BIGSERIAL"
                    | "SMALLSERIAL" | "MONEY" | "INET" | "CIDR" | "MACADDR" | "MACADDR8"
                    | "BIT" | "VARYING" | "TSVECTOR" | "TSQUERY" | "FROM" | "WHERE" | "JOIN"
                    | "INNER" | "LEFT" | "RIGHT" | "FULL" | "CROSS" | "ON" | "USING" | "AS"
                    | "AND" | "OR" | "IN" | "BETWEEN" | "LIKE" | "ILIKE" | "INTO" | "IS"
                    | "EXISTS" | "ANY" | "ALL" | "SOME" | "ORDER" | "BY" | "ASC" | "DESC"
                    | "NULLS" | "FIRST" | "LAST" | "GROUP" | "HAVING" | "LIMIT" | "OFFSET"
                    | "DISTINCT" | "RETURNING" | "VALUES" | "SET" | "UNION" | "INTERSECT"
                    | "EXCEPT" | "CONFLICT" | "NOTHING" | "DO" | "EXCLUDED" | "BEGIN" | "START"
                    | "TRANSACTION" | "COMMIT" | "ROLLBACK" | "ABORT" | "ISOLATION" | "LEVEL"
                    | "READ" | "WRITE" | "SERIALIZABLE" | "REPEATABLE" | "COMMITTED"
                    | "UNCOMMITTED" | "DEFERRABLE" | "EXPLAIN" | "CASCADE" | "RESTRICT" | "IF"
                    | "TRUE" | "FALSE" | "CASE" | "WHEN" | "THEN" | "ELSE" | "END" | "OVER"
                    | "PARTITION" | "FILTER" | "ROWS" | "RANGE" | "GROUPS" | "CURRENT" | "ROW"
                    | "PRECEDING" | "FOLLOWING" | "UNBOUNDED" | "ADD" | "COLUMN" | "RENAME"
                    | "TO" | "DATA" | "TYPE" | "WINDOW" | "FETCH" | "NEXT" | "ONLY" | "WITH"
                    | "RECURSIVE" | "MATERIALIZED" | "UNMATERIALIZED" | "FORCE" | "ENABLE"
                    | "DISABLE" | "SECURITY" | "GRANT" | "REVOKE" | "ROLE" | "ADMIN" | "OPTION"
                    | "PUBLIC" | "TRIGGER" | "FUNCTION" | "PROCEDURE" | "RETURNS" | "RETURN"
                    | "DECLARE" | "LOOP" | "WHILE" | "FOR" | "REVERSE" | "EXIT" | "CONTINUE"
                    | "ELSIF" | "PERFORM" | "EXECUTE" | "RAISE" | "NOTICE" | "EXCEPTION"
                    | "WARNING" | "INFO" | "DEBUG" | "LOG" | "ASSERT" | "FOUND" | "NEW" | "OLD"
                    | "TG_OP" | "TG_TABLE_NAME" | "TG_TABLE_SCHEMA" | "TG_WHEN" | "TG_LEVEL"
                    | "TG_ARGV" | "CAST" | "NOW" | "CURRENT_DATE" | "CURRENT_TIME"
                    | "CURRENT_TIMESTAMP" | "EXTRACT" | "DATE_TRUNC" | "DATE_PART" | "AT"
                    | "ZONE" | "YEAR" | "MONTH" | "DAY" | "HOUR" | "MINUTE" | "SECOND"
                    | "MILLISECOND" | "MICROSECOND" | "DOW" | "DOY" | "ISODOW" | "WEEK"
                    | "QUARTER" | "EPOCH" | "ISOYEAR" | "TIMEZONE" | "TIMEZONE_HOUR"
                    | "TIMEZONE_MINUTE" | "LOCALTIME" | "LOCALTIMESTAMP" | "GREATEST" | "LEAST"
                    | "COALESCE" | "NULLIF" | "ABS" | "CEIL" | "FLOOR" | "ROUND" | "TRUNC"
                    | "SIGN" | "POWER" | "SQRT" | "LN" | "EXP" | "LENGTH" | "UPPER" | "LOWER"
                    | "TRIM" | "LTRIM" | "RTRIM" | "SUBSTRING" | "REPLACE" | "CONCAT"
                    | "CONCAT_WS" | "INITCAP" | "POSITION" | "STRPOS" | "OVERLAY" | "LPAD"
                    | "RPAD" | "REGEXP_REPLACE" | "REGEXP_MATCHES" | "SEQUENCE" | "INCREMENT"
                    | "CACHE" | "CYCLE" | "OWNED" | "MINVALUE" | "MAXVALUE" | "MERGE"
                    | "MATCHED" | "SOURCE" | "TARGET" | "COMPOSITE" | "ENUM" | "NOCYCLE" | "NO"
                    | "NOMINVALUE" | "NOMAXVALUE" | "NOCACHE" | "SCHEMA" | "AUTHORIZATION"
                    | "SESSION" | "LOCAL" | "CHARACTERISTICS" => {
                        tokens.push(Token::Keyword(s));
                    }
                    _ => {
                        tokens.push(Token::Ident(s));
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    tokens.push(Token::Eof);
    tokens
}
