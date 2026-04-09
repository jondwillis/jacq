//! Target emitters — generate platform-specific plugin output.
//!
//! Each target gets its own subdirectory under the output path.
//! The emitter for a target produces all files that target's plugin system expects.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::{JacqError, Result};
use crate::ir::*;
use crate::targets::Target;
use crate::template::RenderEngine;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Emit a plugin IR to the output directory, generating one subdirectory per target.
pub fn emit(ir: &PluginIR, output_dir: &Path) -> Result<()> {
    for target in &ir.manifest.targets {
        let target_dir = output_dir.join(target.as_str());
        fs::create_dir_all(&target_dir).map_err(|e| JacqError::IoWithPath {
            path: target_dir.clone(),
            source: e,
        })?;

        let engine = RenderEngine::new(&ir.manifest.vars, &ir.shared, *target)?;

        match target {
            Target::ClaudeCode => emit_claude_code(ir, &engine, &target_dir)?,
            Target::OpenCode => emit_opencode(ir, &engine, &target_dir)?,
            Target::Codex => emit_codex(ir, &engine, &target_dir)?,
            Target::Cursor => emit_cursor(ir, &engine, &target_dir)?,
            Target::OpenClaw => emit_openclaw(ir, &engine, &target_dir)?,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code emitter — identity/passthrough
// ---------------------------------------------------------------------------

fn emit_claude_code(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    // plugin.json — core manifest
    let plugin_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "author": match &ir.manifest.author {
            Author::Name(n) => serde_json::json!({"name": n}),
            Author::Structured { name, email, url } => {
                let mut m = serde_json::Map::new();
                m.insert("name".to_string(), serde_json::Value::String(name.clone()));
                if let Some(e) = email {
                    m.insert("email".to_string(), serde_json::Value::String(e.clone()));
                }
                if let Some(u) = url {
                    m.insert("url".to_string(), serde_json::Value::String(u.clone()));
                }
                serde_json::Value::Object(m)
            }
        },
        "license": ir.manifest.license,
        "keywords": ir.manifest.keywords,
        "homepage": ir.manifest.homepage,
    });
    write_json(dir, "plugin.json", &plugin_json)?;

    // commands/*.md — skills with frontmatter
    if !ir.skills.is_empty() {
        let commands_dir = dir.join("commands");
        create_dir(&commands_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill, engine)?;
            write_file(&commands_dir.join(format!("{}.md", skill.name)), &content)?;
        }
    }

    // agents/*.md — agents with frontmatter
    if !ir.agents.is_empty() {
        let agents_dir = dir.join("agents");
        create_dir(&agents_dir)?;
        for agent in &ir.agents {
            let content = render_agent_md(agent, engine)?;
            write_file(&agents_dir.join(format!("{}.md", agent.name)), &content)?;
        }
    }

    // hooks — Claude Code hook definitions
    if !ir.hooks.is_empty() {
        let hooks_dir = dir.join("hooks");
        create_dir(&hooks_dir)?;
        for hook in &ir.hooks {
            let content = serde_yaml::to_string(&hook).map_err(|e| {
                JacqError::Serialization { reason: e.to_string() }
            })?;
            write_file(&hooks_dir.join(format!("{}.yaml", hook.name)), &content)?;
        }
    }

    // .mcp.json — MCP server configuration
    if !ir.mcp_servers.is_empty() {
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(dir, ".mcp.json", &mcp_config)?;
    }

    // CLAUDE.md — instructions
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions, engine)?;
        write_file(&dir.join("CLAUDE.md"), &content)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenCode emitter
// ---------------------------------------------------------------------------

fn emit_opencode(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    // package.json — npm-style manifest
    let package_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "license": ir.manifest.license,
        "keywords": ir.manifest.keywords,
    });
    write_json(dir, "package.json", &package_json)?;

    // AGENTS.md — combined instructions, skill docs, and agent descriptions
    let agents_md = render_agents_md(ir, true, engine)?;
    write_file(&dir.join("AGENTS.md"), &agents_md)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Codex emitter
// ---------------------------------------------------------------------------

fn emit_codex(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    // plugin.json — Codex-flavored manifest
    let plugin_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "license": ir.manifest.license,
    });
    write_json(dir, "plugin.json", &plugin_json)?;

    // skills/*.md — Codex has full skill support
    if !ir.skills.is_empty() {
        let skills_dir = dir.join("skills");
        create_dir(&skills_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill, engine)?;
            write_file(&skills_dir.join(format!("{}.md", skill.name)), &content)?;
        }
    }

    // AGENTS.md — instructions and agent descriptions (NOT skills — they're in skill files)
    let agents_md = render_agents_md(ir, false, engine)?;
    write_file(&dir.join("AGENTS.md"), &agents_md)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Cursor emitter (minimal — rules + MCP)
// ---------------------------------------------------------------------------

fn emit_cursor(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions, engine)?;
        write_file(&dir.join(".cursorrules"), &content)?;
    }

    if !ir.mcp_servers.is_empty() {
        let cursor_dir = dir.join(".cursor");
        create_dir(&cursor_dir)?;
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(&cursor_dir, "mcp.json", &mcp_config)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenClaw emitter (minimal — npm package + instructions)
// ---------------------------------------------------------------------------

fn emit_openclaw(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    let package_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "openclaw": {
            "extensions": {}
        }
    });
    write_json(dir, "package.json", &package_json)?;

    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions, engine)?;
        write_file(&dir.join("README.md"), &content)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn yaml_value(v: &impl serde::Serialize) -> Result<serde_yaml::Value> {
    serde_yaml::to_value(v).map_err(|e| JacqError::Serialization {
        reason: e.to_string(),
    })
}

fn render_skill_md(skill: &SkillDef, engine: &RenderEngine) -> Result<String> {
    let mut fm = BTreeMap::new();

    if let Some(desc) = &skill.frontmatter.description {
        fm.insert("description", yaml_value(desc)?);
    }
    if let Some(hint) = &skill.frontmatter.argument_hint {
        fm.insert("argument-hint", yaml_value(hint)?);
    }
    if let Some(tools) = &skill.frontmatter.allowed_tools {
        fm.insert("allowed-tools", yaml_value(tools)?);
    }
    if let Some(color) = &skill.frontmatter.color {
        fm.insert("color", yaml_value(color)?);
    }
    if let Some(examples) = &skill.frontmatter.examples {
        fm.insert("examples", yaml_value(examples)?);
    }

    for (k, v) in &skill.frontmatter.extra {
        fm.insert(k.as_str(), v.clone());
    }

    let rendered_body = engine.render(&skill.body)?;
    wrap_frontmatter(fm, &rendered_body)
}

fn render_agent_md(agent: &AgentDef, engine: &RenderEngine) -> Result<String> {
    let mut fm = BTreeMap::new();

    if let Some(desc) = &agent.frontmatter.description {
        fm.insert("description", yaml_value(desc)?);
    }
    if let Some(model) = &agent.frontmatter.model {
        fm.insert("model", yaml_value(model)?);
    }
    if let Some(tools) = &agent.frontmatter.allowed_tools {
        fm.insert("allowed-tools", yaml_value(tools)?);
    }
    if let Some(color) = &agent.frontmatter.color {
        fm.insert("color", yaml_value(color)?);
    }

    for (k, v) in &agent.frontmatter.extra {
        fm.insert(k.as_str(), v.clone());
    }

    let rendered_body = engine.render(&agent.body)?;
    wrap_frontmatter(fm, &rendered_body)
}

fn wrap_frontmatter(frontmatter: BTreeMap<&str, serde_yaml::Value>, body: &str) -> Result<String> {
    if frontmatter.is_empty() {
        return Ok(body.to_string());
    }
    let yaml = serde_yaml::to_string(&frontmatter).map_err(|e| JacqError::Serialization {
        reason: e.to_string(),
    })?;
    Ok(format!("---\n{}---\n\n{}", yaml, body))
}

fn render_mcp_json(servers: &[McpServerDef]) -> serde_json::Value {
    let mut mcp_servers = serde_json::Map::new();
    for server in servers {
        let mut entry = serde_json::Map::new();
        entry.insert(
            "command".to_string(),
            serde_json::Value::String(server.command.clone()),
        );
        if !server.args.is_empty() {
            entry.insert(
                "args".to_string(),
                serde_json::to_value(&server.args).unwrap_or_default(),
            );
        }
        if !server.env.is_empty() {
            entry.insert(
                "env".to_string(),
                serde_json::to_value(&server.env).unwrap_or_default(),
            );
        }
        mcp_servers.insert(server.name.clone(), serde_json::Value::Object(entry));
    }
    serde_json::json!({ "mcpServers": mcp_servers })
}

/// Render instructions with blank-line separation between files.
fn render_instructions(instructions: &[InstructionDef], engine: &RenderEngine) -> Result<String> {
    let mut rendered = Vec::new();
    for instr in instructions {
        rendered.push(engine.render(&instr.body)?);
    }
    Ok(rendered.join("\n\n"))
}

/// Render AGENTS.md — used by OpenCode and Codex.
/// `include_skills`: if true, document skills in AGENTS.md (OpenCode).
/// If false, skip skills (Codex emits them as native skill files).
fn render_agents_md(ir: &PluginIR, include_skills: bool, engine: &RenderEngine) -> Result<String> {
    let mut sections = Vec::new();

    if !ir.instructions.is_empty() {
        sections.push(render_instructions(&ir.instructions, engine)?);
    }

    if include_skills && !ir.skills.is_empty() {
        let mut skill_section = String::from("## Available Commands\n\n");
        for skill in &ir.skills {
            skill_section.push_str(&format!("### {}\n\n", skill.name));
            if let Some(desc) = &skill.frontmatter.description {
                skill_section.push_str(&format!("{desc}\n\n"));
            }
            skill_section.push_str(&engine.render(&skill.body)?);
            skill_section.push('\n');
        }
        sections.push(skill_section);
    }

    if !ir.agents.is_empty() {
        let mut agent_section = String::from("## Available Agents\n\n");
        for agent in &ir.agents {
            agent_section.push_str(&format!("### {}\n\n", agent.name));
            if let Some(desc) = &agent.frontmatter.description {
                agent_section.push_str(&format!("{desc}\n\n"));
            }
            agent_section.push_str(&engine.render(&agent.body)?);
            agent_section.push('\n');
        }
        sections.push(agent_section);
    }

    Ok(sections.join("\n---\n\n"))
}

/// Write a JSON value to a file with pretty formatting.
fn write_json(dir: &Path, filename: &str, value: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_string_pretty(value).map_err(|e| JacqError::Serialization {
        reason: e.to_string(),
    })?;
    write_file(&dir.join(filename), &content)
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).map_err(|e| JacqError::IoWithPath {
        path: path.to_path_buf(),
        source: e,
    })
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| JacqError::IoWithPath {
        path: path.to_path_buf(),
        source: e,
    })
}
