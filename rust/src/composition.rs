use serde_json::Value;
use thiserror::Error;

use crate::{AgentProfile, Document, object, spec_digest};

/// Pinned reference supplied to a profile composition loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileReference {
    /// Referenced profile name.
    pub name: String,
    /// Referenced profile URI.
    pub uri: String,
    /// Required metadata revision.
    pub revision: u64,
    /// Required canonical profile digest.
    pub digest: String,
}

/// Failure while resolving inherited profiles.
#[derive(Debug, Error)]
pub enum CompositionError {
    /// An inheritance cycle was detected.
    #[error("inheritance cycle")]
    Cycle,
    /// The maximum inheritance depth was exceeded.
    #[error("inheritance depth exceeds 8")]
    Depth,
    /// The loader could not retrieve a referenced profile.
    #[error("{0}")]
    Load(String),
    /// A referenced profile did not match its pinned revision.
    #[error("{0} revision does not match pin")]
    Revision(String),
    /// A referenced profile did not match its pinned digest.
    #[error("{0} digest does not match pin")]
    Digest(String),
    /// A composed profile omitted its required name.
    #[error("composed profile has no name")]
    MissingName,
}

/// Deep-merges a child profile over a base using OAP composition semantics.
pub fn merge_profile_values(base: &Document, child: &Document) -> Document {
    let mut result = base.clone();
    for (key, value) in child {
        if value.is_null() {
            result.remove(key);
        } else if let (Some(left), Some(right)) = (
            result.get(key).and_then(Value::as_object),
            value.as_object(),
        ) {
            result.insert(
                key.clone(),
                Value::Object(merge_profile_values(left, right)),
            );
        } else {
            result.insert(key.clone(), value.clone());
        }
    }
    result
}

/// Resolves and verifies an `extends` chain with the supplied profile loader.
pub fn resolve_composition<F>(
    profile: &AgentProfile,
    mut load: F,
) -> Result<AgentProfile, CompositionError>
where
    F: FnMut(&ProfileReference) -> Result<AgentProfile, CompositionError>,
{
    fn resolve<F>(
        profile: &AgentProfile,
        load: &mut F,
        active: &mut Vec<String>,
    ) -> Result<AgentProfile, CompositionError>
    where
        F: FnMut(&ProfileReference) -> Result<AgentProfile, CompositionError>,
    {
        let name = object(profile.get("metadata"))
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if active.contains(&name) {
            return Err(CompositionError::Cycle);
        }
        if active.len() >= 8 {
            return Err(CompositionError::Depth);
        }
        active.push(name.clone());
        let mut merged = Document::new();
        for raw in profile
            .get("extends")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let value = object(Some(raw));
            let reference = ProfileReference {
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                uri: value
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                revision: value.get("revision").and_then(Value::as_u64).unwrap_or(0),
                digest: value
                    .get("digest")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            };
            let base = load(&reference)?;
            if reference.revision != 0
                && object(base.get("metadata"))
                    .get("revision")
                    .and_then(Value::as_u64)
                    != Some(reference.revision)
            {
                return Err(CompositionError::Revision(reference.name));
            }
            if !reference.digest.is_empty()
                && spec_digest(&base).map_err(|error| CompositionError::Load(error.to_string()))?
                    != reference.digest
            {
                return Err(CompositionError::Digest(reference.name));
            }
            let mut resolved = resolve(&base, load, active)?;
            for key in ["extends", "state", "history"] {
                resolved.remove(key);
            }
            if let Some(metadata) = resolved.get_mut("metadata").and_then(Value::as_object_mut) {
                for key in ["name", "id", "revision"] {
                    metadata.remove(key);
                }
            }
            merged = merge_profile_values(&merged, &resolved);
        }
        active.pop();
        merged = merge_profile_values(&merged, profile);
        merged.insert(
            "metadata".into(),
            profile.get("metadata").cloned().unwrap_or_default(),
        );
        for key in ["state", "history"] {
            if let Some(value) = profile.get(key) {
                merged.insert(key.into(), value.clone());
            } else {
                merged.remove(key);
            }
        }
        merged.remove("extends");
        if object(merged.get("metadata"))
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(CompositionError::MissingName);
        }
        Ok(merged)
    }
    resolve(profile, &mut load, &mut vec![])
}
