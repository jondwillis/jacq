//! The Intermediate Representation (IR) for jacq plugins.
//!
//! A valid Claude Code plugin directory is valid IR input — the IR is a superset.
//! Additional fields enable cross-platform metadata, capability declarations,
//! and target-specific overrides.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::targets::Target;

// ---------------------------------------------------------------------------
// Top-level plugin IR — the in-memory AST after parsing
// ---------------------------------------------------------------------------

/// The fully parsed plugin, combining manifest metadata with discovered content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIR {
    /// Parsed manifest (from plugin.yaml or plugin.json)
    pub manifest: PluginManifest,

    /// Skills discovered from skills/ and commands/ directories
    pub skills: Vec<SkillDef>,

    /// Agents discovered from agents/ directory
    pub agents: Vec<AgentDef>,

    /// Hooks discovered from hooks/ directory
    pub hooks: Vec<HookDef>,

    /// MCP server definitions from mcp/ directory
    pub mcp_servers: Vec<McpServerDef>,

    /// Shared instructions from instructions/ directory
    pub instructions: Vec<InstructionDef>,

    /// Per-target override files from targets/ directory
    pub target_overrides: BTreeMap<Target, Vec<TargetOverride>>,

    /// Root directory this plugin was loaded from
    #[serde(skip)]
    pub source_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Manifest — the plugin.yaml or plugin.json
// ---------------------------------------------------------------------------

/// Plugin manifest — superset of Claude Code's plugin.json.
///
/// When parsing a Claude Code plugin.json, IR-specific fields default to None.
/// When parsing plugin.yaml, all fields are available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin identifier (kebab-case)
    pub name: String,

    /// Semantic version
    pub version: String,

    /// Short description
    pub description: String,

    /// Author — either a string or structured { name, email }.
    /// NOTE: Variant order matters for serde untagged — Name(String) must come
    /// first so that a plain string matches before Structured is attempted.
    #[serde(default)]
    pub author: Author,

    /// License identifier (e.g., "MIT")
    #[serde(default)]
    pub license: Option<String>,

    /// Discovery keywords
    #[serde(default)]
    pub keywords: Vec<String>,

    // -- IR-specific fields (absent in Claude Code plugin.json) --
    /// IR schema version
    #[serde(default)]
    pub ir_version: Option<String>,

    /// Target platforms to compile for
    #[serde(default)]
    pub targets: Vec<Target>,

    /// Capability requirements
    #[serde(default)]
    pub requires: Option<Requirements>,

    /// Graceful degradation strategies per capability per target
    #[serde(default)]
    pub fallbacks: BTreeMap<String, BTreeMap<Target, FallbackStrategy>>,
}

// ---------------------------------------------------------------------------
// Author
// ---------------------------------------------------------------------------

/// Plugin author — accepts both `"name"` and `{ name, email }` forms.
///
/// Variant order matters: serde untagged tries variants in declaration order.
/// `Name(String)` must be first so plain strings don't fail on `Structured`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Author {
    Name(String),
    Structured { name: String, email: Option<String> },
}

impl Default for Author {
    fn default() -> Self {
        Author::Name(String::new())
    }
}

// ---------------------------------------------------------------------------
// Requirements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Requirements {
    /// Capabilities this plugin needs from a host (e.g., "skills", "hooks.pre-tool-use")
    #[serde(default)]
    pub capabilities: Vec<Capability>,

    /// Permissions this plugin needs (e.g., "file-read", "network")
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

// ---------------------------------------------------------------------------
// Capabilities — what the plugin needs from a host
// ---------------------------------------------------------------------------

/// A capability that a plugin requires from its host harness.
///
/// Capabilities use a dotted path notation: "hooks.pre-tool-use", "agents.subagent".
/// The top-level name identifies the category; sub-paths identify specific features.
///
/// Unknown categories produce a deserialization error rather than silently
/// falling back — a compiler must not silently rewrite input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Capability {
    pub category: CapabilityCategory,
    pub feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityCategory {
    Agents,
    Commands,
    Hooks,
    Instructions,
    McpServers,
    Skills,
}

impl TryFrom<String> for Capability {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let (cat_str, feature) = match s.split_once('.') {
            Some((cat, feat)) => {
                if feat.is_empty() {
                    return Err(format!("capability '{s}' has empty feature after '.'"));
                }
                (cat, Some(feat.to_string()))
            }
            None => (s.as_str(), None),
        };
        let category = match cat_str {
            "skills" => CapabilityCategory::Skills,
            "agents" => CapabilityCategory::Agents,
            "hooks" => CapabilityCategory::Hooks,
            "mcp-servers" => CapabilityCategory::McpServers,
            "instructions" => CapabilityCategory::Instructions,
            "commands" => CapabilityCategory::Commands,
            _ => {
                return Err(format!(
                    "unknown capability category '{cat_str}'. \
                     Valid categories: skills, agents, hooks, mcp-servers, instructions, commands"
                ))
            }
        };
        Ok(Capability { category, feature })
    }
}

impl From<Capability> for String {
    fn from(c: Capability) -> String {
        let cat = match c.category {
            CapabilityCategory::Skills => "skills",
            CapabilityCategory::Agents => "agents",
            CapabilityCategory::Hooks => "hooks",
            CapabilityCategory::McpServers => "mcp-servers",
            CapabilityCategory::Instructions => "instructions",
            CapabilityCategory::Commands => "commands",
        };
        match c.feature {
            Some(f) => format!("{cat}.{f}"),
            None => cat.to_string(),
        }
    }
}

// Ord for Capability — enables use as BTreeMap key.
impl PartialOrd for Capability {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Capability {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| self.feature.cmp(&other.feature))
    }
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    FileRead,
    FileWrite,
    Network,
    Subprocess,
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Fallback strategies
// ---------------------------------------------------------------------------

/// What to do when a target doesn't support a required capability.
///
/// This is a closed enum — typos in strategy names produce deserialization errors
/// instead of being silently accepted as custom values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FallbackStrategy {
    /// Emit as an instruction/rule instead of native feature
    InstructionBased,
    /// Emit as a prompt template / saved command
    PromptTemplate,
    /// Emit as a section in AGENTS.md
    AgentsMdSection,
    /// Warn and omit the feature entirely
    Skip,
}

// ---------------------------------------------------------------------------
// Skill definition
// ---------------------------------------------------------------------------

/// A skill/command parsed from a .md file with YAML frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    /// Filename stem (e.g., "notes" from "notes.md")
    pub name: String,

    /// Relative path from plugin root
    pub source_path: PathBuf,

    /// Parsed YAML frontmatter
    pub frontmatter: SkillFrontmatter,

    /// Markdown body (the instruction template)
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillFrontmatter {
    #[serde(default)]
    pub description: Option<String>,

    #[serde(default, rename = "argument-hint")]
    pub argument_hint: Option<StringOrVec>,

    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<StringOrVec>,

    #[serde(default)]
    pub color: Option<String>,

    #[serde(default)]
    pub examples: Option<Vec<String>>,

    /// Catch-all for fields we don't model yet
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

// ---------------------------------------------------------------------------
// Agent definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub name: String,
    pub source_path: PathBuf,
    pub frontmatter: AgentFrontmatter,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentFrontmatter {
    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<StringOrVec>,

    #[serde(default)]
    pub color: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

// ---------------------------------------------------------------------------
// Hook definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDef {
    pub name: String,
    pub source_path: PathBuf,
    pub event: HookEvent,
    pub command: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// MCP server definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDef {
    pub name: String,
    pub source_path: PathBuf,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

// ---------------------------------------------------------------------------
// Instruction definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionDef {
    pub name: String,
    pub source_path: PathBuf,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Target overrides
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetOverride {
    pub path: PathBuf,
    pub content: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Utility types
// ---------------------------------------------------------------------------

/// A field that can be either a single string or a list of strings.
/// Claude Code frontmatter uses both forms for allowed-tools and argument-hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

impl StringOrVec {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            StringOrVec::Single(s) => vec![s.as_str()],
            StringOrVec::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}
