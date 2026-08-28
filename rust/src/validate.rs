use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use regex::Regex;
use serde_json::Value;

use crate::{Digests, Document, Issue, ValidationReport, load, object, profile_digests, strings};

const PROFILE_SCHEMA: &str = include_str!("../schema/agent-profile.schema.json");
const DELTA_SCHEMA: &str = include_str!("../schema/agent-state-delta.schema.json");

fn issue(target: &mut Vec<Issue>, pointer: impl Into<String>, message: impl Into<String>) {
    target.push(Issue {
        pointer: pointer.into(),
        message: message.into(),
    });
}

/// Runs schema, semantic, and security validation on a parsed OAP document.
pub fn validate(document: &Document, filename: Option<&Path>) -> ValidationReport {
    let mut errors = vec![];
    let mut warnings = vec![];
    let version = document
        .get("oap")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let pieces: Vec<&str> = version.split('.').collect();
    if pieces.len() != 2 || pieces.iter().any(|part| part.parse::<u64>().is_err()) {
        issue(
            &mut errors,
            "/oap",
            "missing or malformed spec version string",
        );
    } else if pieces != ["1", "0"] {
        issue(
            &mut errors,
            "/oap",
            format!("unsupported OAP version {version}; unsupported versions fail closed"),
        );
    }
    let kind = document
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(kind, "AgentProfile" | "AgentStateDelta") {
        issue(
            &mut errors,
            "/kind",
            format!("{kind:?} is not a known 1.x kind"),
        );
    }
    let schema_text = if kind == "AgentStateDelta" {
        DELTA_SCHEMA
    } else {
        PROFILE_SCHEMA
    };
    if let Ok(schema) = serde_json::from_str(schema_text) {
        if let Ok(validator) = jsonschema::validator_for(&schema) {
            let instance = Value::Object(document.clone());
            for error in validator.iter_errors(&instance) {
                issue(
                    &mut errors,
                    error.instance_path().to_string(),
                    error.to_string(),
                );
            }
        }
    }
    check_literal_secrets(Value::Object(document.clone()), "", &mut errors);
    if kind == "AgentProfile" {
        check_profile(document, filename, &mut errors, &mut warnings);
    } else if kind == "AgentStateDelta" {
        check_delta(document, &mut errors, &mut warnings);
    }
    let digests: Option<Digests> = if kind == "AgentProfile" && errors.is_empty() {
        profile_digests(document).ok()
    } else {
        None
    };
    ValidationReport {
        kind: kind.into(),
        document: Some(document.clone()),
        ok: errors.is_empty(),
        errors,
        warnings,
        digests,
    }
}

/// Loads and validates an OAP document, returning parse failures as issues.
pub fn validate_path(path: impl AsRef<Path>) -> ValidationReport {
    let path = path.as_ref();
    match load(path) {
        Ok(document) => validate(&document, Some(path)),
        Err(error) => ValidationReport {
            kind: String::new(),
            document: None,
            errors: vec![Issue {
                pointer: String::new(),
                message: error.to_string(),
            }],
            warnings: vec![],
            digests: None,
            ok: false,
        },
    }
}

fn walk_strings(value: &Value, pointer: &str, visit: &mut impl FnMut(&str, &str)) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                walk_strings(
                    child,
                    &format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                    visit,
                );
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                walk_strings(child, &format!("{pointer}/{index}"), visit);
            }
        }
        Value::String(text) => visit(text, pointer),
        _ => {}
    }
}

fn check_literal_secrets(value: Value, pointer: &str, errors: &mut Vec<Issue>) {
    let patterns = [
        (r"AKIA[0-9A-Z]{16}", "AWS access key"),
        (r"gh[pousr]_[A-Za-z0-9_]{20,}", "GitHub token"),
        (r"sk-[A-Za-z0-9]{20,}", "API key"),
        (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "private key"),
    ];
    let compiled: Vec<_> = patterns
        .into_iter()
        .map(|(pattern, label)| (Regex::new(pattern).unwrap(), label))
        .collect();
    walk_strings(&value, pointer, &mut |text, location| {
        for (pattern, label) in &compiled {
            if pattern.is_match(text) {
                issue(
                    errors,
                    location,
                    format!("looks like a literal {label}; use a ${{VARIABLE}} reference"),
                );
            }
        }
    });
}

/// Returns whether a relative path lexically escapes its workspace root.
pub fn escapes_workspace(path: &str) -> bool {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return true;
    }
    let mut depth = 0_i64;
    for component in candidate.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            Component::Normal(_) => depth += 1,
            _ => {}
        }
    }
    false
}

fn check_profile(
    document: &Document,
    filename: Option<&Path>,
    errors: &mut Vec<Issue>,
    warnings: &mut Vec<Issue>,
) {
    let spec = object(document.get("spec"));
    let context = object(spec.get("context"));
    let env = Regex::new(r"^\$\{[A-Z][A-Z0-9_]{0,63}\}$").unwrap();
    let header = Regex::new(r"^(Bearer )?\$\{[A-Z][A-Z0-9_]{0,63}\}$").unwrap();
    for (index, raw) in object(spec.get("tools"))
        .get("mcp_servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let server = object(Some(raw));
        for (key, value) in object(server.get("env")) {
            let text = value.as_str().unwrap_or_default();
            if !env.is_match(text) || text != format!("${{{key}}}") {
                issue(
                    errors,
                    format!("/spec/tools/mcp_servers/{index}/env/{key}"),
                    "must be a same-name ${VARIABLE} reference, not a literal",
                );
            }
        }
        for (key, value) in object(server.get("headers")) {
            if !value.as_str().is_some_and(|text| header.is_match(text)) {
                issue(
                    errors,
                    format!("/spec/tools/mcp_servers/{index}/headers/{key}"),
                    "must be '${VARIABLE}' or 'Bearer ${VARIABLE}'",
                );
            }
        }
    }
    for (index, raw) in context
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(path) = object(Some(raw)).get("path").and_then(Value::as_str) {
            if escapes_workspace(path) {
                issue(
                    errors,
                    format!("/spec/context/files/{index}/path"),
                    format!("{path:?} resolves outside the workspace"),
                );
            }
        }
    }
    if let Some(directory) = context.get("working_directory").and_then(Value::as_str) {
        if escapes_workspace(directory) {
            issue(
                errors,
                "/spec/context/working_directory",
                format!("{directory:?} resolves outside the workspace"),
            );
        }
    }
    let variables: BTreeSet<String> = object(context.get("variables")).keys().cloned().collect();
    let variable = Regex::new(r"\$\{\{\s*vars\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}").unwrap();
    walk_strings(
        &Value::Object(document.clone()),
        "",
        &mut |text, pointer| {
            for capture in variable.captures_iter(text) {
                if !variables.contains(&capture[1]) {
                    issue(
                        errors,
                        pointer,
                        format!("references undefined variable {:?}", &capture[1]),
                    );
                }
                if pointer.starts_with("/state") {
                    issue(
                        warnings,
                        pointer,
                        "contains a ${{ vars.* }} template; substitution never runs inside state",
                    );
                }
            }
        },
    );
    let state = object(document.get("state"));
    for collection in ["facts", "preferences", "open_threads", "glossary"] {
        let mut ids = BTreeSet::new();
        for (index, raw) in state
            .get(collection)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(id) = object(Some(raw)).get("id").and_then(Value::as_str) {
                if !ids.insert(id) {
                    issue(
                        errors,
                        format!("/state/{collection}/{index}/id"),
                        format!("duplicate id {id:?}"),
                    );
                }
            }
        }
    }
    let metadata = object(document.get("metadata"));
    if metadata.contains_key("trust") {
        issue(
            warnings,
            "/metadata/trust",
            "trust in the file must be discarded and recomputed from the discovery root",
        );
    }
    if let (Some(path), Some(name)) = (filename, metadata.get("name").and_then(Value::as_str)) {
        let base = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .split('.')
            .next()
            .unwrap_or_default();
        if base != name {
            issue(
                warnings,
                "/metadata/name",
                format!("{name:?} does not match file name {base:?}; metadata.name wins"),
            );
        }
    }
    let history = document
        .get("history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut previous = 0;
    for entry in &history {
        let revision = object(Some(entry))
            .get("revision")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if revision < previous {
            issue(
                errors,
                "/history",
                "entries must be ordered oldest first by revision",
            );
            break;
        }
        previous = revision;
    }
    if previous
        > metadata
            .get("revision")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    {
        issue(
            errors,
            "/history",
            "newest history revision exceeds metadata.revision",
        );
    }
    let tools = object(spec.get("tools"));
    let policy = tools
        .get("policy")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if policy == "inherit"
        && (!strings(tools.get("allow")).is_empty() || !strings(tools.get("deny")).is_empty())
    {
        issue(
            warnings,
            "/spec/tools",
            "policy is 'inherit', so allow and deny are ignored",
        );
    }
    if policy == "allowlist" && strings(tools.get("allow")).is_empty() {
        issue(
            warnings,
            "/spec/tools",
            "allowlist has an empty allow list, so the agent gets no tools",
        );
    }
}

fn check_delta(document: &Document, errors: &mut Vec<Issue>, warnings: &mut Vec<Issue>) {
    for (index, raw) in document
        .get("operations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let path = object(Some(raw))
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path != "/state" && !path.starts_with("/state/") {
            issue(
                errors,
                format!("/operations/{index}/path"),
                "operation is outside /state; contract changes belong in proposals",
            );
        }
    }
    for (index, raw) in document
        .get("proposals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let proposal = object(Some(raw));
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
            && proposal.get("risk").and_then(Value::as_str) != Some("high")
        {
            issue(
                warnings,
                format!("/proposals/{index}"),
                format!("{path} must be treated as high risk regardless of its declared risk"),
            );
        }
    }
}
