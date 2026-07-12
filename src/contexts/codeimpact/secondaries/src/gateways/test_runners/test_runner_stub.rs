use codeimpact_hexagon::analysis::{AnalysisError, StressTestRun, TestRunnerPort};

pub struct TestRunnerStub {
    result: Result<StressTestRun, AnalysisError>,
}

impl TestRunnerStub {
    pub fn new(result: Result<StressTestRun, AnalysisError>) -> Self {
        Self { result }
    }
}

impl TestRunnerPort for TestRunnerStub {
    fn run_tests(&self, _filter: Option<&str>) -> Result<StressTestRun, AnalysisError> {
        self.result.clone()
    }
}
