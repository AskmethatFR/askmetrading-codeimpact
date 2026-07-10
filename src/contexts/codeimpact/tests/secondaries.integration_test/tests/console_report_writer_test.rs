use codeimpact_hexagon::analysis::CodeLocation;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::EfficiencyClass;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FileDependency;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use std::path::PathBuf;

#[test]
fn write_console_does_not_panic() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(5);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_zero_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(0);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_high_complexity() {
    let writer = ConsoleReportWriter::new();
    let metrics = CodeMetrics::new(50);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_economic_impact() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(18.5, 12600, 19.7, "moderate");
    let metrics = CodeMetrics::new(27).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_memory_mb() {
    let writer = ConsoleReportWriter::new();
    let impact = EconomicImpact::new(50.0, 2_000_000, 50.2, "high");
    let metrics = CodeMetrics::new(30).with_economic_impact(impact);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_ecological_impact() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(6000.0, 0, 6000.0, "low");
    let ecological = EcologicalImpact::new(2.4, 21600.0, EfficiencyClass::B);
    let metrics = CodeMetrics::new(27)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

#[test]
fn write_console_ecological_zero_co2() {
    let writer = ConsoleReportWriter::new();
    let economic = EconomicImpact::new(0.0, 0, 0.0, "low");
    let ecological = EcologicalImpact::new(0.0, 0.0, EfficiencyClass::A);
    let metrics = CodeMetrics::new(1)
        .with_economic_impact(economic)
        .with_ecological_impact(ecological);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn write_project_report_with_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::new(5)
                .with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low"))
                .with_ecological_impact(EcologicalImpact::new(1.0, 9000.0, EfficiencyClass::B)),
        ),
        (
            path("b.rs"),
            CodeMetrics::new(3)
                .with_economic_impact(EconomicImpact::new(20.0, 200, 21.0, "high"))
                .with_ecological_impact(EcologicalImpact::new(2.0, 18000.0, EfficiencyClass::D)),
        ),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

#[test]
fn write_project_report_without_impacts() {
    let writer = ConsoleReportWriter::new();
    let files = vec![
        (path("a.rs"), CodeMetrics::new(5)),
        (path("b.rs"), CodeMetrics::new(3)),
    ];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("b.rs"),
    }];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let result = writer.write_project_report(&graph);
    assert!(result.is_ok());
}

#[test]
fn write_console_with_io_in_loops() {
    let writer = ConsoleReportWriter::new();
    let warnings = vec![
        IoInLoopWarning {
            function: "read_file".to_string(),
            io_call: "std::fs::read".to_string(),
            location: CodeLocation::new("".into(), 5, 9),
        },
        IoInLoopWarning {
            function: "write_data".to_string(),
            io_call: "std::fs::write".to_string(),
            location: CodeLocation::new("".into(), 10, 5),
        },
    ];
    let metrics = CodeMetrics::new(5).with_io_in_loops(warnings);
    let result = writer.write_console(&metrics);
    assert!(result.is_ok());
}
