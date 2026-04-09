# Dogfooding Findings

Results from testing jacq against two real Claude Code plugins.

## Plugins Tested

### notes-app-plugin (simple)
- 1 skill, no agents, no hooks, no MCP
- **Result:** Full pipeline works. Import via `jacq init --from`, validate, inspect, build all succeed.
- **Output:** Claude Code (plugin.json + commands/), OpenCode (package.json + AGENTS.md), Codex (plugin.json + skills/ + AGENTS.md)
- **Roundtrip:** Emitted Claude Code output parses back correctly

### kinelo-connect (complex, proprietary — not committed)
- 5 skills, 6 agents, 1 MCP server, 2 hooks (in original), instructions
- **Result:** Core pipeline works for skills, agents, MCP, instructions. Hooks and some MCP features cannot be fully represented.
- **Output quality:** OpenCode AGENTS.md is genuinely useful — all skills/agents documented in readable sections

## IR Gaps Discovered

These features exist in real Claude Code plugins but our IR cannot represent them yet:

### 1. SessionStart hook event (HIGH)
kinelo-connect has a `SessionStart` hook that fetches usage instructions from the server on every new session. Our `HookEvent` enum only has `PreToolUse`, `PostToolUse`, `Stop`.

**Fix:** Add `SessionStart` to `HookEvent`. Check if other harnesses have equivalents.

### 2. Prompt-type hooks (HIGH)
kinelo-connect's Stop hook uses `"type": "prompt"` — the model evaluates a condition and returns approve/block. Our `HookDef` only has a `command` field (shell command).

**Fix:** Add a `hook_type` field to `HookDef`: `Command(String)` vs `Prompt(String)`. The prompt type is Claude Code-specific but should be representable in the IR for passthrough.

### 3. HTTP MCP servers (HIGH)
kinelo-connect uses `"type": "http"` with a `url` field instead of `command` + `args`. Our `McpServerDef` only models command-based (stdio) servers.

**Fix:** Make `McpServerDef` an enum: `Stdio { command, args, env }` vs `Http { url, headers? }`. MCP spec supports both transport types.

### 4. Environment variable templates (MEDIUM)
kinelo-connect's MCP URL uses `${KINELO_MCP_URL:-https://platform.kinelo.com/mcp}` — a shell-style variable with default. Our env fields are plain strings.

**Fix:** Either support template expressions in string fields, or add a dedicated `env_vars` section in the manifest that declares required environment variables with defaults.

### 5. Plugin-relative paths (MEDIUM)
kinelo-connect's hooks reference `${CLAUDE_PLUGIN_ROOT}/scripts/resolve-env.sh`. This is a runtime variable provided by Claude Code pointing to the installed plugin directory.

**Fix:** Define a `${PLUGIN_ROOT}` IR variable that each emitter resolves to the target's equivalent. Claude Code = `${CLAUDE_PLUGIN_ROOT}`, others = relative path or absolute path.

### 6. Subdirectory skills (LOW)
kinelo-connect uses `skills/ask/SKILL.md` (skill name from directory, file always named SKILL.md). Our parser looks for `skills/*.md` (name from filename).

**Fix:** The parser should also check for `skills/*/SKILL.md` pattern. Skill name comes from the parent directory name.

## What Works Well

- **Capability analysis is immediately valuable.** Running `jacq inspect` on kinelo shows exactly where OpenCode and Codex will differ from Claude Code. This is information no other tool provides.
- **Fallback resolution works correctly.** Declaring `agents: agents-md-section` for OpenCode/Codex makes those targets compatible without errors.
- **The OpenCode AGENTS.md output is genuinely useful.** Converting 5 skills + 6 agents into a single readable document with sections is exactly the "instruction-based fallback" that makes cross-platform work.
- **The roundtrip property holds.** Emitted Claude Code output parses back correctly.
- **Import from existing plugins works.** `jacq init --from` correctly reads a Claude Code plugin and creates an IR manifest.

## Recommendations

1. Address gaps 1-3 (SessionStart, prompt hooks, HTTP MCP) before v0.2 — they prevent full representation of production plugins.
2. Gaps 4-5 (env templates, plugin root) are important but can be worked around by using command-based MCP proxies.
3. Gap 6 (subdirectory skills) is a parser convenience — low priority.
4. The `jacq init --from` command should be smarter about detecting these features and warning when the import is lossy.
