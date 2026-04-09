# jacq Examples

Example plugins in IR format, demonstrating cross-platform compilation.

## notes-app

A macOS Notes.app integration plugin, imported from a real Claude Code plugin.

```bash
# Validate
jacq validate examples/notes-app

# Inspect capability matrix
jacq inspect examples/notes-app

# Build for all targets
jacq build examples/notes-app -o examples/notes-app/dist

# Build for a single target
jacq build examples/notes-app --target opencode -o examples/notes-app/dist
```

**Targets:** claude-code, opencode, codex

| Target | Output |
|---|---|
| Claude Code | `plugin.json` + `commands/notes.md` |
| OpenCode | `package.json` + `AGENTS.md` |
| Codex | `plugin.json` + `skills/notes.md` + `AGENTS.md` |
