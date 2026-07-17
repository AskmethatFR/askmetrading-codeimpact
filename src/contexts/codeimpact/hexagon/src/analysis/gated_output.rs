use super::alert_thresholds::ThresholdReport;

/// Wraps a use case's normal payload together with the threshold-breach
/// outcome (US8, AD-4): the exit-code DECISION belongs to the domain
/// (`ThresholdReport::has_breach`), main.rs only MAPS it to a process exit
/// code — it never re-derives a comparison itself. Plumbing, covered
/// transitively through the use cases that return it (`use-case-driven-design`
/// Test Surface Map) rather than a standalone unit test.
#[derive(Clone, Debug, PartialEq)]
pub struct GatedOutput<T> {
    payload: T,
    thresholds: ThresholdReport,
}

impl<T> GatedOutput<T> {
    pub fn new(payload: T, thresholds: ThresholdReport) -> Self {
        Self {
            payload,
            thresholds,
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
}
