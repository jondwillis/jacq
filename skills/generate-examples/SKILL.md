---
name: generate-examples
description: "Build all vendor plugins (claude-plugins-official) through jacq's pipeline and output to examples/. Use after updating the vendor submodule, after IR changes, or to verify that jacq can round-trip every official and external Claude Code plugin."
---

# Generate Examples

Transliterates every Claude Code plugin in `vendor/claude-plugins-official/` through jacq's build pipeline, producing compiled output in `examples/<plugin-name>/dist/claude-code/` and an IR manifest at `examples/<plugin-name>/plugin.yaml`.

## Usage

```bash
./scripts/generate-examples.sh
```

This will:
1. Build jacq (if needed)
2. Find all plugins with a `plugin.json` manifest in both `plugins/` and `external_plugins/`
3. Run `jacq build --target claude-code` on each
4. Run `jacq init --from` to generate `plugin.yaml` (IR representation)
5. Report pass/fail counts

## When to Run

- After `cd vendor/claude-plugins-official && git pull` to pick up new plugins
- After changing `src/ir.rs`, `src/parser.rs`, or `src/emitter.rs` — verifies nothing broke
- To refresh examples after adding new frontmatter fields or component types
- As a complement to `/spec-check` — spec-check validates conformance, this generates artifacts

## What to Check

If a plugin fails:
1. Run `jacq inspect vendor/claude-plugins-official/plugins/<name>` to see the parse error
2. The error usually points to an unknown frontmatter field or malformed YAML
3. Add the missing field to `src/ir.rs`, run `cargo test`, then regenerate

## Output Structure

```
examples/
├── hookify/
│   ├── plugin.yaml          # IR manifest (from jacq init --from)
│   └── dist/
│       └── claude-code/     # Compiled output (from jacq build)
│           ├── plugin.json
│           ├── commands/
│           └── agents/
├── commit-commands/
│   ├── plugin.yaml
│   └── dist/...
└── ...
```
