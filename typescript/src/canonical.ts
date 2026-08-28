import { createHash } from "node:crypto";

import canonicalize from "canonicalize";

import type { AgentProfile, JsonObject, JsonValue, OapDigests } from "./types.js";

export function canonicalJson(value: JsonValue): Uint8Array {
  const encoded = canonicalize(value);
  if (encoded === undefined) throw new TypeError("value is not representable as canonical JSON");
  return new TextEncoder().encode(encoded);
}

function sha256Hex(value: JsonValue): string {
  return createHash("sha256").update(canonicalJson(value)).digest("hex");
}

export function profileDigest(profile: AgentProfile | JsonObject): string {
  return `sha256:${sha256Hex(profile)}`;
}

export function specDigest(profile: AgentProfile | JsonObject): string {
  const metadata = { ...((profile.metadata as JsonObject | undefined) ?? {}) };
  delete metadata.revision;
  delete metadata.updated_at;
  delete metadata.trust;
  return `sha256:${sha256Hex({ metadata, spec: profile.spec ?? null })}`;
}

export function profileDigests(profile: AgentProfile | JsonObject): OapDigests {
  return { profile: profileDigest(profile), spec: specDigest(profile) };
}
