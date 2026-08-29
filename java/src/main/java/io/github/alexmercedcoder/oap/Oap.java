package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.util.List;

/** Shared OAP constants and immutable result types. */
public final class Oap {
  /** OAP specification version implemented by this library. */
  public static final String SPEC_VERSION = "1.0";
  /** Java support-library version. */
  public static final String SUPPORT_VERSION = "1.0.4";
  private Oap() {}

  /** Supported document encoding. */
  public enum Format { YAML, JSON, MARKDOWN }
  /** One validation issue located by JSON Pointer. */
  public record Issue(String pointer, String message) {}
  /** Full-profile and immutable-specification identities. */
  public record Digests(String profile, String spec) {}
  /** Complete schema, semantic, and security validation result. */
  public record ValidationReport(String kind, ObjectNode document, List<Issue> errors, List<Issue> warnings, Digests digests, boolean ok) {}
  /** A requested policy value narrowed by an effective ceiling. */
  public record Adjustment(String field, JsonNode requested, JsonNode effective, String reason) {}
  /** Successful result of atomically applying a state delta. */
  public record DeltaApplication(ObjectNode profile, List<String> warnings, List<ObjectNode> pendingProposals) {}

  static ObjectNode object(JsonNode value) { return value != null && value.isObject() ? (ObjectNode) value : JsonSupport.emptyObject(); }
}
