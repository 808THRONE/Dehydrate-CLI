use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::fmt;

#[derive(Debug)]
pub struct MissingLockfileError {
    pub ecosystem: String,
    pub generation_command: String,
}

impl fmt::Display for MissingLockfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Missing strict lockfile. Auto-generation command: {}", self.generation_command)
    }
}

impl std::error::Error for MissingLockfileError {}

#[derive(Debug, PartialEq, Eq)]
pub enum EcosystemType {
    Node,
    Rust,
    Python,
}

pub struct PolyglotMetadata {
    pub ecosystems: Vec<EcosystemType>,
    pub package_managers: Vec<String>,
    pub install_commands: Vec<String>,
    pub targets_to_delete: Vec<PathBuf>,
}

struct SingleMetadata {
    ecosystem: EcosystemType,
    package_manager: String,
    install_command: String,
    targets_to_delete: Vec<PathBuf>,
}

pub fn analyze_project(project_dir: &Path) -> Result<PolyglotMetadata> {
    let mut metadata = PolyglotMetadata {
        ecosystems: vec![],
        package_managers: vec![],
        install_commands: vec![],
        targets_to_delete: vec![],
    };

    if project_dir.join("package.json").exists() {
        let m = analyze_node(project_dir)?;
        metadata.ecosystems.push(m.ecosystem);
        metadata.package_managers.push(m.package_manager);
        metadata.install_commands.push(m.install_command);
        metadata.targets_to_delete.extend(m.targets_to_delete);
    }
    
    if project_dir.join("Cargo.toml").exists() {
        let m = analyze_rust(project_dir)?;
        metadata.ecosystems.push(m.ecosystem);
        metadata.package_managers.push(m.package_manager);
        metadata.install_commands.push(m.install_command);
        metadata.targets_to_delete.extend(m.targets_to_delete);
    }
    
    if project_dir.join("requirements.txt").exists() || project_dir.join("pyproject.toml").exists() {
        let m = analyze_python(project_dir)?;
        metadata.ecosystems.push(m.ecosystem);
        metadata.package_managers.push(m.package_manager);
        metadata.install_commands.push(m.install_command);
        metadata.targets_to_delete.extend(m.targets_to_delete);
    }

    if metadata.ecosystems.is_empty() {
        bail!("Unknown ecosystem for directory: {}", project_dir.display());
    }
    
    Ok(metadata)
}

fn analyze_node(project_dir: &Path) -> Result<SingleMetadata> {
    let mut targets = vec![];
    
    // Check for common heavy directories
    let heavy_dirs = ["node_modules", "dist", ".next", "build"];
    for dir in heavy_dirs {
        let p = project_dir.join(dir);
        if p.exists() {
            targets.push(p);
        }
    }

    // Safety net: Must have a lockfile
    let (pm, cmd) = if project_dir.join("pnpm-lock.yaml").exists() {
        ("pnpm", "pnpm install")
    } else if project_dir.join("yarn.lock").exists() {
        ("yarn", "yarn install")
    } else if project_dir.join("package-lock.json").exists() {
        ("npm", "npm ci")
    } else if project_dir.join("bun.lockb").exists() {
        ("bun", "bun install")
    } else {
        return Err(anyhow::Error::new(MissingLockfileError {
            ecosystem: "Node.js".to_string(),
            generation_command: "npm install --package-lock-only".to_string(),
        }));
    };

    Ok(SingleMetadata {
        ecosystem: EcosystemType::Node,
        package_manager: pm.to_string(),
        install_command: cmd.to_string(),
        targets_to_delete: targets,
    })
}

fn analyze_rust(project_dir: &Path) -> Result<SingleMetadata> {
    let mut targets = vec![];
    let target_dir = project_dir.join("target");
    if target_dir.exists() {
        targets.push(target_dir);
    }

    // Safety net: Must have Cargo.lock
    if !project_dir.join("Cargo.lock").exists() {
        return Err(anyhow::Error::new(MissingLockfileError {
            ecosystem: "Rust".to_string(),
            generation_command: "cargo generate-lockfile".to_string(),
        }));
    }

    Ok(SingleMetadata {
        ecosystem: EcosystemType::Rust,
        package_manager: "cargo".to_string(),
        install_command: "cargo fetch".to_string(),
        targets_to_delete: targets,
    })
}

fn analyze_python(project_dir: &Path) -> Result<SingleMetadata> {
    let mut targets = vec![];
    let heavy_dirs = ["venv", ".venv", "__pycache__", ".pytest_cache"];
    for dir in heavy_dirs {
        let p = project_dir.join(dir);
        if p.exists() {
            targets.push(p);
        }
    }

    let pm = if project_dir.join("poetry.lock").exists() {
        "poetry"
    } else if project_dir.join("Pipfile.lock").exists() {
        "pipenv"
    } else if project_dir.join("requirements.txt").exists() {
        // pip usually doesn't have a strict lockfile except pinned requirements.txt
        "pip" 
    } else if project_dir.join("pyproject.toml").exists() {
        return Err(anyhow::Error::new(MissingLockfileError {
            ecosystem: "Python (Poetry)".to_string(),
            generation_command: "poetry lock".to_string(),
        }));
    } else {
        bail!("No lockfile or requirements found for Python project at {}. Cannot safely hibernate.", project_dir.display());
    };

    let cmd = match pm {
        "poetry" => "poetry install",
        "pipenv" => "pipenv install",
        "pip" => "pip install -r requirements.txt",
        _ => unreachable!(),
    };

    Ok(SingleMetadata {
        ecosystem: EcosystemType::Python,
        package_manager: pm.to_string(),
        install_command: cmd.to_string(),
        targets_to_delete: targets,
    })
}
