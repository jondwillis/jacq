# slopcop & Valara — Real-World Prompt/Plugin Patterns

Observations from two local projects that inform jacq's design.

---

## slopcop (@slopcop/slopcop)

Zero-dependency TypeScript type utilities for safe, maintainable LLM prompt
engineering. This is the type-safety layer for prompt content — the complement
to jacq's structure compilation.

### Core Primitives

**`template<const T extends string>(tmpl: T): TypedTemplate<T>`**
```typescript
const greeting = template("Hello {{name}}, you are a {{role}}");
greeting.render({ name: "Jon", role: "reviewer" }); // ✓ type-safe
greeting.render({ name: "Jon" }); // ✗ compile error: missing 'role'
greeting.render({ name: "Jon", role: "reviewer", extra: "x" }); // ✗ unused 'extra'
```
`ExtractPlaceholders<T>` extracts `"name" | "role"` from the template string
literal at compile time. `ValidateTemplate<Template, Vars>` produces error
message types when they don't match.

**`toolNames<const N>(names: N): ToolNameSet<N[number]>`**
```typescript
const TOOLS = toolNames(["report_finding", "summarize"] as const);
TOOLS.ref("report_finding"); // ToolName<"report_finding">
TOOLS.ref("typo");           // compile error
```
`ToolName<N>` is a branded type — it's a string at runtime but statically
distinguished from arbitrary strings.

**`compose(sections: PromptSection[], options?): string`**
```typescript
compose([
  section("Instructions", "Be thorough.", 100),
  section("Context", longText, 50),       // lower priority, trimmed first
  section("Examples", examples, 75),
], { headingStyle: "markdown" });
```
Sections sorted by priority (higher first). Heading styles: markdown (`## X`),
xml (`<X>...</X>`), plain (`X:`).

**`describedEnum(variants: Record<V, string>): DescribedEnum<V>`**
```typescript
const SEVERITY = describedEnum({
  HIGH: "Critical issue requiring immediate attention",
  MEDIUM: "Notable concern worth addressing",
  LOW: "Minor observation",
});
SEVERITY.composedDescription; // "- \"HIGH\": Critical issue...\n- ..."
SEVERITY.format("numbered");  // "1. \"HIGH\": Critical..."
```

**`defineLookup(validIds, table): TypedLookup`**
Referential integrity checking: every value in the table must be a valid ID.
`validate()` throws at runtime if an ID reference is invalid.

### Relevance to jacq

slopcop solves the *content* safety problem. jacq solves the *structure*
compilation problem. Together:

| Concern | slopcop | jacq |
|---------|---------|------|
| Template variables | Compile-time TS checking | Could validate `{{var}}` in skill bodies |
| Tool references | Branded `ToolName<N>` types | Could cross-check tool names against target capabilities |
| Prompt composition | `compose()` with priorities | `render_agents_md()` joins sections |
| Enum descriptions | `describedEnum()` | Could generate from IR schema definitions |
| Target-specific content | Not handled | Emitters could substitute target-specific values |

The natural integration: slopcop is used by TypeScript plugin authors to
write their prompt content safely. jacq compiles the resulting plugins
across targets. They're complementary layers.

---

## Valara (.claude/ and .agents/ patterns)

A real-world Next.js app with rich Claude Code configuration. Shows what
a mature AI-assisted project's plugin/config surface actually looks like.

### Rules (`.claude/rules/`)

Valara has 15+ rule files. They're topic-specific markdown files with
optional YAML frontmatter for path scoping:

```yaml
---
paths:
  - "workflows/**/*.ts"
---
# Schema-Prompt Synchronization
LLM prompts MUST reference schema values through code, never hardcoded...
```

Key patterns observed:

**Schema-prompt sync** — The most interesting rule. Enforces that LLM
prompts interpolate from Zod schema enums/constants, never hardcoded strings.
`${SEVERITY.HIGH}` not `'HIGH'`. This is the "typed prompt content" principle
in practice — and it's enforced by a *rule*, not by a type system.

**No fallback accumulation** — When changing schemas, delete old paths, don't
bridge them. "If something fails, let it fail — don't produce degraded output."
Directly aligns with jacq's philosophy: the compiler should fail at build time,
not silently produce broken output.

**Path-scoped rules** — Rules with `paths:` frontmatter only apply to specific
directories. jacq's IR doesn't model this yet — but it should. A rule that
only applies to `workflows/**` shouldn't be emitted for all contexts.

### Hooks (`.agents/hooks/` + `.claude/settings.json`)

Valara has 5 hooks covering all lifecycle events:

| Hook | Event | What it does |
|------|-------|---|
| `block-env-edit.ts` | PreToolUse (Edit/Write) | Blocks edits to `.env*` files |
| `format-on-edit.ts` | PostToolUse (Edit/Write) | Runs Biome formatter on edited files |
| `verify-on-stop.ts` | Stop | Runs typecheck + tests + lint on changed files |
| `check-harness-drift.ts` | Stop | Checks config hasn't drifted |
| `reset-worktree-on-session.sh` | SessionStart | Resets worktree state |

Key patterns observed:

**Hooks are the enforcement mechanism for rules.** The "schema-prompt sync"
rule tells the agent what to do. The "verify-on-stop" hook catches violations.
Rules are instructions; hooks are guardrails.

**Hooks reference `$TOOL_INPUT` and `$CLAUDE_PROJECT_DIR`.** These are
environment variables provided by Claude Code. Other targets provide different
variables. jacq's emitter for hooks should map these variables per target.

**Hooks use the project's runtime** (Bun, bash). They're not portable as-is —
`bun run .agents/hooks/verify-on-stop.ts` only works if Bun is installed.
jacq could detect the runtime requirement and warn.

### Settings (`.claude/settings.json`)

Hook registration and plugin enablement. Notable: the hook configuration
references script paths relative to the project root, with matchers for
specific tool names (`Edit|Write`, `*`).

This is the most complex artifact jacq needs to generate per target. Claude
Code uses this JSON format; other targets have different hook registration
mechanisms (or none at all).

---

## What jacq Should Learn

### From slopcop
1. **Template variables should be validated at compile time.** `{{name}}` in a
   skill body should be checked against declared variables in `plugin.yaml`.
2. **Tool references in prompts should be typed.** If a skill says "use the
   Grep tool," the compiler should verify Grep is an allowed tool for that skill.
3. **Prompt composition should support priorities.** When emitting AGENTS.md
   for context-limited targets, high-priority instructions should come first.

### From Valara
1. **Path-scoped rules are real.** The IR needs optional `paths:` on instruction
   definitions to generate scoped rules.
2. **Hooks are scripts, not data.** They need runtime detection (Bun vs Node vs
   bash) and environment variable mapping per target.
3. **Rules and hooks are complementary.** The IR should make it easy to pair
   "here's the rule" with "here's the enforcement hook."
4. **Environment variables differ per target.** `$TOOL_INPUT`, `$ARGUMENTS`,
   `$CLAUDE_PROJECT_DIR` are Claude Code–specific. Emitters need variable
   substitution.
