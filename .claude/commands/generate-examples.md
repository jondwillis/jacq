---
name: generate-examples
description: "Build all vendor plugins (claude-plugins-official) through jacq's pipeline and output to examples/. Use after updating the vendor submodule, after IR changes, or to verify that jacq can round-trip every official and external Claude Code plugin."
argument-hint: "[--no-update] [plugin-name]"
---

# Generate Examples

Transliterates Claude Code plugins from `vendor/claude-plugins-official/` through jacq's build pipeline, producing compiled output in `examples/<plugin-name>/dist/claude-code/` and an IR manifest at `examples/<plugin-name>/plugin.yaml`.

## Arguments

Parse `$ARGUMENTS` for:
- **no args (default)**: update submodule, then build all plugins
- `--no-update`: skip the submodule pull (use current vendor state)
- `<plugin-name>`: build only the named plugin (e.g., `hookify`, `commit-commands`)

## Step 1: Update Vendor Plugins

Unless `--no-update` was passed:

```bash
cd vendor/claude-plugins-official && git pull origin main && cd ../..
```

## Step 2: Build

If a specific plugin name was given:
```bash
# Single plugin
cargo build --quiet
./target/debug/jacq build "vendor/claude-plugins-official/plugins/<name>" --target claude-code --output "examples/<name>/dist"
./target/debug/jacq inspect "vendor/claude-plugins-official/plugins/<name>"
```

Otherwise, run the batch script:
```bash
./scripts/generate-examples.sh
```

## Step 3: Report

After building, summarize:
- How many passed/failed
- Any new plugins since last run (compare `examples/` dirs vs vendor dirs)
- If any failed, show the parse error and suggest the fix (usually a missing field in `src/ir.rs`)

## What to Check on Failure

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
