/*!
# sql-script-parser iterates over SQL statements in SQL script

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

*/

/// SQL script single statement.
pub struct SqlScript<'a> {
    /// Start index in source.
    pub start: usize,
    /// End index in source. Either index of `;` or EOF.
    pub end: usize,
    /// SQL Statement.
    /// Includes SQL statement and all trailing whitespaces and comments.
    pub statement: &'a [u8],
}

pub trait SqlScriptTokenizer<'a, Y> {
    fn apply(&self, sql_script: SqlScript<'a>, tokens: &[SqlToken]) -> Y;
}

/// SQL script parser.
pub struct SqlScriptParser<'a, Y, T: SqlScriptTokenizer<'a, Y>> {
    source: &'a [u8],
    position: usize,
    tokenizer: T,
    _p: std::marker::PhantomData<Y>,
}

const SP: &[u8] = b" \t\r\n";
const SP_WO_LF: &[u8] = b" \t\r";

/// SQL token. Start and end are indexes in source (global) array.
#[derive(Debug, PartialEq)]
pub struct SqlToken {
    pub start: usize,
    pub end: usize,
    pub kind: SqlTokenKind,
}

impl SqlToken {
    /// Extracts token from `SqlScript`. Panics if used with wrong SQL script.
    pub fn extract<'a>(&self, sql_script: &SqlScript<'a>) -> &'a [u8] {
        &sql_script.statement[self.start - sql_script.start..self.end - sql_script.start]
    }
}

#[derive(Debug, PartialEq)]
pub enum SqlTokenKind {
    Space,
    Comment,
    Word,
    String,
    Symbol,
}

type SqlTokenPos = (SqlToken, usize);

/// Default no-op SQL script tokenizer. Just returns `SqlScript`.
pub struct DefaultSqlScriptTokenizer;

/// Creates SQL script parser.
///
/// ```rust
/// use sql_script_parser::sql_script_parser;
///
/// let sql = b"select 1;\nselect 2";
///
/// let mut parser = sql_script_parser(sql).map(|x| x.statement);
///
/// assert_eq!(parser.next(), Some(&b"select 1;\n"[..]));
/// assert_eq!(parser.next(), Some(&b"select 2"[..]));
/// assert_eq!(parser.next(), None);
/// ```
pub fn sql_script_parser<'a>(
    source: &'a [u8],
) -> SqlScriptParser<'a, SqlScript<'a>, DefaultSqlScriptTokenizer> {
    SqlScriptParser::new(DefaultSqlScriptTokenizer {}, source)
}

impl<'a> SqlScriptTokenizer<'a, SqlScript<'a>> for DefaultSqlScriptTokenizer {
    fn apply(&self, sql_script: SqlScript<'a>, _tokens: &[SqlToken]) -> SqlScript<'a> {
        sql_script
    }
}

impl<'a, Y, T: SqlScriptTokenizer<'a, Y>> SqlScriptParser<'a, Y, T> {
    pub fn new(tokenizer: T, source: &'a [u8]) -> Self {
        Self {
            source,
            position: 0,
            tokenizer,
            _p: std::marker::PhantomData,
        }
    }

    fn first_of(
        &self,
        matchers: &[fn(&SqlScriptParser<'a, Y, T>, usize) -> Option<SqlTokenPos>],
        position: usize,
    ) -> Option<SqlTokenPos> {
        for matcher in matchers {
            let result = matcher(self, position);
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn space(&self, position: usize) -> Option<SqlTokenPos> {
        self.any_of_space(SP, position)
    }

    fn space_without_eol(&self, position: usize) -> Option<SqlTokenPos> {
        self.any_of_space(SP_WO_LF, position)
    }

    fn eol(&self, position: usize) -> Option<SqlTokenPos> {
        self.any_of_space(b"\r\n", position)
    }

    fn any_of_space(&self, pattern: &[u8], position: usize) -> Option<SqlTokenPos> {
        self.any_of(pattern, position).map(|x| {
            (
                SqlToken {
                    start: position,
                    end: x,
                    kind: SqlTokenKind::Space,
                },
                x,
            )
        })
    }

    fn any_of(&self, pattern: &[u8], position: usize) -> Option<usize> {
        self.source
            .get(position)
            .filter(|x| pattern.contains(x))
            .and_then(|_| {
                let mut position = position + 1;
                while let Some(ch) = self.source.get(position) {
                    if !pattern.contains(ch) {
                        break;
                    }
                    position += 1;
                }
                Some(position)
            })
    }

    fn word(&self, position: usize) -> Option<SqlTokenPos> {
        let start = position;
        let mut position = position;
        while let Some(ch) = self.source.get(position) {
            match ch {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => position += 1,
                _ => break,
            }
        }
        if start == position {
            return None;
        }
        Some((
            SqlToken {
                start,
                end: position,
                kind: SqlTokenKind::Word,
            },
            position,
        ))
    }

    fn line_comment(&self, position: usize) -> Option<SqlTokenPos> {
        if self.source.get(position) == Some(&b'-') {
            let start = position;
            let mut position = position + 1;
            return match (self.source.get(position), self.source.get(position + 1)) {
                (Some(b'-'), Some(b' ')) => {
                    position += 2;
                    while let Some(c) = self.source.get(position) {
                        position += 1;
                        if c == &b'\n' {
                            break;
                        }
                    }
                    Some((
                        SqlToken {
                            start,
                            end: position,
                            kind: SqlTokenKind::Comment,
                        },
                        position,
                    ))
                }
                _ => None,
            };
        }
        None
    }

    fn string(&self, position: usize) -> Option<SqlTokenPos> {
        self.source.get(position).and_then(|border| match border {
            b'\'' | b'"' | b'`' => {
                let start = position;
                let mut position = position + 1;
                while let Some(ch) = self.source.get(position) {
                    position += 1;
                    if ch == border {
                        if self.source.get(position) == Some(border) {
                            position += 1;
                        } else {
                            break;
                        }
                    } else if ch == &b'\\' && self.source.get(position) == Some(border) {
                        position += 1;
                    }
                }
                Some((
                    SqlToken {
                        start,
                        end: position,
                        kind: SqlTokenKind::String,
                    },
                    position,
                ))
            }
            _ => None,
        })
    }

    fn multiline_comment(&self, position: usize) -> Option<SqlTokenPos> {
        match (self.source.get(position), self.source.get(position + 1)) {
            (Some(&b'/'), Some(&b'*')) => {
                let start = position;
                let mut position = position + 2;
                loop {
                    match (self.source.get(position), self.source.get(position + 1)) {
                        (Some(&b'*'), Some(&b'/')) => {
                            position += 2;
                            break;
                        }
                        (Some(_), _) => position += 1,
                        (None, _) => break,
                    }
                }
                Some((
                    SqlToken {
                        start,
                        end: position,
                        kind: SqlTokenKind::Comment,
                    },
                    position,
                ))
            }
            _ => None,
        }
    }

    fn read_statement(&self, position: &mut usize) -> Option<(usize, &'a [u8], Vec<SqlToken>)> {
        if *position == self.source.len() {
            return None;
        }
        let start = *position;
        let mut end = None;
        let mut tokens = vec![];
        loop {
            if let Some((token, p)) = self.first_of(
                &[
                    Self::space,
                    Self::line_comment,
                    Self::multiline_comment,
                    Self::string,
                    Self::word,
                ],
                *position,
            ) {
                *position = p;
                tokens.push(token);
            } else if Some(&b';') == self.source.get(*position) {
                end = Some(*position);
                *position += 1;
                while let Some((token, p)) = self.first_of(
                    &[Self::space_without_eol, Self::multiline_comment],
                    *position,
                ) {
                    *position = p;
                    tokens.push(token);
                }
                if let Some((token, p)) = self.line_comment(*position) {
                    *position = p;
                    tokens.push(token);
                } else if let Some((token, p)) = self.eol(*position) {
                    *position = p;
                    tokens.push(token);
                }
                break;
            } else {
                tokens.push(SqlToken {
                    start: *position,
                    end: *position + 1,
                    kind: SqlTokenKind::Symbol,
                });
                *position += 1;
            }
            if *position == self.source.len() {
                break;
            }
        }
        Some((
            end.unwrap_or_else(|| *position),
            &self.source[start..*position],
            tokens,
        ))
    }
}

impl<'a, Y, T: SqlScriptTokenizer<'a, Y>> Iterator for SqlScriptParser<'a, Y, T> {
    type Item = Y;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.position;
        let mut position = self.position;
        let item = self
            .read_statement(&mut position)
            .map(|(end, statement, tokens)| {
                self.tokenizer.apply(
                    SqlScript {
                        start,
                        end,
                        statement,
                    },
                    &tokens,
                )
            });
        self.position = position;
        item
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::io::Write;

    #[test]
    fn parse_sql() {
        let test_script = br#"select 1;
alter table qqq add column bbb; -- line comment at the end
-- big comment;
--garbage
select * from dual
/* multi line comment
is here
see it */;
/**/
alter table me"#;

        let parser = sql_script_parser(test_script);

        let mut output = vec![];
        let mut sqls = vec![];
        for sql in parser {
            output.write_all(sql.statement).unwrap();
            sqls.push(sql.statement);
        }
        assert_eq!(output, &test_script[..]);
        assert_eq!(sqls[0], b"select 1;\n");
        assert_eq!(
            sqls[1],
            &b"alter table qqq add column bbb; -- line comment at the end\n"[..]
        );
        assert_eq!(
            sqls[2],
            &br#"-- big comment;
--garbage
select * from dual
/* multi line comment
is here
see it */;
"#[..]
        );
        assert_eq!(sqls[3], b"/**/\nalter table me");
    }

    struct TestCommentSqlScriptTokenizer;
    impl<'a> SqlScriptTokenizer<'a, SqlScript<'a>> for TestCommentSqlScriptTokenizer {
        fn apply(&self, sql_script: SqlScript<'a>, tokens: &[SqlToken]) -> SqlScript<'a> {
            assert_eq!(
                tokens.get(0).map(|x| x.extract(&sql_script)),
                Some(&b"/* comment */"[..])
            );
            sql_script
        }
    }

    #[test]
    fn parse_comment() {
        let test_script = b"/* comment */ INSERT INTO table ...";
        let parser = SqlScriptParser::new(TestCommentSqlScriptTokenizer {}, test_script);

        let mut output = vec![];
        for sql in parser {
            output.write_all(sql.statement).unwrap();
        }
        assert_eq!(output, &test_script[..]);
    }
}
