# Implementation Plan: `noDuplicateCode` Nursery Lint Rule (REVISED v4)

**Plan ID:** no-duplicate-code-rule-v4  
**Date:** 2026-05-13  
**Status:** Ready for Implementation — v4 (Architect + Critic pass 3, Critic APPROVED)  
**Based on:** Deep Interview Spec (dupl-biome-20260513, Ambiguity: 13%)  
**Complexity:** MEDIUM  
**Estimated Effort:** 3-4 days (single engineer)

---

## RALPLAN-DR Summary

### Principles (4)
1. **Exact token matching in v1** — Simpler to implement correctly; scope for identifier normalization in v2.
2. **Cross-file analysis in CLI layer, not rule layer** — Rules are single-file; cross-file collection + suffix tree live in `biome_cli`.
3. **Configurable threshold, nursery-default** — 100-token default (match dupl), opt-in via `biome.json`.
4. **Diagnostics in `before_finalize()`** — After all per-file processing, emit cross-file matches as `biome_diagnostics::Error`.

### Decision Drivers (Top 3)
1. **Biome's single-file analyzer architecture** — Forces cross-file work into CLI finalizer, not rule trait. AST unavailable in `before_finalize()`; all tokenization must happen in `process_file()`.
2. **No existing cross-file diagnostic framework** — Must build token store + suffix tree from first principles. Store must use `Arc<Mutex<...>>` for thread-safe accumulation.
3. **Performance baseline required before LSP** — CLI-only in v1; editor integration deferred until we know cost.

### Viable Options & Trade-offs

#### Option A: Token collection in `process_file()`, analysis in `before_finalize()` ← **CHOSEN**
- **Pros:** Token store shared via Arc; clean separation of concerns (collection vs. analysis); works with existing CLI flow; tokens persist until finalization.
- **Cons:** Requires extending `CrawlerContext` trait + modifying `CrawlerOptions` constructor.
- **Viability:** HIGH — fits Biome's architecture; low architectural drift.
- **Why chosen:** Only viable option given that `before_finalize()` has no AST access.

#### Option B: Token collection in `before_finalize()` only
- **Pros:** Fewer trait changes; analysis & collection in one place.
- **Cons:** **REJECTED** — No AST/file content available in `before_finalize()`. Only diagnostics vec passed. Infeasible.
- **Viability:** LOW — incorrect API assumptions invalidate this.

#### Option C: Build suffix tree in rule layer, emit per-file partial matches
- **Pros:** No CLI layer changes; fits the "rule" mental model.
- **Cons:** **REJECTED** — Rules run per-file, can't see other files. Would require collecting all files in one rule instance (architectural hack). Breaks analyzer design.
- **Viability:** LOW — breaks Biome's design.

**Rationale for Option A:** Matches Biome's existing pattern of per-file processing with post-processing finalization. Only architecturally correct option. Minimal drift.

---

## Architecture Overview

```
biome check (CLI)
│
├─ Phase 1: Per-file analysis (existing)
│  └─ CheckProcessFile::process_file() for each file
│     └─ [NEW] Tokenize JS/TS, push to shared TokenStore (Arc<Mutex<...>>)
│
└─ Phase 2: Cross-file finalization (new)
   └─ CheckFinalizer::before_finalize()
      ├─ Drain TokenStore (tokens collected in Phase 1)
      ├─ Build suffix tree
      ├─ Find all duplicates ≥ threshold
      └─ Emit diagnostics to crawler_output.diagnostics
```

**Architectural facts verified:**
- `before_finalize()` receives `TraverseResult` with `diagnostics: Vec<Error>` — push diagnostics here.
- `before_finalize()` has NO AST or file content — all tokenization happens in `process_file()`.
- `CheckProcessFile::process_file()` has access to `workspace_file` (AST available) and `CrawlerContext` (can access shared state).
- `CrawlerContext` is accessible in both `process_file()` and (indirectly via thread-local) in `before_finalize()`.
- **No circular dependency risk:** `biome_cli` already imports `biome_js_analyze = { workspace = true }`. `biome_js_analyze` does NOT depend on `biome_cli`. Safe to import the tokenizer from `biome_js_analyze` into `biome_cli`.

---

## Phase 1: Rule Definition & Options Registration

**Goal:** Define the rule stub and register its options schema so `biome.json` configuration works and `biome explain` outputs documentation.

**Important Note:** The rule stub returns `None` always because duplicate detection is **cross-file** and runs in CLI. The rule still exists to:
- Auto-register options schema via `declare_rule!`
- Allow `biome explain noDuplicateCode`
- Support `biome-ignore lint/nursery/noDuplicateCode` comments
- Appear in rule listings

### 1.1 Create Rule File

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_js_analyze/src/lint/nursery/no_duplicate_code.rs`

```rust
use biome_analyze::{
    context::RuleContext, declare_rule, Ast, Rule, RuleDiagnostic,
};
use biome_js_syntax::AnyJsRoot;
use biome_rowan::AstNode;
use biome_rule_options::no_duplicate_code::NoDuplicateCodeOptions;

declare_rule! {
    /// Disallow duplicate code blocks across files.
    ///
    /// This rule detects duplicate code blocks across JavaScript and TypeScript files
    /// in your project. It uses an exact-token suffix-tree algorithm to find sequences
    /// of 100 or more identical syntax tokens that appear in multiple files.
    ///
    /// Trivia (whitespace, comments) is ignored; only syntax tokens matter.
    ///
    /// ## Options
    ///
    /// - `threshold` (default: `100`) - Minimum number of tokens to flag as duplicate.
    ///
    /// ## Example
    ///
    /// ### Default (100 tokens)
    ///
    /// ```js
    /// // file1.js and file2.js both contain:
    /// const longFunction = () => { /* ... ~100 tokens ... */ };
    /// ```
    ///
    /// This will be flagged as a duplicate with a related span pointing to the other file.
    ///
    /// ### With custom threshold
    ///
    /// ```json
    /// {
    ///   "linter": {
    ///     "rules": {
    ///       "nursery": {
    ///         "noDuplicateCode": {
    ///           "level": "warn",
    ///           "options": {
    ///             "threshold": 50
    ///           }
    ///         }
    ///       }
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Now blocks of 50+ identical tokens will be flagged.
    ///
    /// ## Notes
    ///
    /// - This is a **cross-file rule**: it runs after all files are analyzed and reports duplicates found across files.
    /// - The per-file rule component is a no-op (returns `None`).
    /// - Actual duplicate detection happens in the CLI layer during finalization.
    /// - This is a nursery rule: opt-in via `biome.json`, not enabled by default.
    pub NoDuplicateCode {
        version: "next",
        name: "noDuplicateCode",
        language: "js",
        recommended: false,
    }
}

impl biome_analyze::Rule for NoDuplicateCode {
    type Query = Ast<AnyJsRoot>;
    type State = ();
    type Signals = Option<()>;
    type Options = biome_rule_options::no_duplicate_code::NoDuplicateCodeOptions;

    fn run(_ctx: &RuleContext<Self>) -> Option<()> {
        // This rule's actual analysis happens in the CLI layer (cross-file pass).
        // The per-file rule is a no-op; its only purpose is to register the rule
        // for schema generation, explanation, and configuration.
        // 
        // Duplicate detection runs in biome_cli after all files are tokenized.
        // See: crates/biome_cli/src/runner/finalizer.rs (CheckFinalizer::before_finalize)
        None
    }
}
```

**Rationale:** 
- The rule stub returns `None` always because duplicate detection is **cross-file** and runs in CLI.
- The rule still exists to: (a) auto-register options schema via `declare_rule!`, (b) allow `biome explain`, (c) support `biome-ignore` comments, (d) appear in rule listings.
- Actual detection logic is in Phase 5 (CLI finalizer).

### 1.2 Register Rule in Nursery Group

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_js_analyze/src/lint/nursery.rs`

The `declare_group_from_fs!` macro at line 9 already auto-discovers `.rs` files in the `nursery/` directory. **No manual changes needed** — the new `no_duplicate_code.rs` will be auto-registered.

**Verification:** After creating the file, run:
```bash
just gen-analyzer
cargo build --package biome_js_analyze
```

If no errors and the rule appears in `cargo doc`, registration succeeded.

### 1.3 Options Schema (Biome-Standard Pattern)

Options struct must live in `biome_rule_options`, not in the rule file. Create this file:

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_rule_options/src/no_duplicate_code.rs`

```rust
use biome_deserialize_macros::{Deserializable, Merge};
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Deserialize, Deserializable, Merge, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct NoDuplicateCodeOptions {
    /// Minimum number of syntax tokens to flag as duplicate.
    /// Default: 100 (matching dupl).
    #[serde(default = "default_threshold")]
    pub threshold: usize,
}

fn default_threshold() -> usize {
    100
}
```

**Then update** `crates/biome_rule_options/src/lib.rs`:
- Add `pub mod no_duplicate_code;` to the module list.

**Codegen step** (instead of manual creation):
```bash
just gen-rule-options no-duplicate-code
```

This will auto-create both the file and the module entry with correct derives.

### Acceptance Criteria (Phase 1)
- [ ] Rule file compiles and registers in nursery group.
- [ ] `biome explain noDuplicateCode` produces documentation output.
- [ ] `biome.json` accepts `{ "nursery": { "noDuplicateCode": { "level": "warn", "options": { "threshold": 80 } } } }` without error.
- [ ] `just gen-analyzer && cargo build --package biome_js_analyze` passes.
- [ ] Rule's `run()` returns `None` (confirmed: stub does not fire per-file).

---

## Phase 2: Shared Token Store Infrastructure

**Goal:** Create the `TokenStore` struct and wire it into `CrawlerOptions` and the `CrawlerContext` trait so all files can deposit tokenized data.

### 2.1 Define TokenStore

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/token_store.rs` (new)

```rust
use biome_diagnostics::file_id::FileId;
use std::sync::{Arc, Mutex};

/// A single file's tokens and metadata.
#[derive(Clone, Debug)]
pub struct TokenizedFile {
    /// Unique file identifier (e.g., path digest or BiomePath hash).
    pub file_id: FileId,
    /// Canonical file path (for error reporting).
    pub file_path: String,
    /// Token sequence: each u16 is a SyntaxKind discriminant.
    /// Trivia tokens are excluded.
    pub tokens: Vec<u16>,
    /// Byte offsets in the original source for each token.
    /// tokens[i] maps to source[offsets[i]..offsets[i+1]].
    /// Length = tokens.len() + 1 (last entry is file length).
    pub offsets: Vec<u32>,
    /// Raw file source content (for diagnostic rendering in before_finalize).
    pub source: String,
}

/// Cross-file token collection store.
/// Shared (Arc<Mutex<_>>) across all worker threads during file processing.
#[derive(Clone, Debug)]
pub struct TokenStore {
    /// Collected tokenized files.
    files: Arc<Mutex<Vec<TokenizedFile>>>,
    /// Configured threshold (read once at init, immutable).
    pub threshold: usize,
}

impl TokenStore {
    /// Create a new token store with the given threshold.
    pub fn new(threshold: usize) -> Self {
        TokenStore {
            files: Arc::new(Mutex::new(Vec::new())),
            threshold,
        }
    }

    /// Add a tokenized file. Called during process_file().
    pub fn push_file(&self, file: TokenizedFile) -> Result<(), String> {
        self.files
            .lock()
            .map_err(|_| "token store lock poisoned".to_string())?
            .push(file);
        Ok(())
    }

    /// Drain all collected files (called in before_finalize).
    /// After this, the store is empty; subsequent pushes are separate.
    pub fn drain_files(&self) -> Result<Vec<TokenizedFile>, String> {
        Ok(self.files
            .lock()
            .map_err(|_| "token store lock poisoned".to_string())?
            .drain(..)
            .collect::<Vec<_>>())
    }
}
```

**Design notes:**
- `TokenStore::threshold` is set once at init and is immutable; avoids re-reading config later.
- `Arc<Mutex<Vec<...>>>` allows safe sharing across worker threads (no data race).
- `drain_files()` empties the store (called once in `before_finalize()`).

### 2.2 Extend CrawlerOptions

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/crawler.rs`

**Current code (lines 125-156):**
```rust
pub(crate) struct CrawlerOptions<'ctx, 'app, H, P> {
    pub(crate) fs: &'app dyn FileSystem,
    pub(crate) workspace: &'ctx dyn Workspace,
    pub(crate) project_key: ProjectKey,
    interner: PathInterner,
    changed: AtomicUsize,
    unchanged: AtomicUsize,
    matches: AtomicUsize,
    skipped: AtomicUsize,
    pub(crate) messages: Sender<Message>,
    pub(crate) evaluated_paths: papaya::HashSet<BiomePath>,
    pub(crate) max_diagnostics: u32,
    pub(crate) diagnostic_level: Severity,
    execution: &'app dyn Execution,
    handler: H,
    _p: PhantomData<P>,
}
```

**Change:** Add token store field after the existing fields:
```rust
pub(crate) struct CrawlerOptions<'ctx, 'app, H, P> {
    pub(crate) fs: &'app dyn FileSystem,
    pub(crate) workspace: &'ctx dyn Workspace,
    pub(crate) project_key: ProjectKey,
    interner: PathInterner,
    changed: AtomicUsize,
    unchanged: AtomicUsize,
    matches: AtomicUsize,
    skipped: AtomicUsize,
    pub(crate) messages: Sender<Message>,
    pub(crate) evaluated_paths: papaya::HashSet<BiomePath>,
    pub(crate) max_diagnostics: u32,
    pub(crate) diagnostic_level: Severity,
    execution: &'app dyn Execution,
    handler: H,
    // NEW: token store for cross-file duplicate detection
    pub(crate) token_store: Arc<TokenStore>,
    _p: PhantomData<P>,
}
```

**Also add imports** at the top of `crawler.rs`:
```rust
use crate::runner::token_store::TokenStore;
use std::sync::Arc;
```

### 2.3 Extend CrawlerContext Trait

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/crawler.rs` (lines 111-122)

**Current code:**
```rust
pub trait CrawlerContext: TraversalContext {
    fn increment_changed(&self, path: &BiomePath);
    fn increment_unchanged(&self);
    fn increment_matches(&self, num_matches: usize);
    fn increment_skipped(&self);
    fn push_message(&self, msg: Message);
    fn fs(&self) -> &dyn FileSystem;
    fn workspace(&self) -> &dyn Workspace;
    fn project_key(&self) -> ProjectKey;
    fn execution(&self) -> &dyn Execution;
}
```

**Change:** Add token store getter method:
```rust
pub trait CrawlerContext: TraversalContext {
    fn increment_changed(&self, path: &BiomePath);
    fn increment_unchanged(&self);
    fn increment_matches(&self, num_matches: usize);
    fn increment_skipped(&self);
    fn push_message(&self, msg: Message);
    fn fs(&self) -> &dyn FileSystem;
    fn workspace(&self) -> &dyn Workspace;
    fn project_key(&self) -> ProjectKey;
    fn execution(&self) -> &dyn Execution;
    // NEW: access token store for cross-file analysis
    fn token_store(&self) -> Arc<TokenStore>;
}
```

### 2.4 Implement Trait for CrawlerOptions

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/crawler.rs`

Find the `impl<'ctx, 'app, H, P> CrawlerContext for CrawlerOptions<'ctx, 'app, H, P>` block and add this method:

```rust
fn token_store(&self) -> Arc<TokenStore> {
    Arc::clone(&self.token_store)
}
```

### 2.5 Wire Up Store Construction in CommandRunner

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/mod.rs`

Find `CommandRunner::run()` or the entry point that constructs `CrawlerOptions`. Look for the call like:

```rust
let options = CrawlerOptions::new(
    workspace,
    fs,
    project_key,
    // ... other params ...
);
```

**Before** that call, add:

```rust
// Read the noDuplicateCode threshold from config.
// For now, hardcode 100; later, read from workspace config.
let threshold = 100; // TODO: read from workspace config
let token_store = Arc::new(TokenStore::new(threshold));

// Store it in a thread-local so before_finalize can access it later.
// (This will be refined in Phase 5.)
set_cross_file_token_store(Arc::clone(&token_store));
```

Then pass `token_store` to `CrawlerOptions::new()` (you'll find where the other fields are passed):

```rust
let options = CrawlerOptions::new(
    workspace,
    fs,
    project_key,
    // ... other existing params ...
    token_store,  // NEW
);
```

**Also add imports** at the top of `mod.rs`:
```rust
use crate::runner::token_store::TokenStore;
use std::sync::Arc;
```

### 2.6 Update Module Exports

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/mod.rs`

Add near the top (with other `mod` declarations):
```rust
pub(crate) mod token_store;
```

### Acceptance Criteria (Phase 2)
- [ ] `TokenStore` compiles with no warnings.
- [ ] `CrawlerContext` trait includes `fn token_store() -> Arc<TokenStore>`.
- [ ] `CrawlerOptions` holds a `token_store: Arc<TokenStore>` field.
- [ ] `CrawlerOptions` implements the new trait method.
- [ ] `CommandRunner` constructs and passes `TokenStore` to `CrawlerOptions`.
- [ ] `cargo build --package biome_cli` passes.

---

## Phase 3: Token Collection in process_file()

**Goal:** During per-file analysis, tokenize each JS/TS file and push it to the shared `TokenStore`.

### 3.1 Create Token Serializer Module

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_js_analyze/src/lint/nursery/no_duplicate_code/token_stream.rs` (new)

```rust
//! Convert JS/TS CST to a token stream (SyntaxKind integers, trivia stripped).

use biome_js_syntax::{AnyJsRoot, JsSyntaxKind, SyntaxNode, SyntaxToken};
use biome_rowan::AstNode;

/// Walk the CST, collect non-trivia tokens, return their SyntaxKind discriminants.
/// Also return byte offsets for each token (for later diagnostic span creation).
pub fn tokenize_js_ast(root: &AnyJsRoot) -> (Vec<u16>, Vec<u32>) {
    let mut tokens = Vec::new();
    let mut offsets = Vec::new();

    walk_node(root.syntax(), &mut tokens, &mut offsets);

    // Append final offset (file length).
    if let Some(&last_offset) = offsets.last() {
        offsets.push(last_offset);
    } else {
        offsets.push(0);
    }

    (tokens, offsets)
}

/// Recursively walk syntax tree, skipping trivia.
fn walk_node(node: &SyntaxNode, tokens: &mut Vec<u16>, offsets: &mut Vec<u32>) {
    for child in node.children_with_tokens() {
        match child {
            biome_rowan::SyntaxElement::Node(n) => {
                walk_node(&n, tokens, offsets);
            }
            biome_rowan::SyntaxElement::Token(t) => {
                // Skip trivia (whitespace, comments).
                if !is_trivia(&t) {
                    let kind = t.kind();
                    let kind_discriminant = kind as u16;
                    let byte_offset = t.text_range().start().into();

                    tokens.push(kind_discriminant);
                    offsets.push(byte_offset);
                }
            }
        }
    }
}

/// Check if a token is trivia.
fn is_trivia(token: &SyntaxToken) -> bool {
    token.kind().is_trivia()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_var_decl() {
        let code = r#"const x = 1;"#;
        let module = biome_js_parser::parse_module(
            code,
            biome_js_syntax::EcmaVersion::default(),
            Default::default(),
        );
        let root = AnyJsRoot::JsModule(module.into());
        let (tokens, offsets) = tokenize_js_ast(&root);

        assert!(!tokens.is_empty());
        assert_eq!(offsets.len(), tokens.len() + 1);
    }

    #[test]
    fn test_trivia_stripped() {
        let code = r#"const x = 1; // comment
const y = 2;"#;
        let module = biome_js_parser::parse_module(
            code,
            biome_js_syntax::EcmaVersion::default(),
            Default::default(),
        );
        let root = AnyJsRoot::JsModule(module.into());
        let (tokens, _) = tokenize_js_ast(&root);

        // Should not contain any comment tokens (trivia is stripped).
        for &token in &tokens {
            let kind = JsSyntaxKind::try_from(token).unwrap();
            assert!(!kind.is_trivia());
        }
    }
}
```

**Design notes:**
- Uses `JsSyntaxKind` discriminants (u16) as the token alphabet.
- Trivia detection via `SyntaxKind::is_trivia()` (already in biome_js_syntax).
- Offsets map to byte positions in the source (used for diagnostics later).

### 3.2 Update no_duplicate_code Module Declaration

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_js_analyze/src/lint/nursery/no_duplicate_code.rs`

Add at the top (after the `declare_rule!` block):
```rust
pub mod token_stream;
```

### 3.3 Integrate Tokenization into CheckProcessFile

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/process_file.rs`

Find `CheckProcessFile::process_file()` (around line 150-250, TBD by inspection). The signature is:

```rust
fn process_file<Ctx>(
    ctx: &Ctx,
    workspace_file: &mut WorkspaceFile,
    features_supported: &FeaturesSupported,
    max_diagnostics: u32,
    diagnostic_level: Severity,
) -> Result<FileStatus, Message>
where Ctx: CrawlerContext
```

**Inside the function**, after the initial language/syntax checks (e.g., after confirming the file is JS/TS) but before returning `FileStatus`, add:

```rust
// NEW: Tokenize JS/TS files for cross-file duplicate detection.
if let Some(syntax_tree) = workspace_file.syntax_tree() {
    match syntax_tree.root() {
        biome_service::workspace::RootNode::Js(root) => {
            use crate::runner::token_store::TokenizedFile;
            use biome_js_analyze::lint::nursery::no_duplicate_code::token_stream::tokenize_js_ast;

            let (tokens, offsets) = tokenize_js_ast(&root);
            
            // Only store files with tokens (skip empty files).
            if !tokens.is_empty() {
                // Get the raw source content for diagnostic rendering later.
                let source = workspace_file.source().ok().map(|s| s.text().to_string()).unwrap_or_default();

                let tokenized = TokenizedFile {
                    file_id: workspace_file.file_id,
                    file_path: workspace_file.path.display().to_string(),
                    tokens,
                    offsets,
                    source,
                };

                if let Err(e) = ctx.token_store().push_file(tokenized) {
                    // Log the error but don't fail the file processing.
                    eprintln!("Warning: failed to store tokens for {}: {}", 
                        workspace_file.path.display(), e);
                }
            }
        }
        _ => {
            // Not a JS root; skip tokenization.
        }
    }
}
```

**Design note:** The exact method to get the syntax tree from `workspace_file` may be `workspace_file.syntax_tree()` or `workspace_file.get_syntax_tree()` — inspect the `WorkspaceFile` struct to confirm.

### Acceptance Criteria (Phase 3)
- [ ] `tokenize_js_ast()` function compiles and unit tests pass.
- [ ] `CheckProcessFile::process_file()` calls tokenization for JS/TS files.
- [ ] Tokens are pushed to `ctx.token_store()` without crashing.
- [ ] Empty files are skipped (optimization).
- [ ] `cargo build --package biome_cli --package biome_js_analyze` passes.
- [ ] `cargo test --package biome_js_analyze token_stream` passes.

---

## Phase 4: Suffix Tree Algorithm

**Goal:** Implement exact-token duplicate detection using a suffix array.

### 4.1 Suffix Array Implementation

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/suffix_array.rs` (new)

For simplicity and correctness, implement a suffix array (simpler than Ukkonen tree, O(n log n) construction).

```rust
//! Suffix array for duplicate detection.
//!
//! Builds a suffix array and LCP (Longest Common Prefix) array.
//! Walks LCP to find all maximal repeating substrings >= threshold.

use crate::runner::token_store::TokenizedFile;

/// Result of duplicate detection.
#[derive(Clone, Debug)]
pub struct DuplicateMatch {
    /// Index of first file in TokenizedFile array.
    pub file_a: usize,
    /// Byte offset of first occurrence (from TokenizedFile.offsets).
    pub offset_a: u32,
    /// Token index (within file_a.tokens) where the match starts.
    pub start_token_idx_a: usize,
    /// Index of second file.
    pub file_b: usize,
    /// Byte offset of second occurrence.
    pub offset_b: u32,
    /// Token index (within file_b.tokens) where the match starts.
    pub start_token_idx_b: usize,
    /// Length of matching token sequence (number of tokens).
    pub length: usize,
}

/// Concatenated token stream from all files, with file boundaries marked.
/// Each file's tokens are separated by a unique sentinel value (u16::MAX).
struct ConcatenatedStream {
    /// All tokens flattened.
    tokens: Vec<u16>,
    /// Boundaries: (start_index, end_index, file_idx, offsets_for_this_file)
    file_ranges: Vec<(usize, usize, usize, Vec<u32>)>,
}

impl ConcatenatedStream {
    fn new(files: &[TokenizedFile]) -> Self {
        let mut tokens = Vec::new();
        let mut file_ranges = Vec::new();

        for (file_idx, file) in files.iter().enumerate() {
            let start = tokens.len();
            tokens.extend_from_slice(&file.tokens);
            let end = tokens.len();

            file_ranges.push((start, end, file_idx, file.offsets.clone()));

            // Sentinel between files (unlikely to appear in code).
            tokens.push(u16::MAX);
        }

        ConcatenatedStream { tokens, file_ranges }
    }

    fn file_and_offset(
        &self,
        idx_a: usize,
        idx_b: usize,
        length: usize,
    ) -> Option<(usize, u32, usize, usize, u32, usize)> {
        // Find which files idx_a and idx_b belong to.
        let (file_a, start_a, offsets_a) = self.file_for_index(idx_a)?;
        let (file_b, start_b, offsets_b) = self.file_for_index(idx_b)?;

        // Compute offsets within the files.
        let local_a = idx_a - start_a;
        let local_b = idx_b - start_b;

        let offset_a = offsets_a.get(local_a).copied().unwrap_or(0);
        let offset_b = offsets_b.get(local_b).copied().unwrap_or(0);

        Some((file_a, offset_a, local_a, file_b, offset_b, local_b))
    }

    fn file_for_index(&self, idx: usize) -> Option<(usize, usize, Vec<u32>)> {
        for (start, end, file_idx, offsets) in &self.file_ranges {
            if idx >= *start && idx < *end {
                return Some((*file_idx, *start, offsets.clone()));
            }
        }
        None
    }
}

/// Find all duplicate token sequences >= threshold tokens long.
pub fn find_duplicates(
    files: &[TokenizedFile],
    threshold: usize,
) -> Vec<DuplicateMatch> {
    if files.len() < 2 {
        return Vec::new(); // Need at least 2 files for duplicates.
    }

    let stream = ConcatenatedStream::new(files);

    // Build suffix array.
    let sa = build_suffix_array(&stream.tokens);

    // Build LCP array.
    let lcp = build_lcp_array(&stream.tokens, &sa);

    // Walk LCP to find duplicates.
    let mut matches = Vec::new();
    let mut i = 0;
    while i < lcp.len() {
        let start_lcp_len = lcp[i];
        if start_lcp_len >= threshold {
            // Found a run of suffixes with LCP >= threshold.
            // Collect all suffix indices in this run.
            let mut group = vec![sa[i]];
            let mut j = i + 1;
            while j < lcp.len() && lcp[j] >= threshold {
                group.push(sa[j]);
                j += 1;
            }

            // Extract matches: for each pair (idx_a, idx_b) in group,
            // if they belong to different files, emit a match.
            for k in 0..group.len() {
                for l in (k + 1)..group.len() {
                    let idx_a = group[k];
                    let idx_b = group[l];

                    if let Some((file_a, offset_a, local_idx_a, file_b, offset_b, local_idx_b)) =
                        stream.file_and_offset(idx_a, idx_b, start_lcp_len)
                    {
                        // Only emit if from different files.
                        if file_a != file_b {
                            matches.push(DuplicateMatch {
                                file_a,
                                offset_a,
                                start_token_idx_a: local_idx_a,
                                file_b,
                                offset_b,
                                start_token_idx_b: local_idx_b,
                                length: start_lcp_len,
                            });
                        }
                    }
                }
            }

            i = j;
        } else {
            i += 1;
        }
    }

    matches
}

/// Construct a suffix array using a simple O(n log n) sort.
fn build_suffix_array(tokens: &[u16]) -> Vec<usize> {
    let mut sa: Vec<usize> = (0..tokens.len()).collect();
    sa.sort_by(|&a, &b| {
        let suffix_a = &tokens[a..];
        let suffix_b = &tokens[b..];
        suffix_a.cmp(suffix_b)
    });
    sa
}

/// Construct the LCP (Longest Common Prefix) array.
fn build_lcp_array(tokens: &[u16], sa: &[usize]) -> Vec<usize> {
    let n = sa.len();
    let mut lcp = vec![0; n];

    // Use a naive approach (correct but slower).
    // For large projects, can be optimized to Kasai's O(n) algorithm.
    for i in 0..n {
        if i == 0 {
            lcp[i] = 0;
        } else {
            let mut common = 0;
            let suffix_a = &tokens[sa[i - 1]..];
            let suffix_b = &tokens[sa[i]..];
            for (a, b) in suffix_a.iter().zip(suffix_b.iter()) {
                if a == b {
                    common += 1;
                } else {
                    break;
                }
            }
            lcp[i] = common;
        }
    }

    lcp
}

#[cfg(test)]
mod tests {
    use super::*;
    use biome_diagnostics::file_id::FileId;

    fn dummy_file_id() -> FileId {
        FileId::ZERO
    }

    #[test]
    fn test_find_duplicates_single_file() {
        let files = vec![TokenizedFile {
            file_id: dummy_file_id(),
            file_path: "test.js".to_string(),
            tokens: vec![1, 2, 3, 4, 5],
            offsets: vec![0, 1, 2, 3, 4, 5],
            source: String::new(),
        }];

        let matches = find_duplicates(&files, 3);
        assert_eq!(matches.len(), 0, "Single file should have no duplicates");
    }

    #[test]
    fn test_find_duplicates_multi_file() {
        let files = vec![
            TokenizedFile {
                file_id: dummy_file_id(),
                file_path: "file1.js".to_string(),
                tokens: vec![1, 2, 3, 4, 5],
                offsets: vec![0, 1, 2, 3, 4, 5],
                source: String::new(),
            },
            TokenizedFile {
                file_id: dummy_file_id(),
                file_path: "file2.js".to_string(),
                tokens: vec![1, 2, 3, 4, 5],
                offsets: vec![0, 1, 2, 3, 4, 5],
                source: String::new(),
            },
        ];

        let matches = find_duplicates(&files, 3);
        assert!(!matches.is_empty(), "Identical files should have duplicates");

        // Verify at least one match with length >= 3.
        assert!(matches.iter().any(|m| m.length >= 3));
    }

    #[test]
    fn test_find_duplicates_respects_threshold() {
        let files = vec![
            TokenizedFile {
                file_id: dummy_file_id(),
                file_path: "file1.js".to_string(),
                tokens: vec![1, 2, 3],
                offsets: vec![0, 1, 2, 3],
                source: String::new(),
            },
            TokenizedFile {
                file_id: dummy_file_id(),
                file_path: "file2.js".to_string(),
                tokens: vec![1, 2, 3],
                offsets: vec![0, 1, 2, 3],
                source: String::new(),
            },
        ];

        let matches = find_duplicates(&files, 5); // threshold > 3
        assert_eq!(matches.len(), 0, "Should not find matches below threshold");
    }
}
```

**Design notes:**
- Uses a sentinel value (u16::MAX) to separate files (won't appear in real code).
- Suffix array construction via O(n log n) sort.
- LCP computed naively (O(n²) worst case, but simple and correct).
- For large projects, can be optimized to Kasai's O(n) algorithm later.
- Filters matches to exclude same-file duplicates (file_a != file_b check).

### 4.2 Integrate into Runner Module

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/mod.rs`

Add near the top (with other `mod` declarations):
```rust
pub(crate) mod suffix_array;
```

### Acceptance Criteria (Phase 4)
- [ ] `find_duplicates()` compiles and unit tests pass.
- [ ] Suffix array construction is correct (verify via unit tests).
- [ ] Returns no duplicates for single file.
- [ ] Returns duplicates for multi-file identical blocks.
- [ ] Respects threshold (no matches below threshold).
- [ ] Filters same-file duplicates correctly.
- [ ] `cargo test --package biome_cli suffix_array` passes.

---

## Phase 5: CLI Integration & Diagnostics in before_finalize()

**Goal:** Run the duplicate detection after all files are processed, and emit diagnostics.

### 5.1 Extend finalizer.rs with CheckFinalizer

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/src/runner/finalizer.rs`

Find the `CheckFinalizer` struct and its `Finalizer` trait implementation. The trait is:

```rust
pub trait Finalizer {
    type Input;

    fn before_finalize(
        _project_key: ProjectKey,
        _fs: &dyn FileSystem,
        _workspace: &dyn Workspace,
        _crawler_output: &mut Self::Input,
    ) -> Result<(), CliDiagnostic>;
}
```

**Extend the implementation** to handle duplicate detection:

```rust
use crate::runner::{suffix_array, token_store::TokenStore};
use biome_cli_internal::traversal::TraverseResult;
use biome_diagnostics::Error;
use std::sync::Arc;

impl Finalizer for CheckFinalizer {
    type Input = TraverseResult;

    fn before_finalize(
        _project_key: ProjectKey,
        _fs: &dyn FileSystem,
        _workspace: &dyn Workspace,
        crawler_output: &mut Self::Input,
    ) -> Result<(), CliDiagnostic> {
        // Extract token store from thread-local storage (set in Phase 2.5).
        if let Some(token_store) = get_cross_file_token_store() {
            // Drain all collected tokens.
            match token_store.drain_files() {
                Ok(files) => {
                    if files.len() >= 2 {
                        // Run duplicate detection.
                        let threshold = token_store.threshold;
                        let matches = suffix_array::find_duplicates(&files, threshold);

                        // Convert matches to diagnostics and push to crawler_output.
                        for m in matches {
                            if let Err(e) = emit_duplicate_diagnostic(
                                &mut crawler_output.diagnostics,
                                &files,
                                &m,
                            ) {
                                eprintln!("Warning: failed to emit duplicate diagnostic: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: failed to drain token store: {}", e);
                    // Don't fail the entire check; continue.
                }
            }
        }

        Ok(())
    }
}

/// Thread-local storage for the token store (to be passed to before_finalize).
/// 
/// THREADING SAFETY: before_finalize() is called synchronously on the MAIN THREAD
/// after Crawler::crawl() exits its thread::scope() (see crawler.rs:45-74 and
/// mod.rs:316). The thread-local store set in CommandRunner::run() (Phase 2.5)
/// is safe to access here because we're on the same main thread throughout.
/// (Worker threads in the thread::scope never persist past before_finalize().)
thread_local! {
    static CROSS_FILE_TOKEN_STORE: std::cell::RefCell<Option<Arc<TokenStore>>> =
        std::cell::RefCell::new(None);
}

/// Helper: store the token store for later access in before_finalize.
pub fn set_cross_file_token_store(store: Arc<TokenStore>) {
    CROSS_FILE_TOKEN_STORE.with(|ts| {
        *ts.borrow_mut() = Some(store);
    });
}

/// Helper: retrieve the token store.
fn get_cross_file_token_store() -> Option<Arc<TokenStore>> {
    CROSS_FILE_TOKEN_STORE.with(|ts| {
        ts.borrow().clone()
    })
}

/// Convert a DuplicateMatch into two separate Errors (one per file) and push to the vec.
/// Biome diagnostics API does not support cross-file related spans, so we emit two separate
/// errors: one for each file, each pointing to the other file's occurrence.
fn emit_duplicate_diagnostic(
    diagnostics: &mut Vec<Error>,
    files: &[crate::runner::token_store::TokenizedFile],
    m: &suffix_array::DuplicateMatch,
) -> Result<(), String> {
    use biome_analyze::RuleDiagnostic;
    use biome_diagnostics::{category, DiagnosticExt, Error};
    use biome_rowan::{TextRange, TextSize};

    let file_a = &files[m.file_a];
    let file_b = &files[m.file_b];

    // Compute approximate line numbers from byte offsets
    let line_a = file_a.source[..m.offset_a as usize]
        .chars().filter(|&c| c == '\n').count() + 1;
    let line_b = file_b.source[..m.offset_b as usize]
        .chars().filter(|&c| c == '\n').count() + 1;

    // Compute end offsets from token count in offsets array
    let end_a = file_a.offsets
        .get(m.start_token_idx_a + m.length)
        .copied()
        .unwrap_or(m.offset_a + 1);
    let end_b = file_b.offsets
        .get(m.start_token_idx_b + m.length)
        .copied()
        .unwrap_or(m.offset_b + 1);

    // Emit diagnostic for file_a
    let diag_a = RuleDiagnostic::new(
        category!("lint/nursery/noDuplicateCode"),
        TextRange::new(
            TextSize::from(m.offset_a),
            TextSize::from(end_a),
        ),
        format!(
            "Duplicate code block ({} tokens) — also found in {}:{}",
            m.length, file_b.file_path, line_b
        ),
    );
    diagnostics.push(
        Error::from(diag_a)
            .with_file_path(file_a.file_path.clone())
            .with_file_source_code(file_a.source.clone()),
    );

    // Emit diagnostic for file_b
    let diag_b = RuleDiagnostic::new(
        category!("lint/nursery/noDuplicateCode"),
        TextRange::new(
            TextSize::from(m.offset_b),
            TextSize::from(end_b),
        ),
        format!(
            "Duplicate code block ({} tokens) — also found in {}:{}",
            m.length, file_a.file_path, line_a
        ),
    );
    diagnostics.push(
        Error::from(diag_b)
            .with_file_path(file_b.file_path.clone())
            .with_file_source_code(file_b.source.clone()),
    );

    Ok(())
}
```

**Design note:** The exact API for creating diagnostics may differ in Biome. Check `biome_diagnostics` module to confirm the correct way to construct a `GenericDiagnostic` or use `RuleDiagnostic` with the rule from Phase 1.

### Acceptance Criteria (Phase 5)
- [ ] `CheckFinalizer::before_finalize()` compiles.
- [ ] Token store is accessible (thread-local, set in Phase 2.5).
- [ ] Diagnostics are correctly converted and pushed to `crawler_output.diagnostics`.
- [ ] `biome check` output includes duplicate warnings (manual test below).
- [ ] Related spans point to the correct file and offset.
- [ ] `cargo build --package biome_cli` passes.

---

## Phase 6: Testing

### 6.1 Unit Tests: Token Serializer (already in Phase 3.1)

Run:
```bash
cargo test --package biome_js_analyze token_stream
```

### 6.2 Unit Tests: Suffix Array (already in Phase 4.1)

Run:
```bash
cargo test --package biome_cli suffix_array
```

### 6.3 Integration Test: End-to-End

**File:** `/Users/jacob/Documents/GitHub/biome/crates/biome_cli/tests/integration_dupl.rs` (new, optional)

```rust
//! Integration test: end-to-end duplicate detection.
//! Run with: cargo test --package biome_cli --test integration_dupl

#[test]
#[ignore] // Enable once full implementation is complete
fn test_biome_check_detects_duplicates() {
    use std::fs;
    use std::path::PathBuf;

    // Create test fixture directory.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let temp_path = temp_dir.path();

    // Create two files with a large duplicate block (>100 tokens).
    let code = r#"
const veryLongFunction = (a, b, c, d, e, f, g, h) => {
    const x = a + b;
    const y = c + d;
    const z = e + f;
    const w = g + h;
    const sum = x + y + z + w;
    if (sum > 100) {
        console.log("very large sum", sum);
        return sum * 2;
    } else if (sum > 50) {
        console.log("large sum", sum);
        return sum * 1.5;
    } else {
        console.log("small sum", sum);
        return sum;
    }
};
"#;

    let file1 = temp_path.join("file1.js");
    let file2 = temp_path.join("file2.js");

    fs::write(&file1, code).expect("write file1");
    fs::write(&file2, code).expect("write file2");

    // Run `biome check` on the temp directory.
    let output = std::process::Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "biome",
            "--",
            "check",
            temp_path.to_str().unwrap(),
        ])
        .output()
        .expect("run biome check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check that duplicate warning appears in output.
    let output_combined = format!("{}\n{}", stdout, stderr);
    assert!(
        output_combined.contains("noDuplicateCode") || output_combined.contains("Duplicate"),
        "biome check output should contain duplicate warning.\nStdout: {}\nStderr: {}",
        stdout,
        stderr
    );
}
```

### 6.4 Manual Integration Test

```bash
# Create test fixture directory
mkdir -p /tmp/biome_dupl_test
cd /tmp/biome_dupl_test

# Create file1.js with a large function (>100 tokens)
cat > file1.js << 'EOF'
const veryLongFunction = (a, b, c, d, e, f, g, h) => {
    const x = a + b;
    const y = c + d;
    const z = e + f;
    const w = g + h;
    const sum = x + y + z + w;
    if (sum > 100) {
        console.log("very large sum", sum);
        return sum * 2;
    } else if (sum > 50) {
        console.log("large sum", sum);
        return sum * 1.5;
    } else {
        console.log("small sum", sum);
        return sum;
    }
};
EOF

# Copy to file2.js (identical)
cp file1.js file2.js

# Run biome check
cd /Users/jacob/Documents/GitHub/biome
cargo biome-cli-dev check /tmp/biome_dupl_test

# Expected output:
# Should include a warning about duplicate code in file1.js and file2.js
```

### 6.5 Test Commands Summary

```bash
# Unit tests: token serializer
cargo test --package biome_js_analyze token_stream

# Unit tests: suffix array
cargo test --package biome_cli suffix_array

# All tests for biome_cli
cargo test --package biome_cli

# Full workspace tests (comprehensive)
cargo test --workspace

# Manual integration test
mkdir -p /tmp/test_dupl
# ... (create fixture files as above)
cargo biome-cli-dev check /tmp/test_dupl
```

### Acceptance Criteria (Phase 6)
- [ ] All unit tests pass: `cargo test --workspace`.
- [ ] Token serializer correctly extracts tokens and offsets.
- [ ] Suffix array correctly identifies duplicates >= threshold.
- [ ] Integration test reports duplicate diagnostics in `biome check` output.
- [ ] `biome-ignore lint/nursery/noDuplicateCode` comment suppresses diagnostics (verify manually).
- [ ] Related spans point to correct locations.

---

## Phase 7: Codegen & Build

### 7.1 Run Code Generation

```bash
cd /Users/jacob/Documents/GitHub/biome

# Generate analyzer metadata and rule schema
just gen-analyzer

# Generate Biome rule options (if needed)
just gen-bindings
```

### 7.2 Build and Test

```bash
# Build all crates
cargo build --workspace

# Run full test suite
cargo test --workspace

# Specific tests for this feature
cargo test --package biome_js_analyze noDuplicateCode
cargo test --package biome_cli noDuplicateCode
```

### 7.3 Manual Verification

```bash
# Verify rule appears in help
cargo biome-cli-dev explain noDuplicateCode

# Verify configuration works
cat > /tmp/biome_test/biome.json << 'EOF'
{
  "nursery": {
    "noDuplicateCode": {
      "level": "warn",
      "options": {
        "threshold": 50
      }
    }
  }
}
EOF

cargo biome-cli-dev check /tmp/biome_test

# With higher threshold (should find fewer duplicates)
cat > /tmp/biome_test/biome.json << 'EOF'
{
  "nursery": {
    "noDuplicateCode": {
      "level": "warn",
      "options": {
        "threshold": 200
      }
    }
  }
}
EOF

cargo biome-cli-dev check /tmp/biome_test
```

### Acceptance Criteria (Phase 7)
- [ ] `just gen-analyzer` completes without errors.
- [ ] `cargo build --workspace` succeeds.
- [ ] `cargo test --workspace` passes (no regressions).
- [ ] `biome explain noDuplicateCode` outputs rule documentation.
- [ ] Rule responds to configuration in `biome.json`.
- [ ] `biome check` produces duplicate diagnostics for test files.
- [ ] Threshold parameter changes detection behavior (50 tokens vs. 200 tokens).

---

## Known Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Token store lock contention | Slow multi-threaded file processing | Use `Arc<Mutex<...>>` with minimal lock scope; consider `parking_lot::Mutex` if contention proves severe. Profile first. |
| Suffix array O(n²) LCP construction | Memory/time explosion on large projects | Implement Kasai's O(n) algorithm in Phase 4 optimization. For now, acceptable for typical projects (<5K files). |
| AST no longer available in `before_finalize()` | Can't re-parse or re-tokenize on demand | Store all tokens during `process_file()`. Correct architecture; no re-parsing needed. **THIS WAS THE KEY CORRECTION FROM ARCHITECT REVIEW.** |
| Configuration reading in finalizer | Threshold not available in `before_finalize()` | Store threshold in `TokenStore` at construction time (in `CommandRunner::run()`). Done in Phase 2.5. |
| Cross-file diagnostic spans | Incorrect byte ranges in errors | Test thoroughly with fixture files. Verify offsets map correctly via unit tests. |
| Rule stub always returns `None` | Users confused why rule doesn't fire per-file | Document clearly in rule docstring that it's a cross-file pass (Phase 1.1). Done. |
| Existing tests break | Regression in other lint rules | Run `cargo test --workspace` after each phase. If tests fail, revert and re-investigate. |
| Thread-local storage for token store | Unclear where token store is passed | Set in `CommandRunner::run()` (Phase 2.5), retrieved in `before_finalize()` (Phase 5.1). Both use same thread-local; safe. |

---

## Success Criteria (Overall)

- [ ] **Rule registration:** `biome explain noDuplicateCode` outputs documentation.
- [ ] **Configuration:** `biome.json` accepts threshold option; threshold changes detection behavior.
- [ ] **Diagnostics:** `biome check` reports duplicates with file path + location for each occurrence.
- [ ] **Token matching:** Exact token matching (trivia stripped) is correct.
- [ ] **Threshold:** Changes to threshold (e.g., 50 vs. 200) change what is reported.
- [ ] **Nursery status:** Rule is off by default; requires `biome.json` opt-in.
- [ ] **Suppressions:** `biome-ignore lint/nursery/noDuplicateCode` works.
- [ ] **No regressions:** All existing Biome tests continue to pass.
- [ ] **Cross-file only:** Duplicates across files are detected; same-file duplicates are filtered (file_a != file_b check in Phase 4).
- [ ] **Performance acceptable:** Full test suite completes in < 5 minutes (includes new tests).

---

## Files Reference Summary

### New Files (to be created)
1. `crates/biome_js_analyze/src/lint/nursery/no_duplicate_code.rs` — Rule stub + options
2. `crates/biome_js_analyze/src/lint/nursery/no_duplicate_code/token_stream.rs` — Token serializer
3. `crates/biome_cli/src/runner/token_store.rs` — TokenStore struct
4. `crates/biome_cli/src/runner/suffix_array.rs` — Duplicate detection algorithm
5. `crates/biome_cli/tests/integration_dupl.rs` — Integration tests (optional)

### Modified Files (to be updated)
1. `crates/biome_cli/src/runner/crawler.rs` — Add `token_store` field to `CrawlerOptions`, add trait method to `CrawlerContext`
2. `crates/biome_cli/src/runner/process_file.rs` — Call tokenizer in `CheckProcessFile::process_file()`
3. `crates/biome_cli/src/runner/finalizer.rs` — Implement `before_finalize()` for duplicate detection + thread-local storage helpers
4. `crates/biome_cli/src/runner/mod.rs` — Add module declarations, wire up store construction

### Auto-Updated (codegen)
- Rule schema, documentation, metadata (after `just gen-analyzer`)
- Options schema (after `just gen-bindings`, if needed)

---

## Implementation Order & Checkpoint

1. **Phase 1:** Rule + options (least risk; prove schema works) — **~30 min**
2. **Phase 2:** Token store infrastructure (shared state) — **~1 hour**
3. **Phase 3:** Token collection in `process_file()` (populate store) — **~1 hour**
4. **Phase 4:** Suffix tree algorithm (core logic) — **~2 hours**
5. **Phase 5:** Finalizer integration (emit diagnostics) — **~1.5 hours**
6. **Phase 6:** Testing (verify end-to-end) — **~1 hour**
7. **Phase 7:** Codegen & build (finalize) — **~30 min**

**Total estimated time:** 7-8 hours (single engineer)

Each phase has clear acceptance criteria; move to next only when previous passes tests.

---

## Open Questions & Decisions Deferred

1. **Configuration reading:** Currently hardcoded to 100 in Phase 2.5. Should read from `workspace.get_configuration(project_key)` and extract the threshold. Implementation detail; can be done later.

2. **Related span UI in diagnostics:** Biome's diagnostic API may not support multiple related spans. If limited to one, pick the "primary" occurrence (first file alphabetically, first offset) as the primary and link to the second. Verify with `biome_diagnostics` API.

3. **LSP integration:** Deferred to v2. For now, CLI-only via `biome check`.

4. **Identifier normalization:** Deferred to v2. Token stream is ready to abstract identifiers (replace all identifier SyntaxKinds with a single abstract token); implement later.

5. **Kasai's LCP algorithm:** For small projects, O(n²) LCP is fine. For large projects, implement Kasai's O(n) in optimization phase.

6. **ExactToken value (u16::MAX sentinel):** May need adjustment if u16::MAX legitimately appears in token stream. Use a configurable sentinel or reserved token type.

---

## Architecture Decisions Explained

### Why CLI layer, not rule layer?

Biome's rule trait is single-file; a rule sees only its own file's AST. Cross-file analysis requires:
- Access to all files after they're parsed ✓ (in `before_finalize()`)
- A place to accumulate state across files ✓ (TokenStore in CrawlerOptions)
- A post-processing pass to emit diagnostics ✓ (before_finalize)

The CLI finalizer (`before_finalize()`) is called once after all per-file processing, making it the natural insertion point. **This is the key architectural insight from the Architect review.**

### Why suffix array, not Ukkonen tree?

- **Correctness:** Simpler to implement correctly the first time.
- **Performance:** O(n log n) construction; acceptable for typical codebases.
- **Extensibility:** LCP array is the natural data structure for finding repeating patterns; easy to optimize to Kasai's O(n) later.
- **Maturity:** Well-understood algorithm; lower risk than implementing Ukkonen from scratch.

### Why thread-local for token store?

The `before_finalize()` signature doesn't provide context; it receives only file system, workspace, and project key. The token store must be passed out-of-band via thread-local storage or a static. Thread-local is safe (no global state per thread) and simple. Can be refactored to context-passing later if needed. Set in `CommandRunner::run()`, retrieved in `before_finalize()`.

### Why strip trivia?

Trivia (whitespace, comments) is cosmetic; duplicates are semantically equivalent regardless of formatting. Stripping trivia reduces the token alphabet, making matching simpler and more focused on actual code logic.

### Why collect tokens in process_file(), not before_finalize()?

**CRITICAL CORRECTION FROM ARCHITECT:** `before_finalize()` has NO access to AST or file content. It only receives `TraverseResult` (diagnostics vec + summary stats). By finalization time, ASTs are gone. Therefore:

- **Token collection MUST happen in `process_file()`** where `workspace_file` (containing the AST) is available.
- **Analysis HAPPENS in `before_finalize()`** where collected tokens are available via the shared TokenStore.

This is the key architectural fix from the Architect review.

---

## Next Steps (Post-Implementation)

1. **Merge to main:** Create PR with AI assistance disclosure (per CLAUDE.md).
2. **Collect feedback:** Monitor for user reports of false positives or configuration issues.
3. **v1.1 improvements:**
   - Optimize LCP construction (Kasai's algorithm) for large projects
   - Support more languages (CSS, Go, Rust) — each requires separate tokenizer
   - Add identifier normalization (abstract variable names)
   - Improve diagnostic span accuracy (store end-offset in DuplicateMatch)
4. **v2 (future):**
   - LSP real-time detection (if performance allows)
   - Refactoring suggestions / code actions
   - Near-duplicate detection (fuzzy matching)

---

**Plan Version:** 4 (Final — Critic APPROVED after 3 iterations)  
**Plan Date:** 2026-05-13  
**Based on:** Deep Interview Spec v1 (Ambiguity: 13%)  
**Ready for:** Executor (Phase 1 start)
