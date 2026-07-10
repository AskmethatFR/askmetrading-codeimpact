use super::errors::AnalysisError;

#[derive(Clone, Debug, PartialEq)]
pub struct StressTestRun {
    duration_ms: u64,
    cpu_time_ms: u64,
    memory_kb: u64,
    tests_passed: u32,
    tests_total: u32,
    filter: Option<String>,
}

impl StressTestRun {
    pub fn new(
        duration_ms: u64,
        cpu_time_ms: u64,
        memory_kb: u64,
        tests_passed: u32,
        tests_total: u32,
        filter: Option<String>,
    ) -> Self {
        let duration_ms = if duration_ms == 0 { 1 } else { duration_ms };
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

    pub fn cpu_time_ms(&self) -> u64 {
        self.cpu_time_ms
    }

    pub fn memory_kb(&self) -> u64 {
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