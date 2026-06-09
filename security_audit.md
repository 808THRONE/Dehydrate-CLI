# Security Audit Report for Dehydrate

I have performed a deep security audit of the Dehydrate repository. The tool generally employs several good security practices (e.g. hardcoded RCE whitelists, symlink checks), but there are a few critical security vulnerabilities that must be addressed.

## 1. Path Traversal & Arbitrary File Deletion via Component Symlinks (CRITICAL)

### Description
The vulnerability lies in how Dehydrate identifies and deletes heavy dependency directories. In `src/ecosystem.rs`, the `analyze_node` function adds `.yarn/cache` to the list of `targets_to_delete`:

```rust
let heavy_dirs = ["node_modules", "dist", ".next", "build", ".yarn/cache"];
```

In `src/hibernate.rs`, the `execute_hibernation` function attempts to prevent symlink traversal by checking if the target itself is a symlink:

```rust
if let Ok(metadata) = fs::symlink_metadata(target) {
    if metadata.file_type().is_symlink() {
        eprintln!("  Security Alert: {} is a symlink. Skipping deletion...", target.display());
        continue;
    }
}
```

However, `fs::symlink_metadata` only checks if the *last component* of the path is a symlink. If an attacker controls the `.yarn` component and makes it a symlink pointing to an arbitrary system directory (e.g. `/home/user/.ssh`), the `.yarn/cache` target will resolve to `/home/user/.ssh/cache`. Since the *last component* (`cache`) is typically a normal directory (not a symlink), `fs::symlink_metadata` will return `false` for `is_symlink()`.

Subsequently, Dehydrate will forcefully execute `fs::remove_dir_all(target)`, completely deleting the target directory without restriction.

### Proof of Concept
If an attacker provides a repository with a `.yarn` directory symlinked to `/usr/local/share` (assuming permission allows), when Dehydrate hibernates this repository, it will recursively delete `/usr/local/share/cache`.

### Recommendation
To prevent this, Dehydrate must verify that *no component* of the entire path is a symlink before performing deletion. A helper function can iterate through all path ancestors to ensure none are symlinks, up to the project root directory.

## 2. TOCTOU Race Condition in File Deletion

### Description
There is a documented Time-Of-Check to Time-Of-Use (TOCTOU) vulnerability in `src/hibernate.rs`. The code checks if a path is a symlink and then proceeds to delete it using `fs::remove_dir_all`. Between the time the path is checked and the time it is deleted, an attacker could replace the directory with a symlink, potentially leading to arbitrary file deletion.

### Recommendation
While the code comments acknowledge this as out-of-scope, more robust file deletion logic using OS-specific directory file descriptors (e.g. `openat` and `unlinkat` on Unix) would fully mitigate this attack vector.
