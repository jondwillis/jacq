# Prompt Engineering Prior Art — What "Compilation" Means for Prompts

This document explores the question: **what is jacq's "compiler" actually compiling?**

Today it compiles file structures and manifests. But the bodies of skills,
instructions, and agent definitions are opaque strings — jacq copies them
verbatim. The prior art below suggests these bodies could be *compiled* too:
validated, templated, composed, and optimized.

---

## What jacq Compiles Today

```
plugin.yaml + skills/*.md + agents/*.md + hooks/*.yaml + mcp/*.yaml + instructions/*.md
    ↓ parse
PluginIR (typed AST)
    ↓ analyze
AnalysisReport (capability compatibility)
    ↓ emit
target-specific files (plugin.json, AGENTS.md, .cursorrules, etc.)
```

The compilation handles **structure** (manifest → target manifest, frontmatter →
target format, directories → target directories) but not **content** (the
markdown body of a skill is copied as-is, the instruction text is concatenated
as-is).

## What jacq Could Compile

```
plugin.yaml + skills/*.md (with {{variables}}) + shared/*.md (reusable fragments)
    ↓ parse (with template extraction)
PluginIR (typed AST with template nodes, not just strings)
    ↓ resolve templates (substitute variables, compose sections)
Resolved IR
    ↓ analyze
AnalysisReport
    ↓ emit
target-specific files (with content adapted per target)
```

This would enable:
- **Variables** in skill/instruction bodies (e.g., `{{project_name}}`, `{{language}}`)
- **Reusable fragments** shared across skills (e.g., error handling instructions)
- **Schema-driven content** (enum values interpolated from a schema, never hardcoded)
- **Priority-based composition** (high-priority instructions kept, low-priority trimmed)
- **Target-specific content** (different wording for Claude Code vs Codex)

---

## Prior Art by Category

### Type-Safe Prompt Templates

**slopcop** (@slopcop/slopcop) — Your own project
- TypeScript compile-time template variable checking via `ExtractPlaceholders<T>`
- `template("Hello {{name}}")` → `TypedTemplate<T>` where `.render()` requires `{name: string}`
- `ValidateTemplate<Template, Vars>` catches mismatches at compile time
- `toolNames(["search", "edit"] as const)` → type-safe tool references
- `compose()` assembles `PromptSection[]` with priorities and heading styles
- `describedEnum()` generates LLM-friendly enum descriptions from `Record<Variant, Description>`
- `defineLookup()` with `validate()` for referential integrity in lookup tables

**Key insight for jacq:** slopcop proves that template variables and tool
references can be statically checked. jacq's IR bodies currently have
`$ARGUMENTS` (Claude Code's variable) — but there's no validation that the
variable exists, no checking across targets (does Codex use the same
variable?), no type system for the template content.

**BAML** (BoundaryML/baml) — 7,900 stars, Rust compiler
- Custom `.baml` DSL with types, functions, prompt templates
- Rust compiler pipeline: prompt-parser → baml-core → baml-runtime → codegen
- Prompt parser has its own sub-AST: `code_block`, `variable`, `prompt_text`, `comment_block`
- Template interpolation within prompt strings is parsed and validated
- Emits typed client code in 7 languages from a single source

**Key insight for jacq:** BAML proves the "Rust compiler for typed prompts"
model works. Their prompt-parser sub-AST for template interpolation is exactly
what jacq would need to move from "body is an opaque string" to "body is a
parsed template with validated variables."

### Prompt Compilation / Optimization

**DSPy** (stanfordnlp/dspy) — 33,500 stars, Python
- "Programming, not prompting" — define typed signatures, compile to optimized prompts
- Signatures: `class QA(dspy.Signature): question: str = InputField(); answer: int = OutputField()`
- Teleprompters (compilers): BootstrapFewShot, MIPRO, COPRO, SIMBA, GRPO
- Compilation: module + training examples + metric → optimized few-shot prompts
- `dump_state()` / `load_state()` — compiled prompts are serializable artifacts

**Key insight for jacq:** DSPy compiles prompts by optimizing them against
metrics (runtime compilation). jacq compiles plugins by transforming them
for target platforms (build-time compilation). Different "compile" — but
DSPy's serializable compiled state is analogous to jacq's emitted output.
Both are "source → compile → artifact."

### Priority-Based Prompt Composition

**Priompt** (anysphere/priompt) — 2,800 stars, TypeScript
- Cursor's internal prompt composition library
- JSX component tree with priority annotations per `<Scope>`
- Binary search on priority cutoff against token budget
- `<First>` selects first child above cutoff (progressive degradation)
- `<Isolate>` renders children with independent token budget (encapsulation)
- Source mapping for debugging which content was trimmed

**Key insight for jacq:** Priompt proves priority-based prompt composition
works at Cursor's scale. jacq's `render_agents_md()` already does naive
composition (instructions + skills + agents joined with `---`). Adding
priority weights to IR nodes would enable intelligent trimming. The `<First>`
fallback pattern maps directly to jacq's `FallbackStrategy`.

### Structured Generation & Constraint Enforcement

**Outlines** (dottxt-ai/outlines) — 13,600 stars, Python
- Schema → regex → FSM → token mask during generation
- Guarantees structurally valid output by constraining the decoder

**Guidance** (microsoft/guidance) — 21,400 stars, Python
- Template language with interleaved generation directives
- Grammar nodes: Literal, Regex, Select, Repeat, Rule, Subgrammar
- Programs execute as interleaved text/generation

**LMQL** (eth-sri/lmql) — 4,200 stars, Python
- Query language for LLMs, compiles to constrained async Python
- Multi-stage compiler: parse → scope → validate → transform → codegen

**Key insight for jacq:** These tools constrain LLM *output*. jacq
constrains LLM *input* (the plugin definition). Different direction,
but the principle is the same: types → constraints → guaranteed validity.

### Schema-Driven Prompt Generation (Real-World)

**Valara's schema-prompt-sync pattern** (../valara)
- LLM prompts interpolate from Zod schema enums and constants, never hardcoded
- `${SEVERITY.HIGH}` instead of `'HIGH'`; `${FINDING_FIELDS.RULE_ID}` instead of `'rule_id'`
- Severity guidance generated from `severityValues.map(s => ...)`, not hand-maintained
- CLAUDE.md rule enforces this pattern; hooks verify it

**Key insight for jacq:** This is the real-world validation that prompt content
should derive from schemas. slopcop's `describedEnum()` and `defineLookup()`
are the type-safe implementation of this pattern. jacq could support
`{{schema.enum_name}}` variables that resolve from a schema definition.

---

## What This Means for jacq

### Current State: File Structure Compiler
jacq compiles **structure** — manifest formats, directory layouts, frontmatter
schemas — across targets. Prompt content is opaque.

### Next Step: Template Compiler
Add `{{variable}}` support to skill/instruction bodies with:
- Variable declarations in `plugin.yaml` (with defaults)
- Validation that all variables are defined
- Target-specific variable values (different `$ARGUMENTS` equivalent per target)
- Shared fragments (`includes/` directory with reusable content blocks)

### Future: Prompt Compiler
Treat prompt bodies as parsed templates (not opaque strings):
- slopcop-style compile-time variable checking (Rust equivalent)
- Priompt-style priority-based composition with context budget trimming
- BAML-style sub-AST for template interpolation with type validation
- Schema-derived content (à la Valara's schema-prompt-sync pattern)
- DSPy-style compiled/optimized prompt artifacts

### The Key Architectural Question
Should jacq embed a prompt template engine (like Tera, which is already in
`Cargo.toml`) and parse template variables from skill bodies? Or should prompt
compilation be a separate tool (like slopcop) that runs before/after jacq?

Arguments for embedding:
- Single tool, single pipeline
- Template variables can be validated against the IR (typed checking)
- Target-specific variable resolution (different values per emitter)

Arguments for separation:
- Prompt compilation is a different domain than plugin structure compilation
- slopcop is TypeScript; jacq plugin authors write TypeScript
- Keeps jacq's scope focused (compiler for structure, not for content)

The pragmatic path: jacq handles `{{variable}}` substitution (simple
templating via Tera, already a dependency) and validates variables are defined.
Deep prompt compilation (priority trimming, schema-driven generation,
optimization) stays in user-land tools like slopcop.

---

## Sources

- [DSPy](https://github.com/stanfordnlp/dspy) — 33,500 stars
- [Priompt](https://github.com/anysphere/priompt) — 2,800 stars
- [BAML](https://github.com/BoundaryML/baml) — 7,900 stars
- [Instructor](https://github.com/jxnl/instructor) — 12,700 stars
- [Outlines](https://github.com/dottxt-ai/outlines) — 13,600 stars
- [Guidance](https://github.com/microsoft/guidance) — 21,400 stars
- [LMQL](https://github.com/eth-sri/lmql) — 4,200 stars
- [slopcop](file:///Volumes/Sidecar/slopcop) — local
- [Valara rules](file:///Volumes/Sidecar/valara/.claude/rules/) — local
