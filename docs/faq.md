# FAQ

### Why not just keep the agent process running?

Because a resident process is expensive, fragile, and unreviewable. It dies with the machine, it cannot be diffed, two people cannot share it, and you cannot answer "what changed about this agent last month" by looking at it.

A file can be committed, reviewed, forked, and rolled back. Instantiation is cheap. The only thing you lose by not staying resident is in-memory context, and that is precisely what `state` is for.

### Is this just a config file with extra steps?

A config file describes settings. A profile also carries what the agent learned, the rules for how that learning gets written back, an audit trail of who approved each revision, and a security boundary preventing the agent from editing its own contract.

Take away `state`, `history`, and the `proposals` gate, and yes, it is a config file. Those three are the point.

### Why can't an agent edit its own `spec`?

Because then a single successful prompt injection becomes permanent. An agent that ingests a hostile web page and can write `spec.tools.allow` has just granted itself a shell, in a file that persists, that will be loaded again tomorrow, and that a reviewer will assume a human wrote.

The boundary costs almost nothing. An agent that wants a wider tool surface writes a `proposals` entry with a rationale, and a human reads it. That is a better workflow anyway, because the rationale is usually the interesting part.

### Why not JSON Patch for deltas?

OAP deltas use an RFC 6902 subset (`add`, `replace`, `remove`) rather than the full thing, for three reasons.

`move`, `copy`, and `test` add implementation surface without adding capability that state updates need. Paths are constrained to `/state`, which is the security boundary, and a constrained pointer grammar is easier to verify than a general one. And deltas carry fields JSON Patch has no room for: `reason`, `confidence`, the `session` block, and the `proposals` channel.

The subset is deliberately a subset, so a JSON Patch library will apply the `operations` array correctly. It just will not enforce the parts that matter.

### Why do arrays replace instead of append in `extends`?

Two reasons. With append semantics you cannot remove an inherited item, so a base's tool grant becomes permanent for every descendant. And at review time "which layer contributed this entry" becomes unanswerable, which is exactly the question a reviewer of a security-relevant list is asking.

Replace is more verbose. It is also legible, and you can read the effective list off the child document without holding three files in your head.

### Isn't `state` just going to fill up with nonsense?

If you generate it by asking a model to summarize what it learned, yes, absolutely. See [lifecycle.md](lifecycle.md#5-reconcile).

The format pushes against this from several directions: `writeback: propose` is the default so a human sees entries before they persist, every entry carries `confidence` and `source` so bad ones are visible, retention caps and TTLs age out what stops getting used, and the recommendation is to derive operations from evidence rather than inference.

None of that saves an implementation that pipes free-form model output into the file. Generate operations from concrete evidence.

### Why a `tier` field when I already specified a model?

Because a profile outlives a model id. Share a profile pinned to `claude-sonnet-5` with someone running local models and it breaks; with `tier: advanced` it degrades to whatever their harness considers advanced, and the harness reports the substitution.

Specify both. The exact id is used when it can be, and the tier is what makes the profile portable.

### What if my harness only supports half of this?

Implement Level 1, publish a conformance statement saying exactly what you skipped, and make sure you record and can display every field you dropped.

Partial support is fine and expected. Partial support that pretends to be complete is not, because someone will review a profile, run it on your harness, and get an agent with different capabilities than the one they read.

### Can profiles contain code, or a hook script?

No. `lifecycle.on_start` and `on_end` reference hooks **already registered with the harness**, by name, with string parameters. A profile never carries a command line, a script body, or an install directive.

The moment a profile can carry a command, sharing one is remote code execution, and the format stops being safe to email.

`mcp_servers[].command` is the sole exception, because MCP requires naming an executable. It is a declaration of intent, launching it is gated separately, and harnesses should not auto-launch servers from a `project` or `imported` profile without showing the user the command line.

### How do I share profiles across a team?

Commit them to `.agents/` in the repository. That gives you review, blame, and rollback for free.

Two things to decide as a team. First, whether committed profiles carry `state`: keeping it means shared learning and noisy diffs, dropping it means each person's agent learns separately. A common split is `writeback: off` on committed profiles and a personal copy in `~/.config/<harness>/agents/` that learns.

Second, review the `history` block in pull requests. It is the cheapest way to notice an agent that has quietly drifted.

### What about signing?

Reserved for a future version. Today, pin by spec digest and treat unpinned imported profiles as untrusted, which is what the `imported` trust label is for.

### Why YAML and not TOML?

Multi-line strings. `spec.role.instructions` is often several hundred lines of prose, and YAML block scalars handle that better than anything TOML offers. JSON is the canonical form for digests and tooling; Markdown frontmatter exists because a long system prompt is far nicer to edit as a document body.

### Does this work for non-coding agents?

Yes. Nothing in the format is coding-specific. A research analyst, a support triage agent, and a writing assistant all fit; see `examples/research-analyst.agent.md`.

The coding examples are prominent because that is where multi-session memory has the clearest payoff, not because the format assumes it.

### Who decides what goes in the spec?

See [CONTRIBUTING.md](../CONTRIBUTING.md). Short version: changes need a stated problem, a proposed field, and at least one implementation willing to build it. The bar for adding a field is higher than the bar for adding a doc, because every field is something every implementer has to decide about.
