# Dehydrate

Dehydrate is a high-performance, cross-platform CLI tool built in Rust that helps you reclaim massive amounts of disk space by safely "hibernating" inactive software projects.

If you have dozens of old projects sitting around, you know how quickly `node_modules` and Rust `target` folders can eat up hundreds of gigabytes of storage. Dehydrate intelligently scans your hard drive for stale projects, creates a tiny JSON "DNA" snapshot, and nukes the heavy dependencies. When you're ready to work on the project again months later, a single command brings it all back.

## Features

- **Blazing Fast Scanning:** Uses highly optimized, multi-threaded traversal (via the `ignore` crate) to find stale projects while skipping over junk folders instantly.
- **Polyglot Support:** Got a project with a Rust backend and a React frontend in the same folder? Dehydrate automatically detects both, deletes both `target` and `node_modules`, and rehydrates both in sequence.
- **Zero-Trust Security:** Absolutely refuses to execute arbitrary strings. Rehydration commands are safely reconstructed internally using a strict package manager whitelist to prevent Remote Code Execution (RCE) vulnerabilities.
- **Advanced Bulk UX & Auto-Lockfiles:** Dehydrate strictly requires lockfiles to ensure 100% deterministic re-builds. If you scan 50 projects and 10 are missing lockfiles, Dehydrate won't annoy you with 10 sequential prompts. Instead, it presents a single, unified interactive bulk menu allowing you to instantly auto-generate the 10 lockfiles and seamlessly hibernate all 50 projects in one continuous sweep!
- **Path Traversal Protection:** Explicitly checks for malicious symlinks trying to disguise system folders as dependency caches.

## Installation

Ensure you have Rust and Cargo installed, then clone the repository and run:

```bash
cargo install --path .
```

This will compile the binary and add `dehydrate` to your system's PATH.

## Usage

### 1. See what's eating your space
Find all projects that haven't had their source code touched in 60 days:
```bash
dehydrate scan --stale-days 60
```

### 2. Safely Hibernate
Run a dry run first to see exactly how much space you would save:
```bash
dehydrate hibernate --dry-run
```
When you're ready to pull the trigger:
```bash
dehydrate hibernate --stale-days 60
```

### 3. Bring it back to life
Six months later, when you want to work on a hibernated project, just `cd` into the directory and type:
```bash
dehydrate awake
```
Dehydrate will read the `.dehydrate.json` snapshot, verify your system tools, and stream the fresh dependency installation right to your terminal.

## Supported Ecosystems

* **Node.js** (`npm`, `yarn`, `pnpm`, `bun`)
* **Rust** (`cargo`)
* **Python** (`poetry`, `pipenv`, `pip` with `requirements.txt`)

## Security Architecture

Deleting developer dependencies is inherently dangerous. Dehydrate implements a robust zero-trust security model:

1. **RCE Prevention (Zero Trust):** The `.dehydrate.json` snapshot file does *not* execute arbitrary commands upon rehydration. It strictly enforces an internal package manager whitelist (e.g., `npm`, `cargo`) to prevent Remote Code Execution (RCE).
2. **Native OS Execution:** Dehydrate bypasses shell wrappers completely (e.g., bypassing `cmd.exe` on Windows and invoking `.cmd` scripts natively) to ensure shell metacharacter injection is mathematically impossible.
3. **The Awake Trust Prompt:** Before running any automated reinstall commands during `dehydrate awake`, the CLI explicitly prints what it's about to do and mandates a `(Y/n)` human-in-the-loop interactive confirmation. It automatically aborts if piped from a malicious non-interactive background script.
2. **Path Traversal Protection:** Before attempting to delete any heavy folder, Dehydrate explicitly checks if it is a symlink. This prevents a malicious repository from disguising a system folder (e.g., `C:\Windows`) as `node_modules` to trigger arbitrary file destruction.
3. **Resource Exhaustion (DoS) Limits:** All filesystem traversal algorithms implement a hard `--max-depth` boundary (default: 100) to prevent infinite loops from recursive symlinks.
4. **Memory Exhaustion Prevention:** The `awake` parser will instantly reject any `.dehydrate.json` payload larger than 1MB to prevent Out-Of-Memory (OOM) attacks.
5. **The Lockfile Golden Rule:** Hibernation is strictly gated behind the presence of lockfiles to guarantee deterministic re-builds.

---
*Built to save your SSD.*
