# Competitive Landscape — Cross-Tool AI Agent Config

Researched April 2026. The space has 15+ tools, all born 2025-2026.
None are compilers — they're all file sync/copy/template tools.

---

## The Landscape

### Tier 1: Significant Traction (500+ stars)

**ruler** (intellectronica/ruler) — 2,620 stars, TypeScript, npm
- `.ruler/` dir of Markdown files → tool-specific rule configs for 16+ agents
- Rules only. No MCP, hooks, skills, commands, or agents.
- The popular simple option. Proves demand but intentionally limited scope.

**rulesync** (dyoshikawa/rulesync) — 990 stars, TypeScript, npm
- `.rulesync/*.md` with YAML frontmatter → 21+ tools
- Broadest artifact coverage: rules, skills, MCP, agents, commands
- Bidirectional import/export between tools
- No validation. Silently drops unsupported features.
- Closest feature competitor to jacq, but a file generator, not a compiler.

**iannuttall/dotagents** — 666 stars, TypeScript, bunx
- `.agents/` dir with symlinks into tool-specific locations
- TUI for selecting workspace and clients
- Rules, skills, hooks, commands via symlinks
- No transformation, validation, or capability awareness.

**rulebook-ai** — 590 stars, Python, pip
- Template-based universal rules
- Rules only. Last active Oct 2025 (stale).

### Tier 2: Active Mid-Size (50-500 stars)

**developer-kit** — 200 stars, Python
- Plugin marketplace model for Claude Code with spec-driven validation
- Skills, agents, commands, workflows

**getsentry/dotagents** — 140 stars, TypeScript, npm
- Sentry's skill distribution: install from GitHub repos + symlink
- Skills focus. Backed by Sentry.

**johnlindquist/dotagent** — 123 stars, TypeScript, npm
- Parser/converter library for agent rule formats
- Library, not a CLI. Rules only. Semi-stale (5+ months).

**ai-rulez** — 100 stars, Go
- `ai-rulez.yml` → 18 tools. Context compression. AI enforcement.
- Widest distribution (Homebrew, npm, pip, Go binary).

**block/ai-rules** — 89 stars, **Rust**
- Source dir → tool configs. Backed by Block (Square/Cash App).
- Rules, skills, MCP, commands. No hooks or agents.
- Worth watching: Rust, corporate backing, active.

### Tier 3: Small / Niche (<50 stars)

**glooit** — 22 stars. File sync across 3 tools. Low adoption.
**dot-agents** — 24 stars. Shell script, hardlinks/symlinks.
**ai-rules-sync** — 23 stars. JSON config sync for 8 tools.
**ATK** — 13 stars. MCP plugin manager (install + wire). Python.
**.agents Protocol** — Spec/standard for the `.agents/` directory convention.

### Content Libraries (not tools)

**steipete/agent-rules** — 5,664 stars. Curated rule collection.
**softaworks/agent-toolkit** — 1,426 stars. Curated skill collection.

---

## What Nobody Does (jacq's Differentiators)

1. **Compilation with capability-aware type-checking.** All tools are file copy/sync/template. None validate features against target capabilities. None fail at build time for unsupported features.

2. **Capability matrix model.** No tool tracks what each AI agent supports (Full/Partial/Flags/None) and uses that for build decisions.

3. **Typed IR with semantic analysis.** Everyone works directly with files. jacq parses into a typed AST, validates, analyzes compatibility, then generates. Compiler architecture vs file pipeline.

4. **Fallback strategies.** No tool has the concept of "this feature isn't supported on target X, so do Y instead." They all silently drop or blindly copy.

5. **Cross-platform plugin distribution.** No "npm publish" that makes a plugin available across all tools from a single source.

---

## The `.agents/` Convention

Multiple projects converge on `.agents/` as a standard directory:
- iannuttall/dotagents, getsentry/dotagents, dot-agents, .agents Protocol

jacq's IR structure is compatible. The parser should accept `.agents/` as input.

---

## Competitive Risk

The risk isn't technical competition — it's that file-sync tools become "good enough." ruler (2,620 stars) proves most developers just want rules synced. The deeper question: do enough developers need cross-platform *plugins* (not just rules) to justify a compiler?

The answer is yes — if you're building something like notes-app-plugin or kinelo-connect that has skills, MCP servers, hooks, and agents. For rules-only projects, ruler wins on simplicity. jacq wins when the plugin has real structure.

---

## Sources

All GitHub repos verified April 2026. Stars are approximate.
