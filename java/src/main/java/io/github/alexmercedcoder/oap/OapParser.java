package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;

/** Strict YAML 1.2, JSON, and Markdown parsing. */
public final class OapParser {
  private OapParser() {}

  /** Loading or parsing failure. */
  public static final class ParseException extends Exception {
    private static final long serialVersionUID = 1L;
    ParseException(String message, Throwable cause) { super(message, cause); }
  }

  /** Infers the document format from its extension. */
  public static Oap.Format format(Path path) {
    String value = path.toString().toLowerCase(Locale.ROOT);
    if (value.endsWith(".json")) return Oap.Format.JSON;
    if (value.endsWith(".md")) return Oap.Format.MARKDOWN;
    return Oap.Format.YAML;
  }

  /** Parses an OAP document with duplicate-key rejection. */
  public static ObjectNode parse(String input, Oap.Format format) throws ParseException {
    if (input == null || input.trim().isEmpty()) throw new ParseException("parse error: empty document", null);
    try {
      if (format == Oap.Format.MARKDOWN) {
        if (!input.startsWith("---\n")) throw new ParseException("Markdown profile must begin with YAML frontmatter", null);
        int end = input.indexOf("\n---\n", 4);
        if (end < 0) throw new ParseException("unterminated Markdown frontmatter", null);
        ObjectNode document = parse(input.substring(4, end), Oap.Format.YAML);
        String body = input.substring(end + 5).trim();
        ObjectNode role = Oap.object(Oap.object(document.get("spec")).get("role"));
        if (role.has("instructions") && !body.isEmpty()) throw new ParseException("Markdown encoding supplies spec.role.instructions in both frontmatter and body", null);
        role.put("instructions", body);
        return document;
      }
      JsonNode value = format == Oap.Format.JSON ? JsonSupport.JSON.readTree(input) : JsonSupport.parseYaml(input);
      if (value == null || !value.isObject()) throw new ParseException("document root must be an object", null);
      return (ObjectNode) value;
    } catch (ParseException error) {
      throw error;
    } catch (IOException | RuntimeException error) {
      throw new ParseException("parse error: " + error.getMessage(), error);
    }
  }

  /** Loads an OAP document and infers its format from its path. */
  public static ObjectNode load(Path path) throws ParseException {
    try { return parse(Files.readString(path, StandardCharsets.UTF_8), format(path)); }
    catch (IOException error) { throw new ParseException(error.getMessage(), error); }
  }
}
