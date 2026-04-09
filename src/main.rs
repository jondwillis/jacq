use clap::Parser;

use jacq::cli;

fn main() {
    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Init { name, from } => {
            println!("jacq init: scaffolding plugin '{name}'");
            if let Some(path) = from {
                println!("  importing from: {}", path.display());
            }
            println!("  (not yet implemented)");
        }
        cli::Command::Validate { path, target } => {
            println!("jacq validate: {}", path.display());
            if let Some(t) = target {
                println!("  target: {t}");
            }
            println!("  (not yet implemented)");
        }
        cli::Command::Build {
            path,
            target,
            strict,
            output,
        } => {
            println!("jacq build: {}", path.display());
            if let Some(t) = target {
                println!("  target: {t}");
            }
            if strict {
                println!("  mode: strict (no fallbacks)");
            }
            if let Some(out) = output {
                println!("  output: {}", out.display());
            }
            println!("  (not yet implemented)");
        }
        cli::Command::Test { path, target, live } => {
            println!("jacq test: {}", path.display());
            if let Some(t) = target {
                println!("  target: {t}");
            }
            if live {
                println!("  mode: live smoke-test");
            }
            println!("  (not yet implemented)");
        }
        cli::Command::Inspect { path } => {
            println!("jacq inspect: {}", path.display());
            println!("  (not yet implemented)");
        }
        cli::Command::Pack { path, output } => {
            println!("jacq pack: {}", path.display());
            if let Some(out) = output {
                println!("  output: {}", out.display());
            }
            println!("  (not yet implemented)");
        }
    }
}
