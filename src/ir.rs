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

    /// Output styles from output-styles/ directory
    pub output_styles: Vec<OutputStyleDef>,

    /// LSP server definitions
    pub lsp_servers: Vec<LspServerDef>,

    /// Shared fragments from shared/ directory (for {% include %})
    pub shared: Vec<SharedFragment>,

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

    /// Semantic version (optional — Claude Code plugins often omit this)
    #[serde(default = "default_version")]
    pub version: String,

    /// Short description
    #[serde(default)]
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

    /// Plugin homepage URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Source code repository URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    // -- Component path overrides (replace default directories) --
    /// Custom command file/directory paths (replaces default commands/)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<StringOrVec>,

    /// Custom agent file/directory paths (replaces default agents/)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<StringOrVec>,

    /// Custom skill directory paths (replaces default skills/)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<StringOrVec>,

    /// Hook configuration — path(s) to JSON files or inline object
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,

    /// MCP server configuration — path(s) to JSON files or inline object
    #[serde(default, rename = "mcpServers", skip_serializing_if = "Option::is_none")]
    pub mcp_servers_config: Option<serde_json::Value>,

    /// Output style file/directory paths
    #[serde(default, rename = "outputStyles", skip_serializing_if = "Option::is_none")]
    pub output_styles: Option<StringOrVec>,

    /// LSP server configuration — path(s) to JSON files or inline object
    #[serde(default, rename = "lspServers", skip_serializing_if = "Option::is_none")]
    pub lsp_servers: Option<serde_json::Value>,

    /// User-configurable values prompted at plugin enable time
    #[serde(default, rename = "userConfig", skip_serializing_if = "Option::is_none")]
    pub user_config: Option<BTreeMap<String, UserConfigEntry>>,

    /// Channel declarations for message injection (Telegram, Slack, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<Channel>>,

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

    /// Graceful degradation strategies per capability per target.
    /// Keys are typed Capability values — typos are caught at parse time.
    #[serde(default)]
    pub fallbacks: BTreeMap<Capability, BTreeMap<Target, FallbackStrategy>>,

    /// Template variable declarations.
    /// Skills/agents/instructions can reference these via `{{var_name}}`.
    #[serde(default)]
    pub vars: BTreeMap<String, VarDef>,
}

fn default_version() -> String {
    "0.0.0".to_string()
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
    Structured {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

impl Default for Author {
    fn default() -> Self {
        Author::Name(String::new())
    }
}

// ---------------------------------------------------------------------------
// User configuration
// ---------------------------------------------------------------------------

/// A user-configurable value declaration in plugin.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfigEntry {
    /// Human-readable description shown when prompting the user
    pub description: String,
    /// If true, stored in system keychain rather than settings.json
    #[serde(default)]
    pub sensitive: bool,
}

// ---------------------------------------------------------------------------
// Channel
// ---------------------------------------------------------------------------

/// A message channel declaration that binds to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// MCP server name this channel binds to (must match a key in mcpServers)
    pub server: String,
    /// Per-channel user configuration (e.g., bot tokens)
    #[serde(default, rename = "userConfig", skip_serializing_if = "Option::is_none")]
    pub user_config: Option<BTreeMap<String, UserConfigEntry>>,
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

/// Permissions a plugin requires from its host.
/// Unknown permissions cause a deserialization error — a compiler should
/// reject unrecognized input rather than silently accepting typos.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    FileRead,
    FileWrite,
    Network,
    Subprocess,
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

    /// Markdown body — plain text or template with `{{variable}}` refs
    pub body: BodyContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SkillFrontmatter {
    /// Skill name override (defaults to filename stem)
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default, rename = "argument-hint")]
    pub argument_hint: Option<StringOrVec>,

    /// Allowed tools — some plugins use "allowed-tools", some use "tools"
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<StringOrVec>,

    /// Allowed tools (alternate key used by some plugins)
    #[serde(default)]
    pub tools: Option<StringOrVec>,

    #[serde(default)]
    pub color: Option<String>,

    #[serde(default)]
    pub examples: Option<Vec<String>>,

    /// Whether this skill can be invoked by users via /name
    #[serde(default, rename = "user-invocable")]
    pub user_invocable: Option<LenientBool>,

    /// Whether to hide from the slash command tool list
    #[serde(default, rename = "hide-from-slash-command-tool")]
    pub hide_from_slash_command_tool: Option<LenientBool>,

    /// Whether to disable automatic model invocation
    #[serde(default, rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<LenientBool>,

    /// Skill version
    #[serde(default)]
    pub version: Option<String>,

    /// Skill license
    #[serde(default)]
    pub license: Option<String>,
}

// ---------------------------------------------------------------------------
// Agent definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub name: String,
    pub source_path: PathBuf,
    pub frontmatter: AgentFrontmatter,
    pub body: BodyContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AgentFrontmatter {
    /// Agent name override (defaults to filename stem)
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub model: Option<String>,

    /// Effort level for the agent
    #[serde(default)]
    pub effort: Option<String>,

    /// Maximum number of turns the agent can take
    #[serde(default, rename = "maxTurns")]
    pub max_turns: Option<u64>,

    /// Allowed tools (replaces allowed-tools)
    #[serde(default)]
    pub tools: Option<StringOrVec>,

    /// Tools the agent is not allowed to use
    #[serde(default, rename = "disallowedTools")]
    pub disallowed_tools: Option<StringOrVec>,

    /// Skills available to this agent
    #[serde(default)]
    pub skills: Option<StringOrVec>,

    /// Memory configuration
    #[serde(default)]
    pub memory: Option<serde_yaml::Value>,

    /// Background execution configuration
    #[serde(default)]
    pub background: Option<serde_yaml::Value>,

    /// Isolation mode — only valid value is "worktree"
    #[serde(default)]
    pub isolation: Option<String>,

    #[serde(default)]
    pub color: Option<String>,
}

// ---------------------------------------------------------------------------
// Hook definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDef {
    pub name: String,
    #[serde(skip)]
    pub source_path: PathBuf,
    pub event: HookEvent,
    /// Hook type: "command", "http", "prompt", or "agent"
    #[serde(rename = "type", default = "default_hook_type")]
    pub hook_type: String,
    /// Shell command (for type: command)
    #[serde(default)]
    pub command: Option<String>,
    /// URL (for type: http)
    #[serde(default)]
    pub url: Option<String>,
    /// Prompt template (for type: prompt)
    #[serde(default)]
    pub prompt: Option<String>,
    /// Tool/event matcher pattern (e.g., "Write|Edit")
    #[serde(default)]
    pub matcher: Option<String>,
    /// Timeout in milliseconds
    #[serde(default)]
    pub timeout: Option<u64>,
}

fn default_hook_type() -> String {
    "command".to_string()
}

/// All hook event types from the Claude Code spec.
/// Unknown events cause deserialization errors — typos are caught at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PermissionRequest,
    PermissionDenied,
    PostToolUse,
    PostToolUseFailure,
    Notification,
    SubagentStart,
    SubagentStop,
    TaskCreated,
    TaskCompleted,
    Stop,
    StopFailure,
    TeammateIdle,
    InstructionsLoaded,
    ConfigChange,
    CwdChanged,
    FileChanged,
    WorktreeCreate,
    WorktreeRemove,
    PreCompact,
    PostCompact,
    Elicitation,
    ElicitationResult,
    SessionEnd,
}

// ---------------------------------------------------------------------------
// MCP server definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDef {
    pub name: String,
    #[serde(skip)]
    pub source_path: PathBuf,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Working directory for the MCP server
    #[serde(default)]
    pub cwd: Option<String>,
}

// ---------------------------------------------------------------------------
// Instruction definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionDef {
    pub name: String,
    pub source_path: PathBuf,
    pub body: BodyContent,
}

// ---------------------------------------------------------------------------
// Output style definition
// ---------------------------------------------------------------------------

/// An output style definition from output-styles/ directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStyleDef {
    pub name: String,
    pub source_path: PathBuf,
    pub body: BodyContent,
}

// ---------------------------------------------------------------------------
// LSP server definition
// ---------------------------------------------------------------------------

/// An LSP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerDef {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Maps file extensions to language identifiers (required by spec)
    #[serde(default, rename = "extensionToLanguage")]
    pub extension_to_language: BTreeMap<String, String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, rename = "initializationOptions")]
    pub initialization_options: Option<serde_json::Value>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
    #[serde(default, rename = "workspaceFolder")]
    pub workspace_folder: Option<String>,
    #[serde(default, rename = "startupTimeout")]
    pub startup_timeout: Option<u64>,
    #[serde(default, rename = "shutdownTimeout")]
    pub shutdown_timeout: Option<u64>,
    #[serde(default, rename = "restartOnCrash")]
    pub restart_on_crash: Option<bool>,
    #[serde(default, rename = "maxRestarts")]
    pub max_restarts: Option<u64>,
}

// ---------------------------------------------------------------------------
// Shared fragment definition
// ---------------------------------------------------------------------------

/// A reusable content fragment from the shared/ directory.
/// Can be included in skills/agents/instructions via `{% include "name" %}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFragment {
    /// Fragment name (filename stem, e.g. "error-handling")
    pub name: String,
    /// Relative path from plugin root
    pub source_path: PathBuf,
    /// Fragment content — may itself contain {{variables}}
    pub body: BodyContent,
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
// Body content — plain text or parsed template
// ---------------------------------------------------------------------------

/// The body of a skill, agent, or instruction definition.
///
/// `Plain` bodies are opaque strings — no template processing.
/// `Template` bodies have been scanned for `{{variable}}` references.
/// Bodies without `{{` are always `Plain` — zero overhead for existing plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BodyContent {
    Plain(String),
    Template(TemplateBody),
}

/// A body that contains `{{variable}}` references or `{% include %}` directives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateBody {
    /// The raw template string (before variable substitution)
    pub raw: String,
    /// Extracted variable references with byte-offset spans
    #[serde(skip)]
    pub variables: Vec<VariableRef>,
    /// Names of included shared fragments (e.g. "error-handling" from `{% include "error-handling" %}`)
    #[serde(skip)]
    pub includes: Vec<String>,
}

/// A reference to a `{{variable}}` in a template body.
#[derive(Debug, Clone)]
pub struct VariableRef {
    /// Variable name (without the `{{ }}` delimiters)
    pub name: String,
    /// Byte offsets (start, end) in the raw template string, covering `{{name}}`
    pub span: (usize, usize),
}

impl BodyContent {
    /// Get the raw text content, regardless of whether it's Plain or Template.
    pub fn as_raw(&self) -> &str {
        match self {
            BodyContent::Plain(s) => s,
            BodyContent::Template(t) => &t.raw,
        }
    }

    /// True if this body contains template variables.
    pub fn has_variables(&self) -> bool {
        matches!(self, BodyContent::Template(t) if !t.variables.is_empty())
    }
}

/// Allows `body: "text".into()` for backwards-compatible construction.
impl From<String> for BodyContent {
    fn from(s: String) -> Self {
        BodyContent::Plain(s)
    }
}

impl From<&str> for BodyContent {
    fn from(s: &str) -> Self {
        BodyContent::Plain(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Template variable declarations
// ---------------------------------------------------------------------------

/// A declared template variable in plugin.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarDef {
    /// Human-readable description of what this variable is for
    #[serde(default)]
    pub description: Option<String>,

    /// Default value (used when no target-specific override exists)
    #[serde(default)]
    pub default: Option<String>,

    /// If true, the variable must have a value (default or target-specific) for every target
    #[serde(default)]
    pub required: bool,

    /// Per-target value overrides
    #[serde(default)]
    pub targets: BTreeMap<Target, String>,
}

// ---------------------------------------------------------------------------
// Utility types
// ---------------------------------------------------------------------------

/// A boolean that also accepts "true"/"false" strings (some real plugins use quoted bools).
#[derive(Debug, Clone, Serialize)]
pub struct LenientBool(pub bool);

impl<'de> serde::Deserialize<'de> for LenientBool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = LenientBool;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a boolean or string \"true\"/\"false\"")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<LenientBool, E> {
                Ok(LenientBool(v))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<LenientBool, E> {
                match v {
                    "true" => Ok(LenientBool(true)),
                    "false" => Ok(LenientBool(false)),
                    _ => Err(E::custom(format!("expected true/false, got {v:?}"))),
                }
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

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
