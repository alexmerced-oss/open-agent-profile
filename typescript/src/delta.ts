import { open, rename, unlink } from "node:fs/promises";
import { dirname } from "node:path";

import { stringify as stringifyYaml } from "yaml";

import { profileDigest } from "./canonical.js";
import type { AgentProfile, AgentStateDelta, DeltaApplication, JsonObject, JsonValue } from "./types.js";

export class ApplyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ApplyError";
  }
}

export class ConflictError extends ApplyError {
  constructor(message: string) {
    super(message);
    this.name = "ConflictError";
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function pointerTokens(document: JsonObject, pointer: string): string[] {
  if (!pointer.startsWith("/")) throw new ApplyError(`pointer must start with '/': ${pointer}`);
  const raw = pointer.slice(1).split("/").map((token) => token.replaceAll("~1", "/").replaceAll("~0", "~"));
  const resolved: string[] = [];
  let cursor: unknown = document;
  for (const token of raw) {
    if (Array.isArray(cursor) && token.startsWith("id:")) {
      const id = token.slice(3);
      const index = cursor.findIndex((item) => isObject(item) && item.id === id);
      if (index < 0) resolved.push("-missing-");
      else resolved.push(String(index));
      cursor = index < 0 ? undefined : cursor[index];
      continue;
    }
    resolved.push(token);
    if (token === "-") cursor = undefined;
    else if (Array.isArray(cursor) && /^\d+$/.test(token)) cursor = cursor[Number(token)];
    else if (isObject(cursor)) cursor = cursor[token];
    else cursor = undefined;
  }
  return resolved;
}

function applyOperation(document: JsonObject, operation: JsonObject, warnings: string[]): void {
  const kind = operation.op;
  const pointer = operation.path;
  if (typeof pointer !== "string" || !(pointer === "/state" || pointer.startsWith("/state/"))) throw new ApplyError(`operation path ${JSON.stringify(pointer)} is outside /state`);
  if (kind !== "add" && kind !== "replace" && kind !== "remove") throw new ApplyError(`unknown operation ${JSON.stringify(kind)}`);
  const tokens = pointerTokens(document, pointer);
  if (tokens.includes("-missing-")) {
    if (kind === "remove") {
      warnings.push(`remove on missing path ${JSON.stringify(pointer)} ignored`);
      return;
    }
    throw new ApplyError(`path ${JSON.stringify(pointer)} does not resolve`);
  }
  let parent: unknown = document;
  for (const token of tokens.slice(0, -1)) {
    if (Array.isArray(parent) && /^\d+$/.test(token)) parent = parent[Number(token)];
    else if (isObject(parent)) parent = (parent[token] ??= {});
    else throw new ApplyError(`path ${JSON.stringify(pointer)} does not resolve`);
  }
  const last = tokens.at(-1);
  if (last === undefined) throw new ApplyError("operation path must not be document root");
  if (Array.isArray(parent)) {
    if (kind === "add") {
      if (last === "-") parent.push(structuredClone(operation.value));
      else if (/^\d+$/.test(last)) parent.splice(Number(last), 0, structuredClone(operation.value));
      else throw new ApplyError(`array index expected at ${pointer}`);
    } else if (!/^\d+$/.test(last) || Number(last) >= parent.length) {
      if (kind === "remove") warnings.push(`remove on missing path ${JSON.stringify(pointer)} ignored`);
      else throw new ApplyError(`index out of range at ${pointer}`);
    } else if (kind === "replace") parent[Number(last)] = structuredClone(operation.value);
    else parent.splice(Number(last), 1);
  } else if (isObject(parent)) {
    if (kind === "remove") {
      if (!(last in parent)) warnings.push(`remove on missing path ${JSON.stringify(pointer)} ignored`);
      else delete parent[last];
    } else parent[last] = structuredClone(operation.value);
  } else throw new ApplyError(`path ${JSON.stringify(pointer)} does not resolve to a container`);
}

function entrySortKey(entry: Record<string, unknown>, strategy: string): string | number {
  if (strategy === "least_confident") return typeof entry.confidence === "number" ? entry.confidence : 1;
  if (strategy === "oldest") return String(entry.learned_at ?? entry.opened_at ?? "");
  return String(entry.last_used_at ?? entry.updated_at ?? entry.learned_at ?? "");
}

function compareEntries(a: Record<string, unknown>, b: Record<string, unknown>, strategy: string): number {
  const left = entrySortKey(a, strategy);
  const right = entrySortKey(b, strategy);
  if (typeof left === "number" && typeof right === "number") return left - right;
  return String(left).localeCompare(String(right));
}

function enforceRetention(profile: AgentProfile, warnings: string[], currentTime: string): void {
  const spec = profile.spec;
  const lifecycle = isObject(spec.lifecycle) ? spec.lifecycle : {};
  const retention = isObject(lifecycle.retention) ? lifecycle.retention : {};
  const state = isObject(profile.state) ? profile.state : {};
  const strategy = typeof retention.eviction === "string" ? retention.eviction : "least_recently_used";
  for (const collection of ["facts", "preferences"]) {
    const rawEntries = state[collection];
    const entries: Array<Record<string, unknown>> | undefined = Array.isArray(rawEntries)
      ? (rawEntries as unknown[]).filter(isObject)
      : undefined;
    if (!entries) continue;
    let retained = entries;
    if (retention.fact_ttl_days) {
      retained = entries.filter((entry) => {
        const expired = typeof entry.expires_at === "string" && entry.expires_at < currentTime && entry.pinned !== true;
        if (expired) warnings.push(`evicted expired ${collection} entry ${JSON.stringify(entry.id)}`);
        return !expired;
      });
      state[collection] = retained as JsonValue;
    }
    const cap = collection === "facts" && typeof retention.max_facts === "number" ? retention.max_facts : undefined;
    if (cap === undefined || retained.length <= cap) continue;
    const pinned = retained.filter((entry) => entry.pinned === true);
    const rest = retained.filter((entry) => entry.pinned !== true).sort((a, b) => compareEntries(a, b, strategy));
    const room = Math.max(cap - pinned.length, 0);
    const evicted = rest.slice(0, Math.max(rest.length - room, 0));
    for (const entry of evicted) warnings.push(`evicted ${collection} entry ${JSON.stringify(entry.id)} (${strategy})`);
    const kept = new Set([...pinned, ...rest.slice(evicted.length)]);
    state[collection] = retained.filter((entry) => kept.has(entry)) as JsonValue;
  }
  const threadCap = typeof retention.max_open_threads === "number" ? retention.max_open_threads : undefined;
  const rawThreads = state.open_threads;
  const threads: Array<Record<string, unknown>> | undefined = Array.isArray(rawThreads)
    ? (rawThreads as unknown[]).filter(isObject)
    : undefined;
  if (threads && threadCap !== undefined && threads.length > threadCap) {
    const active = threads.filter((thread) => !thread.status || thread.status === "open" || thread.status === "blocked");
    const closed = threads
      .filter((thread) => thread.status === "done" || thread.status === "abandoned")
      .sort((a, b) => String(a.updated_at ?? "").localeCompare(String(b.updated_at ?? "")));
    const dropped = new Set(closed.slice(0, threads.length - threadCap));
    for (const thread of dropped) warnings.push(`evicted closed thread ${JSON.stringify(thread.id)}`);
    const remaining = threads.filter((thread) => !dropped.has(thread));
    state.open_threads = (remaining.length > threadCap ? remaining.slice(-threadCap) : remaining) as JsonValue;
    if (remaining.length > threadCap) warnings.push(`open_threads still over cap after evicting closed threads (${active.length} active)`);
  }
  const historyCap = typeof retention.max_history === "number" ? retention.max_history : 50;
  if (Array.isArray(profile.history) && profile.history.length > historyCap) profile.history = profile.history.slice(-historyCap);
}

export interface ApplyOptions {
  approved?: boolean;
  actor?: string;
  now?: () => string;
}

export function applyDelta(profile: AgentProfile, delta: AgentStateDelta, options: ApplyOptions = {}): DeltaApplication {
  const metadata = profile.metadata;
  const current = typeof metadata.revision === "number" ? metadata.revision : 1;
  if (delta.target.name !== metadata.name) throw new ApplyError(`delta targets ${JSON.stringify(delta.target.name)} but profile is ${JSON.stringify(metadata.name)}`);
  if (delta.target.revision !== current) throw new ConflictError(`delta targets revision ${delta.target.revision} but profile is at ${current}`);
  if (typeof delta.target.digest === "string" && delta.target.digest !== profileDigest(profile)) throw new ApplyError("target.digest does not match profile");
  const lifecycle = isObject(profile.spec.lifecycle) ? profile.spec.lifecycle : {};
  const writeback = typeof lifecycle.writeback === "string" ? lifecycle.writeback : "propose";
  if (writeback === "off") throw new ApplyError("lifecycle.writeback is 'off'");
  if (writeback === "propose" && !options.approved) throw new ApplyError("lifecycle.writeback is 'propose'; explicit approval is required");

  const working = structuredClone(profile);
  working.state ??= {};
  const warnings: string[] = [];
  const stamp = options.now?.() ?? new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  const operations = Array.isArray(delta.operations) ? delta.operations.filter((value): value is JsonObject => isObject(value)) : [];
  operations.forEach((operation, index) => {
    try {
      applyOperation(working, operation, warnings);
    } catch (error) {
      throw new ApplyError(`operation ${index}: ${(error as Error).message}`);
    }
  });
  enforceRetention(working, warnings, stamp);
  working.metadata.revision = current + 1;
  working.metadata.updated_at = stamp;
  if (operations.length > 0 && isObject(working.state)) {
    working.state.updated_at = stamp;
    working.state.revision = (typeof working.state.revision === "number" ? working.state.revision : 0) + 1;
  }
  const session = delta.session;
  const history: JsonObject = {
    revision: current + 1,
    at: stamp,
    by: typeof session.id === "string" ? session.id : (options.actor ?? "oap-typescript"),
    change: typeof delta.summary === "string" ? delta.summary : `${operations.length} state operations`,
    sections: ["state"],
  };
  if (typeof session.id === "string") history.session_id = session.id;
  if (typeof session.harness === "string") history.harness = session.harness;
  if (options.approved) history.approved_by = options.actor ?? "oap-typescript";
  working.history = [...(Array.isArray(working.history) ? working.history : []), history];
  enforceRetention(working, warnings, stamp);
  const pendingProposals = (Array.isArray(delta.proposals) ? delta.proposals.filter((value): value is JsonObject => isObject(value)) : []).map((proposal) => {
    const copy = structuredClone(proposal);
    if (typeof copy.path === "string" && ["/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents"].some((prefix) => String(copy.path).startsWith(prefix))) copy.risk = "high";
    return copy;
  });
  return { profile: working, warnings, pendingProposals };
}

export function serializeOap(document: JsonObject, format: "yaml" | "json" | "markdown" = "yaml"): string {
  if (format === "json") return `${JSON.stringify(document, null, 2)}\n`;
  if (format === "yaml") return stringifyYaml(document, { lineWidth: 100 });
  const copy = structuredClone(document);
  const spec = copy.spec as JsonObject;
  const role = spec.role as JsonObject;
  const instructions = String(role.instructions ?? "").trimEnd();
  delete role.instructions;
  return `---\n${stringifyYaml(copy, { lineWidth: 100 }).trimEnd()}\n---\n${instructions}\n`;
}

export async function writeAtomically(path: string, text: string): Promise<void> {
  const temporary = `${path}.${process.pid}.${Date.now()}.tmp`;
  let handle;
  try {
    handle = await open(temporary, "wx", 0o600);
    await handle.writeFile(text, "utf8");
    await handle.sync();
    await handle.close();
    handle = undefined;
    await rename(temporary, path);
    const directory = await open(dirname(path), "r");
    try {
      await directory.sync();
    } finally {
      await directory.close();
    }
  } catch (error) {
    if (handle) await handle.close().catch(() => undefined);
    await unlink(temporary).catch(() => undefined);
    throw error;
  }
}
