use std::{fs, path::Path};

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use thiserror::Error;

use crate::{Document, object};

/// Supported OAP document encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OapFormat {
    /// YAML encoding.
    Yaml,
    /// JSON encoding.
    Json,
    /// YAML frontmatter plus Markdown instructions.
    Markdown,
}

impl OapFormat {
    /// Infers an encoding from `.json` or `.md`, defaulting to YAML.
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "json" => Self::Json,
            "md" => Self::Markdown,
            _ => Self::Yaml,
        }
    }
}

/// OAP loading or parsing failure.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ParseError(pub String);

/// Parses an OAP document with duplicate-key rejection.
pub fn parse(input: &str, format: OapFormat) -> Result<Document, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError("parse error: empty document".into()));
    }
    if format == OapFormat::Markdown {
        let rest = input.strip_prefix("---\n").ok_or_else(|| {
            ParseError("Markdown profile must begin with YAML frontmatter".into())
        })?;
        let (frontmatter, body) = rest
            .split_once("\n---\n")
            .ok_or_else(|| ParseError("unterminated Markdown frontmatter".into()))?;
        let mut document = parse(frontmatter, OapFormat::Yaml)?;
        let role = document
            .get_mut("spec")
            .and_then(Value::as_object_mut)
            .and_then(|spec| spec.get_mut("role"))
            .and_then(Value::as_object_mut);
        if let Some(role) = role {
            if role.contains_key("instructions") && !body.trim().is_empty() {
                return Err(ParseError("Markdown encoding supplies spec.role.instructions in both frontmatter and body".into()));
            }
            role.insert("instructions".into(), Value::String(body.trim().into()));
        }
        return Ok(document);
    }
    let value: Value = match format {
        OapFormat::Json => serde_json::from_str(input)
            .map_err(|error| ParseError(format!("parse error: {error}")))?,
        OapFormat::Yaml => serde_yaml_ng::from_str::<UniqueValue>(input)
            .map(|value| value.0)
            .map_err(|error| ParseError(format!("parse error: {error}")))?,
        OapFormat::Markdown => unreachable!(),
    };
    value
        .as_object()
        .cloned()
        .ok_or_else(|| ParseError("document root must be an object".into()))
}

/// Loads an OAP document and infers its encoding from the path.
pub fn load(path: impl AsRef<Path>) -> Result<Document, ParseError> {
    let path = path.as_ref();
    let input = fs::read_to_string(path).map_err(|error| ParseError(error.to_string()))?;
    parse(&input, OapFormat::from_path(path))
}

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a JSON-compatible YAML value")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(v.into())))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(v.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Number::from_f64(v)
                    .map(Value::Number)
                    .map(UniqueValue)
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v.into())))
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v)))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = vec![];
                while let Some(v) = seq.next_element::<UniqueValue>()? {
                    out.push(v.0);
                }
                Ok(UniqueValue(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = Map::new();
                while let Some((key, value)) = map.next_entry::<String, UniqueValue>()? {
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!("duplicate key {key:?}")));
                    }
                    out.insert(key, value.0);
                }
                Ok(UniqueValue(Value::Object(out)))
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

#[allow(dead_code)]
fn _object(document: &Document) {
    let _ = object(document.get("spec"));
}
