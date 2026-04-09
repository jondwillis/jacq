---
name: spec-check
description: "Verify jacq's IR types against the official Claude Code plugin specification. Use this skill when updating jacq's IR, after adding new fields or component types, when a new Claude Code version ships, or when roundtrip tests fail against vendor plugins. Combines programmatic checks (build, test suite, plugin roundtrip) with AI-assisted doc comparison and example generation."
argument-hint: "[--no-update] [--skip-examples]"
---

# Spec Conformance Check

You are verifying that jacq's Intermediate Representation (IR) fully and correctly models the Claude Code plugin specification.

## Arguments

Parse `$ARGUMENTS` for flags:
- **no flags (default)**: update submodule, run programmatic checks, run doc gap analysis, generate examples
- `--no-update`: skip the submodule pull (use current vendor state)
- `--skip-examples`: skip example generation at the end

## Phase 0: Update Vendor Plugins

Unless `--no-update` was passed, pull the latest vendor plugins:

```bash
cd vendor/claude-plugins-official && git pull origin main && cd ../..
```

If the submodule has new or changed plugins, that's the whole point — `deny_unknown_fields` will catch any spec drift in the roundtrip tests.

## Phase 1: Programmatic Checks

Run the spec-check script from the repo root:

```bash
./scripts/spec-check.sh
```

This validates:
- Build succeeds
- All tests pass (unit + integration + roundtrip)
- All official+external plugins roundtrip (parse → IR → emit → compare)
- No `#[serde(flatten)]` catch-alls in `src/ir.rs` (unknown fields must be rejected)
- `deny_unknown_fields` on frontmatter types

If any check fails, fix it before proceeding to Phase 2.

## Phase 2: Documentation Gap Analysis

Fetch the official plugin reference and compare every documented field against the IR.

### Step 1: Fetch the spec

Fetch `https://code.claude.com/docs/en/plugins-reference` and extract:
- All `plugin.json` manifest fields (required + metadata + component paths)
- All agent frontmatter fields
- All hook event types
- All hook types (command, http, prompt, agent)
- All MCP server config fields
- All LSP server config fields
- Any new component types (output-styles, bin/, settings.json, etc.)

### Step 2: Compare against IR

Read `src/ir.rs` and check each documented field exists as a typed field (not in a catch-all map). Produce a gap report:

```
## Spec Coverage Report

### plugin.json manifest
| Field | Spec | IR | Status |
|-------|------|----|--------|
| name | string, required | PluginManifest.name | ✅ |
| ...  | ...  | ...  | ... |

### Agent frontmatter
| Field | Spec | IR | Status |
...

### Hook events
| Event | In HookEvent enum? | Status |
...

### New/changed since last check
- [list anything in the spec not in IR]
```

### Step 3: Check vendor plugins for undocumented fields

Scan all `.md` frontmatter across vendor plugins for fields not in the spec:

```bash
# Agent frontmatter keys
find vendor/claude-plugins-official -path "*/agents/*.md" -exec python3 -c "
import sys, re
content = open(sys.argv[1]).read()
m = re.match(r'^---\n(.*?)\n---', content, re.DOTALL)
if m:
    for line in m.group(1).split('\n'):
        if ':' in line and not line.startswith(' '):
            print(line.split(':')[0].strip())
" {} \; 2>/dev/null | sort -u

# Skill/command frontmatter keys
find vendor/claude-plugins-official -path "*/commands/*.md" -o -path "*/skills/*/SKILL.md" | while read f; do
  python3 -c "
import sys, re
content = open(sys.argv[1]).read()
m = re.match(r'^---\n(.*?)\n---', content, re.DOTALL)
if m:
    for line in m.group(1).split('\n'):
        if ':' in line and not line.startswith(' '):
            print(line.split(':')[0].strip())
" "$f" 2>/dev/null
done | sort -u
```

Any key that appears in real plugins but not in `SkillFrontmatter` or `AgentFrontmatter` is a gap — either the spec is incomplete or we are.

## Phase 3: Generate Examples

Unless `--skip-examples` was passed, regenerate all examples:

```bash
./scripts/generate-examples.sh
```

This builds every vendor plugin through jacq's pipeline, catching any emitter regressions.

## Key Files

- `src/ir.rs` — All IR types (this is what we're validating)
- `src/parser.rs` — Frontmatter parsing with `sanitize_yaml` for lenient input
- `tests/roundtrip_tests.rs` — Plugin conformance suite
- `vendor/claude-plugins-official/` — Ground truth: real Anthropic plugins
- `scripts/spec-check.sh` — Programmatic validation script
- `scripts/generate-examples.sh` — Batch build all vendor plugins
