use clap::{Parser, Subcommand};
use std::path::PathBuf;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::AnalysisRule;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::OutputFormat;
use codeimpact_hexagon::analysis::RunAnalysis;
use codeimpact_hexagon::analysis::RunStressTest;
use codeimpact_hexagon::analysis::TargetType;
use codeimpact_secondaries::gateways::code_parsers::syn_code_parser::SynCodeParser;
use codeimpact_secondaries::gateways::code_readers::file_system_code_reader::FileSystemCodeReader;
use codeimpact_secondaries::gateways::report_writers::console_report_writer::ConsoleReportWriter;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::HtmlReportWriter;
use codeimpact_secondaries::gateways::report_writers::json_report_writer::JsonReportWriter;
use codeimpact_secondaries::gateways::test_runners::cargo_test_runner::CargoTestRunner;

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
        #[arg(long, default_value = "console")]
        format: String,
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
        /// Alert threshold (US8): max acceptable aggregate CPU cost, μ$.
        #[arg(long = "max-cpu")]
        max_cpu: Option<f64>,
        /// Alert threshold (US8): max acceptable aggregate CO2, grams.
        #[arg(long = "max-co2")]
        max_co2: Option<f64>,
    },
    StressTest {
        #[arg(long)]
        filter: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze {
            file,
            path,
            format,
            output,
            max_cpu,
            max_co2,
        } => {
            let thresholds = match AlertThresholds::new(*max_cpu, *max_co2) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("erreur: {}", e);
                    std::process::exit(1);
                }
            };

            let file_path = match (file, path) {
                (Some(f), None) | (None, Some(f)) => f.clone(),
                (Some(_), Some(_)) => {
                    eprintln!(
                        "erreur: spécifiez soit un fichier en argument, soit --path, pas les deux"
                    );
                    std::process::exit(1);
                }
                (None, None) => {
                    eprintln!("erreur: spécifiez un fichier ou dossier à analyser");
                    std::process::exit(1);
                }
            };

            let output_format = match format.as_str() {
                "console" => OutputFormat::Console,
                "json" => OutputFormat::Json,
                "html" => OutputFormat::Html,
                _ => {
                    eprintln!(
                        "erreur: format invalide '{}'. Formats supportés: console, json, html",
                        format
                    );
                    std::process::exit(1);
                }
            };

            let target_type = if file_path.is_dir() {
                TargetType::Project
            } else {
                TargetType::File
            };

            let target = AnalysisTarget::new(file_path, target_type);
            let is_project = *target.target_type() == TargetType::Project;
            let reader = FileSystemCodeReader::new();
            let parser = SynCodeParser::new();
            let rules = &[AnalysisRule::CyclomaticComplexity, AnalysisRule::IoInLoops];

            match output_format {
                OutputFormat::Console => {
                    if output.is_some() {
                        eprintln!(
                            "erreur: --format console ne supporte pas -o (utilisez --format json ou --format html pour écrire dans un fichier)"
                        );
                        std::process::exit(1);
                    }
                    let writer = ConsoleReportWriter::new();
                    let use_case =
                        RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));
                    match use_case.handle(&target, rules, &thresholds) {
                        Ok(()) => std::process::exit(0),
                        Err(e) => {
                            eprintln!("erreur: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                OutputFormat::Json => {
                    let writer = JsonReportWriter::new();
                    let use_case =
                        RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));
                    let result = if is_project {
                        use_case.handle_project_json(&target, rules)
                    } else {
                        use_case.handle_json(&target, rules)
                    };
                    match result {
                        Ok(json) => match output {
                            Some(output_path) => match write_report_file(output_path, &json) {
                                Ok(()) => {
                                    println!("Rapport JSON généré: {}", output_path.display());
                                    std::process::exit(0);
                                }
                                Err(msg) => {
                                    eprintln!("erreur: {}", msg);
                                    std::process::exit(1);
                                }
                            },
                            None => {
                                println!("{}", json);
                                std::process::exit(0);
                            }
                        },
                        Err(e) => {
                            eprintln!("erreur: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                OutputFormat::Html => {
                    if !is_project {
                        eprintln!(
                            "erreur: le format html nécessite une cible projet (--path <dossier>)"
                        );
                        std::process::exit(1);
                    }
                    let writer = HtmlReportWriter::new();
                    let use_case =
                        RunAnalysis::new(Box::new(reader), Box::new(writer), Box::new(parser));
                    match use_case.handle_project_html(&target, rules) {
                        Ok(html) => {
                            let output_path = output
                                .clone()
                                .unwrap_or_else(|| PathBuf::from("report.html"));
                            match write_report_file(&output_path, &html) {
                                Ok(()) => {
                                    println!("Rapport HTML généré: {}", output_path.display());
                                    std::process::exit(0);
                                }
                                Err(msg) => {
                                    eprintln!("erreur: {}", msg);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("erreur: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Commands::StressTest { filter } => {
            let project_dir = std::env::current_dir().unwrap_or_else(|_| {
                eprintln!("erreur: impossible de déterminer le répertoire courant");
                std::process::exit(1);
            });
            let runner = CargoTestRunner::new(project_dir);
            let writer = ConsoleReportWriter::new();
            let use_case = RunStressTest::new(Box::new(runner), Box::new(writer));
            match use_case.handle(filter.as_deref()) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("erreur: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Writes a report (JSON or HTML) to `output_path` (ADR-0006 discipline,
/// scaled to intent: the -o path is user-chosen so path-traversal risk is
/// low, but the parent directory is still canonicalized and error messages
/// stay path-anonymised).
fn write_report_file(output_path: &std::path::Path, content: &str) -> Result<(), String> {
    let parent = match output_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let canonical_parent =
        std::fs::canonicalize(parent).map_err(|_| "dossier de sortie introuvable".to_string())?;
    let file_name = output_path
        .file_name()
        .ok_or_else(|| "nom de fichier de sortie invalide".to_string())?;
    let resolved_path = canonical_parent.join(file_name);

    // `symlink_metadata` does not follow the final path component (unlike
    // `metadata`), so a symlink is reported as itself: `fs::write` would
    // otherwise follow it and clobber whatever it points to (demonstrated
    // arbitrary overwrite), and would block forever opening a FIFO with no
    // reader (demonstrated CI hang). Everything that is not a regular file
    // is refused here, before either happens.
    match std::fs::symlink_metadata(&resolved_path) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err("la cible de sortie n'est pas un fichier régulier".to_string());
        }
        _ => {}
    }

    std::fs::write(&resolved_path, content)
        .map_err(|_| "impossible d'écrire le fichier de sortie".to_string())
}

#[cfg(all(test, unix))]
mod write_report_file_tests {
    use super::write_report_file;
    use std::os::unix::fs::symlink;

    fn isolated_dir(test_name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codeimpact_write_report_file_test_{}_{}",
            test_name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create isolated test dir");
        dir
    }

    #[test]
    fn write_to_new_path_creates_file() {
        let dir = isolated_dir("new_path");
        let target = dir.join("report.json");

        let result = write_report_file(&target, "hello");

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_existing_regular_file_succeeds() {
        let dir = isolated_dir("overwrite");
        let target = dir.join("report.json");
        std::fs::write(&target, "old").unwrap();

        let result = write_report_file(&target, "new");

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_symlink_target_and_leaves_link_target_intact() {
        let dir = isolated_dir("symlink");
        let real_target = dir.join("real_target.txt");
        std::fs::write(&real_target, "untouched").unwrap();
        let link = dir.join("report.json");
        symlink(&real_target, &link).expect("create symlink");

        let result = write_report_file(&link, "malicious overwrite");

        assert!(
            result.is_err(),
            "expected Err refusing a symlink target, got {:?}",
            result
        );
        assert_eq!(
            std::fs::read_to_string(&real_target).unwrap(),
            "untouched",
            "the symlink's target must be left untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_fifo_target_without_hanging() {
        let dir = isolated_dir("fifo");
        let fifo = dir.join("report.json");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("failed to spawn mkfifo");
        assert!(status.success(), "mkfifo must succeed to set up the test");

        let result = write_report_file(&fifo, "content");

        assert!(
            result.is_err(),
            "expected Err refusing a FIFO target, got {:?}",
            result
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refusal_error_contains_no_path() {
        let dir = isolated_dir("no_path_leak");
        let real_target = dir.join("real_target.txt");
        std::fs::write(&real_target, "untouched").unwrap();
        let link = dir.join("report.json");
        symlink(&real_target, &link).expect("create symlink");

        let result = write_report_file(&link, "content");

        let err = result.expect_err("expected Err refusing a symlink target");
        assert!(
            !err.contains(dir.to_str().unwrap()),
            "error message must not leak the absolute path (ADR-0006): {}",
            err
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_directory_target() {
        let dir = isolated_dir("directory_target");
        let target_dir = dir.join("report.json");
        std::fs::create_dir(&target_dir).unwrap();

        let result = write_report_file(&target_dir, "content");

        assert!(
            result.is_err(),
            "expected Err refusing a directory target, got {:?}",
            result
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
