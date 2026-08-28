import { basename } from "node:path";

import Ajv2020, { type ErrorObject } from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

import profileSchema from "../../schema/v1/agent-profile.schema.json";
import deltaSchema from "../../schema/v1/agent-state-delta.schema.json";

import { profileDigests } from "./canonical.js";
import type { AgentProfile, Issue, JsonObject, JsonValue, ValidationReport } from "./types.js";

const ajv = new Ajv2020({ allErrors: true, strict: false });
addFormats(ajv);
const validateProfileSchema = ajv.compile(profileSchema);
const validateDeltaSchema = ajv.compile(deltaSchema);

const ENV_REF = /^\$\{[A-Z][A-Z0-9_]{0,63}\}$/;
const HEADER_REF = /^(Bearer )?\$\{[A-Z][A-Z0-9_]{0,63}\}$/;
const VAR_REF = /\$\{\{\s*vars\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;
const PROFILE_SUFFIXES = [".agent.yaml", ".agent.yml", ".agent.json", ".agent.md"];

const SECRET_PATTERNS: Array<[RegExp, string]> = [
  [/\bsk-[A-Za-z0-9_-]{16,}/, "OpenAI-style API key"],
  [/\bsk-ant-[A-Za-z0-9_-]{16,}/, "Anthropic API key"],
  [/\bgh[pousr]_[A-Za-z0-9]{20,}/, "GitHub token"],
  [/\bAKIA[0-9A-Z]{16}\b/, "AWS access key id"],
  [/\bxox[baprs]-[A-Za-z0-9-]{10,}/, "Slack token"],
  [/-----BEGIN (?:RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----/, "private key"],
  [/\bey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/, "JWT"],
];

type RecordValue = Record<string, unknown>;

function isRecord(value: unknown): value is RecordValue {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function record(value: unknown): RecordValue {
  return isRecord(value) ? value : {};
}

function issue(target: Issue[], pointer: string, message: string): void {
  target.push({ pointer, message });
}

function schemaIssues(errors: ErrorObject[] | null | undefined): Issue[] {
  return (errors ?? []).map((error) => ({ pointer: error.instancePath || "", message: error.message ?? "schema validation failed" }));
}

function* walkStrings(value: unknown, pointer = ""): Generator<[string, string]> {
  if (typeof value === "string") yield [pointer, value];
  else if (Array.isArray(value)) {
    for (const [index, child] of value.entries()) yield* walkStrings(child, `${pointer}/${index}`);
  } else if (isRecord(value)) {
    for (const [key, child] of Object.entries(value)) yield* walkStrings(child, `${pointer}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`);
  }
}

export function escapesWorkspace(path: string): boolean {
  if (path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\")) return true;
  let depth = 0;
  for (const part of path.split(/[\\/]+/)) {
    if (!part || part === ".") continue;
    if (part === "..") {
      depth -= 1;
      if (depth < 0) return true;
    } else depth += 1;
  }
  return false;
}

function checkVersion(document: JsonObject, errors: Issue[]): boolean {
  if (typeof document.oap !== "string" || !/^\d+\.\d+$/.test(document.oap)) {
    issue(errors, "/oap", "missing or malformed spec version string");
    return false;
  }
  const [major, minor] = document.oap.split(".").map(Number);
  if (major !== 1 || (minor ?? 0) > 0) {
    issue(errors, "/oap", `unsupported OAP version ${document.oap}; unsupported versions fail closed`);
    return false;
  }
  return true;
}

function checkSecrets(document: JsonObject, errors: Issue[]): void {
  for (const [pointer, text] of walkStrings(document)) {
    for (const [pattern, label] of SECRET_PATTERNS) {
      if (pattern.test(text)) {
        issue(errors, pointer, `looks like a literal ${label}; use a \${VARIABLE} reference`);
        break;
      }
    }
  }
}

function checkProfile(document: JsonObject, errors: Issue[], warnings: Issue[], filename?: string): void {
  const spec = record(document.spec);
  const tools = record(spec.tools);
  const servers = Array.isArray(tools.mcp_servers) ? tools.mcp_servers : [];
  servers.forEach((server, index) => {
    if (!isRecord(server)) return;
    for (const [key, value] of Object.entries(record(server.env))) {
      if (typeof value !== "string" || !ENV_REF.test(value)) issue(errors, `/spec/tools/mcp_servers/${index}/env/${key}`, "must be a same-name ${VARIABLE} reference, not a literal");
    }
    for (const [key, value] of Object.entries(record(server.headers))) {
      if (typeof value !== "string" || !HEADER_REF.test(value)) issue(errors, `/spec/tools/mcp_servers/${index}/headers/${key}`, "must be '${VARIABLE}' or 'Bearer ${VARIABLE}'");
    }
  });

  const context = record(spec.context);
  const paths: Array<[string, string]> = [];
  (Array.isArray(context.files) ? context.files : []).forEach((entry, index) => {
    if (isRecord(entry) && typeof entry.path === "string") paths.push([`/spec/context/files/${index}/path`, entry.path]);
  });
  if (typeof context.working_directory === "string") paths.push(["/spec/context/working_directory", context.working_directory]);
  const filesystem = record(record(spec.permissions).filesystem);
  for (const key of ["read_roots", "write_roots", "deny_paths"]) {
    (Array.isArray(filesystem[key]) ? filesystem[key] : []).forEach((path, index) => {
      if (typeof path === "string") paths.push([`/spec/permissions/filesystem/${key}/${index}`, path]);
    });
  }
  for (const [pointer, path] of paths) if (escapesWorkspace(path)) issue(errors, pointer, `${JSON.stringify(path)} resolves outside the workspace`);

  const variables = new Set(Object.keys(record(context.variables)));
  const role = record(spec.role);
  const roleFields: Array<[string, string]> = [];
  if (typeof role.instructions === "string") roleFields.push(["/spec/role/instructions", role.instructions]);
  for (const key of ["objectives", "constraints"]) {
    (Array.isArray(role[key]) ? role[key] : []).forEach((value, index) => {
      if (typeof value === "string") roleFields.push([`/spec/role/${key}/${index}`, value]);
    });
  }
  for (const [pointer, text] of roleFields) {
    for (const match of text.matchAll(VAR_REF)) if (!variables.has(match[1] ?? "")) issue(errors, pointer, `references undefined variable ${JSON.stringify(match[1])}`);
  }
  for (const [pointer, text] of walkStrings(document.state ?? {}, "/state")) {
    if (VAR_REF.test(text)) issue(warnings, pointer, "contains a ${{ vars.* }} template; substitution never runs inside state");
    VAR_REF.lastIndex = 0;
  }

  const state = record(document.state);
  for (const key of ["facts", "preferences", "open_threads"]) {
    const seen = new Set<unknown>();
    (Array.isArray(state[key]) ? state[key] : []).forEach((entry, index) => {
      if (!isRecord(entry)) return;
      if (seen.has(entry.id)) issue(errors, `/state/${key}/${index}/id`, `duplicate id ${JSON.stringify(entry.id)}`);
      seen.add(entry.id);
    });
  }

  const metadata = record(document.metadata);
  if (metadata.trust !== undefined) issue(warnings, "/metadata/trust", "trust in the file must be discarded and recomputed from the discovery root");
  if (filename && typeof metadata.name === "string") {
    let stem = basename(filename);
    const suffix = PROFILE_SUFFIXES.find((candidate) => stem.endsWith(candidate));
    if (suffix) stem = stem.slice(0, -suffix.length);
    if (stem && stem !== metadata.name) issue(warnings, "/metadata/name", `${JSON.stringify(metadata.name)} does not match file name ${JSON.stringify(stem)}; metadata.name wins`);
  }
  const history: RecordValue[] = Array.isArray(document.history) ? (document.history as unknown[]).filter(isRecord) : [];
  const revisions = history.map((entry) => entry.revision).filter((value): value is number => Number.isInteger(value));
  if (revisions.some((value, index) => index > 0 && value < (revisions[index - 1] ?? value))) issue(errors, "/history", "entries must be ordered oldest first by revision");
  const current = metadata.revision;
  if (typeof current === "number" && revisions.length > 0 && (revisions.at(-1) ?? 0) > current) issue(errors, "/history", "newest history revision exceeds metadata.revision");

  const policy = typeof tools.policy === "string" ? tools.policy : "allowlist";
  if (policy === "inherit" && (tools.allow || tools.deny)) issue(warnings, "/spec/tools", "policy is 'inherit', so allow and deny are ignored");
  if (policy === "allowlist" && isRecord(document.spec) && tools.allow === undefined && Object.keys(tools).length > 0) issue(warnings, "/spec/tools", "allowlist has an empty allow list, so the agent gets no tools");
}

function checkDelta(document: JsonObject, errors: Issue[], warnings: Issue[]): void {
  (Array.isArray(document.operations) ? document.operations : []).forEach((operation, index) => {
    if (!isRecord(operation)) return;
    if (typeof operation.path !== "string" || !(operation.path === "/state" || operation.path.startsWith("/state/"))) issue(errors, `/operations/${index}/path`, "operation is outside /state; contract changes belong in proposals");
  });
  (Array.isArray(document.proposals) ? document.proposals : []).forEach((proposal, index) => {
    if (!isRecord(proposal) || typeof proposal.path !== "string") return;
    const path = proposal.path;
    if (["/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents"].some((prefix) => path.startsWith(prefix)) && proposal.risk !== "high") {
      issue(warnings, `/proposals/${index}`, `${path} must be treated as high risk regardless of its declared risk`);
    }
  });
}

export interface ValidateOptions {
  filename?: string;
}

export function validateOap(document: JsonObject, options: ValidateOptions = {}): ValidationReport {
  const errors: Issue[] = [];
  const warnings: Issue[] = [];
  const kind = typeof document.kind === "string" ? document.kind : "unknown";
  if (!checkVersion(document, errors)) return { kind, document, errors, warnings, ok: false };
  if (kind === "AgentProfile") {
    if (!validateProfileSchema(document)) errors.push(...schemaIssues(validateProfileSchema.errors));
  } else if (kind === "AgentStateDelta") {
    if (!validateDeltaSchema(document)) errors.push(...schemaIssues(validateDeltaSchema.errors));
  } else {
    issue(errors, "/kind", `${JSON.stringify(document.kind)} is not a known 1.x kind`);
    return { kind, document, errors, warnings, ok: false };
  }
  checkSecrets(document, errors);
  if (kind === "AgentProfile") checkProfile(document, errors, warnings, options.filename);
  else checkDelta(document, errors, warnings);
  const report: ValidationReport = { kind, document, errors, warnings, ok: errors.length === 0 };
  if (kind === "AgentProfile") report.digests = profileDigests(document as AgentProfile);
  return report;
}

export { deltaSchema, profileSchema };
