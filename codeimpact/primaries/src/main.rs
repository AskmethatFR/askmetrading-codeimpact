use clap::{Parser, Subcommand};
use std::path::PathBuf;

use codeimpact_hexagon::domain_model::analysis_rule::AnalysisRule;
use codeimpact_hexagon::domain_model::analysis_target::{AnalysisTarget, TargetType};
use codeimpact_hexagon::use_cases_application_services::run_analysis::RunAnalysis;
use codeimpact_secondaries::gateways::code_readers::file_system_code_reader::FileSystemCodeReader;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;

#[derive(Parser)]
#[command(name = "codeimpact", about = "Outil d'analyse de code statique")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyse un fichier source
    Analyze {
        /// Chemin vers le fichier à analyser
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze { file } => {
            let target = AnalysisTarget::new(file.clone(), TargetType::File);
            let reader = FileSystemCodeReader::new();
            let writer = ConsoleReportWriter::new();
            let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer));

            match use_case.execute(&target, &[AnalysisRule::CyclomaticComplexity]) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("Erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
