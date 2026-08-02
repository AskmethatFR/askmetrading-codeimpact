use std::path::{Component, Path};

/// A single glob pattern is a handful of path segments; this cap still
/// tolerates a deliberately verbose pattern while refusing anything
/// resembling a payload built to exhaust memory/CPU in a glob engine.
const MAX_PATTERN_LENGTH: usize = 512;
/// A real project's include/exclude section is a short list (a handful of
/// globs); this cap bounds the total glob-compilation cost (glob-DoS) an
/// adapter would pay compiling `include` + `exclude` together.
const MAX_PATTERN_COUNT: usize = 256;

/// Standing default excludes (#34 T2, US17): vendored and generated
/// JS/TS output that must never be analyzed, whether or not a
/// `.codeimpact.json` is present. `node_modules/**` covers a
/// project-root-level `node_modules/`; `**/node_modules/**` additionally
/// covers a nested one (`packages/a/node_modules/`, a real npm workspace
/// shape) — a bare root-anchored `node_modules/**` does NOT match a nested
/// path in either the gitignore or globset glob dialect, so both entries
/// are needed (operator ruling Q7: node_modules is never analyzed,
/// wherever it sits). `dist/**` is deliberately root-anchored only — a
/// nested `dist/` is not implied by this ticket and is left to the user's
/// own `exclude` list. `**/*.min.js` matches a minified file at any depth;
/// `Path::extension()`'s single-segment semantics mean this can only ever
/// be reached via a glob, never an extension check. `target/**` is
/// deliberately NOT here (operator ruling): it would silently change
/// behavior for existing Rust projects.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules/**",
    "**/node_modules/**",
    "dist/**",
    "**/*.min.js",
];

/// Order-preserving union of `user_exclude` with `DEFAULT_EXCLUDES`: every
/// user pattern is kept as given, then each default not already present
/// (exact string match) is appended once. Shared by `unrestricted()` and
/// `new()` (ddd-value-object: the invariant is enforced AT construction so
/// neither caller can bypass it — see #34 T2).
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
    TooManyPatterns(usize),
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
            Self::TooManyPatterns(count) => {
                write!(
                    f,
                    "trop de motifs de filtrage: {} (max {})",
                    count, MAX_PATTERN_COUNT
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
    pub fn unrestricted() -> Self {
        Self {
            include: Vec::new(),
            exclude: union_with_default_excludes(Vec::new()),
            respect_gitignore: false,
        }
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
        let exclude = union_with_default_excludes(exclude);
        let total = include.len() + exclude.len();
        if total > MAX_PATTERN_COUNT {
            return Err(FileFilterError::TooManyPatterns(total));
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
}
