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

    // 2. Interactive Trust Prompt
    println!("\nThis project requests to run the following rehydration commands:");
    for pm in &snapshot.package_managers {
        if pm.as_str() == "pip" {
            let venv_name = snapshot.deleted_paths.iter()
                .find(|p| *p == "venv" || *p == ".venv")
                .map(|s| s.as_str())
                .unwrap_or("venv");
            println!("  - python -m venv {}", venv_name);
            let local_pip = if cfg!(target_os = "windows") { format!("{}\\Scripts\\pip", venv_name) } else { format!("{}/bin/pip", venv_name) };
            println!("  - {} install -r requirements.txt", local_pip);
            continue;
        }

        let (bin, args): (&str, &[&str]) = match pm.as_str() {
            "npm" => ("npm", &["ci"]),
            "yarn" => ("yarn", &["install"]),
            "pnpm" => ("pnpm", &["install"]),
            "bun" => ("bun", &["install"]),
            "cargo" => ("cargo", &["fetch"]),
            "poetry" => ("poetry", &["install"]),
            "pipenv" => ("pipenv", &["install"]),
            _ => bail!("Security Error: Unrecognized or malicious package manager '{}'", pm),
        };
        let mut full_cmd = vec![bin];
        full_cmd.extend_from_slice(args);
        println!("  - {}", full_cmd.join(" "));
    }

    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        bail!("Security Alert: Cannot run 'dehydrate awake' in non-interactive mode without explicit user consent.");
    }

    println!("\nDo you trust this project environment? (Y/n)");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" && input.trim() != "" {
        bail!("Rehydration safely aborted by user.");
    }

    println!("Rehydrating {:?} project...", snapshot.ecosystems);

    for pm in &snapshot.package_managers {
        // 3. Strict Whitelist & Command Reconstruction
        let mut local_pip_path = String::new();
        let (bin, args): (&str, Vec<&str>) = match pm.as_str() {
            "npm" => ("npm", vec!["ci"]),
            "yarn" => ("yarn", vec!["install"]),
            "pnpm" => ("pnpm", vec!["install"]),
            "bun" => ("bun", vec!["install"]),
            "cargo" => ("cargo", vec!["fetch"]),
            "poetry" => ("poetry", vec!["install"]),
            "pipenv" => ("pipenv", vec!["install"]),
            "pip" => {
                let venv_name = snapshot.deleted_paths.iter()
                    .find(|p| *p == "venv" || *p == ".venv")
                    .map(|s| s.as_str())
                    .unwrap_or("venv");

                println!("Rebuilding Python virtual environment: {}...", venv_name);
                let venv_status = Command::new("python")
                    .arg("-m")
                    .arg("venv")
                    .arg(venv_name)
                    .current_dir(project_dir)
                    .status()
                    .with_context(|| "Failed to execute 'python -m venv'")?;
                
                if !venv_status.success() {
                    bail!("Failed to recreate Python virtual environment.");
                }

                if cfg!(target_os = "windows") {
                    local_pip_path = project_dir.join(venv_name).join("Scripts").join("pip").to_string_lossy().to_string();
                } else {
                    local_pip_path = project_dir.join(venv_name).join("bin").join("pip").to_string_lossy().to_string();
                }
                
                // We borrow the string reference here to pass back safely
                (local_pip_path.as_str(), vec!["install", "-r", "requirements.txt"])
            },
            _ => bail!("Security Error: Unrecognized package manager '{}'", pm),
        };

        let safe_bin = get_safe_bin(bin);

        // 4. Dependency Check
        let check_output = Command::new(&safe_bin).arg("--version").current_dir(project_dir).output();
        if check_output.is_err() || !check_output.unwrap().status.success() {
            bail!(
                "Missing dependency: Please install '{}' to rehydrate this project.",
                bin
            );
        }

        // 5. Execute Install Command
        println!("Running: {} {:?}", bin, args);
        
        let mut cmd = Command::new(&safe_bin);
        for arg in args {
            cmd.arg(arg);
        }
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

fn get_safe_bin(bin: &str) -> String {
    if cfg!(target_os = "windows") && ["npm", "yarn", "pnpm", "npx", "bun"].contains(&bin) {
        format!("{}.cmd", bin)
    } else {
        bin.to_string()
    }
}
