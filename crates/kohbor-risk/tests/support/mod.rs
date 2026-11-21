#![allow(dead_code)]

use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Self::Object(value) => value
                .get(key)
                .unwrap_or_else(|| panic!("missing key {key}")),
            _ => panic!("expected object"),
        }
    }
    pub fn array(&self) -> &[Json] {
        match self {
            Self::Array(v) => v,
            _ => panic!("expected array"),
        }
    }
    pub fn string(&self) -> &str {
        match self {
            Self::String(v) => v,
            _ => panic!("expected string"),
        }
    }
    pub fn number(&self) -> f64 {
        match self {
            Self::Number(v) => *v,
            _ => panic!("expected number"),
        }
    }
    pub fn boolean(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            _ => panic!("expected bool"),
        }
    }
    pub fn optional_number(&self) -> Option<f64> {
        if *self == Self::Null {
            None
        } else {
            Some(self.number())
        }
    }
}

pub fn load(name: &str) -> Json {
    let path = format!(
        "{}{name}",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/risk-admission/"
        )
    );
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    parse(&data).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn parse(input: &str) -> Result<Json, String> {
    let mut p = Parser {
        input: input.as_bytes(),
        at: 0,
    };
    let result = p.value()?;
    p.space();
    if p.at != p.input.len() {
        return Err(format!("trailing input at {}", p.at));
    }
    Ok(result)
}

struct Parser<'a> {
    input: &'a [u8],
    at: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while matches!(self.input.get(self.at), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.at += 1;
        }
    }
    fn take(&mut self, c: u8) -> bool {
        self.space();
        if self.input.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.space();
        match self.input.get(self.at).copied() {
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Json::Null)
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("expected value at {}", self.at)),
        }
    }
    fn literal(&mut self, value: &[u8]) -> Result<(), String> {
        if self.input.get(self.at..self.at + value.len()) == Some(value) {
            self.at += value.len();
            Ok(())
        } else {
            Err(format!("invalid literal at {}", self.at))
        }
    }
    fn string(&mut self) -> Result<String, String> {
        if !self.take(b'"') {
            return Err(format!("expected string at {}", self.at));
        }
        let mut out = String::new();
        loop {
            let c = *self.input.get(self.at).ok_or("unterminated string")?;
            self.at += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = *self.input.get(self.at).ok_or("unterminated escape")?;
                    self.at += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut n = self.hex()?;
                            if (0xd800..=0xdbff).contains(&n) {
                                if self.input.get(self.at..self.at + 2) != Some(b"\\u") {
                                    return Err("missing low surrogate".into());
                                }
                                self.at += 2;
                                let low = self.hex()?;
                                if !(0xdc00..=0xdfff).contains(&low) {
                                    return Err("invalid low surrogate".into());
                                }
                                n = 0x10000 + ((n - 0xd800) << 10) + low - 0xdc00;
                            }
                            out.push(char::from_u32(n).ok_or("invalid Unicode scalar")?);
                        }
                        _ => return Err("invalid escape".into()),
                    }
                }
                0..=31 => return Err("control character in string".into()),
                _ => {
                    self.at -= 1;
                    let s =
                        std::str::from_utf8(&self.input[self.at..]).map_err(|e| e.to_string())?;
                    let ch = s.chars().next().ok_or("invalid UTF-8")?;
                    out.push(ch);
                    self.at += ch.len_utf8();
                }
            }
        }
    }
    fn hex(&mut self) -> Result<u32, String> {
        let s = self
            .input
            .get(self.at..self.at + 4)
            .ok_or("short Unicode escape")?;
        if !s.iter().all(u8::is_ascii_hexdigit) {
            return Err("invalid Unicode escape".into());
        }
        self.at += 4;
        u32::from_str_radix(std::str::from_utf8(s).map_err(|e| e.to_string())?, 16)
            .map_err(|e| e.to_string())
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.input.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        match self.input.get(self.at) {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.at += 1;
                while matches!(self.input.get(self.at), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err("invalid number".into()),
        }
        if self.input.get(self.at) == Some(&b'.') {
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.input.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.input.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let n: f64 = std::str::from_utf8(&self.input[start..self.at])
            .map_err(|e| e.to_string())?
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        if !n.is_finite() {
            return Err("number out of range".into());
        }
        Ok(Json::Number(n))
    }
    fn digits(&mut self) -> Result<(), String> {
        let start = self.at;
        while matches!(self.input.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if start == self.at {
            Err("expected digits".into())
        } else {
            Ok(())
        }
    }
    fn array(&mut self) -> Result<Json, String> {
        self.take(b'[');
        let mut a = Vec::new();
        if self.take(b']') {
            return Ok(Json::Array(a));
        }
        loop {
            a.push(self.value()?);
            if self.take(b']') {
                return Ok(Json::Array(a));
            }
            if !self.take(b',') {
                return Err("expected comma".into());
            }
        }
    }
    fn object(&mut self) -> Result<Json, String> {
        self.take(b'{');
        let mut a = BTreeMap::new();
        if self.take(b'}') {
            return Ok(Json::Object(a));
        }
        loop {
            let k = self.string()?;
            if !self.take(b':') {
                return Err("expected colon".into());
            }
            let v = self.value()?;
            if a.insert(k, v).is_some() {
                return Err("duplicate key".into());
            }
            if self.take(b'}') {
                return Ok(Json::Object(a));
            }
            if !self.take(b',') {
                return Err("expected comma".into());
            }
        }
    }
}
