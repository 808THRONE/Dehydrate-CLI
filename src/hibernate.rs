use crate::ecosystem::{analyze_project, MissingLockfileError};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Debug)]
pub struct DehydrateSnapshot {
    pub hibernated_at: String,
    pub ecosystems: Vec<String>,
    pub package_managers: Vec<String>,
    pub install_commands: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub space_saved_bytes: u64,
}

/// Executes the hibernation process for a single project directory.
pub fn hibernate_project(project_dir: &Path, dry_run: bool, max_depth: usize) -> Result<()> {
    // 1. Analyze the project (this automatically enforces the lockfile safety rule)
    let metadata = match analyze_project(project_dir) {
        Ok(m) => m,
        Err(e) => {
            if let Some(missing_lock) = e.downcast_ref::<MissingLockfileError>() {
                if dry_run {
                    println!("  [DRY RUN] Would prompt to auto-generate lockfile using: `{}`", missing_lock.generation_command);
                    return Ok(());
                }
                
                use std::io::IsTerminal;
                if !std::io::stdin().is_terminal() {
                    return Err(e).context(format!("Missing lockfile at {}, and cannot auto-generate in non-interactive mode.", project_dir.display()));
                }
                
                println!("  [!] Warning: {} is missing a lockfile.", project_dir.display());
                println!("      Safe rehydration cannot be guaranteed without one.");
                println!("      Would you like Dehydrate to auto-generate it now using `{}`? (Y/n)", missing_lock.generation_command);
                
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                
                if input.trim().to_lowercase() == "y" || input.trim() == "" {
                    println!("  Generating lockfile...");
                    let parts: Vec<&str> = missing_lock.generation_command.split_whitespace().collect();
                    
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
                        println!("  Lockfile generated successfully! Retrying hibernation...");
                        return hibernate_project(project_dir, dry_run, max_depth);
                    } else {
                        anyhow::bail!("Failed to generate lockfile.");
                    }
                } else {
                    println!("  Skipping project.");
                    return Ok(());
                }
            } else {
                return Err(e).context(format!("Failed to analyze project at {}", project_dir.display()));
            }
        }
    };

    // 2. Identify heavy folders to delete and calculate their size
    let mut deleted_paths = Vec::new();
    let mut total_saved_bytes = 0;

    for target in &metadata.targets_to_delete {
        if target.exists() {
            let size = get_dir_size(target, max_depth);
            if size > 0 {
                total_saved_bytes += size;
                deleted_paths.push(target.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }

    if deleted_paths.is_empty() {
        println!("  Skipping: No heavy dependency folders found in {}", project_dir.display());
        return Ok(());
    }

    // 3. Create the JSON snapshot representation
    let snapshot = DehydrateSnapshot {
        hibernated_at: Utc::now().to_rfc3339(),
        ecosystems: metadata.ecosystems.iter().map(|e| format!("{:?}", e)).collect(),
        package_managers: metadata.package_managers,
        install_commands: metadata.install_commands,
        deleted_paths: deleted_paths.clone(),
        space_saved_bytes: total_saved_bytes,
    };

    if dry_run {
        println!("  [DRY RUN] Would hibernate: {}", project_dir.display());
        println!("  [DRY RUN] Would save: {} MB", total_saved_bytes / 1024 / 1024);
        println!("  [DRY RUN] Would delete: {:?}", deleted_paths);
        return Ok(());
    }

    // 4. Save the snapshot file to disk
    let snapshot_path = project_dir.join(".dehydrate.json");
    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(&snapshot_path, json)
        .with_context(|| format!("Failed to write snapshot to {}", snapshot_path.display()))?;

    // 5. Safely delete the heavy dependency folders
    for target in &metadata.targets_to_delete {
        if target.exists() {
            // SECURITY: Explicitly check for symlinks to prevent path traversal / arbitrary deletion attacks
            if let Ok(metadata) = fs::symlink_metadata(target) {
                if metadata.file_type().is_symlink() {
                    eprintln!("  Security Alert: {} is a symlink. Skipping deletion to prevent arbitrary file destruction.", target.display());
                    continue;
                }
            }

            if let Err(e) = remove_dir_all_force(target) {
                // If deletion fails completely, we log it but don't abort
                eprintln!("  Warning: Failed to delete {}: {}", target.display(), e);
            }
        }
    }

    println!("  Hibernated: {} (Saved {} MB)", project_dir.display(), total_saved_bytes / 1024 / 1024);

    Ok(())
}

/// Quickly calculates the total size of a directory in bytes
fn get_dir_size(path: &Path, max_depth: usize) -> u64 {
    WalkDir::new(path)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

/// Fallback mechanism for Windows read-only file deletion failures
fn remove_dir_all_force(path: &Path) -> Result<()> {
    if let Err(e) = fs::remove_dir_all(path) {
        #[cfg(target_os = "windows")]
        {
            // Try to forcefully remove read-only flags
            WalkDir::new(path).into_iter().filter_map(|e| e.ok()).for_each(|entry| {
                if let Ok(mut perms) = entry.metadata().map(|m| m.permissions()) {
                    if perms.readonly() {
                        perms.set_readonly(false);
                        let _ = fs::set_permissions(entry.path(), perms);
                    }
                }
            });
            fs::remove_dir_all(path).with_context(|| format!("Failed to forcefully delete {}", path.display()))?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            anyhow::bail!("Failed to delete {}: {}", path.display(), e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_successful_hibernation_node() -> Result<()> {
        let dir = tempdir()?;
        let project_dir = dir.path();

        // 1. Setup fake project
        File::create(project_dir.join("package.json"))?;
        File::create(project_dir.join("package-lock.json"))?;
        
        let nm = project_dir.join("node_modules");
        fs::create_dir(&nm)?;
        {
            let mut fake_file = File::create(nm.join("fake_dep.js"))?;
            use std::io::Write;
            fake_file.write_all(b"fake data")?;
        }

        // 2. Action
        hibernate_project(project_dir, false, 100)?;

        // 3. Assertions
        assert!(!nm.exists(), "node_modules should be deleted");
        
        let snapshot_path = project_dir.join(".dehydrate.json");
        assert!(snapshot_path.exists(), ".dehydrate.json should be created");
        
        let json = fs::read_to_string(snapshot_path)?;
        let snapshot: DehydrateSnapshot = serde_json::from_str(&json)?;
        assert_eq!(snapshot.package_managers[0], "npm");
        assert_eq!(snapshot.install_commands[0], "npm ci");

        Ok(())
    }

    #[test]
    fn test_safety_guard_missing_lockfile() -> Result<()> {
        let dir = tempdir()?;
        let project_dir = dir.path();

        // Setup: No lockfile
        File::create(project_dir.join("package.json"))?;
        
        let nm = project_dir.join("node_modules");
        fs::create_dir(&nm)?;
        let mut fake_file = File::create(nm.join("fake_dep.js"))?;
        use std::io::Write;
        fake_file.write_all(b"fake data")?;

        // Action: Should error
        let result = hibernate_project(project_dir, false, 100);
        assert!(result.is_err(), "Hibernation should fail without a lockfile");

        // Assertion: Heavy folder MUST still exist
        assert!(nm.exists(), "node_modules must NOT be deleted if lockfile is missing");

        Ok(())
    }
}
