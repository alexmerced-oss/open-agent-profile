export const OAP_VERSION = "1.0";
export const SUPPORT_VERSION = "1.0.1";

export { canonicalJson, profileDigest, profileDigests, specDigest } from "./canonical.js";
export {
  CompositionError,
  mergeProfileValues,
  resolveComposition,
} from "./composition.js";
export {
  ApplyError,
  ConflictError,
  applyDelta,
  serializeOap,
  writeAtomically,
} from "./delta.js";
export { loadOap, OapParseError, parseOap } from "./parse.js";
export { intersectTools, narrowDecision, narrowPermissionMap } from "./policy.js";
export { renderSystemPrompt, substituteVariables } from "./render.js";
export { deltaSchema, escapesWorkspace, profileSchema, validateOap } from "./validate.js";
export type {
  ApplyOptions,
} from "./delta.js";
export type {
  ProfileLoader,
  ProfileReference,
} from "./composition.js";
export type {
  EffectiveTools,
} from "./policy.js";
export type {
  RenderOptions,
} from "./render.js";
export type {
  AgentProfile,
  AgentStateDelta,
  Adjustment,
  DeltaApplication,
  Issue,
  JsonObject,
  JsonPrimitive,
  JsonValue,
  OapDigests,
  PermissionDecision,
  Trust,
  ValidationReport,
} from "./types.js";
export type { ValidateOptions } from "./validate.js";
