use clap::{Parser, Subcommand};
use std::path::PathBuf;

use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
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
    Analyze { file: PathBuf },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze { file } => {
            let target = AnalysisTarget::new(file.clone(), TargetType::File);
            let reader = FileSystemCodeReader::new();
            let writer = ConsoleReportWriter::new();
            let parser = SynCodeParser::new();
            let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

            match use_case.handle(&target, &[AnalysisRule::CyclomaticComplexity]) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
