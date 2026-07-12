use super::errors::AnalysisError;

/// Why a physical quantity (CPU time, memory) could not be sampled.
///
/// There is deliberately no `f64`/`u64` default for "not measured" (#36):
/// a missing reading must be `Unmeasurable`, never a silent `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnmeasurableReason {
    /// No sampler (e.g. `/usr/bin/time`) was available, or the one that
    /// ran produced output that could not be parsed into a reading.
    NoSampler,
}

impl std::fmt::Display for UnmeasurableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSampler => write!(f, "aucun outil de mesure disponible"),
        }
    }
}

/// A physical quantity that either was sampled, or explicitly was not.
///
/// Replaces the previous convention of defaulting to `0` when a measurement
/// tool was unavailable — `0` reads as "free", which is a lie (#36).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measurement<T> {
    Available(T),
    Unmeasurable(UnmeasurableReason),
}

impl<T> Measurement<T> {
    pub fn available(self) -> Option<T> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unmeasurable(_) => None,
        }
    }
}

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
}

pub trait TestRunnerPort: Send + Sync {
    fn run_tests(&self, filter: Option<&str>) -> Result<StressTestRun, AnalysisError>;
}
