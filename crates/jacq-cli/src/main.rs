use std::process;

use clap::Parser;

use jacq_core::analyzer::{self, Severity};
use jacq_core::emitter;
use jacq_core::parser;
use jacq_core::targets::Target;
use jacq_core::template;

mod cli;

fn main() {
    let cli = cli::Cli::parse();

    let result = match cli.command {
        cli::Command::Init {
            name,
            from,
            targets,
        } => cmd_init(name.as_deref(), from.as_deref(), targets),
        cli::Command::Validate { path, target } => cmd_validate(&path, target),
        cli::Command::Build {
            path,
            target,
            strict,
            output,
        } => cmd_build(&path, target, strict, output.as_deref()),
        cli::Command::Test { path, target, .. } => cmd_validate(&path, target),
        cli::Command::Inspect { path } => cmd_inspect(&path),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn cmd_init(
    name: Option<&str>,
    from: Option<&std::path::Path>,
    targets_override: Option<Vec<Target>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve target directory and plugin name. Two roles, formerly conflated:
    // - `dir`: where files get written
    // - `plugin_name`: what goes in plugin.yaml's `name:` field
    // With no NAME, operate on cwd and derive the plugin name from its basename.
    let (dir, plugin_name): (std::path::PathBuf, String) = match name {
        Some(n) => {
            let dir = std::path::PathBuf::from(n);
            // Plugin name is the basename of the path, matching the prior
            // behavior (e.g. `init foo/bar/my-plugin` → name: my-plugin).
            let plugin_name = dir
                .file_name()
                .ok_or_else(|| format!("cannot derive plugin name from '{n}'"))?
                .to_string_lossy()
                .into_owned();
            (dir, plugin_name)
        }
        None => {
            let cwd = std::env::current_dir()?;
            let basename = cwd
                .file_name()
                .ok_or("cannot derive plugin name from current directory")?
                .to_string_lossy()
                .into_owned();
            (std::path::PathBuf::from("."), basename)
        }
    };

    // Mode-specific safety checks:
    // - `--from` import: target must be absent or empty (excluding `.git`)
    // - bare scaffold: refuse only if a plugin.yaml already exists
    if from.is_some() {
        if dir.exists() {
            let mut entries = std::fs::read_dir(&dir)?
                .filter_map(Result::ok)
                .filter(|e| e.file_name() != ".git");
            if entries.next().is_some() {
                return Err(format!(
                    "target directory '{}' is not empty; --from refuses to mix imported \
                     content with existing files",
                    dir.display()
                )
                .into());
            }
        }
    } else if dir.join("plugin.yaml").exists() {
        return Err(format!(
            "'{}' already contains a plugin.yaml",
            dir.display()
        )
        .into());
    }

    if let Some(source) = from {
        // Import existing plugin (any harness layout)
        let ir = parser::parse_plugin(source)?;
        std::fs::create_dir_all(&dir)?;

        // Write IR manifest. Targets policy:
        // - --targets given: use it verbatim (user is explicit)
        // - else if source has detectable target wrappers: use those
        // - else fall back to whatever the manifest declared
        // - else default to [claude-code]
        let mut manifest = ir.manifest.clone();
        manifest.ir_version = Some("0.1".to_string());
        if let Some(t) = targets_override {
            manifest.targets = t;
        } else {
            let detected = parser::detect_targets(source);
            if !detected.is_empty() {
                manifest.targets = detected;
            } else if manifest.targets.is_empty() {
                manifest.targets = vec![Target::ClaudeCode];
            }
        }

        let yaml = serde_yaml::to_string(&manifest)?;
        std::fs::write(dir.join("plugin.yaml"), yaml)?;

        // Copy all source components. Each component preserves its source_path
        // relative to the original plugin root, so we recreate the same layout.
        let copy_component =
            |rel_path: &std::path::Path, name: &str| -> Result<(), Box<dyn std::error::Error>> {
                let src = ir.source_dir.join(rel_path);
                let dst = dir.join(rel_path);
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src, &dst).map_err(|e| {
                    format!(
                        "failed to copy {name} '{}' from {}: {e}",
                        rel_path.display(),
                        src.display()
                    )
                })?;
                Ok(())
            };

        for skill in &ir.skills {
            copy_component(&skill.source_path, "skill")?;
        }
        for agent in &ir.agents {
            copy_component(&agent.source_path, "agent")?;
        }
        for instr in &ir.instructions {
            copy_component(&instr.source_path, "instruction")?;
        }
        for fragment in &ir.shared {
            copy_component(&fragment.source_path, "shared fragment")?;
        }
        for hook in &ir.hooks {
            copy_component(&hook.source_path, "hook")?;
        }
        for mcp in &ir.mcp_servers {
            copy_component(&mcp.source_path, "mcp server")?;
        }

        // Copy upstream LICENSE file (if present) to preserve legal provenance.
        // This is essential when redistributing derived content from upstream plugins.
        for license_name in ["LICENSE", "LICENSE.md", "LICENSE.txt", "COPYING"] {
            let src_license = ir.source_dir.join(license_name);
            if src_license.exists() {
                let dst_license = dir.join(license_name);
                std::fs::copy(&src_license, &dst_license)?;
                break;
            }
        }

        let target_list = manifest
            .targets
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Imported from {} → {}/", source.display(), dir.display());
        println!("  plugin.yaml created with ir_version: 0.1");
        println!("  targets: [{target_list}]");
        println!(
            "  {} skill(s), {} agent(s), {} hook(s), {} MCP, {} instruction(s), {} shared",
            ir.skills.len(),
            ir.agents.len(),
            ir.hooks.len(),
            ir.mcp_servers.len(),
            ir.instructions.len(),
            ir.shared.len(),
        );
        println!("\nNext: run `jacq build` to materialize wrappers for each target");
    } else {
        // Scaffold a new plugin (or add a manifest to an existing repo)
        std::fs::create_dir_all(dir.join("skills"))?;
        std::fs::create_dir_all(dir.join("instructions"))?;

        let targets = targets_override.unwrap_or_else(|| vec![Target::ClaudeCode]);
        let target_list = targets
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let manifest = format!(
            r#"ir_version: "0.1"
targets: [{target_list}]
name: {plugin_name}
version: "0.1.0"
description: ""
author: ""
license: "MIT"
"#
        );
        std::fs::write(dir.join("plugin.yaml"), manifest)?;

        // Don't clobber example files that the user (or upstream) already wrote.
        let example_skill_path = dir.join("skills").join("example.md");
        let wrote_example_skill = !example_skill_path.exists();
        if wrote_example_skill {
            let example_skill = r#"---
description: Example skill
argument-hint: [describe what to do]
---

You are a helpful assistant. The user's request: $ARGUMENTS
"#;
            std::fs::write(&example_skill_path, example_skill)?;
        }

        let rules_path = dir.join("instructions").join("rules.md");
        let wrote_rules = !rules_path.exists();
        if wrote_rules {
            std::fs::write(&rules_path, "# Rules\n\nAdd your instructions here.\n")?;
        }

        println!("Created {}/", dir.display());
        println!("  plugin.yaml  (targets: [{target_list}], name: {plugin_name})");
        if wrote_example_skill {
            println!("  skills/example.md");
        }
        if wrote_rules {
            println!("  instructions/rules.md");
        }
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
    let shared_info = if ir.shared.is_empty() {
        String::new()
    } else {
        format!(", {} shared fragment(s)", ir.shared.len())
    };
    println!(
        "Parsed '{}' v{} ({} skill(s), {} agent(s), {} hook(s), {} MCP server(s){})",
        ir.manifest.name,
        ir.manifest.version,
        ir.skills.len(),
        ir.agents.len(),
        ir.hooks.len(),
        ir.mcp_servers.len(),
        shared_info,
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
        println!(
            "  [{}] [{}] {}",
            diag.severity.label(),
            diag.target,
            diag.message
        );
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
        return Err(
            "no targets declared in plugin manifest. Add targets to plugin.yaml or use --target"
                .into(),
        );
    }

    // If we filled targets via parser inference (and the user didn't override
    // with --target), surface that decision so it's not invisible.
    if ir.targets_inferred && target.is_none() {
        let names: Vec<&str> = ir.manifest.targets.iter().map(|t| t.as_str()).collect();
        eprintln!(
            "  note: inferred targets [{}] from compatibility probe — \
             declare `targets:` in plugin.yaml to override",
            names.join(", ")
        );
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

    if let Some(output_dir) = output {
        // Isolated mode: full per-target trees under output_dir/<target>/
        std::fs::create_dir_all(output_dir)?;
        emitter::emit(&ir, output_dir)?;
        for t in &ir.manifest.targets {
            println!("  Built: {}/{}", output_dir.display(), t);
        }
    } else {
        // In-place mode: target wrappers at the source repo root. Components
        // are not re-emitted; the source repo is the install target.
        emitter::emit_in_place(&ir, path)?;
        for t in &ir.manifest.targets {
            println!("  Built (in-place) for {t}");
        }
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
            ir.skills
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !ir.agents.is_empty() {
        println!(
            "  Agents:       {}  ({})",
            ir.agents.len(),
            ir.agents
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !ir.hooks.is_empty() {
        println!("  Hooks:        {}", ir.hooks.len());
    }
    if !ir.mcp_servers.is_empty() {
        println!(
            "  MCP servers:  {}  ({})",
            ir.mcp_servers.len(),
            ir.mcp_servers
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !ir.shared.is_empty() {
        println!(
            "  Shared:       {}  ({})",
            ir.shared.len(),
            ir.shared
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !ir.instructions.is_empty() {
        println!("  Instructions: {}", ir.instructions.len());
    }

    if ir.manifest.targets.is_empty() {
        println!("\nNo targets declared.");
        return Ok(());
    }

    let provenance = if ir.targets_inferred {
        " (inferred from compatibility probe)"
    } else {
        ""
    };
    println!(
        "\nTargets: {}{provenance}",
        ir.manifest
            .targets
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let report = analyzer::analyze(&ir);
    println!();

    use jacq_core::targets::{CAPABILITY_KEYS, SupportLevel, capability_matrix};

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
        let status = if summary.compatible() {
            "Compatible"
        } else {
            "Incompatible"
        };
        println!(
            "  {target_name}: {status} ({} error(s), {} warning(s))",
            summary.error_count, summary.warning_count
        );
    }

    if !report.diagnostics.is_empty() {
        println!();
        for diag in &report.diagnostics {
            println!(
                "  [{}] [{}] {}",
                diag.severity.label(),
                diag.target,
                diag.message
            );
        }
    }

    Ok(())
}
