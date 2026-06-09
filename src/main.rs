mod ecosystem;
mod hibernate;
mod rehydrate;
mod scanner;

use clap::{Parser, Subcommand};
use std::path::Path;
use rehydrate::rehydrate_project;
use scanner::Scanner;
use std::env;

#[derive(Parser)]
#[command(author, version, about = "The Smart Project Hibernator", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan directories to find stale projects
    Scan {
        /// Number of days a project must be untouched to be considered stale
        #[arg(short, long, default_value_t = 60)]
        stale_days: u32,
        /// Maximum directory depth to scan (prevents infinite traversal)
        #[arg(long, default_value_t = 100)]
        max_depth: usize,
    },
    /// Hibernate stale projects to save disk space
    Hibernate {
        /// Do a dry run without actually deleting any files
        #[arg(long)]
        dry_run: bool,
        /// Maximum directory depth to scan
        #[arg(long, default_value_t = 100)]
        max_depth: usize,
    },
    /// Rehydrate a hibernated project
    Awake {},
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Scan { stale_days, max_depth } => {
            println!("Scanning for projects inactive for {} days...", stale_days);
            
            let current_dir = env::current_dir()?;
            let scanner = Scanner::new(*stale_days, *max_depth);
            let stale_projects = scanner.scan(&current_dir)?;
            
            if stale_projects.is_empty() {
                println!("No stale projects found.");
            } else {
                println!("Found {} stale projects:", stale_projects.len());
                for proj in stale_projects {
                    println!("- {}", proj.display());
                }
            }
        }
        Commands::Hibernate { dry_run, max_depth } => {
            if *dry_run {
                println!("Running hibernation in DRY RUN mode (no files will be deleted).");
            } else {
                println!("Hibernating stale projects...");
            }
            
            let current_dir = env::current_dir()?;
            // We use the scanner to find projects to hibernate
            let scanner = Scanner::new(60, *max_depth);
            let stale_projects = scanner.scan(&current_dir)?;
            
            let mut ready_projects = Vec::new();
            let mut missing_lockfiles = Vec::new();
            let mut errors = Vec::new();

            for proj in stale_projects {
                match crate::ecosystem::analyze_project(&proj) {
                    Ok(metadata) => {
                        ready_projects.push((proj, metadata));
                    }
                    Err(e) => {
                        if let Some(missing_lock) = e.downcast_ref::<crate::ecosystem::MissingLockfileError>() {
                            missing_lockfiles.push((proj, missing_lock.generation_command.clone()));
                        } else {
                            errors.push((proj, e));
                        }
                    }
                }
            }

            let total = ready_projects.len() + missing_lockfiles.len();
            if total == 0 {
                println!("No stale projects found that can be processed.");
                return Ok(());
            }

            println!("\nFound {} stale projects:", total);
            println!("  🟢 {} projects are safe and ready to hibernate.", ready_projects.len());
            if !missing_lockfiles.is_empty() {
                println!("  🟡 {} projects are missing lockfiles.", missing_lockfiles.len());
            }
            if !errors.is_empty() {
                println!("  🔴 {} projects failed analysis due to unsupported configurations.", errors.len());
            }

            if !missing_lockfiles.is_empty() {
                use std::io::IsTerminal;
                if !std::io::stdin().is_terminal() || *dry_run {
                    println!("\nNon-interactive mode or dry-run. Skipping auto-generation for the {} missing lockfiles.", missing_lockfiles.len());
                } else {
                    println!("\nDehydrate is ready to hibernate the {} safe projects.", ready_projects.len());
                    println!("How would you like to handle the {} missing lockfiles?", missing_lockfiles.len());
                    println!("  [A] Auto-generate all {} lockfiles and include them", missing_lockfiles.len());
                    println!("  [L] List them one-by-one to decide individually");
                    println!("  [N] No, skip the missing ones");
                    
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let choice = input.trim().to_uppercase();

                    if choice == "A" {
                        for (proj, cmd_str) in missing_lockfiles {
                            println!("Generating lockfile for {}...", proj.display());
                            if generate_lockfile(&proj, &cmd_str).is_ok() {
                                if let Ok(metadata) = crate::ecosystem::analyze_project(&proj) {
                                    ready_projects.push((proj, metadata));
                                }
                            }
                        }
                    } else if choice == "L" {
                        for (proj, cmd_str) in missing_lockfiles {
                            println!("Auto-generate lockfile for '{}'? (Y/n)", proj.display());
                            let mut inner_input = String::new();
                            std::io::stdin().read_line(&mut inner_input)?;
                            if inner_input.trim().to_lowercase() == "y" || inner_input.trim() == "" {
                                println!("Generating...");
                                if generate_lockfile(&proj, &cmd_str).is_ok() {
                                    if let Ok(metadata) = crate::ecosystem::analyze_project(&proj) {
                                        ready_projects.push((proj, metadata));
                                    }
                                }
                            }
                        }
                    } else {
                        println!("Skipping missing lockfiles.");
                    }
                }
            }

            if ready_projects.is_empty() {
                println!("Nothing to hibernate.");
                return Ok(());
            }

            println!("\nHibernating {} projects...", ready_projects.len());
            for (proj, metadata) in ready_projects {
                if let Err(e) = crate::hibernate::execute_hibernation(&proj, metadata, *dry_run, *max_depth) {
                    eprintln!("Error hibernating {}: {}", proj.display(), e);
                }
            }
        }
        Commands::Awake {} => {
            let current_dir = env::current_dir()?;
            if let Err(e) = rehydrate_project(&current_dir) {
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(())
}

fn generate_lockfile(project_dir: &Path, command: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(parts[0]);
        for arg in &parts[1..] { c.arg(arg); }
        c
    } else {
        let mut c = std::process::Command::new(parts[0]);
        for arg in &parts[1..] { c.arg(arg); }
        c
    };
    
    let status = cmd.current_dir(project_dir).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Failed to generate lockfile.")
    }
}
