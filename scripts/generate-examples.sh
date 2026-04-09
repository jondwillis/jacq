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

# Process a single plugin directory
process_plugin() {
  local plugin_dir="$1"
  local source_label="$2"
  local name=$(basename "$plugin_dir")
  local out="$EXAMPLES_DIR/$name"

  if ! has_manifest "$plugin_dir"; then
    skipped=$((skipped + 1))
    return
  fi

  # Parse and build to claude-code target
  if $JACQ build "$plugin_dir" --target claude-code --output "$out/dist" 2>/dev/null; then
    # Also import as IR for the plugin.yaml
    if [ ! -f "$out/plugin.yaml" ]; then
      $JACQ init "$out.tmp" --from "$plugin_dir" 2>/dev/null && \
        mv "$out.tmp/plugin.yaml" "$out/plugin.yaml" && \
        rm -rf "$out.tmp" 2>/dev/null || true
    fi
    printf "  ✅ %-35s [%s]\n" "$name" "$source_label"
    passed=$((passed + 1))
  else
    printf "  ❌ %-35s [%s]\n" "$name" "$source_label"
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
