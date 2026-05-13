# Deep Interview Spec: Duplicate Code Detection in Biome

## Metadata
- Interview ID: dupl-biome-20260513
- Rounds: 10
- Final Ambiguity Score: 13%
- Type: brownfield
- Generated: 2026-05-13
- Threshold: 0.20
- Initial Context Summarized: no
- Status: PASSED

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.92 | 0.35 | 0.322 |
| Constraint Clarity | 0.87 | 0.25 | 0.218 |
| Success Criteria | 0.83 | 0.25 | 0.208 |
| Context Clarity | 0.80 | 0.15 | 0.120 |
| **Total Clarity** | | | **0.868** |
| **Ambiguity** | | | **13%** |

## Goal

Add a nursery lint rule `noDuplicateCode` to `biome_js_analyze` that detects duplicate code blocks across **all** JS/TS files in a project, using an exact-token suffix-tree algorithm (matching `../dupl`'s approach), with a configurable token-count threshold (default: 100). The rule reports diagnostics via `biome check` (CLI) with both the primary duplicate location and a related-information span pointing to the other occurrence(s). It starts as opt-in (nursery, not recommended), targets JS and TypeScript only in v1, and requires a new cross-file analysis pass in Biome's workspace/CLI pipeline.

## Constraints

- **Language scope (v1):** JavaScript and TypeScript only (`biome_js_syntax` CST).
- **Algorithm:** Exact token matching via suffix tree. Trivia tokens (whitespace, comments) are stripped before serialization. AST node type integers are used as the token alphabet (same approach as dupl).
- **Threshold unit:** Syntax tokens (not lines or bytes). Default = 100 tokens.
- **Configuration:** Rule option `threshold` in `biome.json` (integer, default 100).
- **Pipeline:** CLI-only for v1. LSP/editor integration is a future concern (performance must be validated first).
- **Diagnostics:** Warning-level, no auto-fix. Primary span on the duplicate block; related information span pointing to the original (or all occurrences). Matches dupl's report style.
- **Default status:** Nursery rule — off by default, users opt in via `biome.json`.
- **Architecture:** New cross-file analysis phase required. Biome's existing lint rules are single-file; a workspace-level pass must collect token sequences from all files, build the suffix tree, and emit diagnostics after all files are processed.
- **Normalization:** Exact matching only for v1. Token serialization layer should be designed so identifier normalization can be layered on later (e.g., replace all identifier tokens with a single abstract token type before building the tree).

## Non-Goals

- CSS, GraphQL, HTML, JSON support (deferred to future versions).
- Normalized/semantic matching (identifier abstraction) — designed for, not implemented in v1.
- LSP / real-time editor integration in v1.
- Auto-fix / code actions (refactoring duplicates is a manual task).
- Enabled-by-default / recommended status — starts as nursery.
- Detecting near-duplicates or refactored copies (variable renames, reordering).

## Acceptance Criteria

- [ ] `biome check` reports a diagnostic when two or more JS/TS files contain identical token sequences ≥ 100 tokens long.
- [ ] The diagnostic includes both the primary duplicate location (file + range) and a related span pointing to the other occurrence(s).
- [ ] The rule is configurable: `{ "nursery": { "noDuplicateCode": { "level": "warn", "options": { "threshold": 80 } } } }` changes the threshold.
- [ ] Setting threshold to 50 detects shorter duplicates; setting to 200 requires longer sequences.
- [ ] Trivia (whitespace, blank lines, comments) does not affect matching — two blocks differing only in comments still match.
- [ ] The rule does NOT fire for duplicate blocks within the same file that are below the threshold (single-file, above threshold: fires).
- [ ] The rule does NOT fire when only one file contains a given sequence (no duplicate).
- [ ] `biome-ignore lint/nursery/noDuplicateCode` suppression comment works.
- [ ] Rule appears in `biome explain noDuplicateCode` output.
- [ ] `just gen-analyzer` codegen passes (schema, metadata, documentation).
- [ ] Existing Biome tests continue to pass (`cargo test --workspace`).

## Assumptions Exposed & Resolved

| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| "Flag duplicates" = lint rule | Could be a standalone command | Lint rule in `biome check` |
| Matches dupl = cross-file | Biome rules are single-file today | Accept new cross-file infrastructure cost |
| Need normalized matching | Contrarian: exact matching catches most cases and is 10× simpler | Exact matching in v1, designed to extend |
| Must be in biome check | Could be post-processing or separate command | Must appear in `biome check` output |
| Must work in editor | LSP integration is very expensive for cross-file | CLI first, LSP deferred |
| All languages | Each has a separate AST and crate | JS/TS only for v1 |
| Lines or bytes as threshold | dupl uses tokens | 100-token default, same as dupl |
| Should be on by default | Nursery is the standard new-rule path | Nursery, opt-in |

## Technical Context

### Biome Analyzer Architecture (brownfield findings)

- `biome_analyze/src/lib.rs` — `Analyzer<L>` struct; two-phase: visitors → query matches → rule signals
- `biome_analyze/src/rule.rs` — `Rule` trait: `run()` + `diagnostic()` + `Options`
- `biome_js_analyze/src/lint/nursery/` — where the new rule file goes
- `biome_js_syntax` — rowan-based CST; `SyntaxNode::walk()` for traversal; trivia via `is_trivia()`
- **Cross-file gap:** `biome_analyze` orchestrates per-file. No existing cross-file analysis phase.
- `biome_service/src/workspace.rs` — `WorkspaceServer` manages all files; potential insertion point for a post-analysis cross-file pass.

### dupl Algorithm (reference implementation at `../dupl`)

- `suffixtree/suffixtree.go` — Ukkonen suffix tree (`STree`, active point canonization)
- `suffixtree/dupl.go` — `FindDuplOver(threshold)` walks tree, emits `Match{positions, length}`
- `syntax/syntax.go` — AST → integer token stream; SHA1 for dedup
- `main.go` — CLI; default threshold = 100 tokens

### Proposed Architecture for Cross-File Pass

```
biome check (CLI)
  ├── Phase 1: per-file analysis (existing)
  │     └── biome_js_analyze rules (existing lint rules)
  └── Phase 2: cross-file analysis (NEW)
        ├── Collect token sequences from all JS/TS files
        │     └── Walk biome_js_syntax CST, skip trivia, emit SyntaxKind integers
        ├── Build suffix tree across all sequences
        │     └── Port dupl's Ukkonen tree to Rust OR use `suffix` crate
        ├── FindDuplOver(threshold) → Match { file_a, range_a, file_b, range_b, token_count }
        └── Emit Diagnostic per match (warning, related spans)
```

**Key new code locations:**
- New rule: `crates/biome_js_analyze/src/lint/nursery/no_duplicate_code.rs`
- New cross-file infrastructure: `crates/biome_analyze/src/cross_file.rs` (or similar)
- CLI hook: `crates/biome_cli/src/execute/check.rs` (add Phase 2 after per-file loop)
- Token serializer: `crates/biome_js_analyze/src/lint/nursery/no_duplicate_code/token_stream.rs`
- Suffix tree: either a new `crates/biome_dupl/` crate or inline in the rule module

## Ontology (Key Entities)

| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| LintRule (`noDuplicateCode`) | core domain | name, group (nursery), threshold option | lives in biome_js_analyze |
| DuplicateCodeBlock | core domain | file_path, byte_range, token_count | matched by TokenSequence |
| TokenSequence | supporting | [SyntaxKind integers] (trivia stripped) | serialized from SyntaxNode |
| WorkspaceAnalysisPass | supporting | phase, file list, suffix tree | runs after per-file analysis |
| DiagnosticSpan | supporting | primary range, related ranges | emitted per Match |

## Ontology Convergence

| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|-------------|-----|---------|--------|----------------|
| 1 | 3 | 3 | - | - | N/A |
| 2 | 4 | 1 | 0 | 3 | 75% |
| 3 | 5 | 1 | 0 | 4 | 80% |
| 4–10 | 5 | 0 | 0 | 5 | 100% |

## Interview Transcript

<details>
<summary>Full Q&A (10 rounds)</summary>

### Round 1
**Q:** What user-facing form should this take?
**A:** Lint rule in biome check
**Ambiguity:** 73% (Goal: 0.40, Constraints: 0.20, Criteria: 0.10, Context: 0.35)

### Round 2
**Q:** Cross-file (like dupl) or single-file only?
**A:** Cross-file (like dupl)
**Ambiguity:** 64% (Goal: 0.50, Constraints: 0.35, Criteria: 0.15, Context: 0.40)

### Round 3
**Q:** When the rule fires, what should the developer experience look like?
**A:** Same behaviour / configuration options as golangci dupl
**Ambiguity:** 50% (Goal: 0.60, Constraints: 0.40, Criteria: 0.50, Context: 0.45)

### Round 4 (Contrarian Mode)
**Q:** Exact token matching or normalized (identifiers abstracted)?
**A:** Normalized matching (identifiers abstracted)
**Ambiguity:** 44% (Goal: 0.65, Constraints: 0.50, Criteria: 0.55, Context: 0.45)

### Round 5 (Contrarian Challenge)
**Q:** Is normalized matching necessary for v1 given its implementation cost?
**A:** Start with exact, but design so normalized is easier later
**Ambiguity:** 38%

### Round 6 (Simplifier Mode)
**Q:** Must appear in biome check output, or is post-processing fine?
**A:** Must appear in biome check output
**Ambiguity:** 32%

### Round 7 (Simplifier Challenge)
**Q:** Does this need LSP/editor support or is CLI-only acceptable?
**A:** CLI first, LSP later if performance allows
**Ambiguity:** 28%

### Round 8
**Q:** Which languages for v1?
**A:** JavaScript & TypeScript only
**Ambiguity:** 23%

### Round 9
**Q:** Threshold: match dupl defaults (100 tokens, configurable in biome.json)?
**A:** Yes — match dupl defaults
**Ambiguity:** 16%

### Round 10
**Q:** New rules start in nursery (off by default). Acceptable?
**A:** Yes — nursery is the right starting point
**Ambiguity:** 13%

</details>
