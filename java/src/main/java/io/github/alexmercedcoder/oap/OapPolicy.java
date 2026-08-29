package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.node.TextNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;

/** Fail-closed permission narrowing and tool intersection. */
public final class OapPolicy {
  private OapPolicy() {}
  /** Ordered permission decision. */
  public enum Decision { DENY, ASK, ALLOW }
  /** Effective tools and explanations for removed grants. */
  public record EffectiveTools(List<String> tools, List<Oap.Adjustment> adjustments) {}
  /** Effective permission map and explanations for narrowed decisions. */
  public record EffectivePermissions(Map<String, Decision> permissions, List<Oap.Adjustment> adjustments) {}

  /** Returns the more restrictive policy or requested decision. */
  public static Decision narrow(Decision policy, Decision requested) {
    return policy.ordinal() < requested.ordinal() ? policy : requested;
  }

  private static boolean wildcard(String pattern, String value) {
    if (pattern.equals("*")) return true;
    return pattern.endsWith("*") ? value.startsWith(pattern.substring(0, pattern.length() - 1)) : pattern.equals(value);
  }

  /** Intersects harness-granted tools with the profile tool policy. */
  public static EffectiveTools intersectTools(ObjectNode profile, Collection<String> granted) {
    ObjectNode tools = Oap.object(Oap.object(profile.get("spec")).get("tools")); String policy = tools.path("policy").asText("inherit");
    List<String> allow = JsonSupport.strings(tools.get("allow")); List<String> deny = JsonSupport.strings(tools.get("deny"));
    Set<String> effective = new TreeSet<>(); List<Oap.Adjustment> adjustments = new ArrayList<>();
    for (String tool : granted) {
      boolean accepted = !policy.equals("deny_all") && (!policy.equals("allowlist") || allow.stream().anyMatch(pattern -> wildcard(pattern, tool))) && deny.stream().noneMatch(pattern -> wildcard(pattern, tool));
      if (accepted) effective.add(tool); else adjustments.add(new Oap.Adjustment("tools." + tool, TextNode.valueOf("allow"), TextNode.valueOf("deny"), "profile and harness capabilities intersect; they never union"));
    }
    return new EffectiveTools(List.copyOf(effective), List.copyOf(adjustments));
  }

  /** Applies field-by-field policy ceilings to requested permissions. */
  public static EffectivePermissions narrowPermissions(Map<String, Decision> requested, Map<String, Decision> policy) {
    Set<String> keys = new TreeSet<>(); keys.addAll(requested.keySet()); keys.addAll(policy.keySet());
    Map<String, Decision> effective = new TreeMap<>(); List<Oap.Adjustment> adjustments = new ArrayList<>();
    for (String key : keys) { Decision ask = requested.getOrDefault(key, Decision.ASK); Decision ceiling = policy.getOrDefault(key, Decision.ASK); Decision value = narrow(ceiling, ask); effective.put(key, value);
      if (value != ask) adjustments.add(new Oap.Adjustment(key, TextNode.valueOf(ask.name().toLowerCase()), TextNode.valueOf(value.name().toLowerCase()), "harness policy is the upper bound")); }
    return new EffectivePermissions(Map.copyOf(effective), List.copyOf(adjustments));
  }
}
