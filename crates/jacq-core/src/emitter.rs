//! Target emitters — generate platform-specific plugin output.
//!
//! Each target gets its own subdirectory under the output path.
//! The emitter for a target produces all files that target's plugin system expects.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::{JacqError, Result};
use crate::ir::*;
use crate::targets::{self, FieldSupport, Target};
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
// Shared manifest builder — consults field_matrix for target-specific fields
// ---------------------------------------------------------------------------

/// Build a plugin manifest JSON object with only the fields this target supports.
fn build_manifest_json(manifest: &PluginManifest, target: Target) -> serde_json::Value {
    let fields = targets::field_matrix(target);
    let has = |key: &str| fields.get(key) == Some(&FieldSupport::Yes);

    let mut obj = serde_json::Map::new();

    // Core identity
    if has("name") {
        obj.insert("name".into(), serde_json::json!(manifest.name));
    }
    if has("version") && manifest.version != "0.0.0" {
        obj.insert("version".into(), serde_json::json!(manifest.version));
    }
    if has("description") && !manifest.description.is_empty() {
        obj.insert(
            "description".into(),
            serde_json::json!(manifest.description),
        );
    }
    if has("author") {
        let author_val = match &manifest.author {
            Author::Name(n) if !n.is_empty() => Some(serde_json::json!({"name": n})),
            Author::Structured { name, email, url } => {
                let mut m = serde_json::Map::new();
                m.insert("name".into(), serde_json::json!(name));
                if let Some(e) = email {
                    m.insert("email".into(), serde_json::json!(e));
                }
                if let Some(u) = url {
                    m.insert("url".into(), serde_json::json!(u));
                }
                Some(serde_json::Value::Object(m))
            }
            _ => None,
        };
        if let Some(v) = author_val {
            obj.insert("author".into(), v);
        }
    }
    if has("license")
        && let Some(v) = &manifest.license
    {
        obj.insert("license".into(), serde_json::json!(v));
    }
    if has("keywords") && !manifest.keywords.is_empty() {
        obj.insert("keywords".into(), serde_json::json!(manifest.keywords));
    }

    // URLs
    if has("homepage")
        && let Some(v) = &manifest.homepage
    {
        obj.insert("homepage".into(), serde_json::json!(v));
    }
    if has("repository")
        && let Some(v) = &manifest.repository
    {
        obj.insert("repository".into(), serde_json::json!(v));
    }

    // Cursor-specific
    if has("displayName")
        && let Some(v) = &manifest.display_name
    {
        obj.insert("displayName".into(), serde_json::json!(v));
    }
    if has("logo")
        && let Some(v) = &manifest.logo
    {
        obj.insert("logo".into(), serde_json::json!(v));
    }

    // Codex-specific
    if has("apps")
        && let Some(v) = &manifest.apps
    {
        obj.insert("apps".into(), serde_json::json!(v));
    }
    if has("interface")
        && let Some(v) = &manifest.interface
    {
        obj.insert("interface".into(), v.clone());
    }

    // OpenClaw-specific
    if has("id")
        && let Some(v) = &manifest.id
    {
        obj.insert("id".into(), serde_json::json!(v));
    }
    if has("configSchema")
        && let Some(v) = &manifest.config_schema
    {
        obj.insert("configSchema".into(), v.clone());
    }
    if has("providers")
        && let Some(v) = &manifest.providers
    {
        obj.insert("providers".into(), serde_json::json!(v));
    }

    serde_json::Value::Object(obj)
}

// ---------------------------------------------------------------------------
// Claude Code emitter — identity/passthrough
// ---------------------------------------------------------------------------

fn emit_claude_code(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    let plugin_json = build_manifest_json(&ir.manifest, Target::ClaudeCode);
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
            let content = serde_yaml::to_string(&hook).map_err(|e| JacqError::Serialization {
                reason: e.to_string(),
            })?;
            write_file(&hooks_dir.join(format!("{}.yaml", hook.name)), &content)?;
        }
    }

    // .mcp.json — MCP server configuration
    if !ir.mcp_servers.is_empty() {
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(dir, ".mcp.json", &mcp_config)?;
    }

    // .lsp.json — LSP server configuration
    if !ir.lsp_servers.is_empty() {
        let lsp_config = render_lsp_json(&ir.lsp_servers);
        write_json(dir, ".lsp.json", &lsp_config)?;
    }

    // CLAUDE.md — instructions
    if !ir.instructions.is_empty() {
        let content = render_instructions(&ir.instructions, engine)?;
        write_file(&dir.join("CLAUDE.md"), &content)?;
    }

    // AGENTS.md — portability signal for non-Claude tools that read the dir
    // (OpenAI Codex, Cursor, Aider, pi, etc.). Skills are intentionally
    // excluded: they live in commands/*.md natively.
    let agents_md = render_agents_md(ir, false, engine)?;
    if !agents_md.is_empty() {
        write_file(&dir.join("AGENTS.md"), &agents_md)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenCode emitter
// ---------------------------------------------------------------------------

fn emit_opencode(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    let package_json = build_manifest_json(&ir.manifest, Target::OpenCode);
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
    let plugin_json = build_manifest_json(&ir.manifest, Target::Codex);
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
    // .cursor-plugin/plugin.json
    let cursor_plugin_dir = dir.join(".cursor-plugin");
    create_dir(&cursor_plugin_dir)?;
    let plugin_json = build_manifest_json(&ir.manifest, Target::Cursor);
    write_json(&cursor_plugin_dir, "plugin.json", &plugin_json)?;

    // commands/*.md — skills with frontmatter
    if !ir.skills.is_empty() {
        let commands_dir = dir.join("commands");
        create_dir(&commands_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill, engine)?;
            write_file(&commands_dir.join(format!("{}.md", skill.name)), &content)?;
        }
    }

    // agents/*.md
    if !ir.agents.is_empty() {
        let agents_dir = dir.join("agents");
        create_dir(&agents_dir)?;
        for agent in &ir.agents {
            let content = render_agent_md(agent, engine)?;
            write_file(&agents_dir.join(format!("{}.md", agent.name)), &content)?;
        }
    }

    // mcp.json — MCP server configuration
    if !ir.mcp_servers.is_empty() {
        let mcp_config = render_mcp_json(&ir.mcp_servers);
        write_json(dir, "mcp.json", &mcp_config)?;
    }

    // rules/*.mdc — instructions as rules
    if !ir.instructions.is_empty() {
        let rules_dir = dir.join("rules");
        create_dir(&rules_dir)?;
        for instr in &ir.instructions {
            let content = engine.render(&instr.body)?;
            write_file(&rules_dir.join(format!("{}.mdc", instr.name)), &content)?;
        }
    }

    // AGENTS.md — same portability signal as Claude Code (skills are in
    // commands/*.md natively, so include_skills=false).
    let agents_md = render_agents_md(ir, false, engine)?;
    if !agents_md.is_empty() {
        write_file(&dir.join("AGENTS.md"), &agents_md)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// OpenClaw emitter (minimal — npm package + instructions)
// ---------------------------------------------------------------------------

fn emit_openclaw(ir: &PluginIR, engine: &RenderEngine, dir: &Path) -> Result<()> {
    // openclaw.plugin.json — native OpenClaw manifest
    let manifest_json = build_manifest_json(&ir.manifest, Target::OpenClaw);
    write_json(dir, "openclaw.plugin.json", &manifest_json)?;

    // package.json — npm package
    let mut pkg = serde_json::Map::new();
    pkg.insert("name".into(), serde_json::json!(ir.manifest.name));
    if ir.manifest.version != "0.0.0" {
        pkg.insert("version".into(), serde_json::json!(ir.manifest.version));
    }
    if !ir.manifest.description.is_empty() {
        pkg.insert(
            "description".into(),
            serde_json::json!(ir.manifest.description),
        );
    }
    write_json(dir, "package.json", &serde_json::Value::Object(pkg))?;

    // skills/*.md
    if !ir.skills.is_empty() {
        let skills_dir = dir.join("skills");
        create_dir(&skills_dir)?;
        for skill in &ir.skills {
            let content = render_skill_md(skill, engine)?;
            write_file(&skills_dir.join(format!("{}.md", skill.name)), &content)?;
        }
    }

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
    let fm = &skill.frontmatter;
    let mut out = BTreeMap::new();

    if let Some(v) = &fm.name {
        out.insert("name", yaml_value(v)?);
    }
    if let Some(v) = &fm.description {
        out.insert("description", yaml_value(v)?);
    }
    if let Some(v) = &fm.argument_hint {
        out.insert("argument-hint", yaml_value(v)?);
    }
    if let Some(v) = &fm.allowed_tools {
        out.insert("allowed-tools", yaml_value(v)?);
    }
    if let Some(v) = &fm.tools {
        out.insert("tools", yaml_value(v)?);
    }
    if let Some(v) = &fm.color {
        out.insert("color", yaml_value(v)?);
    }
    if let Some(v) = &fm.examples {
        out.insert("examples", yaml_value(v)?);
    }
    if let Some(v) = &fm.user_invocable {
        out.insert("user-invocable", yaml_value(v)?);
    }
    if let Some(v) = &fm.hide_from_slash_command_tool {
        out.insert("hide-from-slash-command-tool", yaml_value(v)?);
    }
    if let Some(v) = &fm.disable_model_invocation {
        out.insert("disable-model-invocation", yaml_value(v)?);
    }
    if let Some(v) = &fm.version {
        out.insert("version", yaml_value(v)?);
    }
    if let Some(v) = &fm.license {
        out.insert("license", yaml_value(v)?);
    }

    let rendered_body = engine.render(&skill.body)?;
    wrap_frontmatter(out, &rendered_body)
}

fn render_agent_md(agent: &AgentDef, engine: &RenderEngine) -> Result<String> {
    let fm = &agent.frontmatter;
    let mut out = BTreeMap::new();

    if let Some(v) = &fm.name {
        out.insert("name", yaml_value(v)?);
    }
    if let Some(v) = &fm.description {
        out.insert("description", yaml_value(v)?);
    }
    if let Some(v) = &fm.model {
        out.insert("model", yaml_value(v)?);
    }
    if let Some(v) = &fm.effort {
        out.insert("effort", yaml_value(v)?);
    }
    if let Some(v) = &fm.max_turns {
        out.insert("maxTurns", yaml_value(v)?);
    }
    if let Some(v) = &fm.tools {
        out.insert("tools", yaml_value(v)?);
    }
    if let Some(v) = &fm.disallowed_tools {
        out.insert("disallowedTools", yaml_value(v)?);
    }
    if let Some(v) = &fm.skills {
        out.insert("skills", yaml_value(v)?);
    }
    if let Some(v) = &fm.memory {
        out.insert("memory", yaml_value(v)?);
    }
    if let Some(v) = &fm.background {
        out.insert("background", yaml_value(v)?);
    }
    if let Some(v) = &fm.isolation {
        out.insert("isolation", yaml_value(v)?);
    }
    if let Some(v) = &fm.readonly {
        out.insert("readonly", yaml_value(v)?);
    }
    if let Some(v) = &fm.color {
        out.insert("color", yaml_value(v)?);
    }

    let rendered_body = engine.render(&agent.body)?;
    wrap_frontmatter(out, &rendered_body)
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

/// Render `.lsp.json` — keyed by server name, mirroring `.mcp.json`'s shape.
/// LspServerDef already carries serde renames (`extensionToLanguage`,
/// `initializationOptions`, etc.), so we serialize each def whole and lift
/// `name` out into the map key.
fn render_lsp_json(servers: &[LspServerDef]) -> serde_json::Value {
    let mut lsp_servers = serde_json::Map::new();
    for server in servers {
        let Ok(serde_json::Value::Object(mut entry)) = serde_json::to_value(server) else {
            continue;
        };
        entry.remove("name");
        lsp_servers.insert(server.name.clone(), serde_json::Value::Object(entry));
    }
    serde_json::json!({ "lspServers": lsp_servers })
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
