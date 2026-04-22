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
    "lsp-servers",
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

// ---------------------------------------------------------------------------
// Manifest field matrix — which fields each target uses
// ---------------------------------------------------------------------------

/// Whether a manifest field is used by a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSupport {
    /// Target uses this field in its manifest
    Yes,
    /// Target does not use this field
    No,
}

/// Canonical manifest field keys — the single source of truth.
/// Every field_matrix entry uses exactly these keys in order.
pub const MANIFEST_FIELD_KEYS: &[&str] = &[
    // Core identity (all targets)
    "name",
    "version",
    "description",
    "author",
    "license",
    "keywords",
    // URLs
    "homepage",
    "repository",
    // Component paths
    "commands",
    "agents",
    "skills",
    "hooks",
    "mcpServers",
    "outputStyles",
    "lspServers",
    // Config
    "userConfig",
    "channels",
    // Cursor-specific
    "displayName",
    "logo",
    // Codex-specific
    "apps",
    "interface",
    // OpenClaw-specific
    "id",
    "configSchema",
    "providers",
];

/// Build a field matrix from support levels in MANIFEST_FIELD_KEYS order.
fn build_field_matrix(levels: &[FieldSupport]) -> BTreeMap<String, FieldSupport> {
    assert_eq!(
        levels.len(),
        MANIFEST_FIELD_KEYS.len(),
        "field matrix must have exactly {} entries, got {}",
        MANIFEST_FIELD_KEYS.len(),
        levels.len()
    );
    MANIFEST_FIELD_KEYS
        .iter()
        .zip(levels)
        .map(|(k, v)| (k.to_string(), *v))
        .collect()
}

/// Returns which manifest fields a target uses.
///
/// Emitters consult this to include only the fields relevant to their target —
/// Cursor gets displayName, Codex gets apps/interface, OpenClaw gets id/configSchema, etc.
pub fn field_matrix(target: Target) -> BTreeMap<String, FieldSupport> {
    use FieldSupport::*;

    match target {
        //                                name ver  desc auth lic  kw   home repo cmds agts skls hook mcp  oSty lsp  uCfg chan  dNam logo apps intf id   cSch prov
        Target::ClaudeCode => build_field_matrix(&[
            Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes,
            No, No, No, No, No, No, No,
        ]),
        Target::OpenCode => build_field_matrix(&[
            Yes, Yes, Yes, No, Yes, Yes, No, No, No, Yes, No, No, Yes, No, Yes, No, No, No, No, No,
            No, No, No, No,
        ]),
        Target::Codex => build_field_matrix(&[
            Yes, Yes, Yes, Yes, Yes, Yes, No, No, No, No, Yes, No, Yes, No, No, No, No, No, No,
            Yes, Yes, No, No, No,
        ]),
        Target::Cursor => build_field_matrix(&[
            Yes, Yes, Yes, Yes, Yes, Yes, No, No, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes, Yes,
            Yes, No, No, No, No, No,
        ]),
        Target::OpenClaw => build_field_matrix(&[
            Yes, Yes, Yes, No, No, No, No, No, No, No, Yes, No, No, No, No, No, Yes, No, No, No,
            No, Yes, Yes, Yes,
        ]),
    }
}

// ---------------------------------------------------------------------------
// Capability matrix
// ---------------------------------------------------------------------------

/// Returns the built-in capability matrix for a target.
///
/// These are embedded in the binary and updated as targets evolve.
/// This is the "heddle" — the part of the loom that creates the pattern.
pub fn capability_matrix(target: Target) -> CapabilityMatrix {
    use SupportLevel::*;

    match target {
        //                          skills   commands agents  ag.sub  hooks   h.pre   h.post  h.stop  mcp     instr   lsp
        Target::ClaudeCode => build_matrix(&[
            Full, Full, Full, Full, Full, Full, Full, Full, Full, Full, Full,
        ]),
        Target::OpenCode => build_matrix(&[
            Partial, Partial, Partial, Partial, Partial, Partial, None, Partial, Full, Full, None,
        ]),
        Target::Codex => build_matrix(&[
            Full, Full, Partial, Partial, Flags, Flags, None, None, Full, Full, None,
        ]),
        Target::Cursor => build_matrix(&[
            Partial, Partial, Partial, None, None, None, None, None, Full, Full, None,
        ]),
        Target::OpenClaw => build_matrix(&[
            Partial, Partial, None, None, None, None, None, None, Partial, Full, None,
        ]),
    }
}
