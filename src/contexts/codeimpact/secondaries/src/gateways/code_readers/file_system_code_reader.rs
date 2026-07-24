use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::overrides::{Override, OverrideBuilder};
use ignore::WalkBuilder;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_WALK_DEPTH: usize = 128;
const ERR_FILE_NOT_FOUND: &str = "fichier introuvable";
const ERR_INVALID_GLOB: &str = "motif de filtrage invalide (syntaxe glob)";

/// Compiles `patterns` into a matchable `GlobSet` (D1: glob compilation is
/// an adapter concern — `FileFilter` itself carries only validated raw
/// patterns, never a compiled matcher, so the hexagon stays zero-dep).
/// A pattern that is syntactically invalid glob surfaces as an anonymized
/// `AnalysisError` (AC4/ADR-0006) rather than a panic.
fn build_glob_set(patterns: &[String]) -> Result<GlobSet, AnalysisError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))
}

/// Registers `patterns` (already filtered to the dialect-safe subset — see
/// `partition_exclude_patterns`) as walk-time `ignore::overrides::Override`
/// patterns (#96, perf): each pattern is added NEGATED (`!pattern`), which
/// makes `ignore::WalkBuilder` PRUNE a matching subtree during descent
/// instead of yielding every entry underneath it for the post-walk `keep`
/// check below — measured ~34x faster for a 20.6k-file excluded subtree.
///
/// `include` deliberately stays on the post-walk `GlobSet` above rather
/// than moving here too. `ignore::dir::Ignore::matched` gives overrides
/// absolute precedence: ANY override match (whitelist or negated) makes
/// walk-level matching stop and skip the gitignore check entirely for that
/// path. Adding `include` as non-negated (whitelist) overrides would let an
/// include pattern resurrect a file `.gitignore` says to drop — a
/// correctness regression this ticket must not introduce. A negated-only
/// override set never enables that whitelist short-circuit (confirmed by
/// the `ignore` crate's own `only_ignores` test), so moving `exclude` alone
/// is safe: unmatched paths fall through to gitignore exactly as before.
fn build_exclude_overrides(root: &Path, patterns: &[String]) -> Result<Override, AnalysisError> {
    let mut builder = OverrideBuilder::new(root);
    for pattern in patterns {
        builder
            .add(&format!("!{pattern}"))
            .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))?;
    }
    builder
        .build()
        .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))
}

/// Retry #1 (QA CRITICAL, reproduced): moving `exclude` wholesale to
/// walk-time `ignore::overrides::Override` silently swapped the glob
/// DIALECT, not just the evaluation point. Pre-#96, `exclude` matched via
/// `globset::Glob::new` against the ENTIRE relative path (anchored/exact —
/// ADR-0019 §4). `ignore`'s gitignore-line syntax disagrees with that on
/// two points that change RESULTS:
///   1. a pattern with no `/` matches the file's BASENAME AT ANY DEPTH
///      (gitignore semantics) instead of the literal full relative path
///      (globset semantics) — a bare `"generated"` would prune the whole
///      `generated/` subtree instead of matching nothing (the old
///      behavior, since no nested path is literally equal to
///      "generated").
///   2. a single `*` never crosses `/` in gitignore-line syntax, but DOES
///      cross it in globset's default `Glob` (`literal_separator` is
///      `false` unless a `GlobBuilder` says otherwise) — an anchored
///      pattern like `"src/*.rs"` would only prune direct children of
///      `src/` instead of every nested `.rs` file under it too.
///
/// Only patterns of the EXACT shape `<literal>/**` are provably immune to
/// both: `globset`'s own doc comment ("if the glob ends with /**, then it
/// matches all sub-entries... but not foo") and the gitignore spec ("a
/// trailing '/**' matches everything inside... with infinite depth")
/// describe the IDENTICAL semantics for that one shape, and neither
/// dialect's single-`*` behavior comes into play since the only wildcard
/// used is the trailing `**`. Verified empirically against globset 0.4.19
/// and ignore 0.4.31 before writing this fix.
///
/// Every other exclude pattern shape is routed to the post-walk `GlobSet`
/// fallback instead (same code path as pre-#96, byte-identical by
/// construction) rather than the walk-time `Override`.
fn is_dialect_safe_prune_pattern(pattern: &str) -> bool {
    match pattern.strip_suffix("/**") {
        Some(prefix) => {
            !prefix.is_empty()
                && !prefix.contains(['*', '?', '[', ']', '!', '{', '}'])
                && prefix.split('/').all(|segment| !segment.is_empty())
        }
        None => false,
    }
}

/// Splits `exclude` into the walk-time-prunable subset (dialect-safe) and
/// the subset that must stay on the post-walk `GlobSet` fallback to
/// preserve pre-#96 result identity (see `is_dialect_safe_prune_pattern`).
fn partition_exclude_patterns(patterns: &[String]) -> (Vec<String>, Vec<String>) {
    let mut walk_time_safe = Vec::new();
    let mut post_walk_fallback = Vec::new();
    for pattern in patterns {
        if is_dialect_safe_prune_pattern(pattern) {
            walk_time_safe.push(pattern.clone());
        } else {
            post_walk_fallback.push(pattern.clone());
        }
    }
    (walk_time_safe, post_walk_fallback)
}

#[derive(Default)]
pub struct FileSystemCodeReader;

impl FileSystemCodeReader {
    pub fn new() -> Self {
        Self
    }
}

impl CodeReader for FileSystemCodeReader {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        let path = target.path();
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()))?;

        let metadata = std::fs::metadata(&canonical)
            .map_err(|_| AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(AnalysisError::IoError(
                "fichier trop volumineux (max 10 Mo)".to_string(),
            ));
        }

        std::fs::read_to_string(&canonical).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                AnalysisError::IoError("permission refusée".to_string())
            }
            _ => AnalysisError::IoError("erreur de lecture".to_string()),
        })
    }

    fn list_source_files(
        &self,
        dir: &Path,
        extensions: &[&str],
        filter: &FileFilter,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        let canonical_root = std::fs::canonicalize(dir)
            .map_err(|_| AnalysisError::IoError("dossier introuvable".to_string()))?;

        let include_set = build_glob_set(filter.include())?;
        let include_is_empty = filter.include().is_empty();
        let respect_gitignore = filter.respect_gitignore();
        let (walk_time_exclude, fallback_exclude) = partition_exclude_patterns(filter.exclude());
        let fallback_exclude_set = build_glob_set(&fallback_exclude)?;
        let fallback_exclude_is_empty = fallback_exclude.is_empty();
        let exclude_overrides = build_exclude_overrides(&canonical_root, &walk_time_exclude)?;

        let mut files = Vec::new();
        let walker = WalkBuilder::new(&canonical_root)
            .follow_links(false)
            .max_depth(Some(MAX_WALK_DEPTH))
            .hidden(true)
            .overrides(exclude_overrides)
            // `ignore`'s WalkBuilder exposes FOUR independent ignore-source
            // toggles (git_ignore/.gitignore, git_exclude/.git/info/exclude,
            // git_global/the user's global gitignore, ignore/.ignore files)
            // — all default to `true`. Gating only `git_ignore` left the
            // other three ON unconditionally, silently dropping files even
            // under `FileFilter::unrestricted()` (QA finding, retry 1).
            // Every source must move together with `respect_gitignore` so
            // "unrestricted" is byte-identical to the pre-US31 `walkdir`
            // walk, which honored none of them.
            .git_ignore(respect_gitignore)
            .git_exclude(respect_gitignore)
            .git_global(respect_gitignore)
            .ignore(respect_gitignore)
            // The walk root itself is not guaranteed to be an actual git
            // working tree (e.g. an extracted archive, a CI checkout
            // shallow-cloned without `.git`) — honoring `.gitignore` at the
            // root must not silently depend on that.
            .require_git(false)
            // `parents(false)` (Security finding, retry 1): the walker must
            // NEVER consult ignore state from OUTSIDE the analyzed
            // directory. `parents(true)` would read .gitignore/.ignore from
            // every ancestor up to `/`, letting a party outside the
            // repository hide source files from a shared CI host's
            // ancestor directories and evade the --strict energy/CO2 gate
            // (ADR-0017).
            .parents(false)
            .build();

        for entry in walker {
            match entry {
                Ok(entry) => {
                    let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
                    if !is_file {
                        continue;
                    }
                    let path = entry.path();
                    if !path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| extensions.contains(&ext))
                    {
                        continue;
                    }
                    let relative = path.strip_prefix(&canonical_root).unwrap_or(path);
                    let keep = include_is_empty || include_set.is_match(relative);
                    if !keep {
                        continue;
                    }
                    // Result-identity fallback (retry #1): any exclude
                    // pattern NOT dialect-safe for walk-time pruning still
                    // gets the pre-#96 post-walk globset check here.
                    if !fallback_exclude_is_empty && fallback_exclude_set.is_match(relative) {
                        continue;
                    }
                    match std::fs::metadata(path) {
                        Ok(meta) if meta.len() <= MAX_FILE_SIZE => {
                            files.push(path.to_path_buf());
                        }
                        Ok(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (trop volumineux): {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            );
                        }
                        Err(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (illisible): {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Avertissement: erreur d'accès: {}", e);
                }
            }
        }

        Ok(files)
    }

    /// Real canonicalization (US16 T5, Security CRITICAL retry #1) —
    /// falls back to `dir` unchanged when it does not exist on disk,
    /// mirroring `html/view_model.rs`'s `build_tree` fallback (the same
    /// representation-mismatch class of bug, fixed the same way there).
    fn canonical_root(&self, dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
    }
}
