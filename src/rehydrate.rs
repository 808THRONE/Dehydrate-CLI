use crate::hibernate::DehydrateSnapshot;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn rehydrate_project(project_dir: &Path) -> Result<()> {
    let snapshot_path = project_dir.join(".dehydrate.json");
    if !snapshot_path.exists() {
        bail!("No .dehydrate.json found in {}. Project is not hibernated.", project_dir.display());
    }

    // 1. Read and parse snapshot
    // SECURITY: Limit snapshot size to 1MB to prevent Out-Of-Memory (OOM) DoS attacks
    let metadata = fs::metadata(&snapshot_path)
        .with_context(|| format!("Failed to read metadata for {}", snapshot_path.display()))?;
    if metadata.len() > 1024 * 1024 {
        bail!("Security Error: .dehydrate.json is over 1MB. Aborting to prevent memory exhaustion.");
    }

    let json = fs::read_to_string(&snapshot_path)
        .with_context(|| format!("Failed to read {}", snapshot_path.display()))?;
    let snapshot: DehydrateSnapshot = serde_json::from_str(&json)
        .with_context(|| "Failed to parse .dehydrate.json")?;

    println!("Rehydrating {:?} project...", snapshot.ecosystems);

    for pm in &snapshot.package_managers {
        // 2. Strict Whitelist & Command Reconstruction (Security Fix)
        // We DO NOT trust the install_command from the JSON file to prevent Remote Code Execution (RCE).
        let (bin, args): (&str, &[&str]) = match pm.as_str() {
            "npm" => ("npm", &["ci"]),
            "yarn" => ("yarn", &["install"]),
            "pnpm" => ("pnpm", &["install"]),
            "bun" => ("bun", &["install"]),
            "cargo" => ("cargo", &["fetch"]),
            "poetry" => ("poetry", &["install"]),
            "pipenv" => ("pipenv", &["install"]),
            "pip" => ("pip", &["install", "-r", "requirements.txt"]),
            _ => bail!("Security Error: Unrecognized or malicious package manager '{}'", pm),
        };

        // 3. Dependency Check
        let check_cmd = format!("{} --version", bin);
        let check_output = build_command(&check_cmd).current_dir(project_dir).output();
        if check_output.is_err() || !check_output.unwrap().status.success() {
            bail!(
                "Missing dependency: Please install '{}' to rehydrate this project.",
                bin
            );
        }

        // 4. Execute Install Command
        println!("Running: {} {:?}", bin, args);
        
        let mut cmd = build_safe_command(bin, args);
        cmd.current_dir(project_dir);
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let status = cmd.status().with_context(|| format!("Failed to spawn {}", bin))?;

        if !status.success() {
            bail!("Rehydration failed. The command '{}' exited with an error.", bin);
        }
    }

    // 5. Cleanup
    fs::remove_file(&snapshot_path).with_context(|| "Failed to delete .dehydrate.json after successful rehydration")?;

    println!("Success! Project rehydrated.");

    Ok(())
}

/// Builds a shell command to safely execute package managers cross-platform.
/// This prevents "NotFound" errors on Windows for commands like "npm" which are actually "npm.cmd"
#[cfg(target_os = "windows")]
fn build_command(command_str: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(command_str);
    cmd
}

#[cfg(target_os = "windows")]
fn build_safe_command(bin: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(bin);
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}

#[cfg(not(target_os = "windows"))]
fn build_command(command_str: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command_str);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn build_safe_command(bin: &str, args: &[&str]) -> Command {
    // For non-windows, we bypass the shell entirely for the main execution for maximum safety
    let mut cmd = Command::new(bin);
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}
