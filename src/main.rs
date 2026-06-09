mod ecosystem;
mod hibernate;
mod rehydrate;
mod scanner;

use clap::{Parser, Subcommand};
use hibernate::hibernate_project;
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
            
            if stale_projects.is_empty() {
                println!("No stale projects found to hibernate.");
            } else {
                for proj in stale_projects {
                    if let Err(e) = hibernate_project(&proj, *dry_run, *max_depth) {
                        eprintln!("Error hibernating {}: {}", proj.display(), e);
                    }
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
