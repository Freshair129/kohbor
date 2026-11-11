//! Strict JSON parsing for intake bodies: duplicate object keys and
//! excessive nesting are rejected (GSI-INTAKE-SPEC §3).

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::fmt;

/// Maximum nesting depth accepted for an intake body.
pub const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictJsonError {
    Syntax,
    DuplicateKey,
    TooDeep,
}

struct StrictValue {
    depth: usize,
}

impl<'de> de::DeserializeSeed<'de> for StrictValue {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictValue {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(serde_json::Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(de::Error::custom("too deep"));
        }
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(StrictValue {
            depth: self.depth + 1,
        })? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(de::Error::custom("too deep"));
        }
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            let v = map.next_value_seed(StrictValue {
                depth: self.depth + 1,
            })?;
            out.insert(key, v);
        }
        Ok(Value::Object(out))
    }
}

/// Parse UTF-8 JSON, rejecting duplicate keys and nesting deeper than [`MAX_DEPTH`].
pub fn parse(bytes: &[u8]) -> Result<Value, StrictJsonError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value =
        de::DeserializeSeed::deserialize(StrictValue { depth: 0 }, &mut de).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("duplicate key") {
                StrictJsonError::DuplicateKey
            } else if msg.contains("too deep") || msg.contains("recursion limit") {
                StrictJsonError::TooDeep
            } else {
                StrictJsonError::Syntax
            }
        })?;
    de.end().map_err(|_| StrictJsonError::Syntax)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_object() {
        assert!(parse(br#"{"a":{"b":[1,2,3]},"c":"x"}"#).is_ok());
    }

    #[test]
    fn rejects_duplicate_keys_at_any_depth() {
        assert_eq!(
            parse(br#"{"a":1,"a":2}"#),
            Err(StrictJsonError::DuplicateKey)
        );
        assert_eq!(
            parse(br#"{"p":{"x":1,"x":1}}"#),
            Err(StrictJsonError::DuplicateKey)
        );
    }

    #[test]
    fn rejects_excessive_depth() {
        let deep = "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1);
        assert_eq!(parse(deep.as_bytes()), Err(StrictJsonError::TooDeep));
    }

    #[test]
    fn rejects_trailing_garbage_and_bad_utf8() {
        assert_eq!(parse(br#"{"a":1} x"#), Err(StrictJsonError::Syntax));
        assert_eq!(parse(&[0x7b, 0xff, 0x7d]), Err(StrictJsonError::Syntax));
    }
}
