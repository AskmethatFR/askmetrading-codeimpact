use super::economic_impact::EconomicImpact;
use super::stress_test_run::StressTestRun;

pub struct ReactiveAnalyzer;

impl ReactiveAnalyzer {
    /// Cost per CPU-second in microdollars.
    /// Based on ~$0.10/CPU-heure cloud pricing: $0.10 / 3600s = 27.78 μ$/s
    pub const MICRODOLLARS_PER_CPU_SECOND: f64 = 27.7778;

    pub fn analyze(run: &StressTestRun) -> EconomicImpact {
        let cpu_seconds = run.cpu_time_ms() as f64 / 1000.0;
        let cpu_cost = cpu_seconds * Self::MICRODOLLARS_PER_CPU_SECOND;
        let memory_bytes = run.memory_kb() * 1024;
        let total = cpu_cost + memory_bytes as f64 * 0.0001;
        let level = Self::compute_stress_level(total);
        EconomicImpact::new(cpu_cost, memory_bytes, total, level)
    }

    /// Level thresholds for real-world stress test costs (μ$).
    ///
    /// | Range (μ$) | Range ($) | Level |
    /// |---|---|---|
    /// | 0–1 000 | $0–$0.001 | low |
    /// | 1 001–10 000 | $0.001–$0.01 | moderate |
    /// | 10 001–100 000 | $0.01–$0.10 | high |
    /// | 100 001+ | $0.10+ | critical |
    fn compute_stress_level(total_cost_microdollars: f64) -> &'static str {
        if total_cost_microdollars <= 1000.0 {
            "low"
        } else if total_cost_microdollars <= 10_000.0 {
            "moderate"
        } else if total_cost_microdollars <= 100_000.0 {
            "high"
        } else {
            "critical"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stress_test_run::StressTestRun;

    #[test]
    fn stress_level_low() {
        let run = StressTestRun::new(10, 0, 0, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "low");
    }

    #[test]
    fn stress_level_moderate() {
        // 50 MB = 51200 KB → 52428800 bytes → 52428800 * 0.0001 = 5242 μ$
        // total ≈ 5242 μ$ → moderate
        let run = StressTestRun::new(1000, 1000, 51_200, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "moderate");
    }

    #[test]
    fn stress_level_high() {
        // 500 MB = 512000 KB → 524288000 bytes → 52428 μ$
        let run = StressTestRun::new(1000, 1000, 512_000, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "high");
    }

    #[test]
    fn stress_level_critical() {
        // 1 GB = 1048576 KB → 1073741824 bytes → 107374 μ$
        let run = StressTestRun::new(1000, 1000, 1_048_576, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "critical");
    }
}