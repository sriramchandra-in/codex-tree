# codex-tree Format Specification

**Format Version:** 0.1.0
**Status:** Draft

## Overview

The codex-tree format defines a persistent, structured knowledge representation of a software codebase. It is designed to be consumed by AI models and development tools to gain instant understanding of a project without re-parsing source files.

The format is model-independent, IDE-independent, and language-independent.

## Directory Layout

```
.codex-tree/
  version.json                    # Tree metadata and versioning
  tree.json                       # Top-level structure map
  deltas/                         # Incremental updates (pre-compaction)
    {sequence_number}.json
  modules/                        # Per-module structural detail
    {mirrored_source_path}/
      index.json
  intent/                         # AI-generated semantic layer
    decisions.json
    patterns.json
  claude/                         # Claude-specific optimization layer
    l1.md                         # ~500 tokens — project summary
    l2.md                         # ~2,000 tokens — module detail
    l3.md                         # Full detail
```

## Schemas

### version.json

Tracks the tree's own lifecycle, independent of git commits.

```json
{
  "format_version": "0.1.0",
  "tree_version": {
    "generation": 1,
    "delta_count": 3
  },
  "generated_at": "2026-03-27T10:00:00Z",
  "generator": "codex-tree 0.1.0",
  "source_commit": "abc123def456...",
  "source_commit_date": "2026-03-27T09:55:00Z",
  "stats": {
    "total_files": 42,
    "total_symbols": 347,
    "total_lines_of_code": 8420,
    "languages": ["rust", "python"],
    "delta_size_bytes": 45200
  }
}
```

**Versioning rules:**
- `generation` — incremented on full rebuild (`regen`). Represents a complete re-analysis.
- `delta_count` — incremented on each `update`. Reset to 0 on compaction or regen.
- `format_version` — the spec version. Consumers check this for compatibility.
- Tree version is expressed as `{generation}.{delta_count}` (e.g., `1.3`).

### tree.json

Top-level structure map — the "table of contents" for the codebase.

```json
{
  "root": ".",
  "entries": [
    {
      "path": "src/main.rs",
      "type": "file",
      "language": "rust",
      "symbol_count": 5,
      "line_count": 120,
      "imports_count": 3,
      "exports_count": 2
    },
    {
      "path": "src/parser",
      "type": "directory",
      "children_count": 4,
      "total_symbol_count": 28,
      "total_line_count": 890
    }
  ]
}
```

**Entry types:**
- `file` — a source file with parseable symbols.
- `directory` — an aggregation node with summary counts.

Only files recognized by a registered language adapter appear. Non-source files (images, configs, etc.) are excluded.

### modules/{path}/index.json

Per-file structural detail extracted from AST analysis.

```json
{
  "path": "src/parser/registry.rs",
  "language": "rust",
  "content_hash": "sha256:a1b2c3...",
  "symbols": [
    {
      "name": "ParserRegistry",
      "kind": "struct",
      "visibility": "pub",
      "span": { "start_line": 15, "end_line": 25 },
      "signature": "pub struct ParserRegistry",
      "doc_comment": "Maps file extensions to language adapters."
    },
    {
      "name": "register",
      "kind": "function",
      "visibility": "pub",
      "span": { "start_line": 28, "end_line": 35 },
      "signature": "pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>)",
      "parent": "ParserRegistry",
      "doc_comment": null
    }
  ],
  "imports": [
    {
      "source": "crate::language::LanguageAdapter",
      "kind": "use"
    }
  ],
  "exports": [
    {
      "name": "ParserRegistry",
      "kind": "struct"
    }
  ]
}
```

**Symbol kinds:** `function`, `struct`, `enum`, `trait`, `impl`, `type_alias`, `const`, `static`, `module`, `macro`, `interface`, `class`, `method`, `field`, `variable`.

**Visibility:** `pub`, `pub_crate`, `pub_super`, `private`, or language-specific equivalents.

**content_hash:** SHA-256 of the source file contents. Used to detect staleness and as a cache key for the AI intent layer.

### deltas/{sequence}.json

Incremental update records. Each delta describes changes since the previous tree state.

```json
{
  "sequence": 4,
  "timestamp": "2026-03-27T14:30:00Z",
  "source_commit": "def789...",
  "changed_files": [
    "src/parser/registry.rs",
    "src/commands/init.rs"
  ],
  "operations": [
    {
      "op": "modify",
      "path": "src/parser/registry.rs",
      "symbols_added": ["get_adapter"],
      "symbols_removed": [],
      "symbols_modified": ["register"]
    },
    {
      "op": "add",
      "path": "src/commands/init.rs",
      "module_index": { "...": "full index.json for new file" }
    }
  ]
}
```

**Operation types:**
- `add` — new file. Includes the full module index.
- `modify` — changed file. Lists symbols added, removed, and modified.
- `remove` — deleted file. The corresponding module index is removed on compaction.

### Compaction Rules

Compaction merges accumulated deltas into the base tree.

**Triggers** (whichever comes first):
- `delta_count >= 10`
- Total delta file size `>= 102,400 bytes` (100 KB)

**Compaction process:**
1. Read all deltas in sequence order.
2. Apply operations to `tree.json` and `modules/` index files.
3. Delete all files in `deltas/`.
4. Reset `delta_count` to 0 in `version.json`.
5. Update `delta_size_bytes` to 0.
6. Do NOT increment `generation` (compaction is not a regen).

### intent/decisions.json

AI-generated design decisions and rationale. Each entry is anchored to a specific file or symbol.

```json
{
  "generated_at": "2026-03-27T10:00:00Z",
  "generator_model": "claude-sonnet-4-20250514",
  "decisions": [
    {
      "id": "d001",
      "anchored_to": {
        "file": "src/parser/language.rs",
        "symbol": "LanguageAdapter"
      },
      "summary": "Trait-based adapter pattern for language-independent parsing",
      "rationale": "Allows adding new language support without modifying the core parser. Each language implements the LanguageAdapter trait independently.",
      "confidence": 0.9,
      "provenance": "inferred_from_code"
    }
  ]
}
```

**Confidence scale:**
- `1.0` — extracted from explicit documentation (doc comments, PR descriptions)
- `0.7–0.9` — inferred from code patterns with high certainty
- `0.4–0.6` — inferred but multiple interpretations possible
- `0.1–0.3` — speculative, no strong evidence

**Provenance values:**
- `extracted_from_docs` — from doc comments or documentation files
- `extracted_from_commit` — from commit messages or PR descriptions
- `inferred_from_code` — AI analysis of code patterns
- `human_annotated` — manually added by a developer

### intent/patterns.json

AI-generated coding and design pattern observations.

```json
{
  "generated_at": "2026-03-27T10:00:00Z",
  "generator_model": "claude-sonnet-4-20250514",
  "patterns": [
    {
      "id": "p001",
      "name": "Workspace crate separation",
      "description": "The project separates concerns into parser (deterministic AST), analyzer (AI/non-deterministic), and CLI (orchestration) crates.",
      "applies_to": ["crates/codex-parser", "crates/codex-analyzer", "crates/codex-cli"],
      "confidence": 0.95,
      "provenance": "inferred_from_code"
    }
  ]
}
```

### claude/ — Claude Optimization Layer

Pre-formatted summaries optimized for Claude model consumption at three detail levels.

Each file begins with a metadata header:

```markdown
<!-- codex-tree: generation=1, delta_count=3, format_version=0.1.0 -->
```

**Staleness rule:** If the `generation` or `delta_count` in `version.json` differs from the header, the Claude layer is stale and should be regenerated.

**Level definitions:**

| Level | File | Target tokens | Contents |
|-------|------|--------------|----------|
| L1 | l1.md | ~500 | Project name, language, architecture pattern, key modules, primary entry points |
| L2 | l2.md | ~2,000 | L1 + module purposes, key interfaces, dependency graph, design patterns |
| L3 | l3.md | Full | L2 + all symbols, all intents, full dependency details |

**Consumption guidance:**
- Haiku / quick tasks → load L1
- Sonnet / implementation → load L2
- Opus / architecture decisions → load L3

## Staleness Detection

The `check` command compares tree state against the working directory:

1. Read `source_commit` from `version.json`.
2. Run `git diff --name-only {source_commit}..HEAD` to find committed changes.
3. Run `git status --porcelain` to find uncommitted changes.
4. For each changed file, compare `content_hash` in the module index against actual file hash.
5. Report: clean files (trust tree), stale files (explore raw source), missing files (new, not in tree).

## Design Principles

1. **Structure is ground truth.** The AST layer is deterministic and verifiable. The AI intent layer is always derivable from the AST layer + source code.
2. **Progressive disclosure.** Load what you need — tree.json for overview, module index for detail, intent for understanding.
3. **Fail gracefully.** If the tree is stale or corrupt, the consumer falls back to raw source exploration. The tree is an optimization, not a requirement.
4. **Model-independence.** The format uses standard JSON and Markdown. No model-specific encoding or token optimization in the core format. The `claude/` layer is an optional, model-specific cache.
5. **Language-independence.** Symbol kinds and visibility levels are normalized across languages. Language-specific details are preserved in signatures.
