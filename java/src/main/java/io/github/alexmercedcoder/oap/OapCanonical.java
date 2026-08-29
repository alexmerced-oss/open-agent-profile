package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.List;
import org.erdtman.jcs.JsonCanonicalizer;

/** RFC 8785 canonicalization and OAP identities. */
public final class OapCanonical {
  private OapCanonical() {}

  /** Returns the RFC 8785 UTF-8 representation of a JSON value. */
  public static byte[] canonicalJson(JsonNode value) {
    try { return new JsonCanonicalizer(value.toString()).getEncodedUTF8(); }
    catch (IOException error) { throw new IllegalArgumentException("canonical JSON error", error); }
  }
  private static String digest(JsonNode value) {
    try { return "sha256:" + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(canonicalJson(value))); }
    catch (NoSuchAlgorithmException impossible) { throw new IllegalStateException("SHA-256 is unavailable", impossible); }
  }
  /** Computes the identity of the full profile. */
  public static String profileDigest(ObjectNode profile) { return digest(profile); }
  /** Computes the identity of immutable metadata and the profile specification. */
  public static String specDigest(ObjectNode profile) {
    ObjectNode metadata = Oap.object(profile.get("metadata")).deepCopy();
    metadata.remove(List.of("revision", "updated_at", "trust"));
    ObjectNode identity = JsonSupport.emptyObject(); identity.set("metadata", metadata); identity.set("spec", profile.path("spec").deepCopy());
    return digest(identity);
  }
  /** Computes both profile and specification identities. */
  public static Oap.Digests digests(ObjectNode profile) { return new Oap.Digests(profileDigest(profile), specDigest(profile)); }
}
