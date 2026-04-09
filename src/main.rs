use std::process;

use clap::Parser;

use jacq::analyzer::{self, Severity};
use jacq::cli;
use jacq::emitter;
use jacq::parser;
use jacq::targets::Target;
use jacq::template;

fn main() {
    let cli = cli::Cli::parse();

    let result = match cli.command {
        cli::Command::Init { name, from } => cmd_init(&name, from.as_deref()),
        cli::Command::Validate { path, target } => cmd_validate(&path, target),
        cli::Command::Build {
            path,
            target,
            strict,
            output,
        } => cmd_build(&path, target, strict, output.as_deref()),
        cli::Command::Test { path, target, .. } => cmd_validate(&path, target),
        cli::Command::Inspect { path } => cmd_inspect(&path),
        cli::Command::Pack { .. } => {
            Err("jacq pack is not yet implemented".into())
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn cmd_init(name: &str, from: Option<&std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::Path::new(name);
    if dir.exists() {
        return Err(format!("directory '{name}' already exists").into());
    }

    if let Some(source) = from {
        // Import existing Claude Code plugin
        let ir = parser::parse_plugin(source)?;
        std::fs::create_dir_all(dir)?;

        // Write IR manifest
        let mut manifest = ir.manifest.clone();
        manifest.ir_version = Some("0.1".to_string());
        if manifest.targets.is_empty() {
            manifest.targets = vec![Target::ClaudeCode];
        }

        let yaml = serde_yaml::to_string(&manifest)?;
        std::fs::write(dir.join("plugin.yaml"), yaml)?;

        // Copy skills — use ir.source_dir (canonicalized) for path consistency
        if !ir.skills.is_empty() {
            let skills_dir = dir.join("skills");
            std::fs::create_dir_all(&skills_dir)?;
            for skill in &ir.skills {
                let skill_path = ir.source_dir.join(&skill.source_path);
                let content = std::fs::read_to_string(&skill_path).map_err(|e| {
                    format!(
                        "failed to read skill '{}' from {}: {e}",
                        skill.name,
                        skill_path.display()
                    )
                })?;
                std::fs::write(skills_dir.join(format!("{}.md", skill.name)), content)?;
            }
        }

        println!("Imported from {} → {name}/", source.display());
        println!("  plugin.yaml created with ir_version: 0.1");
        println!("  {} skill(s) imported", ir.skills.len());
        println!("\nNext: edit plugin.yaml to add targets and run `jacq build`");
    } else {
        // Scaffold a new plugin
        std::fs::create_dir_all(dir.join("skills"))?;
        std::fs::create_dir_all(dir.join("instructions"))?;

        let plugin_name = std::path::Path::new(name)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let manifest = format!(
            r#"ir_version: "0.1"
targets: [claude-code]
name: {plugin_name}
version: "0.1.0"
description: ""
author: ""
license: "MIT"
"#
        );
        std::fs::write(dir.join("plugin.yaml"), manifest)?;

        let example_skill = r#"---
description: Example skill
argument-hint: [describe what to do]
---

You are a helpful assistant. The user's request: $ARGUMENTS
"#;
        std::fs::write(dir.join("skills").join("example.md"), example_skill)?;

        std::fs::write(
            dir.join("instructions").join("rules.md"),
            "# Rules\n\nAdd your instructions here.\n",
        )?;

        println!("Created {name}/");
        println!("  plugin.yaml");
        println!("  skills/example.md");
        println!("  instructions/rules.md");
        println!("\nNext: edit plugin.yaml and run `jacq build`");
    }

    Ok(())
}

fn cmd_validate(
    path: &std::path::Path,
    target: Option<Target>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ir = parser::parse_plugin(path)?;
    template::extract_all(&mut ir);

    // Validate template variables
    let template_errors = template::validate(&ir);
    if !template_errors.is_empty() {
        for err in &template_errors {
            eprintln!("  [ERROR] {err}");
        }
        return Err(format!("{} template error(s)", template_errors.len()).into());
    }
    println!(
        "Parsed '{}' v{} ({} skill(s), {} agent(s), {} hook(s), {} MCP server(s))",
        ir.manifest.name,
        ir.manifest.version,
        ir.skills.len(),
        ir.agents.len(),
        ir.hooks.len(),
        ir.mcp_servers.len(),
    );

    let report = analyzer::analyze(&ir);

    if report.diagnostics.is_empty() {
        if ir.manifest.targets.is_empty() {
            println!("No targets declared — nothing to analyze.");
        } else {
            println!("All targets compatible.");
        }
        return Ok(());
    }

    let mut has_errors = false;
    for diag in &report.diagnostics {
        if let Some(t) = target
            && diag.target != t
        {
            continue;
        }
        if diag.severity == Severity::Error {
            has_errors = true;
        }
        println!("  [{}] [{}] {}", diag.severity.label(), diag.target, diag.message);
    }

    for (target_name, summary) in &report.target_summaries {
        if let Some(t) = target
            && *target_name != t
        {
            continue;
        }
        let status = if summary.compatible() { "OK" } else { "FAIL" };
        println!(
            "  {target_name}: {status} ({} error(s), {} warning(s))",
            summary.error_count, summary.warning_count
        );
    }

    if has_errors {
        Err("validation failed with errors".into())
    } else {
        Ok(())
    }
}

fn cmd_build(
    path: &std::path::Path,
    target: Option<Target>,
    strict: bool,
    output: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ir = parser::parse_plugin(path)?;
    template::extract_all(&mut ir);

    let template_errors = template::validate(&ir);
    if !template_errors.is_empty() {
        for err in &template_errors {
            eprintln!("  [ERROR] {err}");
        }
        return Err(format!("{} template error(s)", template_errors.len()).into());
    }

    if let Some(t) = target {
        ir.manifest.targets = vec![t];
    }

    if ir.manifest.targets.is_empty() {
        return Err("no targets declared in plugin manifest. Add targets to plugin.yaml or use --target".into());
    }

    let report = analyzer::analyze(&ir);
    let has_errors = report.errors().count() > 0;

    for diag in &report.diagnostics {
        let prefix = if strict && diag.severity == Severity::Warning {
            "ERROR"
        } else {
            diag.severity.label()
        };
        eprintln!("  [{prefix}] [{}] {}", diag.target, diag.message);
    }

    if has_errors || (strict && report.warnings().count() > 0) {
        return Err("build failed due to capability errors".into());
    }

    let output_dir = output.unwrap_or(std::path::Path::new("dist"));
    std::fs::create_dir_all(output_dir)?;

    emitter::emit(&ir, output_dir)?;

    for t in &ir.manifest.targets {
        println!("  Built: {}/{}", output_dir.display(), t);
    }

    Ok(())
}

fn cmd_inspect(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let ir = parser::parse_plugin(path)?;

    println!("Plugin: {} v{}", ir.manifest.name, ir.manifest.version);
    println!("  {}", ir.manifest.description);
    println!();

    println!("Content:");
    if !ir.skills.is_empty() {
        println!(
            "  Skills:       {}  ({})",
            ir.skills.len(),
            ir.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    if !ir.agents.is_empty() {
        println!(
            "  Agents:       {}  ({})",
            ir.agents.len(),
            ir.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    if !ir.hooks.is_empty() {
        println!("  Hooks:        {}", ir.hooks.len());
    }
    if !ir.mcp_servers.is_empty() {
        println!(
            "  MCP servers:  {}  ({})",
            ir.mcp_servers.len(),
            ir.mcp_servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    if !ir.instructions.is_empty() {
        println!("  Instructions: {}", ir.instructions.len());
    }

    if ir.manifest.targets.is_empty() {
        println!("\nNo targets declared.");
        return Ok(());
    }

    println!("\nTargets: {}", ir.manifest.targets.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "));

    let report = analyzer::analyze(&ir);
    println!();

    use jacq::targets::{capability_matrix, SupportLevel, CAPABILITY_KEYS};

    println!("Capability Matrix:");
    print!("  {:24}", "");
    for t in &ir.manifest.targets {
        print!("{:>14}", t.as_str());
    }
    println!();

    for key in CAPABILITY_KEYS {
        print!("  {key:24}");
        for t in &ir.manifest.targets {
            let matrix = capability_matrix(*t);
            let level = matrix.get(*key).unwrap_or(&SupportLevel::None);
            let symbol = match level {
                SupportLevel::Full => "Full",
                SupportLevel::Partial => "Partial",
                SupportLevel::Flags => "Flags",
                SupportLevel::None => "None",
            };
            print!("{symbol:>14}");
        }
        println!();
    }

    println!();
    for (target_name, summary) in &report.target_summaries {
        let status = if summary.compatible() { "Compatible" } else { "Incompatible" };
        println!(
            "  {target_name}: {status} ({} error(s), {} warning(s))",
            summary.error_count, summary.warning_count
        );
    }

    if !report.diagnostics.is_empty() {
        println!();
        for diag in &report.diagnostics {
            println!("  [{}] [{}] {}", diag.severity.label(), diag.target, diag.message);
        }
    }

    Ok(())
}
