//! Target emitters — generate platform-specific plugin output.
//!
//! Each target gets its own subdirectory under the output path.
//! The emitter for a target produces all files that target's plugin system expects.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::ir::*;
use crate::targets::Target;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Options controlling emission behavior.
pub struct EmitOptions {
    /// If true, fail on any capability gap (no fallbacks applied).
    pub strict: bool,
}

/// Emit a plugin IR to the output directory, generating one subdirectory per target.
pub fn emit(ir: &PluginIR, output_dir: &Path, _opts: &EmitOptions) -> Result<()> {
    for target in &ir.manifest.targets {
        let target_dir = output_dir.join(target.as_str());
        fs::create_dir_all(&target_dir)?;

        match target {
            Target::ClaudeCode => emit_claude_code(ir, &target_dir)?,
            Target::OpenCode => emit_opencode(ir, &target_dir)?,
            Target::Codex => emit_codex(ir, &target_dir)?,
            Target::Cursor => emit_cursor(ir, &target_dir)?,
            Target::OpenClaw => emit_openclaw(ir, &target_dir)?,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code emitter — identity/passthrough
// ---------------------------------------------------------------------------

fn emit_claude_code(ir: &PluginIR, dir: &Path) -> Result<()> {
    // plugin.json — core manifest
    let plugin_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "author": match &ir.manifest.author {
            Author::Name(n) => serde_json::json!({"name": n}),
            Author::Structured { name, email } => {
                let mut m = serde_json::Map::new();
                m.insert("name".to_string(), serde_json::Value::String(name.clone()));
                if let Some(e) = email {
                    m.insert("email".to_string(), serde_json::Value::String(e.clone()));
                }
                serde_json::Value::Object(m)
            }
        },
        "license": ir.manifest.license,
        "keywords": ir.manifest.keywords,
    });
    write_json(dir, "plugin.json", &plugin_json)?;

    // commands/*.md — skills with frontmatter
    if !ir.skills.is_empty() {
        let commands_dir = dir.join("commands");
        fs::create_dir_all(&commands_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill);
            fs::write(commands_dir.join(format!("{}.md", skill.name)), content)?;
        }
    }

    // agents/*.md — agents with frontmatter
    if !ir.agents.is_empty() {
        let agents_dir = dir.join("agents");
        fs::create_dir_all(&agents_dir)?;
        for agent in &ir.agents {
            let content = render_agent_md(agent);
            fs::write(agents_dir.join(format!("{}.md", agent.name)), content)?;
        }
    }

    // .mcp.json — MCP server configuration
    if !ir.mcp_servers.is_empty() {
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(dir, ".mcp.json", &mcp_config)?;
    }

    // CLAUDE.md — instructions
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions);
        fs::write(dir.join("CLAUDE.md"), content)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenCode emitter
// ---------------------------------------------------------------------------

fn emit_opencode(ir: &PluginIR, dir: &Path) -> Result<()> {
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
    let agents_md = render_agents_md(ir);
    fs::write(dir.join("AGENTS.md"), agents_md)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Codex emitter
// ---------------------------------------------------------------------------

fn emit_codex(ir: &PluginIR, dir: &Path) -> Result<()> {
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
        fs::create_dir_all(&skills_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill);
            fs::write(skills_dir.join(format!("{}.md", skill.name)), content)?;
        }
    }

    // AGENTS.md — instructions and agent descriptions
    let agents_md = render_agents_md(ir);
    fs::write(dir.join("AGENTS.md"), agents_md)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Cursor emitter (minimal — rules + MCP)
// ---------------------------------------------------------------------------

fn emit_cursor(ir: &PluginIR, dir: &Path) -> Result<()> {
    // .cursorrules — instructions
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions);
        fs::write(dir.join(".cursorrules"), content)?;
    }

    // .cursor/mcp.json — MCP config
    if !ir.mcp_servers.is_empty() {
        let cursor_dir = dir.join(".cursor");
        fs::create_dir_all(&cursor_dir)?;
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(&cursor_dir, "mcp.json", &mcp_config)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenClaw emitter (minimal — npm package + instructions)
// ---------------------------------------------------------------------------

fn emit_openclaw(ir: &PluginIR, dir: &Path) -> Result<()> {
    // package.json — npm-based distribution
    let package_json = serde_json::json!({
        "name": ir.manifest.name,
        "version": ir.manifest.version,
        "description": ir.manifest.description,
        "openclaw": {
            "extensions": {}
        }
    });
    write_json(dir, "package.json", &package_json)?;

    // Instructions as README
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions);
        fs::write(dir.join("README.md"), content)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// Render a skill definition as a .md file with YAML frontmatter.
fn render_skill_md(skill: &SkillDef) -> String {
    let mut frontmatter = BTreeMap::new();

    if let Some(desc) = &skill.frontmatter.description {
        frontmatter.insert("description", serde_yaml::to_value(desc).unwrap());
    }
    if let Some(hint) = &skill.frontmatter.argument_hint {
        frontmatter.insert("argument-hint", serde_yaml::to_value(hint).unwrap());
    }
    if let Some(tools) = &skill.frontmatter.allowed_tools {
        frontmatter.insert("allowed-tools", serde_yaml::to_value(tools).unwrap());
    }
    if let Some(color) = &skill.frontmatter.color {
        frontmatter.insert("color", serde_yaml::to_value(color).unwrap());
    }
    if let Some(examples) = &skill.frontmatter.examples {
        frontmatter.insert("examples", serde_yaml::to_value(examples).unwrap());
    }

    // Include extra fields
    for (k, v) in &skill.frontmatter.extra {
        frontmatter.insert(k.as_str(), v.clone());
    }

    if frontmatter.is_empty() {
        return skill.body.clone();
    }

    let yaml = serde_yaml::to_string(&frontmatter).unwrap();
    format!("---\n{}---\n\n{}", yaml, skill.body)
}

/// Render an agent definition as a .md file with YAML frontmatter.
fn render_agent_md(agent: &AgentDef) -> String {
    let mut frontmatter = BTreeMap::new();

    if let Some(desc) = &agent.frontmatter.description {
        frontmatter.insert("description", serde_yaml::to_value(desc).unwrap());
    }
    if let Some(model) = &agent.frontmatter.model {
        frontmatter.insert("model", serde_yaml::to_value(model).unwrap());
    }
    if let Some(tools) = &agent.frontmatter.allowed_tools {
        frontmatter.insert("allowed-tools", serde_yaml::to_value(tools).unwrap());
    }
    if let Some(color) = &agent.frontmatter.color {
        frontmatter.insert("color", serde_yaml::to_value(color).unwrap());
    }

    for (k, v) in &agent.frontmatter.extra {
        frontmatter.insert(k.as_str(), v.clone());
    }

    if frontmatter.is_empty() {
        return agent.body.clone();
    }

    let yaml = serde_yaml::to_string(&frontmatter).unwrap();
    format!("---\n{}---\n\n{}", yaml, agent.body)
}

/// Render MCP servers as a JSON config.
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
                serde_json::to_value(&server.args).unwrap(),
            );
        }
        if !server.env.is_empty() {
            entry.insert(
                "env".to_string(),
                serde_json::to_value(&server.env).unwrap(),
            );
        }
        mcp_servers.insert(server.name.clone(), serde_json::Value::Object(entry));
    }
    serde_json::json!({ "mcpServers": mcp_servers })
}

/// Render instructions as a combined document.
fn render_instructions(instructions: &[InstructionDef]) -> String {
    instructions
        .iter()
        .map(|i| i.body.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render AGENTS.md — used by OpenCode and Codex.
/// Combines instructions, skill descriptions, and agent descriptions.
fn render_agents_md(ir: &PluginIR) -> String {
    let mut sections = Vec::new();

    // Instructions first
    if !ir.instructions.is_empty() {
        sections.push(render_instructions(&ir.instructions));
    }

    // Skills as documented commands
    if !ir.skills.is_empty() {
        let mut skill_section = String::from("## Available Commands\n\n");
        for skill in &ir.skills {
            skill_section.push_str(&format!("### {}\n\n", skill.name));
            if let Some(desc) = &skill.frontmatter.description {
                skill_section.push_str(&format!("{desc}\n\n"));
            }
            skill_section.push_str(&skill.body);
            skill_section.push('\n');
        }
        sections.push(skill_section);
    }

    // Agents as documented sub-agents
    if !ir.agents.is_empty() {
        let mut agent_section = String::from("## Available Agents\n\n");
        for agent in &ir.agents {
            agent_section.push_str(&format!("### {}\n\n", agent.name));
            if let Some(desc) = &agent.frontmatter.description {
                agent_section.push_str(&format!("{desc}\n\n"));
            }
            agent_section.push_str(&agent.body);
            agent_section.push('\n');
        }
        sections.push(agent_section);
    }

    sections.join("\n---\n\n")
}

/// Write a JSON value to a file with pretty formatting.
fn write_json(dir: &Path, filename: &str, value: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_string_pretty(value).unwrap();
    fs::write(dir.join(filename), content)?;
    Ok(())
}
