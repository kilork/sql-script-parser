# sql-script-parser iterates over SQL statements in SQL script

## Legal

Dual-licensed under `MIT` or the [UNLICENSE](http://unlicense.org/).

## Features

- parses SQL scripts (currently MySQL) to sequence of separate SQL statements.
- marks parts of the SQL statement as different token types (keywords, strings, comments, ...).
- not validating input, only splits SQL statements without checking that they are valid.

## Usage

Add dependency to Cargo.toml:

```toml
[dependencies]
sql-script-parser = "0.1"
```

Parse SQL:

```rust
use sql_script_parser::sql_script_parser;

let sql = include_bytes!("../tests/demo.sql");

let mut parser = sql_script_parser(sql).map(|x| x.statement);

assert_eq!(parser.next(), Some(&b"select 1;\n"[..]));
assert_eq!(parser.next(), Some(&b"select 2"[..]));
assert_eq!(parser.next(), None);
```

Advanced - use custom tokenizer:

```rust
use sql_script_parser::*;

struct DmlDdlSqlScriptTokenizer;

struct SqlStatement<'a> {
    sql_script: SqlScript<'a>,
    kind: SqlStatementKind,
}

#[derive(Debug, PartialEq)]
enum SqlStatementKind {
    Ddl,
    Dml,
}

impl<'a> SqlScriptTokenizer<'a, SqlStatement<'a>> for DmlDdlSqlScriptTokenizer {
    fn apply(&self, sql_script: SqlScript<'a>, tokens: &[SqlToken]) -> SqlStatement<'a> {
        let mut tokens_general = tokens.iter().filter(|x| {
            [
                SqlTokenKind::Word,
                SqlTokenKind::Symbol,
                SqlTokenKind::String,
            ]
            .contains(&x.kind)
        });
        let kind = if let Some(first_keyword) = tokens_general.next() {
            if first_keyword.kind == SqlTokenKind::Word {
                let token = std::str::from_utf8(first_keyword.extract(&sql_script))
                    .unwrap()
                    .to_lowercase();
                match token.as_str() {
                    "alter" | "create" | "drop" => SqlStatementKind::Ddl,
                    _ => SqlStatementKind::Dml,
                }
            } else {
                SqlStatementKind::Dml
            }
        } else {
            SqlStatementKind::Dml
        };
        SqlStatement { sql_script, kind }
    }
}

let sql = include_bytes!("../tests/custom.sql");

let mut parser = SqlScriptParser::new(DmlDdlSqlScriptTokenizer {}, sql).map(|x| x.kind);

assert_eq!(parser.next(), Some(SqlStatementKind::Dml));
assert_eq!(parser.next(), Some(SqlStatementKind::Ddl));
assert_eq!(parser.next(), Some(SqlStatementKind::Dml));
assert_eq!(parser.next(), Some(SqlStatementKind::Ddl));
assert_eq!(parser.next(), None);
```
