//! Capability analyzer.
//!
//! Compares a plugin's actual content (skills, agents, hooks, etc.) against
//! each target's capability matrix. Produces a report with errors (unsupported),
//! warnings (partial/flags support), and info (fallback applied).

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::*;
use crate::targets::{capability_matrix, SupportLevel, Target};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Analyze a plugin IR against its declared targets.
///
/// Returns a report with diagnostics (errors, warnings, info) and per-target
/// compatibility summaries.
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
            let support = matrix.get(cap_key.as_str()).copied().unwrap_or(SupportLevel::None);
            let fallback = ir
                .manifest
                .fallbacks
                .get(cap_key.as_str())
                .and_then(|m| m.get(target));

            match (support, fallback) {
                // Fully supported — no diagnostic needed
                (SupportLevel::Full, _) => {}

                // Not supported, but fallback declared — info
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

                // Not supported, no fallback — error
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

                // Partial/Flags support, fallback declared — info
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

                // Partial/Flags support, no fallback — warning
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
                compatible: error_count == 0,
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

/// Infer which capabilities a plugin actually uses based on its content.
fn infer_capabilities(ir: &PluginIR) -> BTreeSet<String> {
    let mut caps = BTreeSet::new();

    if !ir.skills.is_empty() {
        caps.insert("skills".to_string());
    }

    if !ir.agents.is_empty() {
        caps.insert("agents".to_string());
    }

    if !ir.hooks.is_empty() {
        // Infer specific hook capabilities rather than the parent "hooks".
        // This lets targets and fallbacks address individual hook types precisely.
        for hook in &ir.hooks {
            let key = match hook.event {
                HookEvent::PreToolUse => "hooks.pre-tool-use",
                HookEvent::PostToolUse => "hooks.post-tool-use",
                HookEvent::Stop => "hooks.stop",
            };
            caps.insert(key.to_string());
        }
        // Only infer parent "hooks" if no specific events were identified
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
        FallbackStrategy::PromptTemplate => {
            "feature will be emitted as a saved prompt template"
        }
        FallbackStrategy::AgentsMdSection => {
            "feature will be emitted as a section in AGENTS.md"
        }
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
    /// True if there are no errors (warnings and info are OK).
    pub fn is_ok(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    /// Iterate over error diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Iterate over warning diagnostics.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Iterate over info diagnostics.
    pub fn infos(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Info)
    }

    /// Diagnostics for a specific target.
    pub fn for_target(&self, target: Target) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(move |d| d.target == target)
    }
}

/// A single diagnostic finding.
#[derive(Debug)]
pub struct Diagnostic {
    pub target: Target,
    pub capability: String,
    pub severity: Severity,
    pub message: String,
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// Fixed-width label for diagnostic output (5 chars, right-padded).
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
    /// True if no errors (warnings are acceptable)
    pub compatible: bool,
    /// Number of error-level diagnostics
    pub error_count: usize,
    /// Number of warning-level diagnostics
    pub warning_count: usize,
}
