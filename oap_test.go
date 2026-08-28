package oap

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func fixture(t *testing.T, path string) Document {
	t.Helper()
	document, err := Load(path)
	if err != nil {
		t.Fatal(err)
	}
	return document
}
func TestProfileAndDeltaCorpora(t *testing.T) {
	for _, name := range []string{"base-reviewer.agent.yaml", "code-reviewer.agent.yaml", "data-engineer.agent.yaml", "note-taker.agent.yaml", "python-reviewer.agent.yaml", "research-analyst.agent.md"} {
		t.Run(name, func(t *testing.T) {
			report := Validate(fixture(t, filepath.Join("examples", name)))
			if !report.OK {
				t.Fatalf("%+v", report.Errors)
			}
		})
	}
	if !Validate(fixture(t, "examples/json/note-taker.agent.json")).OK {
		t.Fatal("JSON profile rejected")
	}
	for _, name := range []string{"bad-name.agent.yaml", "future-major.agent.yaml", "literal-secret.agent.yaml", "missing-instructions.agent.yaml", "path-traversal.agent.yaml", "unknown-field.agent.yaml", "unknown-kind.agent.yaml"} {
		t.Run(name, func(t *testing.T) {
			if Validate(fixture(t, filepath.Join("examples", "invalid", name))).OK {
				t.Fatal("expected rejection")
			}
		})
	}
	for _, name := range []string{"learned-conventions.delta.yaml", "closes-thread.delta.yaml"} {
		if !Validate(fixture(t, filepath.Join("tests", "deltas", name))).OK {
			t.Fatal(name)
		}
	}
	for _, name := range []string{"missing-revision.delta.yaml", "proposal-without-rationale.delta.yaml", "remove-with-value.delta.yaml", "writes-metadata.delta.yaml", "writes-spec.delta.yaml"} {
		if Validate(fixture(t, filepath.Join("tests", "deltas", "invalid", name))).OK {
			t.Fatal(name)
		}
	}
}
func TestDigests(t *testing.T) {
	profile := fixture(t, "examples/note-taker.agent.yaml")
	p, _ := ProfileDigest(profile)
	s, _ := SpecDigest(profile)
	if p != "sha256:32ac424528ddffbbc3c0abeb98b1b18887d5ae5d04425a5466f4191a1b30c1e7" {
		t.Fatal(p)
	}
	if s != "sha256:fe2ddb1be24336d05d2b44ffe05d7bbbbfeb0def69c17503b0d5c931ff42fccc" {
		t.Fatal(s)
	}
}
func TestYAMLAndMarkdown(t *testing.T) {
	document, err := Parse([]byte("created: 2026-08-28T00:00:00Z\nyes_value: yes\ntrue_value: true\n"), "yaml")
	if err != nil {
		t.Fatal(err)
	}
	if document["created"] != "2026-08-28T00:00:00Z" || document["yes_value"] != "yes" || document["true_value"] != true {
		t.Fatalf("%#v", document)
	}
	if _, err := Parse([]byte("a: 1\na: 2\n"), "yaml"); err == nil {
		t.Fatal("duplicate accepted")
	}
	if _, err := Parse(nil, "yaml"); err == nil {
		t.Fatal("empty document accepted")
	}
	profile := fixture(t, "examples/research-analyst.agent.md")
	encoded, err := Serialize(profile, "markdown")
	if err != nil {
		t.Fatal(err)
	}
	parsed, err := Parse([]byte(encoded), "markdown")
	if err != nil {
		t.Fatal(err)
	}
	if text(obj(parsed["metadata"])["name"]) != text(obj(profile["metadata"])["name"]) {
		t.Fatal("round trip")
	}
}

func TestFailClosedVersionAndStableSpecIdentity(t *testing.T) {
	profile := fixture(t, "examples/note-taker.agent.yaml")
	changed := AgentProfile(cloneMap(profile))
	changed["oap"] = "1.1"
	if Validate(changed).OK {
		t.Fatal("unsupported minor version accepted")
	}
	changed = AgentProfile(cloneMap(profile))
	changed["state"] = map[string]any{"summary": "different learned state"}
	metadata := obj(changed["metadata"])
	metadata["revision"] = 99
	metadata["updated_at"] = "2026-08-28T12:00:00Z"
	metadata["trust"] = "project"
	originalDigest, err := SpecDigest(profile)
	if err != nil {
		t.Fatal(err)
	}
	changedDigest, err := SpecDigest(changed)
	if err != nil {
		t.Fatal(err)
	}
	if changedDigest != originalDigest {
		t.Fatalf("state-only change altered spec digest: %s != %s", changedDigest, originalDigest)
	}
	canonical, err := CanonicalJSON(Document{"b": 1, "a": 2})
	if err != nil || string(canonical) != `{"a":2,"b":1}` {
		t.Fatalf("canonical JSON: %q, %v", canonical, err)
	}
}
func TestPolicyRenderAndComposition(t *testing.T) {
	if NarrowDecision(Ask, Allow) != Ask {
		t.Fatal("narrowing")
	}
	profile := fixture(t, "examples/code-reviewer.agent.yaml")
	tools := IntersectTools(profile, []string{"read", "search", "shell", "write"})
	if strings.Join(tools.Tools, ",") != "read,search" {
		t.Fatalf("%v", tools.Tools)
	}
	prompt, err := RenderSystemPrompt(profile, RenderOptions{HarnessPreamble: "PRE", HarnessPostamble: "POST"})
	if err != nil {
		t.Fatal(err)
	}
	last := -1
	for _, part := range []string{"PRE", "You are a code reviewer", "Objectives:", "Persona:", "Constraints:", "Examples:", "PROFILE STATE", "POST"} {
		at := strings.Index(prompt, part)
		if at <= last {
			t.Fatalf("bad prompt order at %s", part)
		}
		last = at
	}
	base := fixture(t, "examples/base-reviewer.agent.yaml")
	child := fixture(t, "examples/python-reviewer.agent.yaml")
	resolved, err := ResolveComposition(child, func(ProfileReference) (AgentProfile, error) { return base, nil })
	if err != nil {
		t.Fatal(err)
	}
	if text(obj(resolved["metadata"])["name"]) != text(obj(child["metadata"])["name"]) {
		t.Fatal("identity inherited")
	}
	if _, exists := resolved["extends"]; exists {
		t.Fatal("extends retained after composition")
	}
	resolvedState, _ := json.Marshal(resolved["state"])
	childState, _ := json.Marshal(child["state"])
	if string(resolvedState) != string(childState) {
		t.Fatal("state inherited from base")
	}
}
func TestDeltaSafetyAndRetention(t *testing.T) {
	profile := fixture(t, "examples/code-reviewer.agent.yaml")
	delta := fixture(t, "tests/deltas/learned-conventions.delta.yaml")
	before := intValue(obj(profile["metadata"])["revision"])
	stamp := func() time.Time { return time.Date(2026, 8, 28, 12, 0, 0, 0, time.UTC) }
	result, err := ApplyDelta(profile, delta, ApplyOptions{Approved: true, Actor: "alex", Now: stamp})
	if err != nil {
		t.Fatal(err)
	}
	if intValue(obj(result.Profile["metadata"])["revision"]) != before+1 {
		t.Fatal("revision")
	}
	if intValue(obj(profile["metadata"])["revision"]) != before {
		t.Fatal("input mutated")
	}
	if len(result.PendingProposals) != 1 || text(result.PendingProposals[0]["risk"]) != "high" {
		t.Fatal("proposal gate")
	}
	wrong := AgentStateDelta(cloneMap(delta))
	obj(wrong["target"])["revision"] = 1
	if _, err := ApplyDelta(profile, wrong, ApplyOptions{Approved: true}); err == nil {
		t.Fatal("conflict accepted")
	} else {
		var conflict *ConflictError
		if !errors.As(err, &conflict) {
			t.Fatalf("wanted ConflictError, got %T", err)
		}
	}
	wrongDigest := AgentStateDelta(cloneMap(delta))
	obj(wrongDigest["target"])["digest"] = "sha256:" + strings.Repeat("0", 64)
	if _, err := ApplyDelta(profile, wrongDigest, ApplyOptions{Approved: true}); err == nil {
		t.Fatal("digest conflict accepted")
	}
	beforeFailure, _ := json.Marshal(profile)
	failing := AgentStateDelta(cloneMap(delta))
	failing["operations"] = []any{
		map[string]any{"op": "replace", "path": "/state/summary", "value": "temporary"},
		map[string]any{"op": "replace", "path": "/state/facts/id:does-not-exist", "value": map[string]any{}},
	}
	if _, err := ApplyDelta(profile, failing, ApplyOptions{Approved: true}); err == nil {
		t.Fatal("invalid later operation accepted")
	}
	afterFailure, _ := json.Marshal(profile)
	if string(beforeFailure) != string(afterFailure) {
		t.Fatal("input mutated after failed atomic application")
	}
	retention := obj(obj(obj(profile["spec"])["lifecycle"])["retention"])
	retention["max_facts"] = 2
	state := obj(profile["state"])
	state["facts"] = []any{map[string]any{"id": "fresh", "text": "fresh", "confidence": 0.9}, map[string]any{"id": "weak", "text": "weak", "confidence": 0.05}, map[string]any{"id": "pinned", "text": "pinned", "pinned": true}}
	delta["operations"] = []any{}
	result, err = ApplyDelta(profile, delta, ApplyOptions{Approved: true, Now: stamp})
	if err != nil {
		t.Fatal(err)
	}
	if len(arr(obj(result.Profile["state"])["facts"])) != 2 {
		t.Fatal("retention")
	}
	encoded, err := Serialize(result.Profile, "yaml")
	if err != nil {
		t.Fatal(err)
	}
	reparsed, err := Parse([]byte(encoded), "yaml")
	if err != nil {
		t.Fatal(err)
	}
	if report := Validate(reparsed); !report.OK {
		t.Fatalf("written profile is invalid: %+v", report.Errors)
	}
}
func TestAtomicWrite(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "profile.yaml")
	if err := os.WriteFile(path, []byte("old\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := WriteAtomically(path, []byte("new\n")); err != nil {
		t.Fatal(err)
	}
	raw, _ := os.ReadFile(path)
	if string(raw) != "new\n" {
		t.Fatal("not replaced")
	}
}

func TestRetentionOrderingThreadsAndHistory(t *testing.T) {
	profile := fixture(t, "examples/code-reviewer.agent.yaml")
	retention := obj(obj(obj(profile["spec"])["lifecycle"])["retention"])
	retention["fact_ttl_days"] = 30
	retention["max_facts"] = 2
	retention["eviction"] = "least_confident"
	state := obj(profile["state"])
	state["facts"] = []any{
		map[string]any{"id": "fresh", "text": "fresh", "expires_at": "2026-09-01T00:00:00Z", "confidence": 0.9},
		map[string]any{"id": "expired", "text": "expired", "expires_at": "2026-01-01T00:00:00Z", "confidence": 0.1},
		map[string]any{"id": "pinned", "text": "pinned", "expires_at": "2026-01-01T00:00:00Z", "pinned": true},
		map[string]any{"id": "weak", "text": "weak", "confidence": 0.05},
	}
	retention["max_open_threads"] = 2
	state["open_threads"] = []any{
		map[string]any{"id": "active", "status": "open", "updated_at": "2026-01-03T00:00:00Z"},
		map[string]any{"id": "old-closed", "status": "done", "updated_at": "2026-01-01T00:00:00Z"},
		map[string]any{"id": "new-closed", "status": "abandoned", "updated_at": "2026-01-02T00:00:00Z"},
	}
	delete(retention, "max_history")
	history := make([]any, 55)
	for i := range history {
		history[i] = map[string]any{"revision": i + 1}
	}
	profile["history"] = history
	delta := fixture(t, "tests/deltas/learned-conventions.delta.yaml")
	delta["operations"] = []any{}
	stamp := func() time.Time { return time.Date(2026, 8, 28, 12, 0, 0, 0, time.UTC) }
	result, err := ApplyDelta(profile, delta, ApplyOptions{Approved: true, Now: stamp})
	if err != nil {
		t.Fatal(err)
	}
	facts := arr(obj(result.Profile["state"])["facts"])
	if len(facts) != 2 || text(obj(facts[0])["id"]) != "fresh" || text(obj(facts[1])["id"]) != "pinned" {
		t.Fatalf("fact retention/order: %#v", facts)
	}
	threads := arr(obj(result.Profile["state"])["open_threads"])
	if len(threads) != 2 || text(obj(threads[0])["id"]) != "active" || text(obj(threads[1])["id"]) != "new-closed" {
		t.Fatalf("thread retention/order: %#v", threads)
	}
	if len(arr(result.Profile["history"])) != 50 {
		t.Fatalf("default history cap: %d", len(arr(result.Profile["history"])))
	}
}
