# jacq

**Agnostic plugin compiler for AI coding agents.**

Named for the Jacquard loom (1804) — one source program, many target outputs.

jacq takes a single plugin definition and compiles it to valid plugin output for multiple AI coding agent harnesses: Claude Code, OpenCode, Codex, Cursor, and OpenClaw. The IR (Intermediate Representation) is a superset of Claude Code's `plugin.json` format, so existing Claude Code plugins are valid IR input with zero migration.

```text
plugin dir  →  PARSE  →  IR  →  ANALYZE  →  RENDER  →  EMIT
                                    ↓
              claude-code/  codex/  opencode/  cursor/  openclaw/
```

## Why?

The AI coding agent ecosystem is fragmenting into incompatible plugin systems. Each target has its own manifest format, component layout, supported features, and frontmatter conventions. Authors who want to support multiple tools are forced to maintain parallel copies of the same plugin. jacq fixes that.

## Quick start

```bash
# Install (from source)
cargo install --path crates/jacq-cli

# Create a new plugin
jacq init my-plugin

# Import an existing Claude Code plugin as IR
jacq init my-plugin --from ~/some-existing-cc-plugin

# Validate without emitting
jacq validate my-plugin

# Build for all declared targets
jacq build my-plugin --output dist

# Build for a specific target
jacq build my-plugin --target codex --output dist

# Pack as distributable tar.gz archives (one per target).
# Claude Code targets also get a marketplace.json snippet alongside.
jacq pack my-plugin --target claude-code --output dist
```

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
- **Minimal deps** — 9 direct runtime dependencies (clap, serde, serde_json, serde_yaml, miette, thiserror, walkdir, tera, tar+flate2 for `jacq pack`)

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
