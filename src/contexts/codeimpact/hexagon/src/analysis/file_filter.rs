use std::path::{Component, Path};

/// A single glob pattern is a handful of path segments; this cap still
/// tolerates a deliberately verbose pattern while refusing anything
/// resembling a payload built to exhaust memory/CPU in a glob engine.
const MAX_PATTERN_LENGTH: usize = 512;
/// A real project's include/exclude section is a short list (a handful of
/// globs); this cap bounds the total glob-compilation cost (glob-DoS) an
/// adapter would pay compiling `include` + `exclude` together.
const MAX_PATTERN_COUNT: usize = 256;

/// Standing default excludes (#34 T2, US17): vendored, generated, and
/// build-artifact output that must never be analyzed, whether or not a
/// `.codeimpact.json` is present. `node_modules/**` covers a
/// project-root-level `node_modules/`; `**/node_modules/**` is a
/// DELIBERATE belt-and-braces duplicate (Dev-B, #34 T2 review sweep):
/// empirically, at the pinned `globset 0.4.19` / `ignore 0.4.31` versions,
/// `**/node_modules/**` alone already covers the root-level case in both
/// the gitignore and globset dialects, making the root-anchored entry
/// strictly redundant today — kept anyway because a future crate upgrade
/// is not this codebase's to promise, and a defensive duplicate costs one
/// slot out of 256. Read this as "kept for belt-and-braces", never as
/// "both entries are load-bearing" — a future maintainer adding a seventh
/// default must not infer the wrong lesson from this comment. `dist/**` is
/// deliberately root-anchored only — a nested `dist/` is not implied by
/// this ticket and is left to the user's own `exclude` list. `**/*.min.js`
/// matches a minified file at any depth; `Path::extension()`'s
/// single-segment semantics mean this can only ever be reached via a glob,
/// never an extension check.
///
/// `target/**` (#34 T2 follow-up, operator ruling): the original tech spec
/// excluded it deliberately, reasoning it would silently change behavior
/// for existing Rust projects. Dogfooding this repository at FULL scale
/// (`codeimpact analyze --path .`, not just the ticket's named
/// `node_modules`-heavy subtree) showed the opposite — `target/`, a Rust
/// build directory, is what actually exhausts `MAX_WALK_ENTRIES` for any
/// already-built Rust repo, before the walk ever reaches real sources.
/// The "behavior change" the original ruling worried about IS analyzing
/// generated build artifacts, which is exactly what nobody wants. It is a
/// root-anchored `<literal>/**` shape, so it is already walk-time-prunable
/// through the existing `is_dialect_safe_prune_pattern` predicate with no
/// change needed there.
///
/// `**/target/**` (#34 T2 review sweep, LOW-2 — Security reproduced: a
/// nested build dir in a polyglot monorepo, e.g. `services/api/target/`,
/// still exhausts MAX_WALK_ENTRIES because only the root-anchored
/// `target/**` was added). `target/**`'s justification — build artifacts
/// exhaust the walk cap — is depth-independent, unlike `dist/**`'s
/// deliberately root-only scope, so it gets the same nested twin
/// `node_modules/**` already has.
///
/// Deliberately NOT `pub` (F5, #34 T2 review sweep): this module
/// (`file_filter`) is private and this constant is never re-exported, so a
/// `pub` visibility modifier here was unreachable dead API surface, not a
/// real contract. Callers that need to know which of a `FileFilter`'s
/// `exclude()` entries came from the standing defaults use
/// `FileFilter::default_exclude_patterns()` instead — a real, intentional
/// public method, rather than requiring the raw list itself.
const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules/**",
    "**/node_modules/**",
    "dist/**",
    "**/*.min.js",
    "target/**",
    "**/target/**",
];

/// Order-preserving union of `user_exclude` with `DEFAULT_EXCLUDES`: every
/// user pattern is kept as given, then each default not already present
/// (exact string match) is appended once. Shared by `unrestricted()` and
/// `new()` (ddd-value-object: the invariant is enforced AT construction so
/// neither caller can bypass it — see #34 T2).
///
/// User patterns are deliberately kept FIRST: a user reading their
/// effective exclude list back should see what THEY wrote at the top, not
/// have their own intent buried underneath the standing defaults.
fn union_with_default_excludes(user_exclude: Vec<String>) -> Vec<String> {
    let mut union = user_exclude;
    for default in DEFAULT_EXCLUDES {
        if !union.iter().any(|p| p == default) {
            union.push((*default).to_string());
        }
    }
    union
}

/// Value Object (US31, D1): validated, neutral include/exclude glob
/// patterns plus the gitignore toggle. Holds RAW patterns only — no
/// compiled matcher. Glob compilation is an adapter concern
/// (`ca-ports-adapters`, DIP): the hexagon stays zero-dep (ADR-0001), so it
/// cannot depend on `globset`. Self-validating (`ddd-value-object`):
/// construction rejects anything that could turn a glob into a
/// path-traversal or glob-DoS vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    include: Vec<String>,
    exclude: Vec<String>,
    respect_gitignore: bool,
}

/// Rejected construction of a `FileFilter` — names the offending pattern
/// (or count) so the adapter can surface an actionable error instead of a
/// generic parse failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileFilterError {
    EmptyPattern,
    PatternContainsNul(String),
    AbsolutePattern(String),
    ParentTraversalPattern(String),
    PatternTooLong(String),
    /// F6/LOW-1 (#34 T2 review sweep, Dev-B + Security both flagged the
    /// former bare-total message as opaque): carries the breakdown a user
    /// needs to actually act on this error — how many patterns THEY wrote
    /// (`user_supplied`, across `include` + `exclude`), how many the
    /// standing `DEFAULT_EXCLUDES` union appended on top
    /// (`defaults_added`), and the resulting `total` compared against the
    /// cap. A bare total (e.g. "257") appears nowhere in the user's own
    /// config file, forcing them to reverse-engineer the subtraction.
    TooManyPatterns {
        user_supplied: usize,
        defaults_added: usize,
        total: usize,
    },
}

impl std::fmt::Display for FileFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPattern => write!(f, "motif de filtrage vide"),
            Self::PatternContainsNul(p) => {
                write!(f, "motif de filtrage invalide (caractère NUL): {}", p)
            }
            Self::AbsolutePattern(p) => {
                write!(
                    f,
                    "motif de filtrage invalide (chemin absolu refusé): {}",
                    p
                )
            }
            Self::ParentTraversalPattern(p) => {
                write!(
                    f,
                    "motif de filtrage invalide (traversée de répertoire parent \"..\" refusée): {}",
                    p
                )
            }
            Self::PatternTooLong(p) => {
                write!(
                    f,
                    "motif de filtrage trop long (max {} caractères): {}",
                    MAX_PATTERN_LENGTH, p
                )
            }
            Self::TooManyPatterns {
                user_supplied,
                defaults_added,
                total,
            } => {
                write!(
                    f,
                    "trop de motifs de filtrage: {} fournis + {} exclusions par défaut = {} (max {})",
                    user_supplied, defaults_added, total, MAX_PATTERN_COUNT
                )
            }
        }
    }
}

impl std::error::Error for FileFilterError {}

impl FileFilter {
    /// No *user* restriction, plus the standing default excludes
    /// (`DEFAULT_EXCLUDES`, #34 T2) — no include patterns, gitignore not
    /// honored (D4: absent config file, unchanged). Prior to #34 this
    /// reproduced pre-US31 behavior byte-for-byte; that is no longer true
    /// for `exclude` by design — vendored/generated JS/TS output is
    /// excluded even with no config file at all.
    ///
    /// Routes through `new()` (INF-1, #34 T2 review sweep) rather than
    /// building `Self { .. }` directly: the two constructors previously
    /// diverged on the SAME shared invariant (`new()` validates every
    /// pattern, `unrestricted()` did not) — harmless today only because
    /// `DEFAULT_EXCLUDES`'s entries happen to all pass validation, which is
    /// exactly the kind of fact that silently stops being true the next
    /// time someone edits that constant. The `.expect()` below is
    /// deliberate: if a future edit to `DEFAULT_EXCLUDES` ever broke this,
    /// failing loudly at the very first call (any test, any `analyze`
    /// invocation) beats silently constructing a `FileFilter` whose
    /// exclude list quietly dropped the offending entry.
    pub fn unrestricted() -> Self {
        Self::new(Vec::new(), Vec::new(), false)
            .expect("DEFAULT_EXCLUDES must always be a valid pattern set")
    }

    /// Validates every pattern in `include` and the union of `exclude`
    /// with `DEFAULT_EXCLUDES` before construction (`ddd-value-object`):
    /// rejects empty patterns, interior NUL, absolute paths, any `..`
    /// component, over-length patterns, and an over-large total pattern
    /// count (glob-DoS) — the count and per-pattern validation run on the
    /// UNION (#34 T2), not the caller's `exclude` list alone, so a config
    /// file cannot opt out of the standing defaults by construction.
    pub fn new(
        include: Vec<String>,
        exclude: Vec<String>,
        respect_gitignore: bool,
    ) -> Result<Self, FileFilterError> {
        let user_supplied = include.len() + exclude.len();
        let exclude = union_with_default_excludes(exclude);
        let total = include.len() + exclude.len();
        if total > MAX_PATTERN_COUNT {
            return Err(FileFilterError::TooManyPatterns {
                user_supplied,
                defaults_added: total - user_supplied,
                total,
            });
        }
        for pattern in include.iter().chain(exclude.iter()) {
            Self::validate_pattern(pattern)?;
        }
        Ok(Self {
            include,
            exclude,
            respect_gitignore,
        })
    }

    fn validate_pattern(pattern: &str) -> Result<(), FileFilterError> {
        if pattern.is_empty() {
            return Err(FileFilterError::EmptyPattern);
        }
        if pattern.len() > MAX_PATTERN_LENGTH {
            return Err(FileFilterError::PatternTooLong(pattern.to_string()));
        }
        if pattern.contains('\0') {
            return Err(FileFilterError::PatternContainsNul(pattern.to_string()));
        }
        let path = Path::new(pattern);
        if path.is_absolute() {
            return Err(FileFilterError::AbsolutePattern(pattern.to_string()));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(FileFilterError::ParentTraversalPattern(pattern.to_string()));
        }
        Ok(())
    }

    pub fn include(&self) -> &[String] {
        &self.include
    }

    pub fn exclude(&self) -> &[String] {
        &self.exclude
    }

    pub fn respect_gitignore(&self) -> bool {
        self.respect_gitignore
    }

    /// The subset of `exclude()` that came from the standing
    /// `DEFAULT_EXCLUDES` (#34 T2 MED-1) — lets a caller (the walk
    /// adapter) distinguish "excluded because of a standing default" from
    /// "excluded because the user's own config/CLI said so", without
    /// `DEFAULT_EXCLUDES` itself needing to be public API (F5). A pattern
    /// the user happened to also write themselves, identical to a
    /// default, is still reported here — after the union+dedup, there is
    /// no way (nor reason) to tell the two apart: either way, the file
    /// would have been excluded by the standing default regardless of the
    /// user's own list.
    pub fn default_exclude_patterns(&self) -> Vec<&str> {
        self.exclude
            .iter()
            .filter(|p| DEFAULT_EXCLUDES.contains(&p.as_str()))
            .map(|p| p.as_str())
            .collect()
    }
}
