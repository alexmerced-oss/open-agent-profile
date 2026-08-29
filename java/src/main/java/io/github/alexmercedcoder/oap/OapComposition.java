package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.util.ArrayList;
import java.util.List;

/** Verified profile inheritance and deterministic deep merging. */
public final class OapComposition {
  private OapComposition() {}
  /** A pinned inheritance reference supplied to a loader. */
  public record ProfileReference(String name, String uri, long revision, String digest) {}
  /** Loads one referenced profile. */
  @FunctionalInterface public interface ProfileLoader { ObjectNode load(ProfileReference reference) throws CompositionException; }
  /** Composition cycle, depth, loading, or pinning failure. */
  public static final class CompositionException extends Exception {
    private static final long serialVersionUID = 1L;
    /** Creates a composition failure with a safe public message. */
    public CompositionException(String message) { super(message); }
  }

  /** Deep-merges a child object over a base using OAP null-delete semantics. */
  public static ObjectNode merge(ObjectNode base, ObjectNode child) {
    ObjectNode result = base.deepCopy();
    child.properties().forEach(entry -> { JsonNode value = entry.getValue();
      if (value.isNull()) result.remove(entry.getKey());
      else if (value.isObject() && result.path(entry.getKey()).isObject()) result.set(entry.getKey(), merge((ObjectNode) result.get(entry.getKey()), (ObjectNode) value));
      else result.set(entry.getKey(), value.deepCopy()); });
    return result;
  }

  /** Resolves, verifies, and merges a profile's complete inheritance chain. */
  public static ObjectNode resolve(ObjectNode profile, ProfileLoader loader) throws CompositionException { return resolve(profile, loader, new ArrayList<>()); }
  private static ObjectNode resolve(ObjectNode profile, ProfileLoader loader, List<String> active) throws CompositionException {
    String name = Oap.object(profile.get("metadata")).path("name").asText(""); if (active.contains(name)) throw new CompositionException("inheritance cycle"); if (active.size() >= 8) throw new CompositionException("inheritance depth exceeds 8"); active.add(name);
    ObjectNode merged = JsonSupport.emptyObject(); JsonNode extensions = profile.get("extends");
    if (extensions != null && extensions.isArray()) for (JsonNode raw : extensions) { ObjectNode value = Oap.object(raw); ProfileReference reference = new ProfileReference(value.path("name").asText(""), value.path("uri").asText(""), value.path("revision").asLong(0), value.path("digest").asText("")); ObjectNode base = loader.load(reference);
      if (reference.revision() != 0 && Oap.object(base.get("metadata")).path("revision").asLong() != reference.revision()) throw new CompositionException(reference.name() + " revision does not match pin");
      if (!reference.digest().isEmpty() && !OapCanonical.specDigest(base).equals(reference.digest())) throw new CompositionException(reference.name() + " digest does not match pin");
      ObjectNode resolved = resolve(base, loader, active); resolved.remove(List.of("extends", "state", "history")); ObjectNode metadata = Oap.object(resolved.get("metadata")); metadata.remove(List.of("name", "id", "revision")); merged = merge(merged, resolved); }
    active.remove(active.size() - 1); merged = merge(merged, profile); merged.set("metadata", profile.path("metadata").deepCopy());
    for (String key : List.of("state", "history")) { if (profile.has(key)) merged.set(key, profile.get(key).deepCopy()); else merged.remove(key); }
    merged.remove("extends"); if (Oap.object(merged.get("metadata")).path("name").asText("").isEmpty()) throw new CompositionException("composed profile has no name"); return merged;
  }
}
