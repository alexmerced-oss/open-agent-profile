import { readFile } from "node:fs/promises";

import { parseDocument as parseYamlDocument } from "yaml";

import type { JsonObject, JsonValue } from "./types.js";

export type OapFormat = "json" | "yaml" | "markdown";

export interface ParseOptions {
  filename?: string;
  format?: OapFormat;
}

export class OapParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "OapParseError";
  }
}

function asJsonValue(value: unknown): JsonValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value)) return value as JsonValue;
  if (Array.isArray(value)) return value.map(asJsonValue);
  if (typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, asJsonValue(child)]));
  throw new OapParseError(`document contains a non-JSON value of type ${typeof value}`);
}

function parseYaml(text: string): JsonObject {
  const parsed = parseYamlDocument(text, { schema: "core", uniqueKeys: true, prettyErrors: true });
  if (parsed.errors.length > 0) throw new OapParseError(parsed.errors.map((error) => error.message).join("; "));
  const value = asJsonValue(parsed.toJS({ maxAliasCount: 100 }));
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new OapParseError("document root must be an object");
  return value;
}

function inferFormat(filename?: string): OapFormat {
  if (filename?.endsWith(".md")) return "markdown";
  if (filename?.endsWith(".json")) return "json";
  return "yaml";
}

export function parseOap(text: string, options: ParseOptions = {}): JsonObject {
  const format = options.format ?? inferFormat(options.filename);
  if (format === "json") {
    try {
      const value = asJsonValue(JSON.parse(text));
      if (value === null || typeof value !== "object" || Array.isArray(value)) throw new OapParseError("document root must be an object");
      return value;
    } catch (error) {
      if (error instanceof OapParseError) throw error;
      throw new OapParseError((error as Error).message);
    }
  }
  if (format === "yaml") return parseYaml(text);
  if (!text.startsWith("---")) throw new OapParseError("Markdown encoding requires YAML frontmatter delimited by ---");
  const end = text.indexOf("\n---", 3);
  if (end < 0) throw new OapParseError("unterminated YAML frontmatter");
  const document = parseYaml(text.slice(3, end).replace(/^\n/, ""));
  const body = text.slice(end + 4).replace(/^\n/, "");
  if (!body.trim()) throw new OapParseError("Markdown encoding requires a non-empty body");
  const spec = (document.spec ??= {}) as JsonObject;
  const role = (spec.role ??= {}) as JsonObject;
  if (role.instructions !== undefined) {
    throw new OapParseError("Markdown encoding supplies spec.role.instructions in both frontmatter and body");
  }
  role.instructions = `${body.trimEnd()}\n`;
  return document;
}

export async function loadOap(path: string): Promise<JsonObject> {
  return parseOap(await readFile(path, "utf8"), { filename: path });
}
