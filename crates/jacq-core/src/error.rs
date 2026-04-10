//! Error types for jacq, using miette for rich diagnostics.

use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

use crate::targets::Target;

#[derive(Debug, Error, Diagnostic)]
pub enum JacqError {
    #[error("Directory not found: {}", path.display())]
    #[diagnostic(
        code(jacq::dir_not_found),
        help("Check that the path exists and is a directory")
    )]
    DirectoryNotFound { path: PathBuf },

    #[error("No plugin manifest found in {}", path.display())]
    #[diagnostic(
        code(jacq::no_manifest),
        help("Expected plugin.yaml (IR) or plugin.json (Claude Code) in the plugin directory")
    )]
    NoManifest { path: PathBuf },

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
        severity(warning),
        help("The emitted output may behave differently than on Claude Code")
    )]
    PartialCapability {
        capability: String,
        target: Target,
    },

    #[error("Invalid frontmatter in {}: {reason}", path.display())]
    #[diagnostic(code(jacq::invalid_frontmatter))]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error("Referenced file not found: {}", path.display())]
    #[diagnostic(code(jacq::missing_file))]
    MissingFile { path: PathBuf },

    #[error("IO error at {}: {source}", path.display())]
    #[diagnostic(code(jacq::io))]
    IoWithPath {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    #[diagnostic(code(jacq::io))]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {reason}")]
    #[diagnostic(code(jacq::serialization))]
    Serialization { reason: String },

    #[error("Undeclared template variable '{{{{{}}}}}' in {}", name, path.display())]
    #[diagnostic(
        code(jacq::undeclared_variable),
        help("Declare it in plugin.yaml under 'vars:'")
    )]
    UndeclaredVariable {
        name: String,
        path: PathBuf,
        span: (usize, usize),
    },

    #[error("Required variable '{name}' has no value for target '{target}'")]
    #[diagnostic(
        code(jacq::missing_variable_value),
        help("Add a default value or a target-specific value under 'vars.{name}.targets.{target}:'")
    )]
    MissingVariableValue {
        name: String,
        target: Target,
    },

    #[error("Missing shared fragment '{name}' included in {}", path.display())]
    #[diagnostic(
        code(jacq::missing_include),
        help("Create shared/{name}.md or check the include name")
    )]
    MissingInclude {
        name: String,
        path: PathBuf,
    },
}

pub type Result<T> = std::result::Result<T, JacqError>;
