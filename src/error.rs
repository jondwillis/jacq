//! Error types for jacq, using miette for rich diagnostics.

use miette::Diagnostic;
use thiserror::Error;

use crate::targets::Target;

#[derive(Debug, Error, Diagnostic)]
pub enum JacqError {
    #[error("No plugin manifest found in {path}")]
    #[diagnostic(
        code(jacq::no_manifest),
        help("Expected plugin.yaml (IR) or plugin.json (Claude Code) in the plugin directory")
    )]
    NoManifest { path: String },

    #[error("Failed to parse manifest: {reason}")]
    #[diagnostic(code(jacq::parse_error))]
    ParseError { reason: String },

    #[error("Capability '{capability}' is not supported by target '{target}'")]
    #[diagnostic(
        code(jacq::unsupported_capability),
        help("Declare a fallback strategy in plugin.yaml, or remove this target")
    )]
    UnsupportedCapability {
        capability: String,
        target: Target,
    },

    #[error("Capability '{capability}' is only partially supported by target '{target}'")]
    #[diagnostic(
        code(jacq::partial_capability),
        help("The emitted output may behave differently than on Claude Code")
    )]
    PartialCapability {
        capability: String,
        target: Target,
    },

    #[error("Invalid frontmatter in {path}: {reason}")]
    #[diagnostic(code(jacq::invalid_frontmatter))]
    InvalidFrontmatter { path: String, reason: String },

    #[error("Referenced file not found: {path}")]
    #[diagnostic(code(jacq::missing_file))]
    MissingFile { path: String },

    #[error(transparent)]
    #[diagnostic(code(jacq::io))]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, JacqError>;
