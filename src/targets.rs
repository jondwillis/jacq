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
    Antigravity,
    #[serde(rename = "openclaw")]
    OpenClaw,
}

impl Target {
    pub fn all() -> &'static [Target] {
        &[
            Target::ClaudeCode,
            Target::OpenCode,
            Target::Codex,
            Target::Cursor,
            Target::Antigravity,
            Target::OpenClaw,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Target::ClaudeCode => "claude-code",
            Target::OpenCode => "opencode",
            Target::Codex => "codex",
            Target::Cursor => "cursor",
            Target::Antigravity => "antigravity",
            Target::OpenClaw => "openclaw",
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Capability support level
// ---------------------------------------------------------------------------

/// How well a target supports a given capability.
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
}

// ---------------------------------------------------------------------------
// Capability matrix
// ---------------------------------------------------------------------------

/// What a target supports, keyed by capability string (e.g., "skills", "hooks.pre-tool-use").
pub type CapabilityMatrix = BTreeMap<String, SupportLevel>;

/// Returns the built-in capability matrix for a target.
///
/// These are embedded in the binary and updated as targets evolve.
/// This is the "heddle" — the part of the loom that creates the pattern.
pub fn capability_matrix(target: Target) -> CapabilityMatrix {
    match target {
        Target::ClaudeCode => claude_code_matrix(),
        Target::OpenCode => opencode_matrix(),
        Target::Codex => codex_matrix(),
        Target::Cursor => cursor_matrix(),
        Target::Antigravity => antigravity_matrix(),
        Target::OpenClaw => openclaw_matrix(),
    }
}

fn claude_code_matrix() -> CapabilityMatrix {
    BTreeMap::from([
        ("skills".into(), SupportLevel::Full),
        ("commands".into(), SupportLevel::Full),
        ("agents".into(), SupportLevel::Full),
        ("agents.subagent".into(), SupportLevel::Full),
        ("hooks".into(), SupportLevel::Full),
        ("hooks.pre-tool-use".into(), SupportLevel::Full),
        ("hooks.post-tool-use".into(), SupportLevel::Full),
        ("hooks.stop".into(), SupportLevel::Full),
        ("mcp-servers".into(), SupportLevel::Full),
        ("instructions".into(), SupportLevel::Full),
    ])
}

fn opencode_matrix() -> CapabilityMatrix {
    BTreeMap::from([
        ("skills".into(), SupportLevel::Partial), // JS exports only
        ("commands".into(), SupportLevel::Partial),
        ("agents".into(), SupportLevel::Partial),
        ("agents.subagent".into(), SupportLevel::Partial),
        ("hooks".into(), SupportLevel::Partial), // session lifecycle only
        ("hooks.pre-tool-use".into(), SupportLevel::Partial),
        ("hooks.post-tool-use".into(), SupportLevel::None),
        ("hooks.stop".into(), SupportLevel::Partial),
        ("mcp-servers".into(), SupportLevel::Full), // via npm
        ("instructions".into(), SupportLevel::Full), // AGENTS.md
    ])
}

fn codex_matrix() -> CapabilityMatrix {
    BTreeMap::from([
        ("skills".into(), SupportLevel::Full),
        ("commands".into(), SupportLevel::Full),
        ("agents".into(), SupportLevel::Partial),
        ("agents.subagent".into(), SupportLevel::Partial),
        ("hooks".into(), SupportLevel::Flags), // approval flags
        ("hooks.pre-tool-use".into(), SupportLevel::Flags),
        ("hooks.post-tool-use".into(), SupportLevel::None),
        ("hooks.stop".into(), SupportLevel::None),
        ("mcp-servers".into(), SupportLevel::Full),
        ("instructions".into(), SupportLevel::Full), // AGENTS.md
    ])
}

fn cursor_matrix() -> CapabilityMatrix {
    BTreeMap::from([
        ("skills".into(), SupportLevel::Partial), // via commands
        ("commands".into(), SupportLevel::Partial),
        ("agents".into(), SupportLevel::Partial),
        ("agents.subagent".into(), SupportLevel::None),
        ("hooks".into(), SupportLevel::None),
        ("hooks.pre-tool-use".into(), SupportLevel::None),
        ("hooks.post-tool-use".into(), SupportLevel::None),
        ("hooks.stop".into(), SupportLevel::None),
        ("mcp-servers".into(), SupportLevel::Full), // .cursor/mcp.json
        ("instructions".into(), SupportLevel::Full), // .cursorrules
    ])
}

fn antigravity_matrix() -> CapabilityMatrix {
    BTreeMap::from([
        ("skills".into(), SupportLevel::Partial), // .workflows
        ("commands".into(), SupportLevel::None),
        ("agents".into(), SupportLevel::Partial),
        ("agents.subagent".into(), SupportLevel::None),
        ("hooks".into(), SupportLevel::None),
        ("hooks.pre-tool-use".into(), SupportLevel::None),
        ("hooks.post-tool-use".into(), SupportLevel::None),
        ("hooks.stop".into(), SupportLevel::None),
        ("mcp-servers".into(), SupportLevel::Full),
        ("instructions".into(), SupportLevel::Full), // .rules
    ])
}

fn openclaw_matrix() -> CapabilityMatrix {
    // OpenClaw is the least documented — conservative defaults
    BTreeMap::from([
        ("skills".into(), SupportLevel::Partial),
        ("commands".into(), SupportLevel::Partial),
        ("agents".into(), SupportLevel::None),
        ("agents.subagent".into(), SupportLevel::None),
        ("hooks".into(), SupportLevel::None),
        ("hooks.pre-tool-use".into(), SupportLevel::None),
        ("hooks.post-tool-use".into(), SupportLevel::None),
        ("hooks.stop".into(), SupportLevel::None),
        ("mcp-servers".into(), SupportLevel::Partial),
        ("instructions".into(), SupportLevel::Full),
    ])
}
