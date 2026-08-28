use std::{
    cmp::Ordering,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::{AgentProfile, AgentStateDelta, DeltaApplication, Document, object, profile_digest};

/// Failure while validating or applying an agent state delta.
#[derive(Debug, Error)]
pub enum ApplyError {
    /// Optimistic-concurrency target data did not match the profile.
    #[error(transparent)]
    Conflict(#[from] ConflictError),
    /// The delta was invalid or could not be applied or persisted.
    #[error("{0}")]
    Message(String),
}

/// Optimistic-concurrency conflict raised before any mutation is committed.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ConflictError(pub String);

/// Controls approval, attribution, and time for delta application.
#[derive(Default)]
pub struct ApplyOptions<'a> {
    /// Whether a `propose` writeback policy has explicit human approval.
    pub approved: bool,
    /// Actor recorded in the appended history entry.
    pub actor: Option<&'a str>,
    /// Deterministic timestamp override; current UTC time is used when absent.
    pub now: Option<DateTime<Utc>>,
}

fn pointer_tokens(pointer: &str) -> Vec<String> {
    pointer
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn modify(
    current: &mut Value,
    tokens: &[String],
    kind: &str,
    value: Option<&Value>,
) -> Result<bool, ApplyError> {
    if tokens.is_empty() {
        *current = value.cloned().unwrap_or(Value::Null);
        return Ok(false);
    }
    let token = &tokens[0];
    let last = tokens.len() == 1;
    match current {
        Value::Object(map) => {
            if last {
                match kind {
                    "remove" => return Ok(map.remove(token).is_none()),
                    "add" | "replace" => {
                        map.insert(token.clone(), value.cloned().unwrap_or(Value::Null));
                        return Ok(false);
                    }
                    _ => return Err(ApplyError::Message(format!("unknown operation {kind:?}"))),
                }
            }
            if !map.contains_key(token) {
                map.insert(token.clone(), Value::Object(Map::new()));
            }
            modify(map.get_mut(token).unwrap(), &tokens[1..], kind, value)
        }
        Value::Array(items) => {
            let index = if let Some(id) = token.strip_prefix("id:") {
                items.iter().position(|item| {
                    object(Some(item)).get("id").and_then(Value::as_str) == Some(id)
                })
            } else if token == "-" {
                Some(items.len())
            } else {
                token.parse::<usize>().ok()
            };
            let Some(index) = index else {
                return Ok(true);
            };
            if last {
                match kind {
                    "add" if index <= items.len() => {
                        items.insert(index, value.cloned().unwrap_or(Value::Null));
                        Ok(false)
                    }
                    "remove" if index < items.len() => {
                        items.remove(index);
                        Ok(false)
                    }
                    "replace" if index < items.len() => {
                        items[index] = value.cloned().unwrap_or(Value::Null);
                        Ok(false)
                    }
                    "add" | "remove" | "replace" => Ok(true),
                    _ => Err(ApplyError::Message(format!("unknown operation {kind:?}"))),
                }
            } else if index < items.len() {
                modify(&mut items[index], &tokens[1..], kind, value)
            } else {
                Ok(true)
            }
        }
        _ => Ok(true),
    }
}

fn apply_operation(
    document: &mut Document,
    operation: &Document,
    warnings: &mut Vec<String>,
) -> Result<(), ApplyError> {
    let kind = operation
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let pointer = operation
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if pointer != "/state" && !pointer.starts_with("/state/") {
        return Err(ApplyError::Message(format!(
            "operation path {pointer:?} is outside /state"
        )));
    }
    let mut root = Value::Object(std::mem::take(document));
    let missing = modify(
        &mut root,
        &pointer_tokens(pointer),
        kind,
        operation.get("value"),
    )?;
    *document = root.as_object().cloned().unwrap_or_default();
    if missing {
        if kind == "remove" {
            warnings.push(format!("remove on missing path {pointer:?} ignored"));
            Ok(())
        } else {
            Err(ApplyError::Message(format!(
                "path {pointer:?} does not resolve"
            )))
        }
    } else {
        Ok(())
    }
}

/// Validates and atomically applies state operations to a cloned profile.
///
/// Capability-widening proposals are returned separately and never applied.
pub fn apply_delta(
    profile: &AgentProfile,
    delta: &AgentStateDelta,
    options: ApplyOptions<'_>,
) -> Result<DeltaApplication, ApplyError> {
    let metadata = object(profile.get("metadata"));
    let current = metadata
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let target = object(delta.get("target"));
    if target.get("name").and_then(Value::as_str) != metadata.get("name").and_then(Value::as_str) {
        return Err(ApplyError::Message(format!(
            "delta targets {:?} but profile is {:?}",
            target.get("name"),
            metadata.get("name")
        )));
    }
    if target.get("revision").and_then(Value::as_u64) != Some(current) {
        return Err(ConflictError(format!(
            "delta targets revision {:?} but profile is at {current}",
            target.get("revision")
        ))
        .into());
    }
    if let Some(pinned) = target.get("digest").and_then(Value::as_str) {
        if profile_digest(profile).map_err(|error| ApplyError::Message(error.to_string()))?
            != pinned
        {
            return Err(ApplyError::Message(
                "target.digest does not match profile".into(),
            ));
        }
    }
    let lifecycle = object(object(profile.get("spec")).get("lifecycle"));
    let writeback = lifecycle
        .get("writeback")
        .and_then(Value::as_str)
        .unwrap_or("propose");
    if writeback == "off" {
        return Err(ApplyError::Message("lifecycle.writeback is 'off'".into()));
    }
    if writeback == "propose" && !options.approved {
        return Err(ApplyError::Message(
            "lifecycle.writeback is 'propose'; explicit approval is required".into(),
        ));
    }
    let mut working = profile.clone();
    working
        .entry("state")
        .or_insert_with(|| Value::Object(Map::new()));
    let mut warnings = vec![];
    let operations = delta
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, raw) in operations.iter().enumerate() {
        apply_operation(
            &mut working,
            &raw.as_object().cloned().unwrap_or_default(),
            &mut warnings,
        )
        .map_err(|error| ApplyError::Message(format!("operation {index}: {error}")))?;
    }
    let stamp = options
        .now
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    enforce_retention(&mut working, &mut warnings, &stamp);
    if let Some(metadata) = working.get_mut("metadata").and_then(Value::as_object_mut) {
        metadata.insert("revision".into(), (current + 1).into());
        metadata.insert("updated_at".into(), stamp.clone().into());
    }
    if !operations.is_empty() {
        if let Some(state) = working.get_mut("state").and_then(Value::as_object_mut) {
            let revision = state.get("revision").and_then(Value::as_u64).unwrap_or(0) + 1;
            state.insert("revision".into(), revision.into());
            state.insert("updated_at".into(), stamp.clone().into());
        }
    }
    let session = object(delta.get("session"));
    let actor = options.actor.unwrap_or("oap-rust");
    let by = session.get("id").and_then(Value::as_str).unwrap_or(actor);
    let mut entry = Map::from_iter([
        ("revision".into(), (current + 1).into()),
        ("at".into(), stamp.clone().into()),
        ("by".into(), by.into()),
        (
            "change".into(),
            delta
                .get("summary")
                .cloned()
                .unwrap_or_else(|| format!("{} state operations", operations.len()).into()),
        ),
        ("sections".into(), json!(["state"])),
    ]);
    if let Some(id) = session.get("id") {
        entry.insert("session_id".into(), id.clone());
    }
    if let Some(harness) = session.get("harness") {
        entry.insert("harness".into(), harness.clone());
    }
    if options.approved {
        entry.insert("approved_by".into(), actor.into());
    }
    working
        .entry("history")
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .unwrap()
        .push(Value::Object(entry));
    enforce_retention(&mut working, &mut warnings, &stamp);
    let pending_proposals = delta
        .get("proposals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|raw| raw.as_object().cloned())
        .map(|mut proposal| {
            let path = proposal
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if [
                "/spec/tools",
                "/spec/permissions",
                "/spec/memory",
                "/spec/runtime/subagents",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix))
            {
                proposal.insert("risk".into(), "high".into());
            }
            proposal
        })
        .collect();
    Ok(DeltaApplication {
        profile: working,
        warnings,
        pending_proposals,
    })
}

fn sort_value(entry: &Value, strategy: &str) -> Value {
    let object = object(Some(entry));
    if strategy == "least_confident" {
        object
            .get("confidence")
            .cloned()
            .unwrap_or_else(|| json!(1.0))
    } else if strategy == "oldest" {
        object
            .get("learned_at")
            .or_else(|| object.get("opened_at"))
            .cloned()
            .unwrap_or_default()
    } else {
        ["last_used_at", "updated_at", "learned_at"]
            .iter()
            .find_map(|key| object.get(*key))
            .cloned()
            .unwrap_or_default()
    }
}
fn compare(left: &Value, right: &Value) -> Ordering {
    match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
        _ => left
            .as_str()
            .unwrap_or_default()
            .cmp(right.as_str().unwrap_or_default()),
    }
}

fn enforce_retention(profile: &mut AgentProfile, warnings: &mut Vec<String>, now: &str) {
    let retention =
        object(object(object(profile.get("spec")).get("lifecycle")).get("retention")).clone();
    let strategy = retention
        .get("eviction")
        .and_then(Value::as_str)
        .unwrap_or("least_recently_used");
    if let Some(state) = profile.get_mut("state").and_then(Value::as_object_mut) {
        for collection in ["facts", "preferences"] {
            let Some(entries) = state.get_mut(collection).and_then(Value::as_array_mut) else {
                continue;
            };
            entries.retain(|raw| {
                let entry = object(Some(raw));
                let expired = retention.contains_key("fact_ttl_days")
                    && entry
                        .get("expires_at")
                        .and_then(Value::as_str)
                        .is_some_and(|expiry| expiry < now)
                    && entry.get("pinned") != Some(&Value::Bool(true));
                if expired {
                    warnings.push(format!(
                        "evicted expired {collection} entry {:?}",
                        entry.get("id").and_then(Value::as_str).unwrap_or_default()
                    ));
                }
                !expired
            });
            if collection == "facts" {
                if let Some(cap) = retention
                    .get("max_facts")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                {
                    if entries.len() > cap {
                        let mut candidates: Vec<_> = entries
                            .iter()
                            .filter(|entry| {
                                object(Some(entry)).get("pinned") != Some(&Value::Bool(true))
                            })
                            .cloned()
                            .collect();
                        candidates.sort_by(|a, b| {
                            compare(&sort_value(a, strategy), &sort_value(b, strategy))
                        });
                        let drop_count = entries.len().saturating_sub(cap).min(candidates.len());
                        let dropped: BTreeIds = candidates
                            .into_iter()
                            .take(drop_count)
                            .filter_map(|entry| {
                                object(Some(&entry))
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                            })
                            .collect();
                        for id in &dropped {
                            warnings
                                .push(format!("evicted {collection} entry {id:?} ({strategy})"));
                        }
                        entries.retain(|entry| {
                            !object(Some(entry))
                                .get("id")
                                .and_then(Value::as_str)
                                .is_some_and(|id| dropped.contains(id))
                        });
                    }
                }
            }
        }
        if let Some(cap) = retention
            .get("max_open_threads")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
        {
            if let Some(threads) = state.get_mut("open_threads").and_then(Value::as_array_mut) {
                if threads.len() > cap {
                    let mut closed: Vec<_> = threads
                        .iter()
                        .filter(|thread| {
                            matches!(
                                object(Some(thread)).get("status").and_then(Value::as_str),
                                Some("done" | "abandoned")
                            )
                        })
                        .cloned()
                        .collect();
                    closed.sort_by_key(|thread| {
                        object(Some(thread))
                            .get("updated_at")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    });
                    let dropped: BTreeIds = closed
                        .into_iter()
                        .take((threads.len() - cap).min(threads.len()))
                        .filter_map(|entry| {
                            object(Some(&entry))
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                        .collect();
                    for id in &dropped {
                        warnings.push(format!("evicted closed thread {id:?}"));
                    }
                    threads.retain(|entry| {
                        !object(Some(entry))
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| dropped.contains(id))
                    });
                    if threads.len() > cap {
                        let remove = threads.len() - cap;
                        threads.drain(..remove);
                        warnings.push(
                            "open_threads still over cap after evicting closed threads".into(),
                        );
                    }
                }
            }
        }
    }
    let history_cap = retention
        .get("max_history")
        .and_then(Value::as_u64)
        .unwrap_or(50) as usize;
    if let Some(history) = profile.get_mut("history").and_then(Value::as_array_mut) {
        if history.len() > history_cap {
            let remove = history.len() - history_cap;
            history.drain(..remove);
        }
    }
}

type BTreeIds = std::collections::BTreeSet<String>;

/// Serializes an OAP document as YAML, JSON, or Markdown.
pub fn serialize(document: &Document, format: crate::OapFormat) -> Result<String, ApplyError> {
    match format {
        crate::OapFormat::Json => serde_json::to_string_pretty(document)
            .map(|text| text + "\n")
            .map_err(|error| ApplyError::Message(error.to_string())),
        crate::OapFormat::Yaml => serde_yaml_ng::to_string(document)
            .map_err(|error| ApplyError::Message(error.to_string())),
        crate::OapFormat::Markdown => {
            let mut copy = document.clone();
            let instructions = copy
                .get_mut("spec")
                .and_then(Value::as_object_mut)
                .and_then(|spec| spec.get_mut("role"))
                .and_then(Value::as_object_mut)
                .and_then(|role| role.remove("instructions"))
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            let yaml = serde_yaml_ng::to_string(&copy)
                .map_err(|error| ApplyError::Message(error.to_string()))?;
            Ok(format!(
                "---\n{}---\n{}\n",
                yaml.trim_start_matches("---\n"),
                instructions.trim_end()
            ))
        }
    }
}

/// Replaces a file atomically using a temporary file in the same directory.
pub fn write_atomically(path: impl AsRef<Path>, data: &[u8]) -> Result<(), ApplyError> {
    let path = path.as_ref();
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("oap"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        if let Ok(directory) = fs::File::open(directory) {
            directory.sync_all()?;
        }
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| ApplyError::Message(error.to_string()))
}
