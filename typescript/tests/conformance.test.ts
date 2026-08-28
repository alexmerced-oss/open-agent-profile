import { readFileSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  ApplyError,
  ConflictError,
  applyDelta,
  canonicalJson,
  intersectTools,
  narrowDecision,
  parseOap,
  profileDigest,
  renderSystemPrompt,
  resolveComposition,
  serializeOap,
  specDigest,
  validateOap,
  writeAtomically,
  type AgentProfile,
  type AgentStateDelta,
} from "../src/index.js";

const here = dirname(fileURLToPath(import.meta.url));
const repository = resolve(here, "../..");

async function fixture(relative: string): Promise<ReturnType<typeof parseOap>> {
  const path = resolve(repository, relative);
  return parseOap(await readFile(path, "utf8"), { filename: path });
}

describe("OAP profile corpus", () => {
  for (const name of [
    "base-reviewer.agent.yaml",
    "code-reviewer.agent.yaml",
    "data-engineer.agent.yaml",
    "note-taker.agent.yaml",
    "python-reviewer.agent.yaml",
    "research-analyst.agent.md",
  ]) {
    it(`accepts ${name}`, async () => {
      const document = await fixture(`examples/${name}`);
      const report = validateOap(document, { filename: name });
      expect(report.errors, report.errors.map((error) => `${error.pointer}: ${error.message}`).join("\n")).toEqual([]);
    });
  }

  it("accepts the JSON encoding", async () => {
    expect(validateOap(await fixture("examples/json/note-taker.agent.json")).ok).toBe(true);
  });

  for (const name of [
    "bad-name.agent.yaml",
    "future-major.agent.yaml",
    "literal-secret.agent.yaml",
    "missing-instructions.agent.yaml",
    "path-traversal.agent.yaml",
    "unknown-field.agent.yaml",
    "unknown-kind.agent.yaml",
  ]) {
    it(`rejects ${name}`, async () => {
      const report = validateOap(await fixture(`examples/invalid/${name}`), { filename: name });
      expect(report.ok).toBe(false);
      expect(report.errors.length).toBeGreaterThan(0);
    });
  }

  it("rejects dual Markdown instructions while parsing", async () => {
    const path = resolve(repository, "examples/invalid/double-instructions.agent.md");
    expect(() => parseOap(requireText(path), { filename: path })).toThrow(/both frontmatter and body/);
  });
});

function requireText(path: string): string {
  // Fixtures are tiny and this helper keeps parse-error assertions synchronous.
  return readFileSync(path, "utf8");
}

describe("OAP delta corpus and application", () => {
  for (const name of ["learned-conventions.delta.yaml", "closes-thread.delta.yaml"]) {
    it(`accepts ${name}`, async () => {
      expect(validateOap(await fixture(`tests/deltas/${name}`)).ok).toBe(true);
    });
  }

  for (const name of [
    "missing-revision.delta.yaml",
    "proposal-without-rationale.delta.yaml",
    "remove-with-value.delta.yaml",
    "writes-metadata.delta.yaml",
    "writes-spec.delta.yaml",
  ]) {
    it(`rejects ${name}`, async () => {
      expect(validateOap(await fixture(`tests/deltas/invalid/${name}`)).ok).toBe(false);
    });
  }

  it("applies state operations atomically and keeps proposals pending", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    const result = applyDelta(profile, delta, {
      approved: true,
      actor: "test",
      now: () => "2026-08-28T12:00:00Z",
    });
    expect(result.profile.metadata.revision).toBe(8);
    expect((((result.profile.state as Record<string, unknown>).facts) as unknown[]).length).toBe(4);
    expect(result.pendingProposals).toHaveLength(1);
    expect(result.pendingProposals[0]?.risk).toBe("high");
    expect(profile.metadata.revision).toBe(7);
  });

  it("requires approval for propose writeback", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    expect(() => applyDelta(profile, delta)).toThrow(ApplyError);
  });
});

describe("portable behavioral helpers", () => {
  it("uses YAML 1.2 scalar semantics", () => {
    const document = parseOap("created: 2026-08-28T12:00:00Z\nyes_value: yes\ntrue_value: true\n");
    expect(document).toEqual({ created: "2026-08-28T12:00:00Z", yes_value: "yes", true_value: true });
    expect(() => parseOap("value: 1\nvalue: 2\n")).toThrow(/unique/i);
  });

  it("fails closed on an unsupported minor version", async () => {
    const profile = await fixture("examples/note-taker.agent.yaml") as AgentProfile;
    profile.oap = "1.1";
    expect(validateOap(profile).ok).toBe(false);
  });

  it("matches the normative digest spellings", async () => {
    const profile = await fixture("examples/note-taker.agent.yaml") as AgentProfile;
    expect(profileDigest(profile)).toBe("sha256:32ac424528ddffbbc3c0abeb98b1b18887d5ae5d04425a5466f4191a1b30c1e7");
    expect(specDigest(profile)).toBe("sha256:fe2ddb1be24336d05d2b44ffe05d7bbbbfeb0def69c17503b0d5c931ff42fccc");
  });

  it("keeps spec identity stable across state-only changes and object key order", async () => {
    const profile = await fixture("examples/note-taker.agent.yaml") as AgentProfile;
    const changed = structuredClone(profile);
    changed.state = { summary: "different learned state" };
    changed.metadata.revision = 99;
    changed.metadata.updated_at = "2026-08-28T12:00:00Z";
    changed.metadata.trust = "project";
    expect(specDigest(changed)).toBe(specDigest(profile));
    expect(new TextDecoder().decode(canonicalJson({ b: 1, a: 2 }))).toBe('{"a":2,"b":1}');
  });

  it("narrows permissions and intersects tools", async () => {
    expect(narrowDecision("ask", "allow")).toBe("ask");
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    expect(intersectTools(profile, ["read", "search", "shell", "write"]).tools).toEqual(["read", "search"]);
  });

  it("renders instructions, structured role, untrusted state, and harness postamble in order", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const prompt = renderSystemPrompt(profile, { harnessPreamble: "PRE", harnessPostamble: "POST" });
    const positions = ["PRE", "You are a code reviewer", "Objectives:", "Persona:", "Constraints:", "Examples:", "PROFILE STATE", "POST"].map((text) => prompt.indexOf(text));
    expect(positions.every((position, index) => position >= 0 && (index === 0 || position > (positions[index - 1] ?? -1)))).toBe(true);
    expect(prompt.indexOf("PRE")).toBeLessThan(prompt.indexOf("You are a code reviewer"));
    expect(prompt.indexOf("PROFILE STATE")).toBeLessThan(prompt.indexOf("POST"));
  });

  it("resolves inheritance without inheriting identity or state", async () => {
    const base = await fixture("examples/base-reviewer.agent.yaml") as AgentProfile;
    const child = await fixture("examples/python-reviewer.agent.yaml") as AgentProfile;
    const resolved = await resolveComposition(child, () => base);
    expect(resolved.metadata.name).toBe(child.metadata.name);
    expect(resolved.state).toEqual(child.state);
    expect(resolved.extends).toBeUndefined();
  });

  it("round-trips Markdown encoding and can atomically replace a file", async () => {
    const profile = await fixture("examples/research-analyst.agent.md") as AgentProfile;
    const encoded = serializeOap(profile, "markdown");
    expect(parseOap(encoded, { format: "markdown" })).toEqual(profile);
    const path = resolve(process.env.TMPDIR ?? "/tmp", `oap-typescript-${process.pid}.agent.yaml`);
    await writeFile(path, "old\n");
    await writeAtomically(path, serializeOap(profile));
    expect(validateOap(parseOap(await readFile(path, "utf8"))).ok).toBe(true);
  });
});

describe("delta safety invariants", () => {
  it("rejects revision and digest conflicts", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    const wrongRevision = structuredClone(delta);
    wrongRevision.target.revision = 6;
    expect(() => applyDelta(profile, wrongRevision, { approved: true })).toThrow(ConflictError);
    const wrongDigest = structuredClone(delta);
    wrongDigest.target.digest = `sha256:${"0".repeat(64)}`;
    expect(() => applyDelta(profile, wrongDigest, { approved: true })).toThrow(/digest/);
  });

  it("leaves input untouched when a later operation fails", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const before = structuredClone(profile);
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    delta.operations = [
      { op: "replace", path: "/state/summary", value: "temporary" },
      { op: "replace", path: "/state/facts/id:does-not-exist", value: {} },
    ];
    expect(() => applyDelta(profile, delta, { approved: true })).toThrow();
    expect(profile).toEqual(before);
  });

  it("warns when removing a missing id-addressed entry", async () => {
    const profile = await fixture("examples/research-analyst.agent.md") as AgentProfile;
    const delta = await fixture("tests/deltas/closes-thread.delta.yaml") as AgentStateDelta;
    (delta.operations as Array<Record<string, unknown>>)[0]!.path = "/state/preferences/id:not-present";
    const result = applyDelta(profile, delta, { approved: true, now: () => "2026-08-28T12:00:00Z" });
    expect(result.warnings.some((warning) => warning.includes("missing path"))).toBe(true);
  });

  it("preserves unknown namespaced annotations through writeback", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const annotations = structuredClone(profile.metadata.annotations);
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    const result = applyDelta(profile, delta, { approved: true });
    expect(result.profile.metadata.annotations).toEqual(annotations);
  });

  it("matches Python retention for expiry, pinned facts, and original ordering", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const retention = ((profile.spec.lifecycle as Record<string, unknown>).retention as Record<string, unknown>);
    retention.fact_ttl_days = 30;
    retention.max_facts = 2;
    const state = profile.state as Record<string, unknown>;
    state.facts = [
      { id: "fresh", text: "fresh", expires_at: "2026-09-01T00:00:00Z", confidence: 0.9 },
      { id: "expired", text: "expired", expires_at: "2026-01-01T00:00:00Z", confidence: 0.1 },
      { id: "pinned", text: "pinned", expires_at: "2026-01-01T00:00:00Z", pinned: true },
      { id: "weak", text: "weak", confidence: 0.05 },
    ];
    retention.eviction = "least_confident";
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    delta.operations = [];
    const result = applyDelta(profile, delta, { approved: true, now: () => "2026-08-28T12:00:00Z" });
    expect((result.profile.state as Record<string, unknown>).facts).toEqual([
      expect.objectContaining({ id: "fresh" }),
      expect.objectContaining({ id: "pinned" }),
    ]);
    expect(result.warnings.some((warning) => warning.includes("evicted expired facts"))).toBe(true);
  });

  it("evicts closed threads before applying the max-open-threads cap", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    const retention = ((profile.spec.lifecycle as Record<string, unknown>).retention as Record<string, unknown>);
    retention.max_open_threads = 2;
    (profile.state as Record<string, unknown>).open_threads = [
      { id: "active", status: "open", updated_at: "2026-01-03T00:00:00Z" },
      { id: "old-closed", status: "done", updated_at: "2026-01-01T00:00:00Z" },
      { id: "new-closed", status: "abandoned", updated_at: "2026-01-02T00:00:00Z" },
    ];
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    delta.operations = [];
    const result = applyDelta(profile, delta, { approved: true, now: () => "2026-08-28T12:00:00Z" });
    expect((result.profile.state as Record<string, unknown>).open_threads).toEqual([
      expect.objectContaining({ id: "active" }),
      expect.objectContaining({ id: "new-closed" }),
    ]);
    expect(result.warnings.some((warning) => warning.includes("old-closed"))).toBe(true);
  });

  it("uses the Python default history cap of fifty", async () => {
    const profile = await fixture("examples/code-reviewer.agent.yaml") as AgentProfile;
    delete ((profile.spec.lifecycle as Record<string, unknown>).retention as Record<string, unknown>).max_history;
    profile.history = Array.from({ length: 55 }, (_, index) => ({ revision: index + 1 }));
    const delta = await fixture("tests/deltas/learned-conventions.delta.yaml") as AgentStateDelta;
    delta.operations = [];
    const result = applyDelta(profile, delta, { approved: true, now: () => "2026-08-28T12:00:00Z" });
    expect(result.profile.history).toHaveLength(50);
    expect((result.profile.history as Array<Record<string, unknown>>).at(-1)?.revision).toBe(8);
  });
});
