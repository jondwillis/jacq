#!/bin/bash
# generate-examples.sh — Build all vendor plugins to examples/ via jacq
# Run from the jacq repo root.

set -euo pipefail
JACQ_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$JACQ_ROOT"

JACQ="./target/debug/jacq"
EXAMPLES_DIR="examples"

# Build jacq first
cargo build --quiet 2>&1

mkdir -p "$EXAMPLES_DIR"

passed=0
failed=0
skipped=0

# Check if a directory has any manifest jacq can parse
has_manifest() {
  local dir="$1"
  [ -f "$dir/.claude-plugin/plugin.json" ] ||
  [ -f "$dir/.cursor-plugin/plugin.json" ] ||
  [ -f "$dir/.codex-plugin/plugin.json" ] ||
  [ -f "$dir/openclaw.plugin.json" ] ||
  [ -f "$dir/plugin.json" ] ||
  [ -f "$dir/plugin.yaml" ]
}

# Process a single plugin directory.
# 1. `jacq init --from <source>` copies plugin.yaml + all source components
# 2. `jacq build` compiles to dist/
process_plugin() {
  local plugin_dir="$1"
  local source_label="$2"
  local name=$(basename "$plugin_dir")
  local out="$EXAMPLES_DIR/$name"

  if ! has_manifest "$plugin_dir"; then
    skipped=$((skipped + 1))
    return
  fi

  # Clean any previous output so `jacq init` (which requires empty dir) works
  rm -rf "$out"

  # Import: copies plugin.yaml + all source components (skills, agents, etc.)
  if ! $JACQ init "$out" --from "$plugin_dir" >/dev/null 2>&1; then
    printf "  ❌ %-35s [%s] (init failed)\n" "$name" "$source_label"
    failed=$((failed + 1))
    return
  fi

  # Build: compile the imported example to claude-code target
  if $JACQ build "$out" --target claude-code --output "$out/dist" >/dev/null 2>&1; then
    printf "  ✅ %-35s [%s]\n" "$name" "$source_label"
    passed=$((passed + 1))
  else
    printf "  ❌ %-35s [%s] (build failed)\n" "$name" "$source_label"
    failed=$((failed + 1))
  fi
}

# --- Claude Code official plugins ---
for plugin_dir in vendor/claude-plugins-official/plugins/*/; do
  [ -d "$plugin_dir" ] && process_plugin "$plugin_dir" "cc-official"
done
for plugin_dir in vendor/claude-plugins-official/external_plugins/*/; do
  [ -d "$plugin_dir" ] && process_plugin "$plugin_dir" "cc-external"
done

# --- Cursor marketplace template ---
for plugin_dir in vendor/cursor-marketplace-template/plugins/*/; do
  [ -d "$plugin_dir" ] && process_plugin "$plugin_dir" "cursor"
done

# --- Codex bundled skills (in .codex/skills/) ---
if [ -d "vendor/codex/.codex/skills" ]; then
  for plugin_dir in vendor/codex/.codex/skills/*/; do
    [ -d "$plugin_dir" ] && process_plugin "$plugin_dir" "codex"
  done
fi

echo
echo "Generated: $passed passed, $failed failed, $skipped skipped (no manifest)"
echo "Output: $EXAMPLES_DIR/"

[ "$failed" -eq 0 ] || exit 1
