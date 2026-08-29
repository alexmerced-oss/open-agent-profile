package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.networknt.schema.JsonSchema;
import com.networknt.schema.JsonSchemaFactory;
import com.networknt.schema.SpecVersion;
import com.networknt.schema.ValidationMessage;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Embedded-schema, semantic, and security validation for OAP documents. */
public final class OapValidator {
  private static final JsonSchema PROFILE = schema("/schema/agent-profile.schema.json");
  private static final JsonSchema DELTA = schema("/schema/agent-state-delta.schema.json");
  private static final Pattern ENV = Pattern.compile("^\\$\\{[A-Z][A-Z0-9_]{0,63}}$");
  private static final Pattern HEADER = Pattern.compile("^(Bearer )?\\$\\{[A-Z][A-Z0-9_]{0,63}}$");
  private static final Pattern VARIABLE = Pattern.compile("\\$\\{\\{\\s*vars\\.([A-Za-z_][A-Za-z0-9_]*)\\s*}}") ;
  private static final List<SecretPattern> SECRETS = List.of(
      new SecretPattern(Pattern.compile("AKIA[0-9A-Z]{16}"), "AWS access key"),
      new SecretPattern(Pattern.compile("gh[pousr]_[A-Za-z0-9_]{20,}"), "GitHub token"),
      new SecretPattern(Pattern.compile("sk-[A-Za-z0-9]{20,}"), "API key"),
      new SecretPattern(Pattern.compile("-----BEGIN [A-Z ]*PRIVATE KEY-----"), "private key"));

  private OapValidator() {}
  private record SecretPattern(Pattern pattern, String label) {}

  private static JsonSchema schema(String resource) {
    try (InputStream input = OapValidator.class.getResourceAsStream(resource)) {
      if (input == null) throw new IllegalStateException("embedded OAP schema is missing: " + resource);
      return JsonSchemaFactory.getInstance(SpecVersion.VersionFlag.V202012).getSchema(JsonSupport.JSON.readTree(input));
    } catch (IOException error) { throw new ExceptionInInitializerError(error); }
  }

  /** Validates an already parsed OAP document. */
  public static Oap.ValidationReport validate(ObjectNode document) { return validate(document, null); }

  /** Validates an already parsed OAP document and optionally checks its filename. */
  public static Oap.ValidationReport validate(ObjectNode document, Path filename) {
    List<Oap.Issue> errors = new ArrayList<>(); List<Oap.Issue> warnings = new ArrayList<>();
    String version = document.path("oap").asText("");
    if (!version.matches("\\d+\\.\\d+")) issue(errors, "/oap", "missing or malformed spec version string");
    else if (!Oap.SPEC_VERSION.equals(version)) issue(errors, "/oap", "unsupported OAP version " + version + "; unsupported versions fail closed");
    String kind = document.path("kind").asText("");
    if (!Set.of("AgentProfile", "AgentStateDelta").contains(kind)) issue(errors, "/kind", "\"" + kind + "\" is not a known 1.x kind");
    JsonSchema selected = kind.equals("AgentStateDelta") ? DELTA : PROFILE;
    for (ValidationMessage message : selected.validate(document)) issue(errors, message.getInstanceLocation().toString(), message.getMessage());
    walk(document, "", (text, pointer) -> SECRETS.forEach(secret -> { if (secret.pattern.matcher(text).find()) issue(errors, pointer, "looks like a literal " + secret.label + "; use a ${VARIABLE} reference"); }));
    if (kind.equals("AgentProfile")) checkProfile(document, filename, errors, warnings);
    else if (kind.equals("AgentStateDelta")) checkDelta(document, errors, warnings);
    Oap.Digests digests = kind.equals("AgentProfile") && errors.isEmpty() ? OapCanonical.digests(document) : null;
    return new Oap.ValidationReport(kind, document, List.copyOf(errors), List.copyOf(warnings), digests, errors.isEmpty());
  }

  /** Loads and validates a path, returning parse failures as ordinary issues. */
  public static Oap.ValidationReport validate(Path path) {
    try { return validate(OapParser.load(path), path); }
    catch (OapParser.ParseException error) { return new Oap.ValidationReport("", null, List.of(new Oap.Issue("", error.getMessage())), List.of(), null, false); }
  }

  /** Returns whether a relative path lexically escapes its workspace root. */
  public static boolean escapesWorkspace(String raw) {
    Path path;
    try { path = Path.of(raw); } catch (RuntimeException error) { return true; }
    if (path.isAbsolute()) return true;
    int depth = 0;
    for (Path part : path) { String value = part.toString(); if (value.equals("..")) { if (--depth < 0) return true; } else if (!value.equals(".")) depth++; }
    return false;
  }

  private static void checkProfile(ObjectNode document, Path filename, List<Oap.Issue> errors, List<Oap.Issue> warnings) {
    ObjectNode spec = Oap.object(document.get("spec")); ObjectNode context = Oap.object(spec.get("context"));
    JsonNode servers = Oap.object(spec.get("tools")).get("mcp_servers");
    if (servers != null && servers.isArray()) for (int index = 0; index < servers.size(); index++) {
      final int serverIndex = index; ObjectNode server = Oap.object(servers.get(index));
      Oap.object(server.get("env")).properties().forEach(entry -> { String value = entry.getValue().asText(""); if (!ENV.matcher(value).matches() || !value.equals("${" + entry.getKey() + "}")) issue(errors, "/spec/tools/mcp_servers/" + serverIndex + "/env/" + entry.getKey(), "must be a same-name ${VARIABLE} reference, not a literal"); });
      Oap.object(server.get("headers")).properties().forEach(entry -> { if (!HEADER.matcher(entry.getValue().asText("")).matches()) issue(errors, "/spec/tools/mcp_servers/" + serverIndex + "/headers/" + entry.getKey(), "must be '${VARIABLE}' or 'Bearer ${VARIABLE}'"); });
    }
    JsonNode files = context.get("files"); if (files != null && files.isArray()) for (int index = 0; index < files.size(); index++) { String value = Oap.object(files.get(index)).path("path").asText(""); if (escapesWorkspace(value)) issue(errors, "/spec/context/files/" + index + "/path", "\"" + value + "\" resolves outside the workspace"); }
    if (context.has("working_directory") && escapesWorkspace(context.path("working_directory").asText())) issue(errors, "/spec/context/working_directory", "working directory resolves outside the workspace");
    Set<String> variables = new HashSet<>(); Oap.object(context.get("variables")).properties().forEach(entry -> variables.add(entry.getKey()));
    walk(document, "", (text, pointer) -> { Matcher matcher = VARIABLE.matcher(text); while (matcher.find()) { if (!variables.contains(matcher.group(1))) issue(errors, pointer, "references undefined variable \"" + matcher.group(1) + "\""); if (pointer.startsWith("/state")) issue(warnings, pointer, "contains a ${{ vars.* }} template; substitution never runs inside state"); } });
    ObjectNode state = Oap.object(document.get("state"));
    for (String collection : List.of("facts", "preferences", "open_threads", "glossary")) { Set<String> ids = new HashSet<>(); JsonNode entries = state.get(collection); if (entries != null && entries.isArray()) for (int index = 0; index < entries.size(); index++) { String id = Oap.object(entries.get(index)).path("id").asText(""); if (!id.isEmpty() && !ids.add(id)) issue(errors, "/state/" + collection + "/" + index + "/id", "duplicate id \"" + id + "\""); } }
    ObjectNode metadata = Oap.object(document.get("metadata")); if (metadata.has("trust")) issue(warnings, "/metadata/trust", "trust in the file must be discarded and recomputed from the discovery root");
    Path fileName = filename == null ? null : filename.getFileName();
    if (fileName != null && metadata.has("name")) { String file = fileName.toString(); String base = file.contains(".") ? file.substring(0, file.indexOf('.')) : file; String name = metadata.path("name").asText(); if (!base.equals(name)) issue(warnings, "/metadata/name", "\"" + name + "\" does not match file name \"" + base + "\"; metadata.name wins"); }
    long previous = 0; JsonNode history = document.get("history"); if (history != null && history.isArray()) for (JsonNode entry : history) { long revision = Oap.object(entry).path("revision").asLong(); if (revision < previous) { issue(errors, "/history", "entries must be ordered oldest first by revision"); break; } previous = revision; }
    if (previous > metadata.path("revision").asLong()) issue(errors, "/history", "newest history revision exceeds metadata.revision");
    ObjectNode tools = Oap.object(spec.get("tools")); String policy = tools.path("policy").asText(""); if (policy.equals("inherit") && (!JsonSupport.strings(tools.get("allow")).isEmpty() || !JsonSupport.strings(tools.get("deny")).isEmpty())) issue(warnings, "/spec/tools", "policy is 'inherit', so allow and deny are ignored"); if (policy.equals("allowlist") && JsonSupport.strings(tools.get("allow")).isEmpty()) issue(warnings, "/spec/tools", "allowlist has an empty allow list, so the agent gets no tools");
  }

  private static void checkDelta(ObjectNode document, List<Oap.Issue> errors, List<Oap.Issue> warnings) {
    JsonNode operations = document.get("operations"); if (operations != null && operations.isArray()) for (int index = 0; index < operations.size(); index++) { String path = Oap.object(operations.get(index)).path("path").asText(""); if (!path.equals("/state") && !path.startsWith("/state/")) issue(errors, "/operations/" + index + "/path", "operation is outside /state; contract changes belong in proposals"); }
    JsonNode proposals = document.get("proposals"); if (proposals != null && proposals.isArray()) for (int index = 0; index < proposals.size(); index++) { ObjectNode proposal = Oap.object(proposals.get(index)); String path = proposal.path("path").asText(""); if (highRisk(path) && !proposal.path("risk").asText("").equals("high")) issue(warnings, "/proposals/" + index, path + " must be treated as high risk regardless of its declared risk"); }
  }
  static boolean highRisk(String path) { return List.of("/spec/tools", "/spec/permissions", "/spec/memory", "/spec/runtime/subagents").stream().anyMatch(path::startsWith); }
  private static void issue(List<Oap.Issue> target, String pointer, String message) { target.add(new Oap.Issue(pointer, message)); }
  @FunctionalInterface private interface StringVisitor { void accept(String text, String pointer); }
  private static void walk(JsonNode value, String pointer, StringVisitor visitor) { if (value == null) return; if (value.isObject()) value.properties().forEach(entry -> walk(entry.getValue(), pointer + "/" + entry.getKey().replace("~", "~0").replace("/", "~1"), visitor)); else if (value.isArray()) for (int index = 0; index < value.size(); index++) walk(value.get(index), pointer + "/" + index, visitor); else if (value.isTextual()) visitor.accept(value.textValue(), pointer); }
}
