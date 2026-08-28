export type JsonPrimitive = null | boolean | number | string;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export interface AgentProfile extends JsonObject {
  oap: string;
  kind: "AgentProfile";
  metadata: JsonObject;
  spec: JsonObject;
}

export interface AgentStateDelta extends JsonObject {
  oap: string;
  kind: "AgentStateDelta";
  target: JsonObject;
  session: JsonObject;
}

export interface Issue {
  pointer: string;
  message: string;
}

export interface OapDigests {
  profile: string;
  spec: string;
}

export interface ValidationReport {
  kind: string;
  document?: JsonObject;
  errors: Issue[];
  warnings: Issue[];
  digests?: OapDigests;
  ok: boolean;
}

export type Trust = "managed" | "user" | "project" | "imported";
export type PermissionDecision = "deny" | "ask" | "allow";

export interface Adjustment {
  field: string;
  requested: JsonValue;
  effective: JsonValue;
  reason: string;
}

export interface DeltaApplication {
  profile: AgentProfile;
  warnings: string[];
  pendingProposals: JsonObject[];
}
