//! Native Rust support for Open Agent Profile (OAP) 1.0.
//!
//! Includes YAML, JSON, and Markdown encodings; schema and security validation;
//! RFC 8785 identities; composition; policy narrowing; prompt rendering; and
//! atomic, retention-aware state delta application.

#![warn(missing_docs)]

mod canonical;
mod composition;
mod delta;
mod parse;
mod policy;
mod render;
mod validate;

pub use canonical::{CanonicalError, canonical_json, profile_digest, profile_digests, spec_digest};
pub use composition::{
    CompositionError, ProfileReference, merge_profile_values, resolve_composition,
};
pub use delta::{
    ApplyError, ApplyOptions, ConflictError, apply_delta, serialize, write_atomically,
};
pub use parse::{OapFormat, ParseError, load, parse};
pub use policy::{
    EffectiveTools, PermissionDecision, intersect_tools, narrow_decision, narrow_permission_map,
};
pub use render::{RenderError, RenderOptions, render_system_prompt, substitute_variables};
pub use validate::{escapes_workspace, validate, validate_path};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// OAP specification version implemented by this crate.
pub const OAP_VERSION: &str = "1.0";
/// Version of this Rust support library.
pub const SUPPORT_VERSION: &str = "1.0.3";
/// A parsed OAP document represented as a JSON-compatible object.
pub type Document = Map<String, Value>;
/// A parsed `AgentProfile` document.
pub type AgentProfile = Document;
/// A parsed `AgentStateDelta` document.
pub type AgentStateDelta = Document;

/// One validation issue located by JSON Pointer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// JSON Pointer locating the affected value.
    pub pointer: String,
    /// Human-readable explanation of the issue.
    pub message: String,
}

/// Canonical identities for a full profile and its immutable specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digests {
    /// SHA-256 identity of the full canonical profile.
    pub profile: String,
    /// SHA-256 identity of the canonical `spec` member.
    pub spec: String,
}

/// Complete schema, semantic, and security validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Parsed OAP document kind.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Parsed document, absent only when loading or parsing failed.
    pub document: Option<Document>,
    /// Validation failures.
    pub errors: Vec<Issue>,
    /// Non-fatal validation observations.
    pub warnings: Vec<Issue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Canonical identities for a valid `AgentProfile`.
    pub digests: Option<Digests>,
    /// Whether validation completed without errors.
    pub ok: bool,
}

/// A requested policy value that was narrowed by an effective ceiling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Adjustment {
    /// Logical policy field that changed.
    pub field: String,
    /// Value requested by the profile or harness.
    pub requested: Value,
    /// Effective narrowed value.
    pub effective: Value,
    /// Human-readable explanation of the narrowing.
    pub reason: String,
}

/// Successful result of applying an agent state delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaApplication {
    /// New profile value; the input profile is never mutated.
    pub profile: AgentProfile,
    /// Retention and application warnings.
    pub warnings: Vec<String>,
    /// Capability-widening proposals left for human review.
    pub pending_proposals: Vec<Document>,
}

pub(crate) fn object(value: Option<&Value>) -> &Map<String, Value> {
    value.and_then(Value::as_object).unwrap_or_else(|| {
        static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);
        &EMPTY
    })
}

pub(crate) fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
