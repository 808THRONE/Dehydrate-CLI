use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Scanner {
    stale_days: u32,
    max_depth: usize,
}

impl Scanner {
    pub fn new(stale_days: u32, max_depth: usize) -> Self {
        Self { stale_days, max_depth }
    }

    /// Recursively scans the given root directory to find stale project directories.
    pub fn scan(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut stale_projects = Vec::new();
        let mut seen_dirs = HashSet::new();

        // Build a walker that ignores common junk directories to drastically speed up traversal
        let mut builder = WalkBuilder::new(root);
        builder.max_depth(Some(self.max_depth)).hidden(true).git_ignore(true).filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != "node_modules" && name != "target" && name != "venv" && name != ".venv" 
            && name != ".git" && name != ".idea" && name != ".vscode" && name != ".DS_Store"
        });

        let walker = builder.build();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let file_name = entry.file_name().to_string_lossy();
                if is_project_marker(&file_name) {
                    if let Some(project_dir) = entry.path().parent() {
                        let project_dir_buf = project_dir.to_path_buf();
                        // Prevent scanning the same directory twice if it has multiple markers
                        if seen_dirs.insert(project_dir_buf.clone()) {
                            if self.is_stale(project_dir)? {
                                stale_projects.push(project_dir_buf);
                            }
                        }
                    }
                }
            }
        }

        Ok(stale_projects)
    }

    /// Determines if a project is stale by checking the modified date of its source code.
    fn is_stale(&self, project_dir: &Path) -> Result<bool> {
        let mut newest_time = SystemTime::UNIX_EPOCH;

        let mut builder = WalkBuilder::new(project_dir);
        // We also ignore junk folders when checking modified dates so that
        // an `npm install` doesn't make a project look "recently active" if no code was changed.
        builder.max_depth(Some(self.max_depth)).hidden(true).git_ignore(true).filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != "node_modules" && name != "target" && name != "venv" && name != ".venv" 
            && name != ".git" && name != ".idea" && name != ".vscode" && name != ".DS_Store"
        });

        let walker = builder.build();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified > newest_time {
                            newest_time = modified;
                        }
                    }
                }
            }
        }

        let now = SystemTime::now();
        if let Ok(duration) = now.duration_since(newest_time) {
            let days = duration.as_secs() / (60 * 60 * 24);
            Ok(days >= self.stale_days as u64)
        } else {
            // If the time calculation fails (e.g., system time went backward), assume it's not stale
            Ok(false)
        }
    }
}

fn is_project_marker(name: &str) -> bool {
    matches!(
        name,
        "package.json" | "Cargo.toml" | "requirements.txt" | "pyproject.toml"
    )
}
