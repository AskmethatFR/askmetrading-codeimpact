use super::alert_thresholds::AlertThresholds;
use super::ecological_impact::EcologicalImpactEstimator;
use super::errors::AnalysisError;
use super::gated_output::GatedOutput;
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

    /// `thresholds` (US8 T5): the same gate as `RunAnalysis`, reusing the
    /// existing `Measurement<EconomicImpact>` — an `Unmeasurable` run
    /// derives `(None, None)`, which `evaluate` honestly never breaches
    /// (ADR-0010), same shape as an unmeasured file/project.
    pub fn handle(
        &self,
        filter: Option<&str>,
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<()>, AnalysisError> {
        let run = self.test_runner.run_tests(filter)?;
        let impact = ReactiveAnalyzer::analyze(&run);
        let economic = impact.clone().available();
        let cpu = economic.as_ref().map(|e| e.cpu_cost_microdollars());
        let co2 = economic.map(|e| {
            EcologicalImpactEstimator::estimate(
                &e,
                EcologicalImpactEstimator::DEFAULT_CO2_G_PER_KWH,
            )
            .co2_grams()
        });
        let report = thresholds.evaluate(cpu, co2);
        self.reporter.write_stress_test(&run, &impact)?;
        Ok(GatedOutput::new((), report))
    }
}
