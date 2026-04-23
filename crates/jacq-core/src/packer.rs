//! Distribution packaging — bundle a built target directory into a single archive.
//!
//! `pack` consumes the output of `emitter::emit` (one subdirectory per target)
//! and produces a `<name>-<version>-<target>.tar.gz` next to it. For Claude Code,
//! it also emits a `marketplace.json` snippet — the only target with a real
//! registry-style consumer.
//!
//! Format choice: tar.gz. Unix-native, what npm/git/Claude Code marketplaces
//! all consume, and the deflate is pure-Rust via flate2's `rust_backend`
//! feature so we don't pull in C zlib.

use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;

use crate::error::{JacqError, Result};
use crate::ir::PluginManifest;
use crate::targets::Target;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Pack a target's emitted directory into a single tar.gz archive.
///
/// `build_dir` is the directory containing the target's emitted files
/// (typically `dist/<target>/`). `output_dir` is where the archive is written.
/// Returns the path to the created archive.
pub fn pack(
    target: Target,
    manifest: &PluginManifest,
    build_dir: &Path,
    output_dir: &Path,
) -> Result<PathBuf> {
    if !build_dir.exists() {
        return Err(JacqError::DirectoryNotFound {
            path: build_dir.to_path_buf(),
        });
    }

    std::fs::create_dir_all(output_dir).map_err(|e| JacqError::IoWithPath {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let archive_name = format!(
        "{}-{}-{}.tar.gz",
        manifest.name,
        manifest.version,
        target.as_str(),
    );
    let archive_path = output_dir.join(&archive_name);

    write_tar_gz(build_dir, &archive_path)?;

    if matches!(target, Target::ClaudeCode) {
        let marketplace_path = output_dir.join(format!("{}-marketplace.json", manifest.name));
        let entry = marketplace_entry(manifest, &archive_name);
        let json = serde_json::to_string_pretty(&entry).map_err(|e| JacqError::Serialization {
            reason: e.to_string(),
        })?;
        std::fs::write(&marketplace_path, json).map_err(|e| JacqError::IoWithPath {
            path: marketplace_path,
            source: e,
        })?;
    }

    Ok(archive_path)
}

// ---------------------------------------------------------------------------
// tar.gz writing
// ---------------------------------------------------------------------------

fn write_tar_gz(src_dir: &Path, archive_path: &Path) -> Result<()> {
    let file = File::create(archive_path).map_err(|e| JacqError::IoWithPath {
        path: archive_path.to_path_buf(),
        source: e,
    })?;
    let gz = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(gz);

    // append_dir_all walks the directory in tar.rs's own deterministic order
    // and stores entries with paths relative to the second arg. Rooting at "."
    // keeps extracted archives self-contained.
    tar.append_dir_all(".", src_dir)
        .map_err(|e| JacqError::IoWithPath {
            path: src_dir.to_path_buf(),
            source: e,
        })?;

    tar.into_inner()
        .and_then(|gz| gz.finish())
        .map_err(|e| JacqError::IoWithPath {
            path: archive_path.to_path_buf(),
            source: e,
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code marketplace JSON
// ---------------------------------------------------------------------------

/// Build a single marketplace.json entry suitable for pasting into a Claude Code
/// marketplace listing. Shape mirrors the official marketplace schema: name,
/// version, description, author, plus an `archive` pointer.
fn marketplace_entry(manifest: &PluginManifest, archive_name: &str) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert("name".into(), serde_json::json!(manifest.name));
    entry.insert("version".into(), serde_json::json!(manifest.version));
    if !manifest.description.is_empty() {
        entry.insert(
            "description".into(),
            serde_json::json!(manifest.description),
        );
    }

    let author_name = match &manifest.author {
        crate::ir::Author::Name(n) if !n.is_empty() => Some(n.clone()),
        crate::ir::Author::Structured { name, .. } if !name.is_empty() => Some(name.clone()),
        _ => None,
    };
    if let Some(name) = author_name {
        entry.insert("author".into(), serde_json::json!({ "name": name }));
    }

    if let Some(repo) = &manifest.repository {
        entry.insert("repository".into(), serde_json::json!(repo));
    }
    if let Some(home) = &manifest.homepage {
        entry.insert("homepage".into(), serde_json::json!(home));
    }
    if !manifest.keywords.is_empty() {
        entry.insert("keywords".into(), serde_json::json!(manifest.keywords));
    }

    entry.insert("archive".into(), serde_json::json!(archive_name));

    serde_json::Value::Object(entry)
}
