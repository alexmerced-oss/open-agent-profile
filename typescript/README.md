# Open Agent Profile for TypeScript

TypeScript support for [Open Agent Profile 1.0](../spec/v1/SPEC.md). It provides all three document
encodings, JSON Schema and security validation, RFC 8785 digests, inheritance, authority-narrowing
helpers, normative prompt assembly, and atomic state-delta application.

```bash
npm install open-agent-profile
```

```ts
import { loadOap, renderSystemPrompt, validateOap } from "open-agent-profile";

const profile = await loadOap(".agents/reviewer.agent.yaml");
const report = validateOap(profile);
if (!report.ok) throw new Error(report.errors.map((item) => item.message).join("\n"));

const prompt = renderSystemPrompt(profile, {
  harnessPreamble: "Local harness policy applies.",
  harnessPostamble: "Never exceed the effective tool and permission set.",
});
```

The package targets Node.js 20 or newer and supports ESM and CommonJS consumers. Loading and
resolution are side-effect free; file mutation is exposed separately through `writeAtomically`.

It also installs `oap-validate` and `oap-apply`. Contract-changing proposals remain pending even
when approved state writeback is enabled.
