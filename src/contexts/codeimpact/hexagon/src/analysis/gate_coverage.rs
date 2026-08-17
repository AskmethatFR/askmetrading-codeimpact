/// Value Object (US128, issue #128): whether the gate that decides
/// `--strict`'s process exit code (`AlertThresholds::evaluate`, mapped by
/// `gated_exit_code` in `primaries/src/main.rs`) covered every
/// file/measurement it needed, or had to decide on an incomplete view.
///
/// `AlertThresholds::evaluate` is correct to never breach on an absent
/// metric — absence is not a confident zero (ADR-0010) — but combined with
/// `--strict` that collapsed "nothing breached" and "nothing I managed to
/// measure breached" onto the identical exit code 0. Security demonstrated
/// the consequence: inflating one file past the 1 MiB size guard dropped it
/// out of the gated aggregate sum and turned a real threshold overrun into
/// exit 0. `GateCoverage` makes the distinction representable so the CLI's
/// exit-code mapping can honor it instead of silently losing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateCoverage {
    /// Every file/measurement the gate needed was available — either
    /// nothing went unmeasured, or no threshold was configured at all (an
    /// ungated project has nothing the gate could have missed).
    Complete,
    /// One or more files could not be measured (any reason — too large,
    /// unreadable, unparseable, unsupported language): the gated aggregate
    /// is missing an unknown, possibly non-zero, contribution.
    Partial { unmeasurable_files: usize },
    /// There were no files to be partial about — the run's single
    /// measurement itself could not be taken (e.g. a stress test whose
    /// economic impact is `Unmeasurable`). Distinct from `Partial { .. }`
    /// (ADR-0032 AD-5): an adapter that cannot read a signal propagates the
    /// absence, it never manufactures a plausible count.
    Absent,
}

impl GateCoverage {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}
