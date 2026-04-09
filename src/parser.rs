//! Plugin directory parser.
//!
//! Reads a plugin from disk, auto-detecting the format:
//! - **IR format**: `plugin.yaml` at root
//! - **Claude Code native**: `.claude-plugin/plugin.json` or `plugin.json` at root
//!
//! Walks the directory for skills, agents, hooks, MCP servers, and instructions,
//! then assembles a `PluginIR`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::error::{JacqError, Result};
use crate::ir::*;
use crate::targets::Target;

/// Parse a plugin directory into an in-memory IR.
pub fn parse_plugin(dir: &Path) -> Result<PluginIR> {
    let dir = dir.canonicalize().map_err(|_| JacqError::NoManifest {
        path: dir.to_path_buf(),
    })?;

    let (manifest, manifest_format) = parse_manifest(&dir)?;

    let skills = parse_md_files(&dir, "skills", &manifest_format)?;
    let commands = parse_md_files(&dir, "commands", &manifest_format)?;
    let all_skills: Vec<SkillDef> = skills.into_iter().chain(commands).collect();

    let agents = parse_agent_files(&dir)?;
    let hooks = parse_hook_files(&dir)?;
    let mcp_servers = parse_mcp_files(&dir)?;
    let instructions = parse_instruction_files(&dir)?;
    let target_overrides = parse_target_overrides(&dir, &manifest)?;

    Ok(PluginIR {
        manifest,
        skills: all_skills,
        agents,
        hooks,
        mcp_servers,
        instructions,
        target_overrides,
        source_dir: dir,
    })
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ManifestFormat {
    /// plugin.yaml at root (IR format)
    Ir,
    /// .claude-plugin/plugin.json or plugin.json (Claude Code native)
    ClaudeCode,
}

// ---------------------------------------------------------------------------
// Manifest parsing
// ---------------------------------------------------------------------------

fn parse_manifest(dir: &Path) -> Result<(PluginManifest, ManifestFormat)> {
    // Try IR format first: plugin.yaml at root
    let ir_path = dir.join("plugin.yaml");
    if ir_path.exists() {
        let content = fs::read_to_string(&ir_path).map_err(JacqError::Io)?;
        let manifest: PluginManifest = serde_yaml::from_str(&content).map_err(|e| {
            JacqError::ParseError {
                reason: format!("{}: {e}", ir_path.display()),
            }
        })?;
        return Ok((manifest, ManifestFormat::Ir));
    }

    // Try Claude Code format: .claude-plugin/plugin.json
    let cc_path = dir.join(".claude-plugin").join("plugin.json");
    if cc_path.exists() {
        return parse_json_manifest(&cc_path).map(|m| (m, ManifestFormat::ClaudeCode));
    }

    // Try root plugin.json
    let root_json = dir.join("plugin.json");
    if root_json.exists() {
        return parse_json_manifest(&root_json).map(|m| (m, ManifestFormat::ClaudeCode));
    }

    Err(JacqError::NoManifest {
        path: dir.to_path_buf(),
    })
}

fn parse_json_manifest(path: &Path) -> Result<PluginManifest> {
    let content = fs::read_to_string(path).map_err(JacqError::Io)?;
    serde_json::from_str(&content).map_err(|e| JacqError::ParseError {
        reason: format!("{}: {e}", path.display()),
    })
}

// ---------------------------------------------------------------------------
// YAML frontmatter extraction
// ---------------------------------------------------------------------------

/// Split a file into YAML frontmatter and markdown body.
/// Expects the file to start with `---\n`, then YAML, then `---\n`, then body.
/// If no frontmatter delimiters are found, returns None for frontmatter.
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content);
    }

    // Find the opening delimiter line
    let after_first = match trimmed.strip_prefix("---") {
        Some(rest) => rest.strip_prefix('\n').unwrap_or(rest),
        None => return (None, content),
    };

    // Find the closing ---
    if let Some(end_idx) = after_first.find("\n---") {
        let yaml = &after_first[..end_idx];
        let after_close = &after_first[end_idx + 4..]; // skip \n---
        let body = after_close.strip_prefix('\n').unwrap_or(after_close);
        (Some(yaml), body)
    } else {
        // No closing delimiter — treat entire content as body
        (None, content)
    }
}

// ---------------------------------------------------------------------------
// Skill/command parsing (.md files with YAML frontmatter)
// ---------------------------------------------------------------------------

fn parse_md_files(dir: &Path, subdir: &str, format: &ManifestFormat) -> Result<Vec<SkillDef>> {
    let search_dir = match format {
        // Claude Code plugins may not have a skills/ dir — they use commands/
        ManifestFormat::Ir | ManifestFormat::ClaudeCode => dir.join(subdir),
    };

    if !search_dir.exists() {
        return Ok(vec![]);
    }

    let mut skills = Vec::new();
    for entry in WalkDir::new(&search_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "md")
        })
    {
        let path = entry.path();
        let content = fs::read_to_string(path).map_err(JacqError::Io)?;
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let (yaml_str, body) = split_frontmatter(&content);

        let frontmatter: SkillFrontmatter = match yaml_str {
            Some(yaml) => serde_yaml::from_str(yaml).map_err(|e| {
                JacqError::InvalidFrontmatter {
                    path: rel_path.clone(),
                    reason: e.to_string(),
                }
            })?,
            None => SkillFrontmatter::default(),
        };

        skills.push(SkillDef {
            name,
            source_path: rel_path,
            frontmatter,
            body: body.to_string(),
        });
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

// ---------------------------------------------------------------------------
// Agent parsing (.md files with YAML frontmatter)
// ---------------------------------------------------------------------------

fn parse_agent_files(dir: &Path) -> Result<Vec<AgentDef>> {
    let agents_dir = dir.join("agents");
    if !agents_dir.exists() {
        return Ok(vec![]);
    }

    let mut agents = Vec::new();
    for entry in WalkDir::new(&agents_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "md")
        })
    {
        let path = entry.path();
        let content = fs::read_to_string(path).map_err(JacqError::Io)?;
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let (yaml_str, body) = split_frontmatter(&content);

        let frontmatter: AgentFrontmatter = match yaml_str {
            Some(yaml) => serde_yaml::from_str(yaml).map_err(|e| {
                JacqError::InvalidFrontmatter {
                    path: rel_path.clone(),
                    reason: e.to_string(),
                }
            })?,
            None => AgentFrontmatter::default(),
        };

        agents.push(AgentDef {
            name,
            source_path: rel_path,
            frontmatter,
            body: body.to_string(),
        });
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

// ---------------------------------------------------------------------------
// Hook parsing (.yaml files)
// ---------------------------------------------------------------------------

fn parse_hook_files(dir: &Path) -> Result<Vec<HookDef>> {
    let hooks_dir = dir.join("hooks");
    if !hooks_dir.exists() {
        return Ok(vec![]);
    }

    let mut hooks = Vec::new();
    for entry in WalkDir::new(&hooks_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
    {
        let path = entry.path();
        let content = fs::read_to_string(path).map_err(JacqError::Io)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let mut hook: HookDef = serde_yaml::from_str(&content).map_err(|e| {
            JacqError::InvalidFrontmatter {
                path: rel_path.clone(),
                reason: e.to_string(),
            }
        })?;
        // Override source_path with the actual file location
        hook.source_path = rel_path;

        hooks.push(hook);
    }

    hooks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(hooks)
}

// ---------------------------------------------------------------------------
// MCP server parsing (.yaml files)
// ---------------------------------------------------------------------------

fn parse_mcp_files(dir: &Path) -> Result<Vec<McpServerDef>> {
    let mcp_dir = dir.join("mcp");
    if !mcp_dir.exists() {
        return Ok(vec![]);
    }

    let mut servers = Vec::new();
    for entry in WalkDir::new(&mcp_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
    {
        let path = entry.path();
        let content = fs::read_to_string(path).map_err(JacqError::Io)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let mut server: McpServerDef = serde_yaml::from_str(&content).map_err(|e| {
            JacqError::InvalidFrontmatter {
                path: rel_path.clone(),
                reason: e.to_string(),
            }
        })?;
        server.source_path = rel_path;

        servers.push(server);
    }

    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

// ---------------------------------------------------------------------------
// Instruction parsing (.md files, body only — no frontmatter)
// ---------------------------------------------------------------------------

fn parse_instruction_files(dir: &Path) -> Result<Vec<InstructionDef>> {
    let instr_dir = dir.join("instructions");
    if !instr_dir.exists() {
        return Ok(vec![]);
    }

    let mut instructions = Vec::new();
    for entry in WalkDir::new(&instr_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "md")
        })
    {
        let path = entry.path();
        let content = fs::read_to_string(path).map_err(JacqError::Io)?;
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        instructions.push(InstructionDef {
            name,
            source_path: rel_path,
            body: content,
        });
    }

    instructions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(instructions)
}

// ---------------------------------------------------------------------------
// Target overrides (targets/<target-name>/*)
// ---------------------------------------------------------------------------

fn parse_target_overrides(
    dir: &Path,
    manifest: &PluginManifest,
) -> Result<BTreeMap<Target, Vec<TargetOverride>>> {
    let targets_dir = dir.join("targets");
    if !targets_dir.exists() {
        return Ok(BTreeMap::new());
    }

    let mut overrides = BTreeMap::new();

    for target in &manifest.targets {
        let target_dir = targets_dir.join(target.as_str());
        if !target_dir.exists() {
            continue;
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(&target_dir)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let rel_path = path.strip_prefix(&target_dir).unwrap_or(path).to_path_buf();
            let content = fs::read(path).map_err(JacqError::Io)?;
            files.push(TargetOverride {
                path: rel_path,
                content,
            });
        }

        if !files.is_empty() {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            overrides.insert(*target, files);
        }
    }

    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_with_yaml() {
        let content = "---\ndescription: test\n---\nBody here";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, Some("description: test"));
        assert_eq!(body, "Body here");
    }

    #[test]
    fn split_frontmatter_no_yaml() {
        let content = "Just a body with no frontmatter";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_no_closing_delimiter() {
        let content = "---\ndescription: test\nno closing";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_multiline_yaml() {
        let content = "---\ndescription: test\ncolor: blue\nallowed-tools:\n  - Read\n  - Write\n---\n\nMarkdown body\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.unwrap().contains("description: test"));
        assert!(fm.unwrap().contains("allowed-tools:"));
        assert!(body.starts_with('\n'));
        assert!(body.contains("Markdown body"));
    }

    #[test]
    fn split_frontmatter_real_skill_format() {
        let content = "\
---
description: CRUD operations for macOS Notes.app
argument-hint: [describe what you want to do]
allowed-tools: Bash(osascript:*)
---

You are a macOS Notes.app assistant.
";
        let (fm, body) = split_frontmatter(content);
        let yaml = fm.expect("should have frontmatter");
        assert!(yaml.contains("description: CRUD operations"));
        assert!(yaml.contains("allowed-tools: Bash(osascript:*)"));

        // Verify the frontmatter parses as SkillFrontmatter
        let parsed: SkillFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            parsed.description.as_deref(),
            Some("CRUD operations for macOS Notes.app")
        );

        assert!(body.contains("You are a macOS Notes.app assistant."));
    }
}
