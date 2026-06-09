# DEHYDRATE(1) - User Manual

## NAME
**dehydrate** - The Smart Project Hibernator

## SYNOPSIS
`dehydrate` [*COMMAND*] [*OPTIONS*]

## DESCRIPTION
**Dehydrate** is a high-performance, cross-platform CLI utility designed to reclaim massive amounts of disk space for software developers. Over time, development directories accumulate gigabytes of heavy, reproducible dependencies (`node_modules`, `target`, `venv`, etc.). 

Dehydrate scans your file system for stale projects (projects that haven't had their source code modified recently) and safely deletes these heavy dependency folders. It features full Polyglot Support—meaning if a project directory contains multiple languages (like a Rust backend + React frontend), Dehydrate will process all of them concurrently! Before deletion, it creates a deterministic Snapshot DNA (`.dehydrate.json`), allowing you to completely restore the exact environment later with a single command.

## COMMANDS

### `scan`
Recursively traverses the current directory to find software projects. It evaluates the "stale" status of each project by analyzing the `last_modified` timestamp of actual source code files (explicitly ignoring dependency folder modifications).
- **Usage:** `dehydrate scan [OPTIONS]`
- **Options:**
  - `-s, --stale-days <DAYS>`: The number of days a project must be untouched to be considered stale. (Default: 60)
  - `-m, --max-depth <DEPTH>`: The maximum directory depth to traverse, preventing DoS from infinite recursive symlinks. (Default: 100)

### `hibernate`
Executes the hibernation sequence. It utilizes the scanner to find stale projects in the current directory tree. For each stale project found, it enforces safety rules (checking for lockfiles), generates a `.dehydrate.json` snapshot, and permanently deletes heavy dependency directories to reclaim disk space.
- **Usage:** `dehydrate hibernate [OPTIONS]`
- **Options:**
  - `--dry-run`: Performs a simulated hibernation. It will print out which folders *would* be deleted and exactly how many megabytes *would* be saved, but will not actually delete anything. Highly recommended for first-time use.
  - `-m, --max-depth <DEPTH>`: The maximum directory depth to traverse during operations. (Default: 100)

### `awake`
Restores a previously hibernated project back to a working state. Must be run from inside the root of a hibernated project directory. It reads the `.dehydrate.json` file, verifies your system has the required package manager installed, spawns a child process to seamlessly reinstall all missing dependencies, and finally cleans up the snapshot file.
- **Usage:** `dehydrate awake`

## ECOSYSTEMS SUPPORTED

Dehydrate natively understands multiple language ecosystems:

- **Node.js / JavaScript / TypeScript**
  - **Markers:** `package.json`
  - **Targets Deleted:** `node_modules`, `dist`, `.next`, `build`
  - **Package Managers:** `npm`, `yarn`, `pnpm`, `bun`
- **Rust**
  - **Markers:** `Cargo.toml`
  - **Targets Deleted:** `target`
  - **Package Managers:** `cargo`
- **Python**
  - **Markers:** `requirements.txt`, `pyproject.toml`
  - **Targets Deleted:** `venv`, `.venv`, `__pycache__`, `.pytest_cache`
  - **Package Managers:** `pip`, `poetry`, `pipenv`

## SAFETY GUARANTEES

Dehydrate employs strict "Safety Net" heuristics to ensure it never accidentally destroys a project environment that cannot be restored:

1. **The Lockfile Golden Rule & Auto-Generator:** Dehydrate strictly enforces that any project must contain a lockfile (e.g., `package-lock.json`, `Cargo.lock`) to guarantee 100% deterministic re-builds in the future. However, if a project is missing one, Dehydrate does not just skip it—it interactively prompts the user via `stdin` and offers to auto-generate the lockfile safely on the spot using the native toolchain! When bulk-hibernating dozens of projects, it leverages a Two-Phase unified summary prompt to give you granular control (Auto-generate All, List individually, or Skip) without blocking the terminal sequentially.
2. **Zero-Trust Command Execution:** Dehydrate reads snapshot data to rehydrate, but ignores arbitrary commands, matching them against an internal whitelist to prevent Remote Code Execution.
3. **Symlink Traversal Protection:** Prevents arbitrary file destruction by ignoring symlinked `node_modules` folders.
4. **Junk Exclusions:** Dehydrate ignores `node_modules` and `target` folders when calculating a project's "stale date". This prevents false positives where simply running `npm install` makes an old project appear active despite no source code being modified.

## EXAMPLES

**1. See how much space you can save without deleting anything:**
```bash
$ cd ~/Developer
$ dehydrate hibernate --dry-run
```

**2. Find all projects that haven't been touched in 30 days:**
```bash
$ dehydrate scan --stale-days 30
```

**3. Hibernate all stale projects to free up space:**
```bash
$ dehydrate hibernate
```

**4. Return to work on a hibernated project 6 months later:**
```bash
$ cd ~/Developer/my-old-react-app
$ dehydrate awake
```

## FILES
- `.dehydrate.json` — The JSON snapshot file generated in the root of a project during hibernation. Contains metadata regarding the runtime, package manager, and explicit paths that were deleted.

## AUTHOR
Built in Rust. Designed for safety and speed.
