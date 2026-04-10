# jacq Example Plugins

This directory contains 37 plugins transliterated into jacq's IR format as a test
corpus and developer reference. Each directory is a self-contained jacq IR plugin
with `plugin.yaml` + source components (`skills/`, `agents/`, `commands/`, etc.)
and a compiled `dist/` output.

## Regenerating

```bash
./scripts/generate-examples.sh
```

This re-imports every applicable plugin from `vendor/` submodules and regenerates
the content here. **Don't edit these files directly** — changes will be lost on
the next regen.

## Attribution & Licensing

The plugins here are **derivative works** from upstream repositories. Each plugin
directory preserves its upstream LICENSE file where available.

| Group | Source | Upstream License |
|-------|--------|------------------|
| 20 official | [anthropics/claude-plugins-official](https://github.com/anthropics/claude-plugins-official) (plugins/) | Apache 2.0 (per-plugin LICENSE) |
| 17 external | [anthropics/claude-plugins-official](https://github.com/anthropics/claude-plugins-official) (external_plugins/) | Varies — see individual plugin LICENSE or upstream repo |

### Official plugins (Anthropic)
agent-sdk-dev, claude-code-setup, claude-md-management, code-review,
code-simplifier, commit-commands, example-plugin, explanatory-output-style,
feature-dev, frontend-design, hookify, learning-output-style, math-olympiad,
mcp-server-dev, playground, plugin-dev, pr-review-toolkit, ralph-loop,
security-guidance, skill-creator

### External plugins (third-party, distributed via Anthropic's marketplace)
asana, context7, discord, fakechat, firebase, github, gitlab, greptile,
imessage, laravel-boost, linear, playwright, serena, slack, supabase,
telegram, terraform

## Not included

**Cursor marketplace plugins** are in `vendor/cursor-marketplace-template/` but
are **not** redistributed here because the upstream repo has no explicit license
grant. They still participate in jacq's roundtrip tests (via vendor/) and can be
generated locally via `./scripts/generate-examples.sh --include-unlicensed`.

## jacq's own code license

The jacq compiler code (everything outside `examples/` and `vendor/`) is licensed
under MIT. See the top-level `LICENSE` file.

The per-plugin content in this directory retains its original upstream license.
If you're a plugin author and want your content removed, please open an issue.
