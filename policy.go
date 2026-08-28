package oap

import (
	"path"
	"sort"
)

type PermissionDecision string

const (
	Deny  PermissionDecision = "deny"
	Ask   PermissionDecision = "ask"
	Allow PermissionDecision = "allow"
)

var decisionRank = map[PermissionDecision]int{Deny: 0, Ask: 1, Allow: 2}

func NarrowDecision(policy, requested PermissionDecision) PermissionDecision {
	if decisionRank[policy] <= decisionRank[requested] {
		return policy
	}
	return requested
}

type EffectiveTools struct {
	Tools       []string
	Adjustments []Adjustment
}

func wildcard(pattern, value string) bool {
	matched, _ := path.Match(pattern, value)
	if matched {
		return true
	}
	if len(pattern) >= 2 && pattern[len(pattern)-2:] == "**" {
		return len(value) >= len(pattern)-2 && value[:len(pattern)-2] == pattern[:len(pattern)-2]
	}
	return false
}
func matchesAny(value string, patterns []string) bool {
	for _, pattern := range patterns {
		if wildcard(pattern, value) {
			return true
		}
	}
	return false
}
func IntersectTools(profile AgentProfile, granted []string) EffectiveTools {
	tools := obj(obj(profile["spec"])["tools"])
	policy := text(tools["policy"])
	if policy == "" {
		policy = "allowlist"
	}
	allow := strs(tools["allow"])
	deny := strs(tools["deny"])
	out := EffectiveTools{}
	for _, tool := range granted {
		included := policy == "inherit" || policy == "denylist" || matchesAny(tool, allow)
		if included && !matchesAny(tool, deny) {
			out.Tools = append(out.Tools, tool)
		} else {
			reason := "not included by profile allowlist"
			if matchesAny(tool, deny) {
				reason = "profile deny rule"
			}
			out.Adjustments = append(out.Adjustments, Adjustment{Field: "spec.tools:" + tool, Requested: tool, Effective: nil, Reason: reason})
		}
	}
	sort.Strings(out.Tools)
	return out
}
func NarrowPermissionMap(requested, policy map[string]PermissionDecision) (map[string]PermissionDecision, []Adjustment) {
	keys := map[string]bool{}
	for key := range requested {
		keys[key] = true
	}
	for key := range policy {
		keys[key] = true
	}
	out := map[string]PermissionDecision{}
	adjustments := []Adjustment{}
	for key := range keys {
		wanted := requested[key]
		if wanted == "" {
			wanted = Ask
		}
		ceiling := policy[key]
		if ceiling == "" {
			ceiling = Ask
		}
		effective := NarrowDecision(ceiling, wanted)
		out[key] = effective
		if effective != wanted {
			adjustments = append(adjustments, Adjustment{Field: "spec.permissions." + key, Requested: wanted, Effective: effective, Reason: "local policy ceiling"})
		}
	}
	return out, adjustments
}
