//! Capability analyzer.
//!
//! Compares a plugin's actual content (skills, agents, hooks, etc.) against
//! each target's capability matrix. Produces a report with errors (unsupported),
//! warnings (partial/flags support), and info (fallback applied).

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::*;
use crate::targets::{SupportLevel, Target, capability_matrix};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Analyze a plugin IR against its declared targets.
pub fn analyze(ir: &PluginIR) -> AnalysisReport {
    let inferred = infer_capabilities(ir);

    if ir.manifest.targets.is_empty() {
        return AnalysisReport {
            inferred_capabilities: inferred,
            diagnostics: vec![],
            target_summaries: BTreeMap::new(),
        };
    }

    let mut diagnostics = Vec::new();
    let mut summaries = BTreeMap::new();

    for target in &ir.manifest.targets {
        let matrix = capability_matrix(*target);
        let mut error_count = 0;
        let mut warning_count = 0;

        for cap_key in &inferred {
            let support = matrix
                .get(cap_key.as_str())
                .copied()
                .unwrap_or(SupportLevel::None);

            // Look up fallback: try parsing the key as a Capability for typed lookup
            let fallback = Capability::try_from(cap_key.clone())
                .ok()
                .and_then(|cap| ir.manifest.fallbacks.get(&cap))
                .and_then(|m| m.get(target));

            match (support, fallback) {
                (SupportLevel::Full, _) => {}

                (SupportLevel::None, Some(fb)) => {
                    diagnostics.push(Diagnostic {
                        target: *target,
                        capability: cap_key.clone(),
                        severity: Severity::Info,
                        message: format!(
                            "'{cap_key}' not supported by {target}, will {}: {}",
                            fallback_verb(fb),
                            fallback_description(fb),
                        ),
                    });
                }

                (SupportLevel::None, None) => {
                    error_count += 1;
                    diagnostics.push(Diagnostic {
                        target: *target,
                        capability: cap_key.clone(),
                        severity: Severity::Error,
                        message: format!(
                            "'{cap_key}' is not supported by {target}. \
                             Declare a fallback strategy in plugin.yaml or remove this target.",
                        ),
                    });
                }

                (SupportLevel::Partial | SupportLevel::Flags, Some(fb)) => {
                    diagnostics.push(Diagnostic {
                        target: *target,
                        capability: cap_key.clone(),
                        severity: Severity::Info,
                        message: format!(
                            "'{cap_key}' has limited support on {target}, will {}: {}",
                            fallback_verb(fb),
                            fallback_description(fb),
                        ),
                    });
                }

                (SupportLevel::Partial | SupportLevel::Flags, None) => {
                    warning_count += 1;
                    diagnostics.push(Diagnostic {
                        target: *target,
                        capability: cap_key.clone(),
                        severity: Severity::Warning,
                        message: format!(
                            "'{cap_key}' has only {:?} support on {target}. \
                             Output may behave differently than on Claude Code.",
                            support,
                        ),
                    });
                }
            }
        }

        summaries.insert(
            *target,
            TargetSummary {
                error_count,
                warning_count,
            },
        );
    }

    AnalysisReport {
        inferred_capabilities: inferred,
        diagnostics,
        target_summaries: summaries,
    }
}

// ---------------------------------------------------------------------------
// Capability inference
// ---------------------------------------------------------------------------

fn infer_capabilities(ir: &PluginIR) -> BTreeSet<String> {
    let mut caps = BTreeSet::new();

    if !ir.skills.is_empty() {
        caps.insert("skills".to_string());
    }

    if !ir.agents.is_empty() {
        caps.insert("agents".to_string());
    }

    if !ir.hooks.is_empty() {
        for hook in &ir.hooks {
            let key = match hook.event {
                HookEvent::PreToolUse => "hooks.pre-tool-use",
                HookEvent::PostToolUse => "hooks.post-tool-use",
                HookEvent::Stop => "hooks.stop",
                HookEvent::SessionStart => "hooks.session-start",
                HookEvent::UserPromptSubmit => "hooks.user-prompt-submit",
                HookEvent::PermissionRequest => "hooks.permission-request",
                HookEvent::PermissionDenied => "hooks.permission-denied",
                HookEvent::PostToolUseFailure => "hooks.post-tool-use-failure",
                HookEvent::Notification => "hooks.notification",
                HookEvent::SubagentStart => "hooks.subagent-start",
                HookEvent::SubagentStop => "hooks.subagent-stop",
                HookEvent::TaskCreated => "hooks.task-created",
                HookEvent::TaskCompleted => "hooks.task-completed",
                HookEvent::StopFailure => "hooks.stop-failure",
                HookEvent::TeammateIdle => "hooks.teammate-idle",
                HookEvent::InstructionsLoaded => "hooks.instructions-loaded",
                HookEvent::ConfigChange => "hooks.config-change",
                HookEvent::CwdChanged => "hooks.cwd-changed",
                HookEvent::FileChanged => "hooks.file-changed",
                HookEvent::WorktreeCreate => "hooks.worktree-create",
                HookEvent::WorktreeRemove => "hooks.worktree-remove",
                HookEvent::PreCompact => "hooks.pre-compact",
                HookEvent::PostCompact => "hooks.post-compact",
                HookEvent::Elicitation => "hooks.elicitation",
                HookEvent::ElicitationResult => "hooks.elicitation-result",
                HookEvent::SessionEnd => "hooks.session-end",
            };
            caps.insert(key.to_string());
        }
        if !caps.iter().any(|k| k.starts_with("hooks.")) {
            caps.insert("hooks".to_string());
        }
    }

    if !ir.mcp_servers.is_empty() {
        caps.insert("mcp-servers".to_string());
    }

    if !ir.instructions.is_empty() {
        caps.insert("instructions".to_string());
    }

    if !ir.lsp_servers.is_empty() {
        caps.insert("lsp-servers".to_string());
    }

    caps
}

// ---------------------------------------------------------------------------
// Fallback descriptions
// ---------------------------------------------------------------------------

fn fallback_verb(fb: &FallbackStrategy) -> &'static str {
    match fb {
        FallbackStrategy::Skip => "skip",
        FallbackStrategy::InstructionBased => "use instruction-based fallback",
        FallbackStrategy::PromptTemplate => "use prompt-template fallback",
        FallbackStrategy::AgentsMdSection => "use agents-md-section fallback",
    }
}

fn fallback_description(fb: &FallbackStrategy) -> &'static str {
    match fb {
        FallbackStrategy::Skip => "feature will be omitted from output",
        FallbackStrategy::InstructionBased => {
            "feature will be emitted as instructions/rules instead of native hook"
        }
        FallbackStrategy::PromptTemplate => "feature will be emitted as a saved prompt template",
        FallbackStrategy::AgentsMdSection => "feature will be emitted as a section in AGENTS.md",
    }
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

/// The result of analyzing a plugin against its targets.
#[derive(Debug)]
pub struct AnalysisReport {
    /// Capabilities inferred from the plugin's actual content
    pub inferred_capabilities: BTreeSet<String>,

    /// All diagnostics across all targets
    pub diagnostics: Vec<Diagnostic>,

    /// Per-target compatibility summary
    pub target_summaries: BTreeMap<Target, TargetSummary>,
}

impl AnalysisReport {
    pub fn is_ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn infos(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Info)
    }

    pub fn for_target(&self, target: Target) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.target == target)
    }
}

#[derive(Debug)]
pub struct Diagnostic {
    pub target: Target,
    pub capability: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN ",
            Severity::Info => "INFO ",
        }
    }
}

/// Per-target compatibility summary.
#[derive(Debug)]
pub struct TargetSummary {
    pub error_count: usize,
    pub warning_count: usize,
}

impl TargetSummary {
    /// True if no errors (warnings are acceptable).
    pub fn compatible(&self) -> bool {
        self.error_count == 0
    }
}
