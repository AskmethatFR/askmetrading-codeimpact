/// Whether a call recorded in a loop (`LoopCall`) is I/O — three states, not
/// a `bool` (#56 T2, ADR-0010 measurement honesty): a receiver whose type
/// could not be resolved is a THIRD thing, distinct from "resolved and known
/// not to be I/O". Collapsing it into `false` would silently fabricate a
/// negative — the exact confident-zero ADR-0010 forbids, one layer down from
/// `0` CPU/memory: here the zero is a boolean `false` standing in for "we
/// don't know".
///
/// Illegal states are unrepresentable: there is no `bool` + separate
/// "confidence" flag that could drift out of sync, because there is no
/// second field to drift — the sum type is priced.
///
/// Every consumer must `match` all three variants explicitly. No `_` arm is
/// permitted where a variant name is available — an unnamed catch-all is
/// exactly how a future fourth state (or a typo'd two-of-three) would slip
/// past review unnoticed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoClassification {
    /// The receiver's type was resolved and is a known I/O type (adapter's
    /// `KNOWN_IO_TYPES`), or the call is a free-function form matching a
    /// known I/O prefix (`IO_PREFIXES`).
    Io,
    /// The receiver's type was resolved and is NOT a known I/O type, OR the
    /// call name/method name gives no reason to suspect I/O. An honest
    /// negative, not an abstention.
    NotIo,
    /// The receiver's type could not be resolved at all, but the method
    /// name is on the suspicious-name list (T2, human-approved Q3) — enough
    /// doubt to withhold a `NotIo` verdict, not enough evidence for `Io`.
    /// Reported only as an aggregate count (ADR-0010 measurement honesty,
    /// ADR-0014 §4 precedent) — never surfaced as a per-line warning, which
    /// would turn abstention into a pseudo-warning.
    Unknown,
}
