package io.github.alexmercedcoder.oap;

import static org.junit.jupiter.api.Assertions.*;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

final class OapConformanceTest {
  private static final Path REPOSITORY = Path.of("..").toAbsolutePath().normalize();
  private static ObjectNode fixture(String path) throws Exception { return OapParser.load(REPOSITORY.resolve(path)); }

  @Test void acceptsProfileAndDeltaCorpora() {
    for (String name : List.of("base-reviewer.agent.yaml", "code-reviewer.agent.yaml", "data-engineer.agent.yaml", "note-taker.agent.yaml", "python-reviewer.agent.yaml", "research-analyst.agent.md")) assertTrue(OapValidator.validate(REPOSITORY.resolve("examples").resolve(name)).ok(), name);
    assertTrue(OapValidator.validate(REPOSITORY.resolve("examples/json/note-taker.agent.json")).ok());
    for (String name : List.of("bad-name.agent.yaml", "future-major.agent.yaml", "literal-secret.agent.yaml", "missing-instructions.agent.yaml", "path-traversal.agent.yaml", "unknown-field.agent.yaml", "unknown-kind.agent.yaml")) assertFalse(OapValidator.validate(REPOSITORY.resolve("examples/invalid").resolve(name)).ok(), name);
    for (String name : List.of("learned-conventions.delta.yaml", "closes-thread.delta.yaml")) assertTrue(OapValidator.validate(REPOSITORY.resolve("tests/deltas").resolve(name)).ok(), name);
    for (String name : List.of("missing-revision.delta.yaml", "proposal-without-rationale.delta.yaml", "remove-with-value.delta.yaml", "writes-metadata.delta.yaml", "writes-spec.delta.yaml")) assertFalse(OapValidator.validate(REPOSITORY.resolve("tests/deltas/invalid").resolve(name)).ok(), name);
  }

  @Test void portableParsingAndDigests() throws Exception {
    ObjectNode document = OapParser.parse("created: 2026-08-28T00:00:00Z\nyes_value: yes\ntrue_value: true\n", Oap.Format.YAML);
    assertEquals("2026-08-28T00:00:00Z", document.path("created").asText()); assertEquals("yes", document.path("yes_value").asText()); assertTrue(document.path("true_value").asBoolean());
    assertThrows(OapParser.ParseException.class, () -> OapParser.parse("a: 1\na: 2\n", Oap.Format.YAML)); assertThrows(OapParser.ParseException.class, () -> OapParser.parse("", Oap.Format.YAML));
    ObjectNode profile = fixture("examples/note-taker.agent.yaml"); assertEquals("sha256:32ac424528ddffbbc3c0abeb98b1b18887d5ae5d04425a5466f4191a1b30c1e7", OapCanonical.profileDigest(profile)); assertEquals("sha256:fe2ddb1be24336d05d2b44ffe05d7bbbbfeb0def69c17503b0d5c931ff42fccc", OapCanonical.specDigest(profile));
    assertEquals("{\"a\":2,\"b\":1}", new String(OapCanonical.canonicalJson(JsonSupport.JSON.readTree("{\"b\":1,\"a\":2}")), java.nio.charset.StandardCharsets.UTF_8));
    ObjectNode changed = profile.deepCopy(); changed.set("state", JsonSupport.JSON.readTree("{\"summary\":\"different\"}")); ObjectNode metadata = Oap.object(changed.get("metadata")); metadata.put("revision", 99); metadata.put("updated_at", "2026-08-28T12:00:00Z"); metadata.put("trust", "project"); assertEquals(OapCanonical.specDigest(profile), OapCanonical.specDigest(changed)); changed.put("oap", "1.1"); assertFalse(OapValidator.validate(changed).ok());
  }

  @Test void policyRenderAndCompositionMatchOtherLibraries() throws Exception {
    assertEquals(OapPolicy.Decision.ASK, OapPolicy.narrow(OapPolicy.Decision.ASK, OapPolicy.Decision.ALLOW)); ObjectNode profile = fixture("examples/code-reviewer.agent.yaml"); assertEquals(List.of("read", "search"), OapPolicy.intersectTools(profile, List.of("read", "search", "shell", "write")).tools());
    String prompt = OapRenderer.render(profile, new OapRenderer.Options("PRE", "POST", Map.of())); int last = 0; for (String part : List.of("PRE", "You are a code reviewer", "Objectives:", "Persona:", "Constraints:", "Examples:", "PROFILE STATE", "POST")) { int at = prompt.indexOf(part); assertTrue(at >= last, part); last = at; }
    ObjectNode base = fixture("examples/base-reviewer.agent.yaml"); ObjectNode child = fixture("examples/python-reviewer.agent.yaml"); ObjectNode resolved = OapComposition.resolve(child, ignored -> base.deepCopy()); assertEquals(child.path("metadata").path("name"), resolved.path("metadata").path("name")); assertEquals(child.get("state"), resolved.get("state")); assertFalse(resolved.has("extends"));
  }

  @Test void deltaApplicationIsAtomicAndConflictSafe() throws Exception {
    ObjectNode profile = fixture("examples/code-reviewer.agent.yaml"); ObjectNode before = profile.deepCopy(); ObjectNode delta = fixture("tests/deltas/learned-conventions.delta.yaml"); OapDelta.ApplyOptions approved = new OapDelta.ApplyOptions(true, "alex", Instant.parse("2026-08-28T12:00:00Z")); Oap.DeltaApplication result = OapDelta.apply(profile, delta, approved);
    assertEquals(8, result.profile().path("metadata").path("revision").asInt()); assertEquals(before, profile); assertEquals(1, result.pendingProposals().size()); assertEquals("high", result.pendingProposals().get(0).path("risk").asText()); assertThrows(OapDelta.ApplyException.class, () -> OapDelta.apply(profile, delta, OapDelta.ApplyOptions.defaults()));
    ObjectNode revision = delta.deepCopy(); Oap.object(revision.get("target")).put("revision", 1); assertTrue(assertThrows(OapDelta.ApplyException.class, () -> OapDelta.apply(profile, revision, approved)).getMessage().contains("revision")); ObjectNode digest = delta.deepCopy(); Oap.object(digest.get("target")).put("digest", "sha256:" + "0".repeat(64)); assertThrows(OapDelta.ApplyException.class, () -> OapDelta.apply(profile, digest, approved));
    ObjectNode failing = delta.deepCopy(); failing.set("operations", JsonSupport.JSON.readTree("[{\"op\":\"replace\",\"path\":\"/state/summary\",\"value\":\"temporary\"},{\"op\":\"replace\",\"path\":\"/state/facts/id:does-not-exist\",\"value\":{}}]")); assertThrows(OapDelta.ApplyException.class, () -> OapDelta.apply(profile, failing, approved)); assertEquals(before, profile);
  }

  @Test void retentionMatchesOtherLibraries() throws Exception {
    ObjectNode profile = fixture("examples/code-reviewer.agent.yaml"); ObjectNode retention = Oap.object(Oap.object(Oap.object(profile.get("spec")).get("lifecycle")).get("retention")); retention.put("fact_ttl_days", 30); retention.put("max_facts", 2); retention.put("eviction", "least_confident"); retention.remove("max_history"); ObjectNode state = Oap.object(profile.get("state")); state.set("facts", JsonSupport.JSON.readTree("[{\"id\":\"fresh\",\"text\":\"fresh\",\"expires_at\":\"2026-09-01T00:00:00Z\",\"confidence\":0.9},{\"id\":\"expired\",\"text\":\"expired\",\"expires_at\":\"2026-01-01T00:00:00Z\",\"confidence\":0.1},{\"id\":\"pinned\",\"text\":\"pinned\",\"expires_at\":\"2026-01-01T00:00:00Z\",\"pinned\":true},{\"id\":\"weak\",\"text\":\"weak\",\"confidence\":0.05}]")); retention.put("max_open_threads", 2); state.set("open_threads", JsonSupport.JSON.readTree("[{\"id\":\"active\",\"status\":\"open\",\"updated_at\":\"2026-01-03T00:00:00Z\"},{\"id\":\"old-closed\",\"status\":\"done\",\"updated_at\":\"2026-01-01T00:00:00Z\"},{\"id\":\"new-closed\",\"status\":\"abandoned\",\"updated_at\":\"2026-01-02T00:00:00Z\"}]")); ArrayNode history = profile.putArray("history"); for (int revision = 1; revision <= 55; revision++) history.addObject().put("revision", revision);
    ObjectNode delta = fixture("tests/deltas/learned-conventions.delta.yaml"); delta.putArray("operations"); Oap.DeltaApplication result = OapDelta.apply(profile, delta, new OapDelta.ApplyOptions(true, null, Instant.parse("2026-08-28T12:00:00Z"))); assertEquals(List.of("fresh", "pinned"), ids(result.profile().path("state").path("facts"))); assertEquals(List.of("active", "new-closed"), ids(result.profile().path("state").path("open_threads"))); assertEquals(50, result.profile().path("history").size());
  }

  @Test void markdownAndAtomicWriteRoundTrip(@TempDir Path directory) throws Exception {
    ObjectNode profile = fixture("examples/research-analyst.agent.md"); String encoded = OapDelta.serialize(profile, Oap.Format.MARKDOWN); assertArrayEquals(OapCanonical.canonicalJson(profile), OapCanonical.canonicalJson(OapParser.parse(encoded, Oap.Format.MARKDOWN))); Path path = directory.resolve("profile.yaml"); Files.writeString(path, "old\n"); OapDelta.writeAtomically(path, OapDelta.serialize(profile, Oap.Format.YAML)); assertTrue(OapValidator.validate(path).ok());
  }
  private static List<String> ids(JsonNode array) { java.util.ArrayList<String> result = new java.util.ArrayList<>(); array.forEach(item -> result.add(item.path("id").asText())); return result; }
}
