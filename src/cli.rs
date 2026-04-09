//! CLI argument parsing with clap derive.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::targets::Target;

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
    /// Scaffold a new plugin
    Init {
        /// Plugin name
        name: String,

        /// Import from an existing Claude Code plugin directory
        #[arg(long)]
        from: Option<PathBuf>,
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

        /// Output directory (defaults to ./dist)
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

    /// Package plugin for distribution
    Pack {
        /// Plugin directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

// clap needs FromStr for Target in --target flags
impl std::str::FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "claude-code" => Ok(Target::ClaudeCode),
            "opencode" => Ok(Target::OpenCode),
            "codex" => Ok(Target::Codex),
            "cursor" => Ok(Target::Cursor),
            "antigravity" => Ok(Target::Antigravity),
            "openclaw" => Ok(Target::OpenClaw),
            _ => Err(format!(
                "unknown target '{s}'. Valid targets: claude-code, opencode, codex, cursor, antigravity, openclaw"
            )),
        }
    }
}
