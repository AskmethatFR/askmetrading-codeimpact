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
    Analyze {
        file: Option<PathBuf>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze { file, path } => {
            let file_path = match (file, path) {
                (Some(f), None) | (None, Some(f)) => f.clone(),
                (Some(_), Some(_)) => {
                    eprintln!("erreur: spécifiez soit un fichier en argument, soit --path, pas les deux");
                    std::process::exit(1);
                }
                (None, None) => {
                    eprintln!("erreur: spécifiez un fichier ou dossier à analyser");
                    std::process::exit(1);
                }
            };

            let target_type = if file_path.is_dir() {
                TargetType::Project
            } else {
                TargetType::File
            };

            let target = AnalysisTarget::new(file_path, target_type);
            let reader = FileSystemCodeReader::new();
            let writer = ConsoleReportWriter::new();
            let parser = SynCodeParser::new();
            let use_case = RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));

            match use_case.handle(&target, &[AnalysisRule::CyclomaticComplexity, AnalysisRule::IoInLoops]) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
