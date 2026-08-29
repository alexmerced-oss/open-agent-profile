package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.fasterxml.jackson.databind.node.JsonNodeFactory;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.fasterxml.jackson.dataformat.yaml.YAMLFactory;
import com.fasterxml.jackson.dataformat.yaml.YAMLGenerator;
import java.util.ArrayList;
import java.util.List;
import org.snakeyaml.engine.v2.api.Load;
import org.snakeyaml.engine.v2.api.LoadSettings;
import org.snakeyaml.engine.v2.schema.JsonSchema;

final class JsonSupport {
  static final ObjectMapper JSON = new ObjectMapper().enable(DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS);
  static final ObjectMapper YAML = new ObjectMapper(YAMLFactory.builder()
      .disable(YAMLGenerator.Feature.WRITE_DOC_START_MARKER).build());
  static {
    JSON.enable(JsonParser.Feature.STRICT_DUPLICATE_DETECTION);
    JSON.enable(SerializationFeature.INDENT_OUTPUT);
  }
  private JsonSupport() {}
  static ObjectNode emptyObject() { return JsonNodeFactory.instance.objectNode(); }
  static List<String> strings(JsonNode value) {
    List<String> out = new ArrayList<>(); if (value != null && value.isArray()) value.forEach(item -> { if (item.isTextual()) out.add(item.textValue()); }); return out;
  }
  static JsonNode parseYaml(String input) {
    LoadSettings settings = LoadSettings.builder().setAllowDuplicateKeys(false).setAllowRecursiveKeys(false)
        .setAllowNonScalarKeys(false).setSchema(new JsonSchema()).build();
    try { return JSON.readTree(JSON.writeValueAsBytes(new Load(settings).loadFromString(input))); }
    catch (java.io.IOException error) { throw new IllegalArgumentException("YAML conversion failed", error); }
  }
}
