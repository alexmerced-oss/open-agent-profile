use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{AgentProfile, Digests, Document, object};

/// Failure while producing canonical JSON.
#[derive(Debug, Error)]
pub enum CanonicalError {
    /// The value could not be serialized.
    #[error("canonical JSON error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Serializes a value using RFC 8785 JSON Canonicalization Scheme rules.
pub fn canonical_json(value: &impl serde::Serialize) -> Result<Vec<u8>, CanonicalError> {
    Ok(serde_jcs::to_vec(value)?)
}
fn digest(value: &impl serde::Serialize) -> Result<String, CanonicalError> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(canonical_json(value)?)
    ))
}
/// Computes the `sha256:<hex>` identity of the full profile.
pub fn profile_digest(profile: &AgentProfile) -> Result<String, CanonicalError> {
    digest(profile)
}
/// Computes the `sha256:<hex>` identity of the profile's `spec` member.
pub fn spec_digest(profile: &AgentProfile) -> Result<String, CanonicalError> {
    let mut metadata = object(profile.get("metadata")).clone();
    for key in ["revision", "updated_at", "trust"] {
        metadata.remove(key);
    }
    let mut identity = Document::new();
    identity.insert("metadata".into(), metadata.into());
    identity.insert(
        "spec".into(),
        profile.get("spec").cloned().unwrap_or_default(),
    );
    digest(&identity)
}
/// Computes both full-profile and specification identities.
pub fn profile_digests(profile: &AgentProfile) -> Result<Digests, CanonicalError> {
    Ok(Digests {
        profile: profile_digest(profile)?,
        spec: spec_digest(profile)?,
    })
}
