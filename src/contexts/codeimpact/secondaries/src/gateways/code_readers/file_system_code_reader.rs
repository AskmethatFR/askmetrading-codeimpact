use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use codeimpact_hexagon::analysis::sanitize_console_text;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
use codeimpact_hexagon::analysis::SourceFileListing;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::overrides::{Override, OverrideBuilder};
use ignore::WalkBuilder;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_WALK_DEPTH: usize = 128;
/// Total walk-entry cap (#95, Security DoS residual, ADR-0006) —
/// `MAX_WALK_DEPTH` bounds how DEEP the walk goes and `MAX_FILE_SIZE`
/// bounds how BIG one file can be, but neither bounds how MANY entries a
/// single shallow directory can hold. A directory with hundreds of
/// thousands of small files at shallow depth (a planted/generated tree
/// not gitignored, plausible under `respectGitignore:false`) was fully
/// enumerated with no early abort. Trust model stays "user points the
/// tool at their own codebase" (ADR-0006), so this is a residual guard,
/// not hardening against an adversarial filesystem: 50 000 sits
/// generously above any legitimate single-language codebase's file count
/// while stopping well before "hundreds of thousands or millions" —
/// chosen small enough to also keep the boundary test's fixture
/// construction fast.
const MAX_WALK_ENTRIES: usize = 50_000;
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

/// Whether `path` even qualifies as a measurement candidate: matches one of
/// the registered `extensions` AND satisfies `include` (empty `include`
/// means unrestricted). Extracted out of the `Ok` arm's inline checks below
/// (#128 retry 2) — zero behavior change there, still exercised by every
/// extension/include fixture in the integration suite.
///
/// #128 retry 2, Security MINOR finding 3: the walker's `Err` arm used to
/// count ANY named file as unmeasurable without re-checking eligibility —
/// an out-of-scope file (wrong extension, explicitly excluded) that
/// happened to trip a walker-level access error would inflate the count.
/// A fix reusing this SAME predicate in the `Err` arm was written and then
/// REVERTED: six independent real-filesystem probes (an unreadable
/// `.gitignore`, a malformed `.gitignore`, a no-exec directory holding a
/// registered- and an unregistered-extension file, an invalid-UTF-8
/// filename) all failed to make `ignore::Walk` ever yield
/// `Err(WithPath{path})` naming a real, still-existing FILE on this stack
/// (macOS/APFS, `ignore` 0.4.31) — every reachable `WithPath` names a
/// DIRECTORY whose `read_dir()` itself failed (already handled above,
/// `unexplored_subtree`). The mutation gate confirmed this empirically: the
/// eligibility check, once added, had zero live test reaching it and
/// survived every mutation. Shipping unreachable, unverified branching is
/// worse than the asymmetry it claimed to close (cc-yagni) — see the
/// developer's field-6 answer for the full accounting.
fn matches_extension_and_include(
    path: &Path,
    canonical_root: &Path,
    extensions: &[&str],
    include_set: &GlobSet,
    include_is_empty: bool,
) -> bool {
    let has_registered_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.contains(&ext));
    if !has_registered_extension {
        return false;
    }
    let relative = path.strip_prefix(canonical_root).unwrap_or(path);
    include_is_empty || include_set.is_match(relative)
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
/// #34 T2 extends this to the shape `**/<literal>/**` (e.g.
/// `**/node_modules/**`, `DEFAULT_EXCLUDES`) for the SAME reason, by the
/// SAME method: `globset`'s doc for a leading `**` component ("a sequence
/// of `**` ... matches zero or more path components") and the gitignore
/// spec's identical rule for a leading `**/` ("a leading '**' followed by a
/// slash means match in all directories") describe the SAME semantics —
/// "match this literal component at any depth" — and the trailing `/**`
/// still contributes the same "everything inside, infinite depth" reading
/// analyzed above. No single `*` is present in either dialect's reading of
/// this shape, so point 2 above still never applies. Verified empirically
/// against the same two pinned crate versions:
/// `globset::Glob::new("**/node_modules/**")` matches
/// `packages/x/node_modules/e.js` and `a/b/c/node_modules/deep/e.js` but
/// not a bare `node_modules` entry or an unrelated `not_node_modules/e.js`;
/// an `ignore::overrides::Override` built from `"!**/node_modules/**"`
/// prunes descent into `packages/x/node_modules/` and `a/b/c/node_modules/`
/// during a real walk, for the identical match set. This matters beyond
/// the identical result set: `MAX_WALK_ENTRIES` counts every entry the
/// walker VISITS, regardless of where it lands afterward — a nested
/// `node_modules/` (an npm workspace shape a root-anchored
/// `node_modules/**` alone does not reach) large enough to exceed the cap
/// would still trip it under a fallback-only match, since the fallback
/// only filters AFTER the walker has already visited (and counted) every
/// entry underneath it.
///
/// Every other exclude pattern shape is routed to the post-walk `GlobSet`
/// fallback instead (same code path as pre-#96, byte-identical by
/// construction) rather than the walk-time `Override`.
fn is_dialect_safe_prune_pattern(pattern: &str) -> bool {
    let Some(prefix) = pattern.strip_suffix("/**") else {
        return false;
    };
    let literal = prefix.strip_prefix("**/").unwrap_or(prefix);
    !literal.is_empty()
        && !literal.contains(['*', '?', '[', ']', '!', '{', '}'])
        && literal.split('/').all(|segment| !segment.is_empty())
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

/// What a walker access error's named path resolves to when independently
/// re-checked (#128 retry 2 Security HIGH / retry 3 Security HIGH, was
/// ticket #149). `ignore::Error::WithPath` names a path for SOME variants
/// and not others, and even a named path may be a DIRECTORY the walker
/// could not descend into, not a single measurable file — so the re-stat
/// below is the only honest source of truth, never a guess from the error
/// text.
///
/// Extracted out of the walker's `Err` arm (retry 3) so the "the re-stat
/// itself fails" branch is directly, deterministically testable: on a real,
/// single-threaded walk over a filesystem nobody is concurrently mutating,
/// that branch is unreachable through `ignore::Walk` alone (confirmed
/// empirically — even a tuned background-thread race against a 3000-decoy
/// walk won the exact window under 50% of the time, unusable for a
/// non-flaky CI test), but the SAME `std::fs::metadata` failure it depends
/// on is trivially reproduced by pointing this function at a path that was
/// never there to begin with (`FileSystemCodeReader`'s own inline test
/// module below, not the adapter's `list_source_files` integration
/// boundary — a deliberate, documented exception to this file's usual
/// "helpers are tested only through `list_source_files`" convention,
/// justified by the OS-race unreproducibility above).
enum WalkErrorAttribution {
    /// The path names a directory whose own listing failed — the walk
    /// could not enumerate what is underneath (retry 2), OR the re-stat
    /// itself failed and the path no longer resolves to anything at all
    /// (retry 3, TOCTOU: it vanished between the walker's own error and
    /// this check). Either way we cannot know whether a file or a whole
    /// subtree was lost, so the honest, conservative answer folds both
    /// into the SAME unquantified signal as a truncated subtree, never
    /// silence.
    UnexploredSubtree,
    /// The path names one, precisely-identified, still-readable-as-
    /// metadata file — precise enough to attribute to `dropped_files`.
    DroppedFile,
    /// The path resolves to neither a directory nor a file (unreached in
    /// practice on this stack — see `matches_extension_and_include`'s doc
    /// for the sibling precedent of an unreachable branch, reverted rather
    /// than shipped unverified). Preserved as a no-op, exactly the silent
    /// fallthrough this had before the re-stat was extracted into its own
    /// function.
    Unattributable,
}

fn classify_walk_error_path(path: &Path) -> WalkErrorAttribution {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => WalkErrorAttribution::UnexploredSubtree,
        Ok(meta) if meta.is_file() => WalkErrorAttribution::DroppedFile,
        Ok(_) => WalkErrorAttribution::Unattributable,
        // #128 retry 3 (Security HIGH, was ticket #149): TOCTOU — the
        // path named by the walker's own error no longer resolves to
        // anything at all. We cannot know whether a file or a whole
        // subtree was lost, so — like the sibling directory branch above
        // — the conservative, honest answer is "something was lost, and
        // we don't know how much," never silence.
        Err(_) => WalkErrorAttribution::UnexploredSubtree,
    }
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
    ) -> Result<SourceFileListing, AnalysisError> {
        let canonical_root = std::fs::canonicalize(dir)
            .map_err(|_| AnalysisError::IoError("dossier introuvable".to_string()))?;

        let include_set = build_glob_set(filter.include())?;
        let include_is_empty = filter.include().is_empty();
        let respect_gitignore = filter.respect_gitignore();
        let (walk_time_exclude, fallback_exclude) = partition_exclude_patterns(filter.exclude());
        let fallback_exclude_set = build_glob_set(&fallback_exclude)?;
        let fallback_exclude_is_empty = fallback_exclude.is_empty();
        let exclude_overrides = build_exclude_overrides(&canonical_root, &walk_time_exclude)?;
        // #34 T2 MED-1 (ADR-0010): a SEPARATE, additive glob check over
        // JUST the DEFAULT_EXCLUDES-derived subset of `exclude()` — built
        // once here, checked (read-only) alongside the EXISTING exclusion
        // decisions below, never replacing them. Matching against the full
        // relative path works uniformly for both a real file (the
        // **/*.min.js case, never walk-time-safe) and a directory PROBED
        // with a synthetic trailing component (see the walk loop below) —
        // globset's own `<literal>/**` / `**/<literal>/**` semantics
        // require "something after the literal" to match, which a bare
        // directory path never has on its own.
        let default_exclude_patterns: Vec<String> = filter
            .default_exclude_patterns()
            .into_iter()
            .map(String::from)
            .collect();
        let default_exclude_set = build_glob_set(&default_exclude_patterns)?;
        let default_exclude_is_empty = default_exclude_patterns.is_empty();
        // #147 (Volet B): the analyzed repo's OWN exclude patterns —
        // `filter.exclude()` is the union of user patterns + standing
        // defaults, so the user subset is the union minus the exact-string
        // default subset (same convention as
        // `FileFilter::default_exclude_patterns()`). Distinct count so a CI
        // has a field to branch on (ADR-0006's remediation overrides
        // thresholds, never the measured set). Split into the walk-time-safe
        // subset (the ONLY patterns that can prune a DIRECTORY during
        // descent — the dir probe below must be faithful to that, a
        // fallback-only pattern like `generated/*` would false-positive a
        // `<dir>/__probe__` match) and the full set (post-walk FILE
        // fallback, where any user pattern can drop a precise file).
        let user_exclude_patterns: Vec<String> = filter
            .exclude()
            .iter()
            .filter(|p| !default_exclude_patterns.contains(*p))
            .cloned()
            .collect();
        let user_exclude_set = build_glob_set(&user_exclude_patterns)?;
        let user_exclude_is_empty = user_exclude_patterns.is_empty();
        let (user_walk_time_exclude, _user_fallback) =
            partition_exclude_patterns(&user_exclude_patterns);
        let user_walk_time_exclude_set = build_glob_set(&user_walk_time_exclude)?;
        let user_walk_time_exclude_is_empty = user_walk_time_exclude.is_empty();

        let mut files = Vec::new();
        let mut default_excluded_count: usize = 0;
        // #147 (Volet B): counts the analyzed repo's OWN exclude patterns
        // (the union minus DEFAULT_EXCLUDES) — see
        // `SourceFileListing::user_excluded_count`.
        let mut user_excluded_count: usize = 0;
        let mut entries_visited: usize = 0;
        // Security HIGH (#128 retry 1): every walk-time drop below used to
        // ONLY `eprintln!` — the adapter observed the drop and never told
        // the gate. Each push here is paired 1:1 with the `eprintln!` right
        // next to it (never a NEW drop reason, just a NAMED one).
        let mut dropped_files: Vec<(PathBuf, UnmeasurableReason)> = Vec::new();
        // Security HIGH (#128 retry 2): an UNQUANTIFIED companion to
        // `dropped_files` — set when the walk left at least one directory
        // subtree unexplored, either because `MAX_WALK_DEPTH` truncated the
        // descent or because a subtree's own listing failed outright (a
        // permission-denied directory). Neither condition can honestly
        // populate `dropped_files` (that would require enumerating files
        // the walk never visited) — see `SourceFileListing::
        // unexplored_subtree`'s doc for why this stays a bool, never a
        // fabricated count.
        let mut unexplored_subtree = false;
        // #147 (Volet A): the crate's `hidden(true)` filter would silently
        // drop every `.`-prefixed entry before the consumer ever saw it —
        // the file vanished from the gated sum with no count on any
        // surface, coverage read `Complete`, `--strict` exited 0 on a
        // project that genuinely breached (Security demo on #128: `.heavy/`
        // vs `heavy/`). The skip stays (`.git/`, `.venv/` must not enter
        // the analysis) but the filter is turned OFF and re-applied HERE,
        // where the drop is countable: `filter_entry` runs after the
        // crate's own hidden/ignore checks (walk.rs `skip_entry`), so an
        // entry reaching it was NOT dropped for any other reason — counting
        // it here never double-counts a gitignore/override drop. The
        // closure prunes a hidden DIRECTORY before descent (one entry,
        // whatever lives inside). Deliberate, doc'd corner: a gitignore
        // WHITELIST of a hidden path (`!.heavy/`) no longer re-includes it
        // — hiddenness now wins over the whitelist, the honesty signal
        // takes precedence (the crate's own hidden check also applied only
        // when `m.is_none()`, so this only differs for a
        // whitelisted-then-hidden path, which previously reached the
        // measured set with no trace at all).
        let hidden_count = Arc::new(AtomicUsize::new(0));
        let hidden_count_in_filter = Arc::clone(&hidden_count);
        let walker = WalkBuilder::new(&canonical_root)
            .follow_links(false)
            .max_depth(Some(MAX_WALK_DEPTH))
            .hidden(false)
            .filter_entry(move |entry| {
                // Every `.`-prefixed entry (dir or file) is counted and
                // skipped — same scope as the crate's own `hidden(true)`
                // check (`is_hidden_entry`, which applies to ANY entry
                // regardless of extension), so the count answers exactly
                // "how many walk entries did hiddenness drop". A hidden
                // DIRECTORY is pruned before descent (one entry, whatever
                // lives inside); the depth-0 root is never a drop.
                if entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.') {
                    hidden_count_in_filter.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                true
            })
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
            entries_visited += 1;
            if entries_visited > MAX_WALK_ENTRIES {
                return Err(AnalysisError::IoError(format!(
                    "arborescence trop volumineuse (plus de {} entrées) — \
                     analyse interrompue avant d'énumérer le reste",
                    MAX_WALK_ENTRIES
                )));
            }
            match entry {
                Ok(entry) => {
                    let file_type = entry.file_type();
                    let is_file = file_type.map(|t| t.is_file()).unwrap_or(false);
                    if !is_file {
                        // Security HIGH (#128 retry 2): `WalkBuilder::
                        // max_depth` makes the walker yield the LAST
                        // directory entry it will ever descend into, then
                        // stop — silently, no `Ok` entry and no `Err` for
                        // anything underneath. A directory entry AT
                        // `MAX_WALK_DEPTH` is therefore the one honest
                        // signal available: "the walker will not go past
                        // here." Checked BEFORE the default-exclude probe
                        // below (independent facts, not mutually
                        // exclusive: a truncated subtree can also happen
                        // to match a default pattern).
                        let is_dir = file_type.map(|t| t.is_dir()).unwrap_or(false);
                        if is_dir && entry.depth() == MAX_WALK_DEPTH {
                            unexplored_subtree = true;
                        }
                        // #34 T2 MED-1: a DIRECTORY entry reaching here is
                        // either a normal directory the walker is about to
                        // descend into, or one whose contents were just
                        // PRUNED by a walk-time Override match (the walker
                        // still yields the directory entry itself, per
                        // `ignore`'s own traversal order, even though it
                        // never recurses into it — confirmed empirically).
                        // A bare directory path never satisfies a
                        // `<literal>/**` glob on its own (the trailing
                        // `/**` requires "something after"), so probe with
                        // a synthetic child component to ask "would
                        // anything under here match a default exclude" —
                        // if yes, this ONE walk entry represents an entire
                        // subtree pruned because of a standing default;
                        // count the ENTRY, not a file count we never
                        // computed (that's the whole point of pruning).
                        if !default_exclude_is_empty {
                            let relative = entry
                                .path()
                                .strip_prefix(&canonical_root)
                                .unwrap_or(entry.path());
                            let probe = relative.join("__codeimpact_default_excluded_probe__");
                            if default_exclude_set.is_match(&probe) {
                                default_excluded_count += 1;
                            } else if !user_walk_time_exclude_is_empty
                                && user_walk_time_exclude_set.is_match(&probe)
                            {
                                // #147 (Volet B): the same pruned-subtree
                                // probe, attributed to the analyzed repo's
                                // OWN pattern instead of a standing default.
                                user_excluded_count += 1;
                            }
                        }
                        continue;
                    }
                    let path = entry.path();
                    if !matches_extension_and_include(
                        path,
                        &canonical_root,
                        extensions,
                        &include_set,
                        include_is_empty,
                    ) {
                        continue;
                    }
                    let relative = path.strip_prefix(&canonical_root).unwrap_or(path);
                    // Result-identity fallback (retry #1): any exclude
                    // pattern NOT dialect-safe for walk-time pruning still
                    // gets the pre-#96 post-walk globset check here.
                    if !fallback_exclude_is_empty && fallback_exclude_set.is_match(relative) {
                        // #34 T2 MED-1: this FILE (not a pruned subtree —
                        // an exact, precisely-known entry) is additionally
                        // attributable to a standing default when its
                        // path ALSO matches the default-only subset.
                        if !default_exclude_is_empty && default_exclude_set.is_match(relative) {
                            default_excluded_count += 1;
                        } else if !user_exclude_is_empty && user_exclude_set.is_match(relative) {
                            // #147 (Volet B): same precise-file attribution
                            // for the analyzed repo's OWN pattern.
                            user_excluded_count += 1;
                        }
                        continue;
                    }
                    match std::fs::metadata(path) {
                        Ok(meta) if meta.len() <= MAX_FILE_SIZE => {
                            files.push(path.to_path_buf());
                        }
                        Ok(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (trop volumineux): {}",
                                sanitize_console_text(
                                    &path.file_name().unwrap_or_default().to_string_lossy()
                                )
                            );
                            dropped_files
                                .push((path.to_path_buf(), UnmeasurableReason::SourceTooLarge));
                        }
                        Err(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (illisible): {}",
                                sanitize_console_text(
                                    &path.file_name().unwrap_or_default().to_string_lossy()
                                )
                            );
                            dropped_files
                                .push((path.to_path_buf(), UnmeasurableReason::SourceUnreadable));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Avertissement: erreur d'accès: {}", e);
                    // Best effort only (retry 1): `ignore::Error` names a
                    // path for SOME variants (`WithPath`) and not others
                    // (e.g. a symlink `Loop`) — only attribute the drop
                    // when we can independently confirm what the errored
                    // path actually is (`classify_walk_error_path`), never
                    // guess from the error text alone.
                    if let ignore::Error::WithPath { path, .. } = &e {
                        match classify_walk_error_path(path) {
                            WalkErrorAttribution::UnexploredSubtree => {
                                unexplored_subtree = true;
                            }
                            WalkErrorAttribution::DroppedFile => {
                                dropped_files
                                    .push((path.clone(), UnmeasurableReason::SourceUnreadable));
                            }
                            WalkErrorAttribution::Unattributable => {}
                        }
                    }
                }
            }
        }

        Ok(SourceFileListing {
            files,
            default_excluded_count,
            user_excluded_count,
            // #147 (Volet A): read AFTER the walk — the counter lives in the
            // walker's own `filter_entry` closure (possibly worker threads),
            // accumulated there because skipped entries are never yielded to
            // this loop.
            hidden_excluded_count: hidden_count.load(Ordering::Relaxed),
            dropped_files,
            unexplored_subtree,
        })
    }

    /// Real canonicalization (US16 T5, Security CRITICAL retry #1) —
    /// falls back to `dir` unchanged when it does not exist on disk,
    /// mirroring `html/view_model.rs`'s `build_tree` fallback (the same
    /// representation-mismatch class of bug, fixed the same way there).
    fn canonical_root(&self, dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
    }
}

// `classify_walk_error_path` is tested HERE, inline, rather than only
// through `list_source_files` (this file's usual convention for its other
// private helpers, e.g. `is_dialect_safe_prune_pattern`) — a deliberate,
// documented exception. The Directory/File branches ARE already covered
// end-to-end by `file_system_code_reader_test.rs`'s
// `permission_denied_subtree_reports_unexplored_subtree` and
// `file_past_max_walk_depth_vanishes_and_reports_unexplored_subtree`. The
// NEW branch this retry adds (retry 3, was ticket #149) — the re-stat
// itself failing — is a genuine filesystem TOCTOU: empirically confirmed
// unreproducible through `ignore::Walk` without a flaky background-thread
// race (a tuned 3000-decoy-file race against `filter_entry`-free
// `list_source_files` won the exact window under 50% of the time). The
// SAME `std::fs::metadata` failure a real vanished-mid-walk path would
// produce is trivially and deterministically reproduced here with a path
// that was simply never on disk to begin with.
//
// Test List:
// 1. a real directory -> UnexploredSubtree
// 2. a real file -> DroppedFile
// 3. a path that was never on disk (metadata fails, retry 3's fix) ->
//    UnexploredSubtree, never silently unattributed
#[cfg(test)]
mod classify_walk_error_path_tests {
    use super::*;

    #[test]
    fn classify_walk_error_path_of_a_directory_is_unexplored_subtree() {
        let dir = std::env::temp_dir().join(format!(
            "codeimpact_classify_dir_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let outcome = classify_walk_error_path(&dir);

        assert!(matches!(outcome, WalkErrorAttribution::UnexploredSubtree));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classify_walk_error_path_of_a_file_is_dropped_file() {
        let dir = std::env::temp_dir().join(format!(
            "codeimpact_classify_file_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("victim.rs");
        std::fs::write(&file, "fn f() {}").unwrap();

        let outcome = classify_walk_error_path(&file);

        assert!(matches!(outcome, WalkErrorAttribution::DroppedFile));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Mutation gate survivor (#128 retry 3, cargo-mutants): `Ok(meta) if
    // meta.is_file()`'s guard mutated to `true` survived — the `is_dir()`
    // guard above already short-circuits both the real-directory and
    // real-file cases, so the ONLY path where that guard's actual value
    // matters is `Ok(meta)` naming neither a directory nor a regular
    // file. A FIFO (named pipe) is the portable, dependency-free way to
    // construct exactly that: `std::fs::metadata` resolves it (`Ok`), but
    // `is_dir()` and `is_file()` are both `false`.
    #[cfg(unix)]
    #[test]
    fn classify_walk_error_path_of_a_fifo_is_unattributable() {
        let dir = std::env::temp_dir().join(format!(
            "codeimpact_classify_fifo_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("neither_dir_nor_file");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo must be available on this platform to run this test");
        assert!(
            status.success(),
            "mkfifo must succeed to set up the fixture"
        );
        let meta = std::fs::metadata(&fifo).expect("a FIFO must be re-statable");
        assert!(
            !meta.is_dir() && !meta.is_file(),
            "fixture precondition: a FIFO must be neither a directory nor a regular file"
        );

        let outcome = classify_walk_error_path(&fifo);

        assert!(
            matches!(outcome, WalkErrorAttribution::Unattributable),
            "a path that is neither a directory nor a file must stay unattributed, exactly the \
             silent fallthrough this had before the re-stat was extracted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // #128 retry 3 (Security HIGH, was ticket #149): before this fix, a
    // re-stat that itself failed (the path named by the walker's error no
    // longer resolves to anything — a TOCTOU) fell through BOTH
    // `dropped_files` and `unexplored_subtree` with zero trace. A
    // nonexistent-root fixture reproduces the exact `std::fs::metadata`
    // failure a real vanished-mid-walk path would produce, with no race
    // required.
    #[test]
    fn classify_walk_error_path_of_a_vanished_path_is_unexplored_subtree_not_silence() {
        let path = std::env::temp_dir().join(format!(
            "codeimpact_classify_never_existed_{}_{}/victim.rs",
            std::process::id(),
            line!()
        ));
        assert!(
            std::fs::metadata(&path).is_err(),
            "fixture must not exist on disk: {path:?}"
        );

        let outcome = classify_walk_error_path(&path);

        assert!(
            matches!(outcome, WalkErrorAttribution::UnexploredSubtree),
            "a re-stat that itself fails must be folded into unexplored_subtree, never silently \
             unattributed"
        );
    }
}
