use std::{collections::BTreeMap, fs, path::PathBuf};

use chrono::{TimeZone, Utc};
use open_agent_profile::{
    ApplyOptions, OapFormat, PermissionDecision, RenderOptions, apply_delta, canonical_json,
    intersect_tools, load, narrow_decision, parse, profile_digest, render_system_prompt,
    resolve_composition, serialize, spec_digest, validate, validate_path, write_atomically,
};
use serde_json::{Value, json};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn fixture(path: &str) -> open_agent_profile::Document {
    load(repository().join(path)).unwrap()
}

#[test]
fn accepts_profile_and_delta_corpora() {
    for name in [
        "base-reviewer.agent.yaml",
        "code-reviewer.agent.yaml",
        "data-engineer.agent.yaml",
        "note-taker.agent.yaml",
        "python-reviewer.agent.yaml",
        "research-analyst.agent.md",
    ] {
        let report = validate_path(repository().join("examples").join(name));
        assert!(report.ok, "{name}: {:?}", report.errors);
    }
    assert!(validate_path(repository().join("examples/json/note-taker.agent.json")).ok);
    for name in [
        "bad-name.agent.yaml",
        "future-major.agent.yaml",
        "literal-secret.agent.yaml",
        "missing-instructions.agent.yaml",
        "path-traversal.agent.yaml",
        "unknown-field.agent.yaml",
        "unknown-kind.agent.yaml",
    ] {
        assert!(
            !validate_path(repository().join("examples/invalid").join(name)).ok,
            "{name}"
        );
    }
    for name in ["learned-conventions.delta.yaml", "closes-thread.delta.yaml"] {
        assert!(
            validate_path(repository().join("tests/deltas").join(name)).ok,
            "{name}"
        );
    }
    for name in [
        "missing-revision.delta.yaml",
        "proposal-without-rationale.delta.yaml",
        "remove-with-value.delta.yaml",
        "writes-metadata.delta.yaml",
        "writes-spec.delta.yaml",
    ] {
        assert!(
            !validate_path(repository().join("tests/deltas/invalid").join(name)).ok,
            "{name}"
        );
    }
}

#[test]
fn portable_parsing_and_digests() {
    let document = parse(
        "created: 2026-08-28T00:00:00Z\nyes_value: yes\ntrue_value: true\n",
        OapFormat::Yaml,
    )
    .unwrap();
    assert_eq!(document["created"], "2026-08-28T00:00:00Z");
    assert_eq!(document["yes_value"], "yes");
    assert_eq!(document["true_value"], true);
    assert!(parse("a: 1\na: 2\n", OapFormat::Yaml).is_err());
    assert!(parse("", OapFormat::Yaml).is_err());
    let profile = fixture("examples/note-taker.agent.yaml");
    assert_eq!(
        profile_digest(&profile).unwrap(),
        "sha256:32ac424528ddffbbc3c0abeb98b1b18887d5ae5d04425a5466f4191a1b30c1e7"
    );
    assert_eq!(
        spec_digest(&profile).unwrap(),
        "sha256:fe2ddb1be24336d05d2b44ffe05d7bbbbfeb0def69c17503b0d5c931ff42fccc"
    );
    assert_eq!(
        String::from_utf8(canonical_json(&json!({"b": 1, "a": 2})).unwrap()).unwrap(),
        "{\"a\":2,\"b\":1}"
    );
    let mut changed = profile.clone();
    changed.insert("state".into(), json!({"summary": "different"}));
    let metadata = changed
        .get_mut("metadata")
        .unwrap()
        .as_object_mut()
        .unwrap();
    metadata.insert("revision".into(), 99.into());
    metadata.insert("updated_at".into(), "2026-08-28T12:00:00Z".into());
    metadata.insert("trust".into(), "project".into());
    assert_eq!(
        spec_digest(&changed).unwrap(),
        spec_digest(&profile).unwrap()
    );
    changed.insert("oap".into(), "1.1".into());
    assert!(!validate(&changed, None).ok);
}

#[test]
fn policy_render_and_composition_match_other_libraries() {
    assert_eq!(
        narrow_decision(PermissionDecision::Ask, PermissionDecision::Allow),
        PermissionDecision::Ask
    );
    let profile = fixture("examples/code-reviewer.agent.yaml");
    assert_eq!(
        intersect_tools(&profile, ["read", "search", "shell", "write"]).tools,
        ["read", "search"]
    );
    let prompt = render_system_prompt(
        &profile,
        &RenderOptions {
            harness_preamble: "PRE".into(),
            harness_postamble: "POST".into(),
            variables: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut last = 0;
    for part in [
        "PRE",
        "You are a code reviewer",
        "Objectives:",
        "Persona:",
        "Constraints:",
        "Examples:",
        "PROFILE STATE",
        "POST",
    ] {
        let at = prompt.find(part).unwrap();
        assert!(at >= last, "{part}");
        last = at;
    }
    let base = fixture("examples/base-reviewer.agent.yaml");
    let child = fixture("examples/python-reviewer.agent.yaml");
    let resolved = resolve_composition(&child, |_| Ok(base.clone())).unwrap();
    assert_eq!(resolved["metadata"]["name"], child["metadata"]["name"]);
    assert_eq!(resolved.get("state"), child.get("state"));
    assert!(!resolved.contains_key("extends"));
}

#[test]
fn delta_application_is_atomic_and_conflict_safe() {
    let profile = fixture("examples/code-reviewer.agent.yaml");
    let mut delta = fixture("tests/deltas/learned-conventions.delta.yaml");
    let before = profile.clone();
    let now = Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap();
    let result = apply_delta(
        &profile,
        &delta,
        ApplyOptions {
            approved: true,
            actor: Some("alex"),
            now: Some(now),
        },
    )
    .unwrap();
    assert_eq!(result.profile["metadata"]["revision"], 8);
    assert_eq!(profile, before);
    assert_eq!(result.pending_proposals.len(), 1);
    assert_eq!(result.pending_proposals[0]["risk"], "high");
    assert!(apply_delta(&profile, &delta, ApplyOptions::default()).is_err());
    delta.get_mut("target").unwrap()["revision"] = 1.into();
    assert!(
        apply_delta(
            &profile,
            &delta,
            ApplyOptions {
                approved: true,
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("revision")
    );
    let mut digest_conflict = fixture("tests/deltas/learned-conventions.delta.yaml");
    digest_conflict.get_mut("target").unwrap()["digest"] =
        format!("sha256:{}", "0".repeat(64)).into();
    assert!(
        apply_delta(
            &profile,
            &digest_conflict,
            ApplyOptions {
                approved: true,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut failing = fixture("tests/deltas/learned-conventions.delta.yaml");
    failing.insert("operations".into(), json!([{ "op": "replace", "path": "/state/summary", "value": "temporary" }, { "op": "replace", "path": "/state/facts/id:does-not-exist", "value": {} }]));
    assert!(
        apply_delta(
            &profile,
            &failing,
            ApplyOptions {
                approved: true,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(profile, before);
}

#[test]
fn retention_matches_python_and_typescript() {
    let mut profile = fixture("examples/code-reviewer.agent.yaml");
    let retention = profile
        .get_mut("spec")
        .unwrap()
        .get_mut("lifecycle")
        .unwrap()
        .get_mut("retention")
        .unwrap()
        .as_object_mut()
        .unwrap();
    retention.insert("fact_ttl_days".into(), 30.into());
    retention.insert("max_facts".into(), 2.into());
    retention.insert("eviction".into(), "least_confident".into());
    retention.remove("max_history");
    let state = profile.get_mut("state").unwrap().as_object_mut().unwrap();
    state.insert("facts".into(), json!([{ "id": "fresh", "text": "fresh", "expires_at": "2026-09-01T00:00:00Z", "confidence": 0.9 }, { "id": "expired", "text": "expired", "expires_at": "2026-01-01T00:00:00Z", "confidence": 0.1 }, { "id": "pinned", "text": "pinned", "expires_at": "2026-01-01T00:00:00Z", "pinned": true }, { "id": "weak", "text": "weak", "confidence": 0.05 }]));
    let retention = profile
        .get_mut("spec")
        .unwrap()
        .get_mut("lifecycle")
        .unwrap()
        .get_mut("retention")
        .unwrap()
        .as_object_mut()
        .unwrap();
    retention.insert("max_open_threads".into(), 2.into());
    let state = profile.get_mut("state").unwrap().as_object_mut().unwrap();
    state.insert("open_threads".into(), json!([{ "id": "active", "status": "open", "updated_at": "2026-01-03T00:00:00Z" }, { "id": "old-closed", "status": "done", "updated_at": "2026-01-01T00:00:00Z" }, { "id": "new-closed", "status": "abandoned", "updated_at": "2026-01-02T00:00:00Z" }]));
    profile.insert(
        "history".into(),
        Value::Array(
            (1..=55)
                .map(|revision| json!({"revision": revision}))
                .collect(),
        ),
    );
    let mut delta = fixture("tests/deltas/learned-conventions.delta.yaml");
    delta.insert("operations".into(), json!([]));
    let now = Utc.with_ymd_and_hms(2026, 8, 28, 12, 0, 0).unwrap();
    let result = apply_delta(
        &profile,
        &delta,
        ApplyOptions {
            approved: true,
            actor: None,
            now: Some(now),
        },
    )
    .unwrap();
    assert_eq!(
        result.profile["state"]["facts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["fresh", "pinned"]
    );
    assert_eq!(
        result.profile["state"]["open_threads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["active", "new-closed"]
    );
    assert_eq!(result.profile["history"].as_array().unwrap().len(), 50);
}

#[test]
fn markdown_and_atomic_write_round_trip() {
    let profile = fixture("examples/research-analyst.agent.md");
    let encoded = serialize(&profile, OapFormat::Markdown).unwrap();
    assert_eq!(parse(&encoded, OapFormat::Markdown).unwrap(), profile);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profile.yaml");
    fs::write(&path, "old\n").unwrap();
    write_atomically(
        &path,
        serialize(&profile, OapFormat::Yaml).unwrap().as_bytes(),
    )
    .unwrap();
    assert!(validate_path(&path).ok);
}
