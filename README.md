# jacq

**Agnostic plugin compiler for AI coding agents.**

Named for the Jacquard loom (1804) — one source program, many target outputs.

jacq takes a single plugin definition and compiles it to valid plugin output for multiple AI coding agent harnesses: Claude Code, OpenCode, Codex, Cursor, and OpenClaw. The IR (Intermediate Representation) is a superset of Claude Code's `plugin.json` format, so existing Claude Code plugins are valid IR input with zero migration.

```text
plugin dir  →  PARSE  →  IR  →  ANALYZE  →  RENDER  →  EMIT
                                    ↓
              in-place wrappers at repo root  (default)
              or  dist/<target>/...           (with --output)
```

## Why?

The AI coding agent ecosystem is fragmenting into incompatible plugin systems. Each target has its own manifest format, component layout, supported features, and frontmatter conventions. Authors who want to support multiple tools are forced to maintain parallel copies of the same plugin. jacq fixes that.

## Quick start

```bash
# Install (from source)
cargo install --path crates/jacq-cli

# Create a new plugin (defaults to claude-code target)
jacq init my-plugin

# Scaffold a polyglot plugin from day one
jacq init my-plugin --targets claude-code,codex,opencode

# Import an existing plugin and seed targets from its layout
jacq init my-import --from ~/some-existing-cc-plugin --targets claude-code,codex

# Validate without emitting
jacq validate my-plugin

# Build IN-PLACE — adds target wrappers at the repo root.
# Components (commands/, agents/, hooks/) stay at canonical locations,
# never duplicated per target. The repo becomes a polyglot plugin.
jacq build my-plugin

# Build to an isolated tree (legacy mode) — useful for CI staging or
# any case where artifacts must be separable from the source repo.
jacq build my-plugin --output dist
```

## Build modes

`jacq build` has two output policies:

- **In-place (default).** Writes only target-specific *wrappers* at conventional locations relative to the source repo root: `.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`, `.cursor-plugin/plugin.json`, `package.json` (OpenCode), `openclaw.plugin.json`, plus MCP/LSP config and `AGENTS.md`. Components are not re-emitted — the source repo is trusted to have them at canonical paths. Result: one repo, polyglot, components live once.
- **Isolated (`--output <dir>`).** Writes a full per-target tree under `<dir>/<target>/...`, including a copy of every component. Useful for CI staging, scripted distribution, or any flow that needs artifacts separable from source.

## Marketplace metadata

When a plugin's IR includes a `marketplace:` section (in `plugin.yaml`) or a hand-authored `.claude-plugin/marketplace.json` exists at the source root, jacq emits `.claude-plugin/marketplace.json` for the Claude Code target. plugin.yaml's `marketplace:` wins when both sources are present (with a warning). This lets a single repo be both a marketplace catalog and a plugin source — Claude Code discovers it from the repo root with no `dist/` indirection.

## Workspace

This is a Cargo workspace with two published crates:

| Crate | Description |
|-------|-------------|
| [`crates/jacq-core`](crates/jacq-core) | The compiler library — IR, parser, analyzer, emitters. Depend on this from other Rust tools (LSP server, WASM build, etc.). |
| [`crates/jacq-cli`](crates/jacq-cli) | The `jacq` command-line binary. Built on top of `jacq-core`. |

## Features

- **Typed IR** — Every field is explicitly modeled. Unknown fields are rejected at parse time (`deny_unknown_fields`), not silently dropped.
- **Capability matrix** — Each target declares what it supports. The analyzer refuses to build if a plugin uses features the target can't provide and no fallback is declared.
- **Fallback strategies** — `instruction-based`, `prompt-template`, `agents-md-section`, or `skip`. A plugin using hooks can compile to a target without hook support by degrading gracefully.
- **Template compilation** — `{{variable}}` substitution with target-specific values, `{% include %}` shared fragments from a `shared/` directory, Tera rendering.
- **Multi-format parsing** — Auto-detects `.claude-plugin/plugin.json`, `.cursor-plugin/plugin.json`, `.codex-plugin/plugin.json`, `openclaw.plugin.json`, `plugin.yaml`, and bare `plugin.json`.
- **Lenient input** — Handles real-world quirks: unquoted YAML colons, string `"true"`/`"false"` booleans.
- **44-plugin roundtrip suite** — Tests against real upstream plugins from Anthropic's official marketplace and the Cursor marketplace template. parse → IR → emit → compare.
- **Polyglot in-place build** — One source repo, multiple harness wrappers at conventional locations, no component duplication. `jacq build` adds the missing wrappers; `--output` toggles to legacy isolated-tree emit.
- **Marketplace round-trip** — Parses both `plugin.yaml` `marketplace:` sections and `.claude-plugin/marketplace.json`, emits Claude Code marketplace listings byte-equivalent to the source.

## Spec coverage

| Target | Manifest fields | Components | Vendor corpus |
|--------|-----------------|------------|---------------|
| Claude Code | Full (all 17 documented fields) | skills, agents, hooks, MCP, instructions, output styles, LSP | 37 plugins roundtrip ✅ |
| Cursor | Full + `displayName`, `logo` | skills, agents, commands, MCP, rules | 7 plugins roundtrip ✅ |
| Codex | `apps`, `interface` | skills, MCP, apps | via vendor/codex |
| OpenClaw | `id`, `configSchema`, `providers` | 98 native plugins | via vendor/openclaw |
| OpenCode | npm package.json | agents, MCP, LSP | via vendor/opencode |

## Supply chain hygiene

Per [Rust Supply Chain Nightmare](https://kerkour.com/rust-supply-chain-nightmare):

- **Pinned deps** — `[workspace.dependencies]` locks specific versions; `cargo update` is an explicit action
- **MSRV locked** — `rust-version = "1.94"` in `workspace.package` and `rust-toolchain.toml`
- **License allowlist** — `deny.toml` enforces approved licenses only (MIT, Apache-2.0, BSD, ISC, MPL-2.0, a few others)
- **Registry allowlist** — `deny.toml` blocks git URL and alternate registry dependencies
- **cargo-audit + cargo-deny in CI** — weekly scheduled runs catch new advisories against unchanged deps
- **Minimal deps** — 7 direct runtime dependencies (clap, serde, serde_json, serde_yaml, miette, thiserror, walkdir, tera)

Run the full audit locally:

```bash
cargo install cargo-deny cargo-audit
cargo deny check
cargo audit
```

## Contributing

See [docs/learning-guide.md](docs/learning-guide.md) for a deep tour of jacq's internals, including the IR design, capability matrix pattern, template compilation, and multi-target conformance testing.

## License

jacq compiler code is MIT licensed. See [LICENSE](LICENSE).

Content in [`examples/`](examples/) is derived from upstream plugin sources and retains each upstream's original license. See [`examples/README.md`](examples/README.md) for attribution.

Content in [`vendor/`](vendor) is git submodules of upstream repositories, each under its own license.
