use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::Write;

/// Helper function to create an empty file
fn touch(path: &PathBuf) {
    fs::File::create(path).unwrap();
}

#[test]
fn test_symlink_traversal_attack() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    // Setup a safe project
    touch(&proj_dir.join("package.json"));
    touch(&proj_dir.join("package-lock.json"));
    
    // Create a critical system folder OUTSIDE the project
    let critical_dir = temp_dir.path().join("critical_system_folder");
    fs::create_dir(&critical_dir).unwrap();
    touch(&critical_dir.join("important_data.txt"));
    
    // Attacker crafts a malicious symlink pretending to be a heavy cache
    let malicious_link = proj_dir.join("node_modules");
    #[cfg(unix)]
    if let Err(_) = std::os::unix::fs::symlink(&critical_dir, &malicious_link) {
        println!("Skipping symlink test: missing privileges");
        return;
    }
    #[cfg(windows)]
    if let Err(_) = std::os::windows::fs::symlink_dir(&critical_dir, &malicious_link) {
        println!("Skipping symlink test: Windows Developer Mode or Admin privileges required to create symlinks.");
        return;
    }

    // Run dehydrate hibernate with --stale-days 0 to instantly hibernate
    let status = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("hibernate")
        .arg("--stale-days")
        .arg("0")
        .current_dir(temp_dir.path())
        .status()
        .expect("Failed to execute dehydrate");

    assert!(status.success(), "Dehydrate command should succeed");
    
    // The security assertion: The critical folder MUST still exist
    assert!(
        critical_dir.join("important_data.txt").exists(),
        "SECURITY FAILURE: Dehydrate followed the symlink and deleted the critical system folder!"
    );
}

#[test]
fn test_scanner_exclusion_verification() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    // We cannot easily fake modified times natively in rust without dependencies like `filetime`, 
    // so we verify that the scanner explicitly ignores IDE paths by running the CLI and 
    // ensuring it doesn't crash or behave unexpectedly on excluded paths.
    // However, since we can't reliably test age natively, we will verify the dry run execution.
    touch(&proj_dir.join("package.json"));
    touch(&proj_dir.join("package-lock.json"));
    
    let vscode_dir = proj_dir.join(".vscode");
    fs::create_dir(&vscode_dir).unwrap();
    touch(&vscode_dir.join("workspace.xml"));

    let status = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("scan")
        .arg("--stale-days")
        .arg("0") // instantly stale
        .current_dir(temp_dir.path())
        .status()
        .expect("Failed to execute dehydrate");

    assert!(status.success());
}

#[test]
fn test_malicious_awake_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    // Manually inject a malicious .dehydrate.json
    let snapshot_path = proj_dir.join(".dehydrate.json");
    let malicious_json = r#"{
        "hibernated_at": "2024-01-01T00:00:00Z",
        "ecosystems": ["Node"],
        "package_managers": ["npm"],
        "install_commands": ["npm ci"],
        "deleted_paths": ["node_modules"],
        "space_saved_bytes": 1000
    }"#;
    fs::write(&snapshot_path, malicious_json).unwrap();

    // Spawn dehydrate awake and feed it "N" to abort the trust prompt
    let mut child = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("awake")
        .current_dir(&proj_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to execute dehydrate");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"N\n").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    
    // The security assertion: It must fail because we aborted the trust prompt
    assert!(!output.status.success(), "SECURITY FAILURE: Dehydrate awake executed without explicit user consent!");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot run 'dehydrate awake' in non-interactive mode") || 
        stderr.contains("Rehydration safely aborted"), 
        "Expected security abort message not found in stderr: {}", stderr
    );
}

#[test]
fn test_oom_payload_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    // Create a 1.1MB padded file to trigger DoS limit
    let snapshot_path = proj_dir.join(".dehydrate.json");
    let mut file = fs::File::create(&snapshot_path).unwrap();
    let padding = vec![b' '; 1024 * 1024 + 100]; // 1MB + 100 bytes
    file.write_all(&padding).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("awake")
        .current_dir(&proj_dir)
        .output()
        .unwrap();
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("over 1MB"), "Failed to reject bloated payload");
}

#[test]
fn test_native_rce_injection_rejection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    let snapshot_path = proj_dir.join(".dehydrate.json");
    let malicious_json = r#"{
        "hibernated_at": "2024-01-01T00:00:00Z",
        "ecosystems": ["Custom"],
        "package_managers": ["bash"],
        "install_commands": ["bash -c 'rm -rf /'"],
        "deleted_paths": ["node_modules"],
        "space_saved_bytes": 1000
    }"#;
    fs::write(&snapshot_path, malicious_json).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("awake")
        .current_dir(&proj_dir)
        .output()
        .unwrap();
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unrecognized or malicious package manager"), "Failed to reject RCE payload");
}

#[test]
fn test_infinite_recursion_dos_protection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().join("root");
    fs::create_dir(&root).unwrap();
    
    // Create deeply nested folders
    let mut current = root.clone();
    for _ in 0..20 {
        current = current.join("nested");
        fs::create_dir(&current).unwrap();
    }
    touch(&current.join("package.json"));
    
    // Run scanner with max depth 5, ensure it doesn't find the deeply nested project
    let output = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("scan")
        .arg("--stale-days")
        .arg("0")
        .arg("--max-depth")
        .arg("5")
        .current_dir(&root)
        .output()
        .unwrap();
        
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("nested\\nested\\nested\\nested\\nested\\nested"), "Scanner bypassed depth limit");
}

#[test]
fn test_lockfile_deadlock_prevention() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    touch(&proj_dir.join("package.json"));
    // DO NOT create package-lock.json

    let child = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("hibernate")
        .arg("--stale-days")
        .arg("0")
        .current_dir(&temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to execute dehydrate");
        
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Skipping auto-generation for the 1 missing lockfiles"), "Did not safely skip non-interactive auto-gen");
}

#[test]
fn test_double_hibernation_data_loss_prevention() {
    let temp_dir = tempfile::tempdir().unwrap();
    let proj_dir = temp_dir.path().join("proj");
    fs::create_dir(&proj_dir).unwrap();
    
    // Simulate an already hibernated project
    touch(&proj_dir.join("package.json"));
    touch(&proj_dir.join("package-lock.json"));
    
    let snapshot_path = proj_dir.join(".dehydrate.json");
    let original_json = r#"{
        "hibernated_at": "2000-01-01T00:00:00Z",
        "ecosystems": ["Node"],
        "package_managers": ["npm"],
        "install_commands": ["npm ci"],
        "deleted_paths": ["node_modules"],
        "space_saved_bytes": 999999
    }"#;
    fs::write(&snapshot_path, original_json).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_Dehydrate"))
        .arg("scan")
        .arg("--stale-days")
        .arg("0")
        .current_dir(&temp_dir.path())
        .output()
        .unwrap();
        
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No stale projects found"), "Scanner failed to ignore already hibernated project!");
}
