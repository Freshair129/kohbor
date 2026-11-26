//! Small, dependency-free parser for the Valve KeyValues text format.
//!
//! This intentionally parses syntax only. Directives such as `#base` remain
//! ordinary key/value pairs so callers can resolve inheritance explicitly.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvValue {
    Scalar(String),
    Object(KvObject),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvObject {
    entries: Vec<(String, KvValue)>,
}

impl KvObject {
    pub fn entries(&self) -> &[(String, KvValue)] {
        &self.entries
    }

    /// Return the last case-insensitive occurrence, matching Valve's override
    /// behavior while preserving duplicate entries for inspection.
    pub fn get(&self, key: &str) -> Option<&KvValue> {
        self.entries
            .iter()
            .rev()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    }

    pub fn scalar(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            KvValue::Scalar(value) => Some(value),
            KvValue::Object(_) => None,
        }
    }

    pub fn object(&self, key: &str) -> Option<&KvObject> {
        match self.get(key)? {
            KvValue::Object(value) => Some(value),
            KvValue::Scalar(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KeyValues parse error at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Open,
    Close,
    End,
}

struct Lexer<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn error(&self, message: &'static str) -> ParseError {
        ParseError {
            offset: self.offset,
            message,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.offset..)?.chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.input.get(self.offset..)?.chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Some('\u{feff}') if self.offset == 0 => {
                    self.bump();
                }
                Some(ch) if ch.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_next() == Some('/') => {
                    self.bump();
                    self.bump();
                    while let Some(ch) = self.bump() {
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                Some('/') if self.peek_next() == Some('*') => {
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                break;
                            }
                            Some(_) => {}
                            None => return Err(self.error("unterminated block comment")),
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn next(&mut self) -> Result<Token, ParseError> {
        self.skip_trivia()?;
        match self.peek() {
            None => Ok(Token::End),
            Some('{') => {
                self.bump();
                Ok(Token::Open)
            }
            Some('}') => {
                self.bump();
                Ok(Token::Close)
            }
            Some('"') => self.quoted(),
            Some(_) => self.unquoted(),
        }
    }

    fn quoted(&mut self) -> Result<Token, ParseError> {
        self.bump();
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(Token::Word(value)),
                Some('\\') => match self.bump() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some(other) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => return Err(self.error("unfinished escape in quoted string")),
                },
                Some(ch) => value.push(ch),
                None => return Err(self.error("unterminated quoted string")),
            }
        }
    }

    fn unquoted(&mut self) -> Result<Token, ParseError> {
        let start = self.offset;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || ch == '{' || ch == '}' {
                break;
            }
            if ch == '/' && self.peek_next() == Some('/') {
                break;
            }
            self.bump();
        }
        if self.offset == start {
            return Err(self.error("expected token"));
        }
        Ok(Token::Word(self.input[start..self.offset].to_owned()))
    }
}

pub fn parse(input: &str) -> Result<KvObject, ParseError> {
    let mut lexer = Lexer::new(input);
    parse_object(&mut lexer, false)
}

fn parse_object(lexer: &mut Lexer<'_>, nested: bool) -> Result<KvObject, ParseError> {
    let mut object = KvObject::default();
    loop {
        let key = match lexer.next()? {
            Token::Word(key) => key,
            Token::End if !nested => return Ok(object),
            Token::End => return Err(lexer.error("unterminated object")),
            Token::Close if nested => return Ok(object),
            Token::Close => return Err(lexer.error("unexpected closing brace")),
            Token::Open => return Err(lexer.error("expected key before opening brace")),
        };
        let value = match lexer.next()? {
            Token::Word(value) => KvValue::Scalar(value),
            Token::Open => KvValue::Object(parse_object(lexer, true)?),
            Token::End => return Err(lexer.error("missing value after key")),
            Token::Close => return Err(lexer.error("missing value before closing brace")),
        };
        object.entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, KvValue};

    #[test]
    fn parses_comments_directives_and_nested_values() {
        let parsed = parse(
            r#"// header
            #base "heroes/base.txt"
            "DOTAUnits" {
                "npc_dota_hero_demo" {
                    "AttributeBaseStrength" "20"
                    "AbilityValues" { "bonus_strength" "10" }
                }
            }
            /* trailing block comment */"#,
        )
        .expect("valid fixture");

        assert_eq!(parsed.scalar("#base"), Some("heroes/base.txt"));
        let unit = parsed
            .object("DOTAUnits")
            .and_then(|units| units.object("npc_dota_hero_demo"))
            .expect("hero object");
        assert_eq!(unit.scalar("AttributeBaseStrength"), Some("20"));
        assert_eq!(
            unit.object("AbilityValues")
                .and_then(|values| values.scalar("bonus_strength")),
            Some("10")
        );
    }

    #[test]
    fn preserves_duplicate_keys_and_uses_last_value_for_lookup() {
        let parsed =
            parse("\"Root\" { \"Value\" \"first\" \"value\" \"last\" }").expect("valid fixture");
        let root = parsed.object("root").expect("root object");
        assert_eq!(root.scalar("VALUE"), Some("last"));
        assert_eq!(root.entries().len(), 2);
    }

    #[test]
    fn decodes_quoted_escapes_and_utf8() {
        let parsed = parse(r#""name" "é \"hero\" \\slot\nnext""#).expect("valid fixture");
        assert_eq!(parsed.scalar("name"), Some("é \"hero\" \\slot\nnext"));
    }

    #[test]
    fn rejects_unbalanced_and_incomplete_input() {
        assert!(parse("\"Root\" { \"Key\" \"value\"").is_err());
        assert!(parse("\"Root\" { \"Key\" }").is_err());
        assert!(parse("\"Root\" { } }").is_err());
    }

    #[test]
    fn handles_utf8_bom() {
        let parsed = parse("\u{feff}\"key\" \"value\"").expect("valid fixture");
        assert!(matches!(parsed.get("key"), Some(KvValue::Scalar(value)) if value == "value"));
    }
}
