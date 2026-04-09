#!/bin/bash
# spec-check.sh — Programmatic spec conformance checks for jacq
# Run from the jacq repo root.

set -euo pipefail
JACQ_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$JACQ_ROOT"

echo "=== jacq spec conformance check ==="
echo

# 1. Build
echo "▶ Building jacq..."
cargo build --quiet 2>&1 || { echo "FAIL: cargo build failed"; exit 1; }
echo "  ✅ Build OK"
echo

# 2. Run all tests
echo "▶ Running test suite..."
TEST_OUTPUT=$(cargo test 2>&1)
TOTAL=$(echo "$TEST_OUTPUT" | grep "test result:" | awk '{sum += $4} END {print sum}')
FAILED=$(echo "$TEST_OUTPUT" | grep "test result:" | awk '{sum += $6} END {print sum}')
echo "  Tests: $TOTAL passed, $FAILED failed"
if [ "$FAILED" -gt 0 ]; then
    echo "  ❌ Test failures detected"
    echo "$TEST_OUTPUT" | grep "FAILED"
    exit 1
fi
echo "  ✅ All tests pass"
echo

# 3. Roundtrip results
echo "▶ Roundtrip plugin conformance..."
ROUNDTRIP=$(cargo test roundtrip_all -- --nocapture 2>&1)
OFFICIAL_PASS=$(echo "$ROUNDTRIP" | grep "Official plugins:" | grep -o '[0-9]* passed' | head -1 || echo "?")
EXTERNAL_PASS=$(echo "$ROUNDTRIP" | grep "External plugins:" | grep -o '[0-9]* passed' | head -1 || echo "?")
PARSE_FAIL=$(echo "$ROUNDTRIP" | grep -c "known limitation" || true)
ROUNDTRIP_FAIL=$(echo "$ROUNDTRIP" | { grep -c "^  ❌" || true; })
echo "  Official: $OFFICIAL_PASS"
echo "  External: $EXTERNAL_PASS"
[ "$PARSE_FAIL" -gt 0 ] && { echo "  ⚠️  $PARSE_FAIL parse failures:"; echo "$ROUNDTRIP" | grep "⚠️" | sed 's/^/    /'; }
[ "$ROUNDTRIP_FAIL" -gt 0 ] && { echo "  ❌ $ROUNDTRIP_FAIL roundtrip failures:"; echo "$ROUNDTRIP" | grep "❌" | sed 's/^/    /'; }
# Check if roundtrip tests actually passed
if echo "$ROUNDTRIP" | grep -q "test result: ok"; then
    echo "  ✅ Roundtrip OK"
else
    echo "  ❌ Roundtrip tests failed"
    exit 1
fi
echo

# 4. Check for #[serde(flatten)] — should not exist on frontmatter/def types
echo "▶ Checking for serde(flatten) catch-alls..."
FLATTEN_COUNT=$(grep -c 'serde(flatten)' src/ir.rs 2>/dev/null || true)
if [ "$FLATTEN_COUNT" -gt 0 ]; then
    echo "  ❌ Found $FLATTEN_COUNT serde(flatten) in ir.rs — unknown fields silently accepted"
    grep -n 'serde(flatten)' src/ir.rs
    exit 1
fi
echo "  ✅ No flatten catch-alls (unknown fields rejected)"
echo

# 5. Check deny_unknown_fields on frontmatter types
echo "▶ Checking deny_unknown_fields on frontmatter types..."
for TYPE in SkillFrontmatter AgentFrontmatter; do
    if ! grep -B1 "pub struct $TYPE" src/ir.rs | grep -q 'deny_unknown_fields'; then
        echo "  ❌ $TYPE missing deny_unknown_fields"
        exit 1
    fi
done
echo "  ✅ Frontmatter types reject unknown fields"
echo

# 6. Inventory — what the IR models
echo "▶ IR coverage inventory:"
echo "  Manifest fields:  $(grep 'pub ' src/ir.rs | grep -c 'PluginManifest' || true) (in struct)"
echo "  Skill FM fields:  $(grep -A50 'struct SkillFrontmatter' src/ir.rs | grep -c 'pub ' || true)"
echo "  Agent FM fields:  $(grep -A50 'struct AgentFrontmatter' src/ir.rs | grep -c 'pub ' || true)"
HOOK_EVENTS=$(grep -c '^\s\+[A-Z][a-zA-Z]*,' src/ir.rs || true)
echo "  Hook events:      $HOOK_EVENTS"
echo

echo "=== All checks passed ==="
