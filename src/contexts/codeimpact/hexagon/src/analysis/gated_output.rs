use super::alert_thresholds::ThresholdReport;
use super::gate_coverage::GateCoverage;

/// Wraps a use case's normal payload together with the threshold-breach
/// outcome (US8, AD-4) and the gate's own coverage (US128): the exit-code
/// DECISION belongs to the domain (`ThresholdReport::has_breach`,
/// `GateCoverage::is_complete`), main.rs only MAPS both to a process exit
/// code — it never re-derives either comparison itself. Plumbing, covered
/// transitively through the use cases that return it (`use-case-driven-design`
/// Test Surface Map) rather than a standalone unit test.
///
/// `new` takes `coverage` as a THIRD, mandatory constructor argument
/// (US128) — deliberately not a `with_coverage` builder. A builder would
/// make the coverage forgettable, and a call site that forgets it falls
/// back silently to whatever the type's default happens to be — i.e. to
/// the very lie this ticket exists to fix. Every call site must state its
/// position; the compiler enforces it.
#[derive(Clone, Debug, PartialEq)]
pub struct GatedOutput<T> {
    payload: T,
    thresholds: ThresholdReport,
    coverage: GateCoverage,
}

impl<T> GatedOutput<T> {
    pub fn new(payload: T, thresholds: ThresholdReport, coverage: GateCoverage) -> Self {
        Self {
            payload,
            thresholds,
            coverage,
        }
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    pub fn into_payload(self) -> T {
        self.payload
    }

    pub fn thresholds(&self) -> &ThresholdReport {
        &self.thresholds
    }

    pub fn coverage(&self) -> GateCoverage {
        self.coverage
    }
}
