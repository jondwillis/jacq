//! Target platform definitions and capability matrices.
//!
//! Each target declares what it supports. The analyzer compares plugin requirements
//! against these matrices at build time — if a capability is missing and no fallback
//! is declared, it's a compile error.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Target enum
// ---------------------------------------------------------------------------

/// A target harness that jacq can compile plugins for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    ClaudeCode,
    #[serde(rename = "opencode")]
    OpenCode,
    Codex,
    Cursor,
    #[serde(rename = "openclaw")]
    OpenClaw,
}

/// All targets with their canonical string names.
/// This is the single source of truth for the Target ↔ string mapping.
const TARGET_NAMES: &[(Target, &str)] = &[
    (Target::ClaudeCode, "claude-code"),
    (Target::OpenCode, "opencode"),
    (Target::Codex, "codex"),
    (Target::Cursor, "cursor"),
    (Target::OpenClaw, "openclaw"),
];

impl Target {
    pub fn all() -> &'static [Target] {
        // Derived from TARGET_NAMES at compile time isn't possible with const,
        // but we keep this in sync via the `target_names_covers_all_variants` test.
        &[
            Target::ClaudeCode,
            Target::OpenCode,
            Target::Codex,
            Target::Cursor,
            Target::OpenClaw,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        TARGET_NAMES
            .iter()
            .find(|(t, _)| t == self)
            .expect("TARGET_NAMES must cover all variants")
            .1
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// FromStr delegates to as_str() — no duplicate mapping.
impl std::str::FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        TARGET_NAMES
            .iter()
            .find(|(_, name)| *name == s)
            .map(|(t, _)| *t)
            .ok_or_else(|| {
                let valid: Vec<&str> = TARGET_NAMES.iter().map(|(_, n)| *n).collect();
                format!("unknown target '{s}'. Valid targets: {}", valid.join(", "))
            })
    }
}

// ---------------------------------------------------------------------------
// Capability support level
// ---------------------------------------------------------------------------

/// How well a target supports a given capability.
/// Ordered from most supported to least: Full > Partial > Flags > None.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportLevel {
    /// Fully supported, native implementation
    Full,
    /// Supported with limitations or different semantics
    Partial,
    /// Available only through flags/config, not a first-class feature
    Flags,
    /// Not supported at all
    None,
}

impl SupportLevel {
    pub fn is_supported(&self) -> bool {
        !matches!(self, SupportLevel::None)
    }

    fn ordinal(&self) -> u8 {
        match self {
            SupportLevel::Full => 3,
            SupportLevel::Partial => 2,
            SupportLevel::Flags => 1,
            SupportLevel::None => 0,
        }
    }
}

impl PartialOrd for SupportLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SupportLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordinal().cmp(&other.ordinal())
    }
}

// ---------------------------------------------------------------------------
// Capability matrix
// ---------------------------------------------------------------------------

/// What a target supports, keyed by capability string (e.g., "skills", "hooks.pre-tool-use").
pub type CapabilityMatrix = BTreeMap<String, SupportLevel>;

/// Canonical capability keys — the single source of truth.
/// Every matrix must use exactly these keys.
pub const CAPABILITY_KEYS: &[&str] = &[
    "skills",
    "commands",
    "agents",
    "agents.subagent",
    "hooks",
    "hooks.pre-tool-use",
    "hooks.post-tool-use",
    "hooks.stop",
    "mcp-servers",
    "instructions",
];

/// Build a capability matrix from support levels in CAPABILITY_KEYS order.
/// Panics if `levels` length doesn't match CAPABILITY_KEYS.
fn build_matrix(levels: &[SupportLevel]) -> CapabilityMatrix {
    assert_eq!(
        levels.len(),
        CAPABILITY_KEYS.len(),
        "capability matrix must have exactly {} entries, got {}",
        CAPABILITY_KEYS.len(),
        levels.len()
    );
    CAPABILITY_KEYS
        .iter()
        .zip(levels)
        .map(|(k, v)| (k.to_string(), *v))
        .collect()
}

/// Returns the built-in capability matrix for a target.
///
/// These are embedded in the binary and updated as targets evolve.
/// This is the "heddle" — the part of the loom that creates the pattern.
pub fn capability_matrix(target: Target) -> CapabilityMatrix {
    use SupportLevel::*;

    match target {
        //                          skills   commands agents  ag.sub  hooks   h.pre   h.post  h.stop  mcp     instr
        Target::ClaudeCode => build_matrix(&[Full,    Full,    Full,   Full,   Full,   Full,   Full,   Full,   Full,   Full]),
        Target::OpenCode   => build_matrix(&[Partial, Partial, Partial,Partial,Partial,Partial,None,   Partial,Full,   Full]),
        Target::Codex      => build_matrix(&[Full,    Full,    Partial,Partial,Flags,  Flags,  None,   None,   Full,   Full]),
        Target::Cursor     => build_matrix(&[Partial, Partial, Partial,None,   None,   None,   None,   None,   Full,   Full]),
        Target::OpenClaw   => build_matrix(&[Partial, Partial, None,   None,   None,   None,   None,   None,   Partial,Full]),
    }
}
