# jacq — Agnostic Plugin Compiler

> Named for the Jacquard loom (1804) — the first programmable machine. Schema = punch cards. Compiler = loom. Output = woven plugins.

## Context

Every AI coding agent (Claude Code, OpenCode, Codex, Cursor, Antigravity, OpenClaw) has its own plugin format, config files, distribution mechanism, and capability model. A developer who builds a great plugin for one tool cannot port it to another without manual rewriting. Existing cross-tool sync projects (glooit, ATK, dot-agents) only copy files — none validate, type-check, or generate platform-optimized artifacts.

This project is the open-source "Fastlane for AI dev tools" from Jon's Tier 1 idea list: a compiler that takes a plugin definition and emits valid, optimized plugins for multiple host systems. Separate from Kinelo, but dogfooded against existing plugins in /Volumes/Sidecar (notes-app-plugin, kinelo-connect patterns, etc.).

### The Native-Hybrid-Web Analogy
- **Native** = write separate plugins per tool (current painful state)
- **Hybrid/Compiled** = one source, compiled per-platform with awareness of divergence (what we're building — the Kotlin Multiplatform approach)
- **Web/Runtime** = MCP as universal lowest-common-denominator runtime (useful but lossy — can't express hooks, agents, isolation, approval models)

---

## Key Principles (Derived from Prior Art Research)

### 1. "Valid Claude Code plugin IS valid IR" (TypeScript ⊇ JavaScript)
The IR is a superset that wraps Claude Code's plugin format. Zero migration cost for existing plugins. Additional IR fields enable cross-platform metadata, capability declarations, and target-specific overrides.

**Why:** TypeScript succeeded because it didn't force a rewrite. Babel succeeded because valid ES5 was valid input. The adoption curve must be zero-friction for the primary harness.

### 2. "Not write once, run everywhere — compile same source, separately, per platform" (Kotlin Multiplatform)
Accept that some capabilities are platform-specific. Make divergence explicit in the IR via capability annotations, not hidden behind a lowest-common-denominator abstraction.

**Why:** Airbnb, Dropbox, and Facebook all abandoned "write once" mobile frameworks. The 20-30% that's platform-specific is where the value lives. Our compiler should surface this clearly, not suppress it.

### 3. "Abstraction at the ecosystem level, not the implementation level" (LSP)
Define the IR around plugin *concepts* (skills, tools, hooks, agents, instructions, distribution) — not around any one tool's file format. Claude Code's format is the starting point, not the ceiling.

**Why:** LSP succeeded (M editors + N languages = M+N instead of M×N) because it described editor-level concepts (text URIs, cursor positions), not language ASTs. Our IR should describe plugin-level concepts, not Claude Code's specific YAML fields.

### 4. "Minimal core, maximal plugins" (PostCSS)
The compiler core does: parse → validate → analyze capabilities → generate. Everything else (target emitters, validators, testers, registry interaction) is a plugin to the compiler itself.

**Why:** PostCSS's tiny core enabled an ecosystem of 300+ plugins. Terraform's plugin framework enabled thousands of providers. Our target emitters should be pluggable so the community can add new targets without forking.

### 5. Compile-time safety over runtime discovery
Type-check capability requirements against target capability matrices at build time. If your plugin uses hooks and your target is Cursor (which has no hook system), fail at compile time with a clear error and suggested alternatives — don't silently drop the feature.

**Why:** The user explicitly wants "compile-time-checked, evaluable" plugins. This is the gap none of the existing sync tools fill.

### 6. Antifragile through capability matrices
Each target declares what it supports via a capability matrix. As targets evolve (e.g., Cursor adds hooks), updating the matrix is all that's needed — no compiler changes. If a new tool appears, adding a new matrix + emitter makes it a target.

---

## Plugin System Landscape (Research Summary)

### Convergences (shared ground the IR can leverage)
1. **MCP** — All tools converging on Model Context Protocol for tool integration
2. **Markdown-based instructions** — CLAUDE.md, AGENTS.md, .cursorrules, .rules
3. **Skill/command architecture** — Discrete, composable units of functionality
4. **Config-as-code** — JSON/YAML manifests, text-based rules
5. **npm-adjacent distribution** — npm (OpenCode), marketplace (Claude Code, Cursor)

### Divergences (the compiler must handle)
1. **Manifest format** — plugin.json (Claude Code, Codex) vs. package.json (OpenCode) vs. config-driven (Cursor) vs. none (Antigravity)
2. **Hook/approval model** — PreToolUse/PostToolUse/Stop (Claude Code) vs. --ask-for-approval flags (Codex) vs. session lifecycle hooks (OpenCode) vs. none (Cursor)
3. **Agent isolation** — Worktree (Claude Code) vs. Sandbox (Codex) vs. context-based (OpenCode) vs. none
4. **Skill format** — SKILL.md with YAML frontmatter (Claude Code) vs. JS exports (OpenCode) vs. text workflows (Antigravity)
5. **Distribution** — Marketplace (Claude Code, Cursor) vs. npm (OpenCode) vs. bundled (Antigravity)

### Capability Matrix (v0.1 targets)

| Capability | Claude Code | OpenCode | Codex |
|-----------|:-----------:|:--------:|:-----:|
| Skills/Commands | full | JS-only | full |
| Agents/Subagents | full | basic | basic |
| Hooks (lifecycle) | full | partial | flags |
| MCP Servers | native | via npm | native |
| Instructions/Rules | CLAUDE.md | AGENTS.md | AGENTS.md |
| Plugin Manifest | plugin.json | package.json | plugin.json |
| Tool Permissions | frontmatter | context | manifest |
| Distribution | marketplace+local | npm | plugin system |
| Isolation | worktree | none | sandbox |

---

## Architecture

### The IR (Intermediate Representation)

The IR is a directory structure + manifest. A valid Claude Code plugin is a valid IR input (zero migration). The IR extends it with:

```
my-plugin/
  plugin.yaml              # IR manifest (superset of plugin.json)
  README.md
  LICENSE
  skills/
    my-skill.md            # Claude Code compatible SKILL.md format
  agents/
    my-agent.md            # Agent definition (YAML frontmatter + instructions)
  hooks/
    on-tool-use.yaml       # Hook definitions (abstract, compiled to platform-specific)
  mcp/
    servers.yaml           # MCP server definitions
  instructions/
    rules.md               # Shared instructions (compiled to CLAUDE.md / AGENTS.md / .cursorrules)
  targets/                 # Optional per-target overrides
    opencode/
      custom-tool.ts       # OpenCode-specific JS export
    cursor/
      extra-rules.md       # Cursor-specific rules
  tests/
    smoke.yaml             # Test definitions for validation
```

### plugin.yaml (IR Manifest)

```yaml
# IR-specific fields
ir_version: "0.1"
targets: [claude-code, opencode, codex]

# Claude-Code-compatible fields (pass-through for primary target)
name: my-plugin
version: "1.0.0"
description: "A cross-platform plugin"
author: "Jon Willis"
license: "MIT"

# Capability declarations (what this plugin NEEDS from a host)
requires:
  capabilities:
    - mcp-servers          # all targets support
    - skills               # all targets support (compiled differently)
    - hooks.pre-tool-use   # Claude Code native, Codex via approval flags, OpenCode partial
    - agents.subagent      # Claude Code native, others degraded
  
  permissions:
    - file-read
    - file-write
    - network
    
# Optional: graceful degradation strategies  
fallbacks:
  hooks.pre-tool-use:
    opencode: "instruction-based"   # emit as AGENTS.md instruction instead of hook
    cursor: "skip"                   # warn and omit
  agents.subagent:
    opencode: "prompt-template"      # emit as saved prompt/command
    codex: "agents-md-section"       # emit as AGENTS.md section
```

### Compiler Pipeline

```
Source (IR or Claude Code plugin)
  │
  ├─ 1. PARSE ──────── Read manifest + files, build in-memory AST
  │
  ├─ 2. VALIDATE ───── Schema validation, file reference checks, frontmatter parsing
  │
  ├─ 3. ANALYZE ────── Compare required capabilities vs. target capability matrices
  │                     Generate warnings/errors for unsupported features
  │                     Apply fallback strategies where declared
  │
  ├─ 4. GENERATE ───── For each target:
  │     ├─ claude-code/  → plugin.json + skills/ + agents/ + hooks/ + .mcp.json
  │     ├─ opencode/     → package.json + agents/ + commands/ + AGENTS.md
  │     ├─ codex/        → plugin.json + skills/ + AGENTS.md + sandbox config
  │     └─ cursor/       → .cursorrules + .cursor/mcp.json + .cursor/commands/
  │
  ├─ 5. TEST ──────── Validate each emitted target against its schema
  │                    Run smoke tests if target runtime available
  │
  └─ 6. PACKAGE ───── Bundle each target's output for distribution
```

### CLI Interface

```bash
# Scaffold a new plugin
jacq init my-plugin                    # interactive, asks about targets
jacq init my-plugin --from ./existing-claude-plugin  # import existing

# Validate without building
jacq validate                          # lint, type-check capabilities
jacq validate --target opencode        # check specific target compatibility

# Build for targets
jacq build                             # all declared targets
jacq build --target claude-code        # single target
jacq build --target opencode --strict  # fail on any capability gap (no fallbacks)

# Test
jacq test                              # validate outputs against target schemas
jacq test --target claude-code --live  # actually install and smoke-test

# Package / distribute  
jacq pack                              # create distributable archives per target
jacq publish                           # push to registry (future)
```

---

## v0.1 Implementation Plan

### Phase 1: Project scaffold + IR types
- `cargo init jacq` with binary crate
- Add dependencies: clap, serde, serde_yaml, serde_json, comrak, miette, tera, walkdir, insta
- Define IR types as Rust structs/enums with serde derives:
  - `PluginManifest` (superset of Claude Code plugin.json)
  - `Capability` enum (Skill, Hook, Agent, McpServer, Instructions, Command)
  - `SkillDef`, `HookDef`, `AgentDef`, `McpServerDef` — each with typed fields
  - `Target` enum (ClaudeCode, OpenCode, Codex, Cursor, Antigravity, OpenClaw)
  - `CapabilityMatrix` — what each target supports
  - `FallbackStrategy` — what to do when a target doesn't support a capability

### Phase 2: Parser
- Read plugin.yaml (IR format) OR plugin.json (Claude Code native) — auto-detect
- Parse YAML frontmatter from .md skill/agent files using comrak + serde_yaml
- Validate file references (skills/, hooks/, agents/, mcp/ dirs)
- Build in-memory IR AST from parsed files
- Rich error reporting via miette (file paths, line numbers, suggestions)

### Phase 3: Capability matrix + analyzer
- Define capability matrices as embedded Rust data (or TOML/YAML files)
- Analyzer compares plugin's required capabilities vs. target matrices
- Generate human-readable compatibility report per target
- Apply declared fallback strategies
- Emit miette-styled warnings/errors for unsupported capabilities

### Phase 4: Target emitters
- **Claude Code emitter** — identity/passthrough: validate that IR produces valid Claude Code structure
- **OpenCode emitter** — generate package.json, convert skills to TS/JS exports, emit AGENTS.md
- **Codex emitter** — generate Codex-flavored plugin.json, AGENTS.md, skill dirs, sandbox config
- Emitter trait: `trait Emitter { fn emit(&self, ir: &PluginIR, output_dir: &Path) -> Result<()>; }`
- Each emitter uses Tera templates for file generation

### Phase 5: CLI commands
- `jacq init <name>` — scaffold new plugin (interactive via dialoguer crate)
- `jacq validate [--target <target>]` — parse + analyze without generating
- `jacq build [--target <target>]` — full pipeline: parse → validate → analyze → generate
- `jacq test` — validate generated output against target schemas
- `jacq pack` — create distributable archives per target
- `jacq inspect` — show capability matrix and compatibility report

### Phase 6: Dogfooding + snapshot tests
- Import notes-app-plugin as test fixture, compile to OpenCode + Codex
- Import a kinelo-connect skill as test fixture
- Snapshot tests (insta crate) for all generated output — catches regressions
- Document what works, what needs fallbacks, what's impossible

---

## Technology Choices

- **Language:** Rust (algebraic types for IR AST, exhaustive match for emitters, compile-time correctness, single binary distribution)
- **CLI:** clap (the standard Rust CLI framework — derive macros for zero-boilerplate arg parsing)
- **Serialization:** serde + serde_yaml + serde_json + toml (one `#[derive(Serialize, Deserialize)]` = parse any format)
- **Diagnostics:** miette (rich error reporting with source spans, suggestions, fix hints — like Biome's output)
- **Templates:** Tera (Jinja2-like, runtime templates for generated files) or Askama (compile-time checked templates via proc macros)
- **Markdown parsing:** comrak (CommonMark + GFM, for skill/agent file parsing)
- **YAML frontmatter:** gray_matter or custom serde_yaml extraction
- **File system:** walkdir + std::fs
- **Testing:** cargo test (built-in) + insta (snapshot testing for generated output)
- **Distribution:** cargo-dist (cross-platform binary releases), `cargo install jacq`, and brew formula
- **Build:** cargo (with workspace layout if needed)

### Why Rust over TypeScript

The entire JS tooling ecosystem is migrating to Rust (Babel→SWC, ESLint+Prettier→Biome, webpack→Turbopack, PostCSS→Lightning CSS). jacq is a compiler-shaped tool that benefits from:
1. Exhaustive `match` — adding a new target forces handling it everywhere (compile-time guarantee)
2. Algebraic enums — `Capability::Skill(SkillDef) | Capability::Hook(HookDef) | ...` is the natural IR representation
3. `serde` derives — the type system IS the schema, no runtime validation layer needed
4. Single binary distribution — `curl -fsSL jacq.dev/install | sh`, no runtime dependency
5. WASM compilation target — jacq could itself be used as an Extism plugin or run in browsers

---

## Name: jacq

Short for **Jacquard** — the first programmable machine (1804). Joseph Marie Jacquard's loom used punch cards to control weaving patterns: one program producing different outputs from the same machine. That's exactly what this compiler does.

- npm: `jacq` (available)
- CLI: `jacq build`, `jacq validate`, `jacq init`
- GitHub: `jacq` or `jacq-dev`
- No conflicts found in developer tooling space

The loom metaphor extends naturally to documentation and features:
- **Pattern** = plugin definition (the IR schema)
- **Warp** = the host harness's plugin format (the fixed structure)
- **Weft** = the plugin author's capabilities crossing between harnesses
- **Heddle** = the capability matrix (separating what works on each target)

---

## Verification Plan

### How to test the v0.1 end-to-end:
1. `cargo build` — compiles jacq binary
2. `jacq init test-plugin` — generates valid IR scaffold
3. Add a simple skill, an MCP server declaration, and a rules file
4. `jacq validate` — passes with no errors
5. `jacq build --target claude-code` — emits valid Claude Code plugin structure
6. `jacq build --target opencode` — emits valid OpenCode plugin structure  
7. `jacq test` — validates both outputs against their respective schemas
8. Manually install Claude Code output in a Claude Code session — verify skill works
9. Manually install OpenCode output via `opencode plugin` — verify it loads
10. `cargo test` — all unit tests + snapshot tests pass
11. **Dogfood**: run against notes-app-plugin from /Volumes/Sidecar/notes-app-plugin

### Key dogfooding repos:
- `/Volumes/Sidecar/notes-app-plugin` — Claude Code plugin (MCP + skills)
- `uni-industries/kinelo-connect/claude-code/` — rich plugin with skills, agents, hooks, MCP, OAuth

---

## Scope Decision

**Coding agent harnesses only.** The six targets (Claude Code, OpenCode, Codex, Cursor, Antigravity, OpenClaw) already have enough divergence. Non-coding targets (Slack, GitHub Actions, Linear) would force the capability model into excessive abstraction. "Do one thing well."

Distribution is platform-native: each emitter produces what its target expects (marketplace, npm, plugin system). No separate registry needed — jacq plugin source is just files in a git repo.
