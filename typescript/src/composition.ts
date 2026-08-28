import { specDigest } from "./canonical.js";
import type { AgentProfile, JsonObject, JsonValue } from "./types.js";

export interface ProfileReference {
  name: string;
  uri?: string;
  revision?: number;
  digest?: string;
}

export type ProfileLoader = (reference: ProfileReference) => AgentProfile | Promise<AgentProfile>;

export class CompositionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CompositionError";
  }
}

function isObject(value: JsonValue | undefined): value is JsonObject {
  return value !== null && value !== undefined && typeof value === "object" && !Array.isArray(value);
}

export function mergeProfileValues(base: JsonObject, child: JsonObject): JsonObject {
  const result = structuredClone(base);
  for (const [key, value] of Object.entries(child)) {
    if (value === null) delete result[key];
    else if (isObject(value) && isObject(result[key])) result[key] = mergeProfileValues(result[key], value);
    else result[key] = structuredClone(value);
  }
  return result;
}

function withoutIdentity(profile: AgentProfile): JsonObject {
  const copy = structuredClone(profile) as JsonObject;
  delete copy.extends;
  delete copy.state;
  delete copy.history;
  const metadata = (copy.metadata as JsonObject | undefined) ?? {};
  delete metadata.name;
  delete metadata.id;
  delete metadata.revision;
  return copy;
}

function references(profile: AgentProfile): ProfileReference[] {
  if (!Array.isArray(profile.extends)) return [];
  return profile.extends.filter((value): value is JsonObject => value !== null && typeof value === "object" && !Array.isArray(value)).map((value) => ({
    name: String(value.name),
    ...(typeof value.uri === "string" ? { uri: value.uri } : {}),
    ...(typeof value.revision === "number" ? { revision: value.revision } : {}),
    ...(typeof value.digest === "string" ? { digest: value.digest } : {}),
  }));
}

export async function resolveComposition(profile: AgentProfile, load: ProfileLoader, active: string[] = []): Promise<AgentProfile> {
  const name = String(profile.metadata.name);
  if (active.includes(name)) throw new CompositionError(`inheritance cycle: ${[...active, name].join(" -> ")}`);
  if (active.length >= 8) throw new CompositionError("inheritance depth exceeds 8");
  let merged: JsonObject = {};
  for (const reference of references(profile)) {
    const base = await load(reference);
    if (reference.revision !== undefined && base.metadata.revision !== reference.revision) throw new CompositionError(`${reference.name} revision does not match pin`);
    if (reference.digest && specDigest(base) !== reference.digest) throw new CompositionError(`${reference.name} digest does not match pin`);
    const resolved = await resolveComposition(base, load, [...active, name]);
    merged = mergeProfileValues(merged, withoutIdentity(resolved));
  }
  merged = mergeProfileValues(merged, profile);
  merged.metadata = structuredClone(profile.metadata);
  if (profile.state !== undefined) merged.state = structuredClone(profile.state);
  else delete merged.state;
  if (profile.history !== undefined) merged.history = structuredClone(profile.history);
  else delete merged.history;
  delete merged.extends;
  return merged as AgentProfile;
}
