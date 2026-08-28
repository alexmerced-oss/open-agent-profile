use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Adjustment, AgentProfile, object, strings};

/// Ordered permission decision used for fail-closed narrowing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    /// Operation is forbidden.
    Deny,
    /// Operation requires approval.
    Ask,
    /// Operation is allowed.
    Allow,
}

/// Returns the more restrictive of a policy ceiling and requested decision.
pub fn narrow_decision(
    policy: PermissionDecision,
    requested: PermissionDecision,
) -> PermissionDecision {
    std::cmp::min(policy, requested)
}

/// Effective tool set and explanations for removed grants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveTools {
    /// Sorted, deduplicated tools allowed by both profile and harness.
    pub tools: Vec<String>,
    /// Explanations for requested tools that were removed.
    pub adjustments: Vec<Adjustment>,
}

fn wildcard(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    pattern == value
}

/// Intersects harness-granted tools with an OAP profile's tool policy.
pub fn intersect_tools(
    profile: &AgentProfile,
    granted: impl IntoIterator<Item = impl AsRef<str>>,
) -> EffectiveTools {
    let tools = object(object(profile.get("spec")).get("tools"));
    let policy = tools
        .get("policy")
        .and_then(Value::as_str)
        .unwrap_or("inherit");
    let allow = strings(tools.get("allow"));
    let deny = strings(tools.get("deny"));
    let mut effective = vec![];
    let mut adjustments = vec![];
    for raw in granted {
        let tool = raw.as_ref();
        let requested = policy != "deny_all"
            && (policy != "allowlist" || allow.iter().any(|item| wildcard(item, tool)))
            && !deny.iter().any(|item| wildcard(item, tool));
        if requested {
            effective.push(tool.to_owned());
        } else {
            adjustments.push(Adjustment {
                field: format!("tools.{tool}"),
                requested: Value::String("allow".into()),
                effective: Value::String("deny".into()),
                reason: "profile and harness capabilities intersect; they never union".into(),
            });
        }
    }
    effective.sort();
    effective.dedup();
    EffectiveTools {
        tools: effective,
        adjustments,
    }
}

/// Applies field-by-field harness ceilings to a requested permission map.
pub fn narrow_permission_map(
    requested: &std::collections::BTreeMap<String, PermissionDecision>,
    policy: &std::collections::BTreeMap<String, PermissionDecision>,
) -> (
    std::collections::BTreeMap<String, PermissionDecision>,
    Vec<Adjustment>,
) {
    let keys: BTreeSet<_> = requested.keys().chain(policy.keys()).cloned().collect();
    let mut effective = std::collections::BTreeMap::new();
    let mut adjustments = vec![];
    for key in keys {
        let ask = *requested.get(&key).unwrap_or(&PermissionDecision::Ask);
        let ceiling = *policy.get(&key).unwrap_or(&PermissionDecision::Ask);
        let value = narrow_decision(ceiling, ask);
        effective.insert(key.clone(), value);
        if value != ask {
            adjustments.push(Adjustment {
                field: key,
                requested: serde_json::to_value(ask).unwrap(),
                effective: serde_json::to_value(value).unwrap(),
                reason: "harness policy is the upper bound".into(),
            });
        }
    }
    (effective, adjustments)
}
