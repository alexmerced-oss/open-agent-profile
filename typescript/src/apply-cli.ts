import { extname, resolve } from "node:path";

import { applyDelta, serializeOap, writeAtomically } from "./delta.js";
import { loadOap } from "./parse.js";
import type { AgentProfile, AgentStateDelta } from "./types.js";
import { validateOap } from "./validate.js";

function formatFor(path: string): "yaml" | "json" | "markdown" {
  if (path.endsWith(".md")) return "markdown";
  return extname(path).toLowerCase() === ".json" ? "json" : "yaml";
}

async function main(): Promise<number> {
  const arguments_ = process.argv.slice(2);
  const approved = arguments_.includes("--approve");
  const dryRun = arguments_.includes("--dry-run");
  const actorPosition = arguments_.indexOf("--actor");
  const actor = actorPosition >= 0 ? arguments_[actorPosition + 1] : undefined;
  const paths = arguments_.filter((value, index) => !value.startsWith("--") && index !== actorPosition + 1);
  if (paths.length !== 2) {
    console.error("usage: oap-apply <profile> <delta> [--approve] [--dry-run] [--actor <name>]");
    return 2;
  }
  const profilePath = resolve(paths[0]!);
  const deltaPath = resolve(paths[1]!);
  const profile = await loadOap(profilePath);
  const delta = await loadOap(deltaPath);
  for (const [path, document] of [[profilePath, profile], [deltaPath, delta]] as const) {
    const report = validateOap(document, { filename: path });
    if (!report.ok) {
      for (const error of report.errors) console.error(`${path}: ${error.pointer || "<root>"}: ${error.message}`);
      return 1;
    }
  }
  const result = applyDelta(profile as AgentProfile, delta as AgentStateDelta, {
    approved,
    ...(actor ? { actor } : {}),
  });
  const text = serializeOap(result.profile, formatFor(profilePath));
  for (const warning of result.warnings) console.error(`warn: ${warning}`);
  if (result.pendingProposals.length > 0) console.error(`${result.pendingProposals.length} proposal(s) require human review and were not applied`);
  if (dryRun) process.stdout.write(text);
  else await writeAtomically(profilePath, text);
  console.error(`${dryRun ? "would write" : "wrote"} revision ${result.profile.metadata.revision}`);
  return 0;
}

main().then(
  (code) => {
    process.exitCode = code;
  },
  (error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  },
);
