//! CLI argument parsing with clap derive.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use jacq_core::targets::Target;

#[derive(Debug, Parser)]
#[command(
    name = "jacq",
    version,
    about = "Agnostic plugin compiler for AI coding agents",
    long_about = "jacq compiles plugin definitions into valid, optimized plugins \
                  for multiple AI coding agent harnesses (Claude Code, OpenCode, \
                  Codex, Cursor, and more).\n\n\
                  Named for the Jacquard loom (1804) — the first programmable machine."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scaffold a new plugin or import from an existing one
    ///
    /// With no NAME, operates on the current directory and derives the plugin
    /// name from its basename. Bare scaffold mode adds a `plugin.yaml` (and
    /// any missing scaffold files) without clobbering existing content;
    /// `--from` mode requires the target directory to be empty (excluding
    /// `.git`).
    Init {
        /// Plugin name (defaults to current directory's basename)
        name: Option<String>,

        /// Import from an existing plugin directory (any harness layout)
        #[arg(long)]
        from: Option<PathBuf>,

        /// Comma-separated target list (e.g. `claude-code,codex,opencode`).
        /// When omitted: scaffold defaults to `[claude-code]`; `--from` probes
        /// the source for existing target wrappers and seeds targets accordingly.
        #[arg(long, value_delimiter = ',')]
        targets: Option<Vec<Target>>,
    },

    /// Validate a plugin without building
    Validate {
        /// Plugin directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Check compatibility with a specific target only
        #[arg(long)]
        target: Option<Target>,
    },

    /// Build plugin for target platforms
    ///
    /// Default: emit target-specific wrappers (manifests, MCP/LSP config, etc.)
    /// in-place at the source repo root. Components (commands/, agents/, hooks/)
    /// stay at their canonical locations — jacq does not duplicate them per
    /// target. The repo becomes a polyglot plugin discoverable by every
    /// declared target.
    ///
    /// Pass `--output <dir>` to emit isolated per-target trees under
    /// `<dir>/<target>/...` instead. Use this for CI staging, archival, or
    /// any case where artifacts must be separable from the source repo.
    Build {
        /// Plugin directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Build for a specific target only
        #[arg(long)]
        target: Option<Target>,

        /// Fail on any capability gap (no fallbacks applied)
        #[arg(long)]
        strict: bool,

        /// Output directory for isolated per-target trees. When omitted,
        /// jacq emits in-place at the source repo root.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Test generated output against target schemas
    Test {
        /// Plugin directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Test a specific target only
        #[arg(long)]
        target: Option<Target>,

        /// Actually install and smoke-test (requires target runtime)
        #[arg(long)]
        live: bool,
    },

    /// Show capability matrix and compatibility report
    Inspect {
        /// Plugin directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}
