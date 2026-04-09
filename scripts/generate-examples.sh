#!/bin/bash
# generate-examples.sh — Build all vendor plugins to examples/ via jacq
# Run from the jacq repo root.

set -euo pipefail
JACQ_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$JACQ_ROOT"

JACQ="./target/debug/jacq"
VENDOR_DIRS=("vendor/claude-plugins-official/plugins" "vendor/claude-plugins-official/external_plugins")
EXAMPLES_DIR="examples"

# Build jacq first
cargo build --quiet 2>&1

mkdir -p "$EXAMPLES_DIR"

passed=0
failed=0
skipped=0

for vendor_dir in "${VENDOR_DIRS[@]}"; do
  [ -d "$vendor_dir" ] || continue
  for plugin_dir in "$vendor_dir"/*/; do
    [ -d "$plugin_dir" ] || continue
    name=$(basename "$plugin_dir")
    out="$EXAMPLES_DIR/$name"

    # Must have a manifest
    if [ ! -f "$plugin_dir/.claude-plugin/plugin.json" ] && [ ! -f "$plugin_dir/plugin.json" ]; then
      skipped=$((skipped + 1))
      continue
    fi

    # Parse and build to claude-code target
    if $JACQ build "$plugin_dir" --target claude-code --output "$out/dist" 2>/dev/null; then
      # Also import as IR for the plugin.yaml
      if [ ! -f "$out/plugin.yaml" ]; then
        $JACQ init "$out.tmp" --from "$plugin_dir" 2>/dev/null && \
          mv "$out.tmp/plugin.yaml" "$out/plugin.yaml" && \
          rm -rf "$out.tmp" 2>/dev/null || true
      fi
      printf "  ✅ %s\n" "$name"
      passed=$((passed + 1))
    else
      printf "  ❌ %s\n" "$name"
      failed=$((failed + 1))
    fi
  done
done

echo
echo "Generated: $passed passed, $failed failed, $skipped skipped (no manifest)"
echo "Output: $EXAMPLES_DIR/"

[ "$failed" -eq 0 ] || exit 1
