# codex-tree CLI Interface Specification

**Version:** 0.1.0
**Status:** Draft

## Overview

`codex-tree` is a command-line tool that generates and maintains knowledge trees for AI codebase comprehension. It analyzes source code using AST parsing and optionally enriches the tree with AI-generated intent analysis.

## Global Options

```
codex-tree [OPTIONS] <COMMAND>

Options:
  --path <DIR>       Target directory (default: current directory)
  --verbose, -v      Increase output verbosity
  --quiet, -q        Suppress non-error output
  --help, -h         Print help
  --version, -V      Print version
```

## Commands

### init

Generate a `.codex-tree/` directory from scratch.

```
codex-tree init [OPTIONS]

Options:
  --no-intent        Skip AI intent layer generation
  --no-claude        Skip Claude optimization layer generation
  --languages <L>    Comma-separated list of languages to parse (default: auto-detect)
  --dry-run          Show what would be generated without writing files
```

**Behavior:**
1. Locate project root (find `.git` directory, error if not found)
2. Walk source files, filtering by `.gitignore` and default exclusions
3. Parse each file with the appropriate language adapter (tree-sitter)
4. Generate `version.json` (generation=1, delta_count=0)
5. Generate `tree.json` (top-level structure map)
6. Generate `modules/` (per-file index.json)
7. If `--no-intent` is NOT set: call Claude API to generate `intent/`
8. If `--no-claude` is NOT set: generate `claude/` (L1, L2, L3)
9. Print summary: files parsed, symbols found, tree size, time elapsed

**Default exclusions** (in addition to `.gitignore`):
- `.git/`, `.codex-tree/`, `target/`, `node_modules/`, `__pycache__/`
- `*.lock`, `*.min.js`, `*.min.css`
- Binary files (detected by content)

**Exit codes:**
- `0` — success
- `1` — error (no git repo, parse failure, API error)
- `2` — `.codex-tree/` already exists (use `regen` or delete first)

**Error messages follow the teaching pattern:**
```
Error: .codex-tree/ already exists at /home/user/project/.codex-tree/

A knowledge tree has already been generated for this project.

  To update incrementally:  codex-tree update
  To rebuild from scratch:  codex-tree regen
  To force overwrite:       rm -rf .codex-tree && codex-tree init

If you're seeing this in CI, the tree may already be committed.
Check if .codex-tree/ exists in your repository.
```

### update

Apply incremental delta based on changes since the last tree state.

```
codex-tree update [OPTIONS]

Options:
  --no-intent        Skip AI intent analysis for changed files
  --no-claude        Skip Claude layer regeneration
  --no-compact       Skip auto-compaction even if thresholds are met
```

**Behavior:**
1. Read `version.json` to get `source_commit`
2. Run `git diff --name-only {source_commit}..HEAD` to identify changed files
3. If no changes detected, print "Tree is up to date" and exit
4. Re-parse only changed files
5. Compute structural diff against existing module indexes
6. Write delta to `deltas/{next_sequence}.json`
7. Update `version.json` (increment `delta_count`, update `source_commit`)
8. If compaction thresholds met and `--no-compact` is NOT set: run compaction
9. If `--no-intent` is NOT set: update intent for changed files
10. If `--no-claude` is NOT set: regenerate Claude layer
11. Print summary: files changed, symbols added/modified/removed, delta size

**Compaction thresholds:**
- `delta_count >= 10`
- Total delta file size `>= 102,400 bytes` (100 KB)

**Fallback for shallow clones:**
If `source_commit` is not in git history (shallow clone), fall back to file hash comparison using `content_hash` in module indexes.

### regen

Full rebuild of the knowledge tree. Creates a new generation.

```
codex-tree regen [OPTIONS]

Options:
  --no-intent        Skip AI intent layer generation
  --no-claude        Skip Claude optimization layer generation
  --languages <L>    Comma-separated list of languages to parse
```

**Behavior:**
1. Delete existing `.codex-tree/` directory
2. Run the same process as `init` but with `generation` incremented from the previous value
3. Print summary including comparison with previous generation (if available)

### report

Display benchmarked token savings and tree statistics.

```
codex-tree report [OPTIONS]

Options:
  --format <FMT>     Output format: text (default), json
  --benchmark        Run actual AI benchmark (with/without tree token comparison)
```

**Default output (text):**
```
codex-tree report
═══════════════════════════════════════════════

  Tree version:          1.3
  Format version:        0.1.0
  Last updated:          2026-03-27T14:30:00Z
  Source commit:         abc123d

  Codebase:
    Files:               42
    Lines of code:       8,420
    Languages:           rust, python

  Tree:
    Symbols indexed:     347
    Tree size:           12.4 KB
    Raw source size:     340 KB
    Compression ratio:   27:1

  Staleness:
    Clean files:         40
    Stale files:         2
    Missing files:       0

  Estimated savings:
    Tokens (without):    ~22,000
    Tokens (with L2):    ~4,000
    Savings:             ~82%

═══════════════════════════════════════════════
```

**Benchmark mode (`--benchmark`):**
Runs an actual AI session comparing token usage with and without the tree:
1. Session A: Load `.codex-tree/claude/l2.md`, ask Claude to describe the project
2. Session B: No tree, let Claude explore the codebase from scratch
3. Measure: tokens consumed, time elapsed, accuracy of understanding
4. Report comparison

### check

Staleness safety valve — compare tree against current working state.

```
codex-tree check [OPTIONS]

Options:
  --format <FMT>     Output format: text (default), json
  --fail-if-stale    Exit with code 1 if any files are stale (useful in CI)
```

**Behavior:**
1. Read `source_commit` from `version.json`
2. Compare against current HEAD and working tree
3. For each file, verify `content_hash` matches
4. Output categorized file list:

```
codex-tree check

  Tree version: 1.3
  Source commit: abc123d (2 commits behind HEAD)

  Clean (trust tree):    40 files
  Stale (explore raw):   2 files
    - src/parser/registry.rs (modified in abc456)
    - src/commands/init.rs (uncommitted changes)
  Missing (not in tree): 0 files

  Recommendation: run 'codex-tree update' to refresh
```

**Exit codes:**
- `0` — tree is clean (or `--fail-if-stale` not set)
- `1` — stale files detected (only with `--fail-if-stale`)

## Environment Variables

| Variable | Purpose | Required |
|----------|---------|----------|
| `ANTHROPIC_API_KEY` | Claude API authentication | Only for intent/claude layer generation |
| `CODEX_TREE_MODEL` | Claude model to use for intent (default: `claude-sonnet-4-20250514`) | No |
| `CODEX_TREE_BUDGET` | Max token spend per run (default: unlimited) | No |

## Exit Code Summary

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Runtime error (parse failure, API error, stale with `--fail-if-stale`) |
| 2 | Precondition failure (no git repo, tree already exists, tree not found) |

## Error Message Design

All error messages follow a teaching pattern with four components:

1. **What went wrong** — the immediate error
2. **Why it matters** — context for understanding the error
3. **How to fix** — actionable steps
4. **AI hint** — additional context that helps AI agents self-correct

Example:
```
Error: No git repository found.

codex-tree requires a git repository because:
  - Tree versioning tracks the source commit
  - The 'update' command uses git diff to detect changes
  - The 'check' command compares against git status

To fix:
  git init
  git add .
  git commit -m "initial commit"
  codex-tree init

[AI hint: If running in CI, ensure the checkout action includes
git history. Use actions/checkout with fetch-depth: 0]
```
