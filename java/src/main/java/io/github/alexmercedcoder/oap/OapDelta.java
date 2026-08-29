package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Conflict-safe state delta application, retention, serialization, and atomic persistence. */
public final class OapDelta {
  private OapDelta() {}
  /** Approval, attribution, and deterministic time controls. */
  public record ApplyOptions(boolean approved, String actor, Instant now) {
    /** Default unapproved options using the current time. */
    public static ApplyOptions defaults() { return new ApplyOptions(false, null, null); }
  }
  /** Delta conflict or application failure. */
  public static final class ApplyException extends Exception {
    private static final long serialVersionUID = 1L;
    ApplyException(String message) { super(message); }
    ApplyException(String message, Throwable cause) { super(message, cause); }
  }

  /** Applies validated state operations to a deep copy; the input profile is never mutated. */
  public static Oap.DeltaApplication apply(ObjectNode profile, ObjectNode delta, ApplyOptions options) throws ApplyException {
    ObjectNode metadata = Oap.object(profile.get("metadata")); long current = metadata.path("revision").asLong(1); ObjectNode target = Oap.object(delta.get("target"));
    if (!target.path("name").asText("").equals(metadata.path("name").asText(""))) throw new ApplyException("delta targets a different profile name");
    if (target.path("revision").asLong(Long.MIN_VALUE) != current) throw new ApplyException("delta targets revision " + target.path("revision") + " but profile is at " + current);
    if (target.has("digest") && !target.path("digest").asText().equals(OapCanonical.profileDigest(profile))) throw new ApplyException("target.digest does not match profile");
    String writeback = Oap.object(Oap.object(profile.get("spec")).get("lifecycle")).path("writeback").asText("propose");
    if (writeback.equals("off")) throw new ApplyException("lifecycle.writeback is 'off'");
    if (writeback.equals("propose") && !options.approved()) throw new ApplyException("lifecycle.writeback is 'propose'; explicit approval is required");
    ObjectNode working = profile.deepCopy(); if (!working.path("state").isObject()) working.set("state", JsonSupport.emptyObject()); List<String> warnings = new ArrayList<>();
    JsonNode operations = delta.get("operations"); if (operations != null && operations.isArray()) for (int index = 0; index < operations.size(); index++) try { applyOperation(working, Oap.object(operations.get(index)), warnings); } catch (ApplyException error) { throw new ApplyException("operation " + index + ": " + error.getMessage(), error); }
    String stamp = (options.now() == null ? Instant.now() : options.now()).truncatedTo(ChronoUnit.SECONDS).toString(); enforceRetention(working, warnings, stamp);
    ObjectNode workingMetadata = Oap.object(working.get("metadata")); workingMetadata.put("revision", current + 1); workingMetadata.put("updated_at", stamp);
    if (operations != null && !operations.isEmpty()) { ObjectNode state = Oap.object(working.get("state")); state.put("revision", state.path("revision").asLong(0) + 1); state.put("updated_at", stamp); }
    ObjectNode session = Oap.object(delta.get("session")); String actor = options.actor() == null ? "oap-java" : options.actor(); String by = session.path("id").asText(actor); ObjectNode entry = JsonSupport.emptyObject(); entry.put("revision", current + 1); entry.put("at", stamp); entry.put("by", by); entry.put("change", delta.has("summary") ? delta.path("summary").asText() : arraySize(operations) + " state operations"); entry.putArray("sections").add("state"); if (session.has("id")) entry.set("session_id", session.get("id").deepCopy()); if (session.has("harness")) entry.set("harness", session.get("harness").deepCopy()); if (options.approved()) entry.put("approved_by", actor);
    ArrayNode history; if (working.path("history").isArray()) history = (ArrayNode) working.get("history"); else { history = JsonSupport.JSON.createArrayNode(); working.set("history", history); } history.add(entry); enforceRetention(working, warnings, stamp);
    List<ObjectNode> pending = new ArrayList<>(); JsonNode proposals = delta.get("proposals"); if (proposals != null && proposals.isArray()) proposals.forEach(raw -> { ObjectNode proposal = Oap.object(raw).deepCopy(); if (OapValidator.highRisk(proposal.path("path").asText(""))) proposal.put("risk", "high"); pending.add(proposal); });
    return new Oap.DeltaApplication(working, List.copyOf(warnings), List.copyOf(pending));
  }

  private static int arraySize(JsonNode value) { return value != null && value.isArray() ? value.size() : 0; }
  private static List<String> tokens(String pointer) { List<String> result = new ArrayList<>(); for (String token : pointer.split("/", -1)) if (!token.isEmpty()) result.add(token.replace("~1", "/").replace("~0", "~")); return result; }
  private static void applyOperation(ObjectNode document, ObjectNode operation, List<String> warnings) throws ApplyException {
    String kind = operation.path("op").asText(""); String pointer = operation.path("path").asText(""); if (!pointer.equals("/state") && !pointer.startsWith("/state/")) throw new ApplyException("operation path \"" + pointer + "\" is outside /state");
    boolean missing = modify(document, tokens(pointer), 0, kind, operation.get("value")); if (!missing) return; if (kind.equals("remove")) warnings.add("remove on missing path \"" + pointer + "\" ignored"); else throw new ApplyException("path \"" + pointer + "\" does not resolve");
  }
  private static boolean modify(JsonNode current, List<String> tokens, int position, String kind, JsonNode value) throws ApplyException {
    if (position >= tokens.size()) throw new ApplyException("root replacement is unsupported"); String token = tokens.get(position); boolean last = position == tokens.size() - 1;
    if (current.isObject()) { ObjectNode object = (ObjectNode) current; if (last) { if (kind.equals("remove")) return object.remove(token) == null; if (kind.equals("add") || kind.equals("replace")) { object.set(token, value == null ? JsonSupport.JSON.nullNode() : value.deepCopy()); return false; } throw new ApplyException("unknown operation \"" + kind + "\""); } if (!object.has(token)) object.set(token, JsonSupport.emptyObject()); return modify(object.get(token), tokens, position + 1, kind, value); }
    if (current.isArray()) { ArrayNode array = (ArrayNode) current; int index = arrayIndex(array, token); if (index < 0) return true; if (last) { if (kind.equals("add") && index <= array.size()) { array.insert(index, value == null ? JsonSupport.JSON.nullNode() : value.deepCopy()); return false; } if (kind.equals("remove") && index < array.size()) { array.remove(index); return false; } if (kind.equals("replace") && index < array.size()) { array.set(index, value == null ? JsonSupport.JSON.nullNode() : value.deepCopy()); return false; } if (Set.of("add", "remove", "replace").contains(kind)) return true; throw new ApplyException("unknown operation \"" + kind + "\""); } return index < array.size() ? modify(array.get(index), tokens, position + 1, kind, value) : true; }
    return true;
  }
  private static int arrayIndex(ArrayNode array, String token) { if (token.equals("-")) return array.size(); if (token.startsWith("id:")) { String id = token.substring(3); for (int index = 0; index < array.size(); index++) if (Oap.object(array.get(index)).path("id").asText("").equals(id)) return index; return -1; } try { return Integer.parseUnsignedInt(token); } catch (NumberFormatException error) { return -1; } }

  private static void enforceRetention(ObjectNode profile, List<String> warnings, String now) {
    ObjectNode retention = Oap.object(Oap.object(Oap.object(profile.get("spec")).get("lifecycle")).get("retention")); String strategy = retention.path("eviction").asText("least_recently_used"); ObjectNode state = Oap.object(profile.get("state"));
    for (String collection : List.of("facts", "preferences")) if (state.path(collection).isArray()) { ArrayNode entries = (ArrayNode) state.get(collection); List<JsonNode> kept = new ArrayList<>(); for (JsonNode raw : entries) { ObjectNode entry = Oap.object(raw); boolean expired = retention.has("fact_ttl_days") && entry.has("expires_at") && entry.path("expires_at").asText().compareTo(now) < 0 && !entry.path("pinned").asBoolean(false); if (expired) warnings.add("evicted expired " + collection + " entry \"" + entry.path("id").asText("") + "\""); else kept.add(raw); } entries.removeAll(); entries.addAll(kept);
      if (collection.equals("facts") && retention.has("max_facts") && entries.size() > retention.path("max_facts").asInt()) evictFacts(entries, retention.path("max_facts").asInt(), strategy, warnings);
    }
    if (retention.has("max_open_threads") && state.path("open_threads").isArray()) evictThreads((ArrayNode) state.get("open_threads"), retention.path("max_open_threads").asInt(), warnings);
    int historyCap = retention.path("max_history").asInt(50); if (profile.path("history").isArray()) { ArrayNode history = (ArrayNode) profile.get("history"); while (history.size() > historyCap) history.remove(0); }
  }
  private static void evictFacts(ArrayNode entries, int cap, String strategy, List<String> warnings) { List<ObjectNode> candidates = new ArrayList<>(); entries.forEach(raw -> { ObjectNode entry = Oap.object(raw); if (!entry.path("pinned").asBoolean(false)) candidates.add(entry); }); candidates.sort(Comparator.comparing(entry -> sortValue(entry, strategy))); int count = Math.min(entries.size() - cap, candidates.size()); Set<String> dropped = new HashSet<>(); for (int index = 0; index < count; index++) { String id = candidates.get(index).path("id").asText(""); dropped.add(id); warnings.add("evicted facts entry \"" + id + "\" (" + strategy + ")"); } List<JsonNode> kept = new ArrayList<>(); entries.forEach(raw -> { if (!dropped.contains(Oap.object(raw).path("id").asText(""))) kept.add(raw); }); entries.removeAll(); entries.addAll(kept); }
  private static String sortValue(ObjectNode entry, String strategy) { if (strategy.equals("least_confident")) return String.format("%020.10f", entry.path("confidence").asDouble(1.0)); if (strategy.equals("oldest")) return entry.has("learned_at") ? entry.path("learned_at").asText() : entry.path("opened_at").asText(""); for (String key : List.of("last_used_at", "updated_at", "learned_at")) if (entry.has(key)) return entry.path(key).asText(); return ""; }
  private static void evictThreads(ArrayNode threads, int cap, List<String> warnings) { if (threads.size() <= cap) return; List<ObjectNode> closed = new ArrayList<>(); threads.forEach(raw -> { ObjectNode entry = Oap.object(raw); if (Set.of("done", "abandoned").contains(entry.path("status").asText())) closed.add(entry); }); closed.sort(Comparator.comparing(entry -> entry.path("updated_at").asText(""))); int count = Math.min(threads.size() - cap, closed.size()); Set<String> dropped = new HashSet<>(); for (int index = 0; index < count; index++) { String id = closed.get(index).path("id").asText(""); dropped.add(id); warnings.add("evicted closed thread \"" + id + "\""); } List<JsonNode> kept = new ArrayList<>(); threads.forEach(raw -> { if (!dropped.contains(Oap.object(raw).path("id").asText(""))) kept.add(raw); }); threads.removeAll(); threads.addAll(kept); if (threads.size() > cap) { while (threads.size() > cap) threads.remove(0); warnings.add("open_threads still over cap after evicting closed threads"); } }

  /** Serializes an OAP document as YAML, JSON, or Markdown. */
  public static String serialize(ObjectNode document, Oap.Format format) throws ApplyException {
    try { if (format == Oap.Format.JSON) return JsonSupport.JSON.writeValueAsString(document) + "\n"; if (format == Oap.Format.YAML) return JsonSupport.YAML.writeValueAsString(document); ObjectNode copy = document.deepCopy(); ObjectNode role = Oap.object(Oap.object(copy.get("spec")).get("role")); String instructions = role.path("instructions").asText(""); role.remove("instructions"); String yaml = JsonSupport.YAML.writeValueAsString(copy); return "---\n" + yaml.replaceFirst("^---\\s*\\n", "") + "---\n" + instructions.stripTrailing() + "\n"; } catch (JsonProcessingException error) { throw new ApplyException("serialization error", error); }
  }
  /** Replaces a file using a same-directory temporary and an atomic move when supported. */
  public static void writeAtomically(Path path, byte[] data) throws ApplyException { Path absolute = path.toAbsolutePath(); Path directory = absolute.getParent(); if (directory == null || absolute.getFileName() == null) throw new ApplyException("target must be a file path"); try { Path temporary = Files.createTempFile(directory, "." + absolute.getFileName() + ".", ".tmp"); try { Files.write(temporary, data); try (FileChannel file = FileChannel.open(temporary, StandardOpenOption.WRITE)) { file.force(true); } Files.move(temporary, absolute, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING); try (FileChannel folder = FileChannel.open(directory, StandardOpenOption.READ)) { folder.force(true); } } finally { Files.deleteIfExists(temporary); } } catch (IOException error) { throw new ApplyException(error.getMessage(), error); } }
  /** UTF-8 convenience overload for atomic replacement. */
  public static void writeAtomically(Path path, String data) throws ApplyException { writeAtomically(path, data.getBytes(StandardCharsets.UTF_8)); }
}
