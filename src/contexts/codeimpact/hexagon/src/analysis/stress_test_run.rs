use super::errors::AnalysisError;

// `Measurement<T>` / `UnmeasurableReason` moved to `measurement.rs` (#50):
// a measurement primitive is not a stress-test concept. Re-exported here so
// every existing `stress_test_run::{Measurement, UnmeasurableReason}` path
// (in particular `mod.rs`'s `pub use`) keeps resolving unchanged.
pub use super::measurement::{Measurement, UnmeasurableReason};

#[derive(Clone, Debug, PartialEq)]
pub struct StressTestRun {
    duration_ms: u64,
    cpu_time_ms: Measurement<u64>,
    memory_kb: Measurement<u64>,
    tests_passed: u32,
    tests_total: u32,
    filter: Option<String>,
}

impl StressTestRun {
    pub fn new(
        duration_ms: u64,
        cpu_time_ms: Measurement<u64>,
        memory_kb: Measurement<u64>,
        tests_passed: u32,
        tests_total: u32,
        filter: Option<String>,
    ) -> Self {
        let tests_passed = tests_passed.min(tests_total);
        Self {
            duration_ms,
            cpu_time_ms,
            memory_kb,
            tests_passed,
            tests_total,
            filter,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    pub fn cpu_time_ms(&self) -> Measurement<u64> {
        self.cpu_time_ms
    }

    pub fn memory_kb(&self) -> Measurement<u64> {
        self.memory_kb
    }

    pub fn tests_passed(&self) -> u32 {
        self.tests_passed
    }

    pub fn tests_total(&self) -> u32 {
        self.tests_total
    }

    pub fn filter(&self) -> Option<String> {
        self.filter.clone()
    }

    /// Folds N per-binary `StressTestRun`s (one per `--workspace` test
    /// target) into one, honestly (#39). Empty slice is an error, not a
    /// synthesized all-zero run: nothing was built, so there is nothing to
    /// report.
    pub fn aggregate(runs: &[StressTestRun]) -> Result<StressTestRun, AnalysisError> {
        if runs.is_empty() {
            return Err(AnalysisError::TestRunnerError(
                "aucun binaire de test n'a été construit".into(),
            ));
        }

        let tests_passed = runs.iter().map(|r| r.tests_passed).sum();
        let tests_total = runs.iter().map(|r| r.tests_total).sum();
        let duration_ms = runs.iter().map(|r| r.duration_ms).sum();
        let cpu_time_ms = Self::sum_measurements(runs.iter().map(|r| r.cpu_time_ms));
        let memory_kb = Self::max_measurement(runs.iter().map(|r| r.memory_kb));
        let filter = runs[0].filter.clone();

        Ok(Self::new(
            duration_ms,
            cpu_time_ms,
            memory_kb,
            tests_passed,
            tests_total,
            filter,
        ))
    }

    /// Takes the peak of a series of `Measurement<u64>` (e.g. per-binary
    /// memory), propagating the first `Unmeasurable` found. Peak RSS of
    /// processes that never coexist is a max, never a sum — a sum would
    /// invent memory that was never simultaneously resident.
    fn max_measurement(measurements: impl Iterator<Item = Measurement<u64>>) -> Measurement<u64> {
        let mut peak = 0_u64;
        for measurement in measurements {
            match measurement {
                Measurement::Available(value) => peak = peak.max(value),
                Measurement::Unmeasurable(reason) => return Measurement::Unmeasurable(reason),
            }
        }
        Measurement::Available(peak)
    }

    /// Sums a series of `Measurement<u64>` (e.g. per-binary CPU time),
    /// propagating the first `Unmeasurable` found — a total that folded in
    /// an unmeasured input would be a fabricated number, exactly the #36
    /// disease the `Measurement` type exists to prevent (ADR-0010).
    fn sum_measurements(measurements: impl Iterator<Item = Measurement<u64>>) -> Measurement<u64> {
        let mut total = 0_u64;
        for measurement in measurements {
            match measurement {
                Measurement::Available(value) => total += value,
                Measurement::Unmeasurable(reason) => return Measurement::Unmeasurable(reason),
            }
        }
        Measurement::Available(total)
    }
}

pub trait TestRunnerPort: Send + Sync {
    fn run_tests(&self, filter: Option<&str>) -> Result<StressTestRun, AnalysisError>;
}
