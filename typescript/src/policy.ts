import type { AgentProfile, Adjustment, JsonObject, PermissionDecision } from "./types.js";

const DECISION_RANK: Readonly<Record<PermissionDecision, number>> = { deny: 0, ask: 1, allow: 2 };

export function narrowDecision(policy: PermissionDecision, requested: PermissionDecision): PermissionDecision {
  return DECISION_RANK[policy] <= DECISION_RANK[requested] ? policy : requested;
}

function wildcard(pattern: string): RegExp {
  const escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replaceAll("**", "\u0000").replaceAll("*", "[^/]*").replaceAll("\u0000", ".*");
  return new RegExp(`^${escaped}$`);
}

function matchesAny(value: string, patterns: string[]): boolean {
  return patterns.some((pattern) => wildcard(pattern).test(value));
}

export interface EffectiveTools {
  tools: string[];
  adjustments: Adjustment[];
}

export function intersectTools(profile: AgentProfile | JsonObject, grantedTools: Iterable<string>): EffectiveTools {
  const spec = (profile.spec as JsonObject | undefined) ?? {};
  const tools = (spec.tools as JsonObject | undefined) ?? {};
  const policy = typeof tools.policy === "string" ? tools.policy : "allowlist";
  const allow = Array.isArray(tools.allow) ? tools.allow.filter((value): value is string => typeof value === "string") : [];
  const deny = Array.isArray(tools.deny) ? tools.deny.filter((value): value is string => typeof value === "string") : [];
  const granted = [...grantedTools];
  const requested = policy === "inherit" ? granted : policy === "denylist" ? granted : granted.filter((tool) => matchesAny(tool, allow));
  const effective = requested.filter((tool) => !matchesAny(tool, deny));
  const dropped = granted.filter((tool) => !effective.includes(tool));
  const adjustments: Adjustment[] = dropped.map((tool) => ({
    field: `spec.tools:${tool}`,
    requested: tool,
    effective: null,
    reason: deny.includes(tool) || matchesAny(tool, deny) ? "profile deny rule" : "not included by profile allowlist",
  }));
  return { tools: effective.sort(), adjustments };
}

export function narrowPermissionMap(
  requested: Record<string, PermissionDecision>,
  policy: Record<string, PermissionDecision>,
): { permissions: Record<string, PermissionDecision>; adjustments: Adjustment[] } {
  const permissions: Record<string, PermissionDecision> = {};
  const adjustments: Adjustment[] = [];
  for (const key of new Set([...Object.keys(requested), ...Object.keys(policy)])) {
    const wanted = requested[key] ?? "ask";
    const ceiling = policy[key] ?? "ask";
    const effective = narrowDecision(ceiling, wanted);
    permissions[key] = effective;
    if (effective !== wanted) adjustments.push({ field: `spec.permissions.${key}`, requested: wanted, effective, reason: "local policy ceiling" });
  }
  return { permissions, adjustments };
}
