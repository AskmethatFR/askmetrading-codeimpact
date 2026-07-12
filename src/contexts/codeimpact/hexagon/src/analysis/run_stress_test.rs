use super::errors::AnalysisError;
use super::reactive_analyzer::ReactiveAnalyzer;
use super::report_writer::ReportWriter;
use super::stress_test_run::TestRunnerPort;

pub struct RunStressTest {
    test_runner: Box<dyn TestRunnerPort>,
    reporter: Box<dyn ReportWriter>,
}

impl RunStressTest {
    pub fn new(test_runner: Box<dyn TestRunnerPort>, reporter: Box<dyn ReportWriter>) -> Self {
        Self {
            test_runner,
            reporter,
        }
    }

    pub fn handle(&self, filter: Option<&str>) -> Result<(), AnalysisError> {
        let run = self.test_runner.run_tests(filter)?;
        let impact = ReactiveAnalyzer::analyze(&run);
        self.reporter.write_stress_test(&run, &impact)
    }
}
