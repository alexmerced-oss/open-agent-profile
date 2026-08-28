import { resolve } from "node:path";

import { loadOap } from "./parse.js";
import { validateOap } from "./validate.js";

async function main(): Promise<number> {
  const arguments_ = process.argv.slice(2);
  const strictPosition = arguments_.indexOf("--strict");
  const digestPosition = arguments_.indexOf("--digest");
  const strict = strictPosition >= 0;
  const showDigests = digestPosition >= 0;
  for (const position of [strictPosition, digestPosition].filter((position) => position >= 0).sort((a, b) => b - a)) arguments_.splice(position, 1);
  if (arguments_.length === 0) {
    console.error("usage: oap-validate [--strict] [--digest] <profile-or-delta> [...]");
    return 2;
  }
  let failed = false;
  for (const argument of arguments_) {
    try {
      const path = resolve(argument);
      const report = validateOap(await loadOap(path), { filename: path });
      for (const error of report.errors) console.error(`error: ${error.pointer || "<root>"}: ${error.message}`);
      for (const warning of report.warnings) console.error(`warn: ${warning.pointer || "<root>"}: ${warning.message}`);
      if (!report.ok || (strict && report.warnings.length > 0)) failed = true;
      else console.log(`${argument}: valid ${report.kind}`);
      if (showDigests && report.digests) console.log(`  profile ${report.digests.profile}\n  spec    ${report.digests.spec}`);
    } catch (error) {
      console.error(`error: ${argument}: ${(error as Error).message}`);
      failed = true;
    }
  }
  return failed ? 1 : 0;
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
