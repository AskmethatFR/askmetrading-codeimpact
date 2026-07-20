use std::path::PathBuf;

use codeimpact_hexagon::analysis::ConfigReaderPort;
use codeimpact_secondaries::gateways::config_readers::file_system_config_reader::FileSystemConfigReader;

// US8 slice 4 (AD-5) / US31 (#31) — FileSystemConfigReader, SECURITY
// SURFACE (ADR-0006 discipline mirrored from write_report_file/
// FileSystemCodeReader): canonicalize, size cap, refuse non-regular files,
// no absolute-path leak in error messages.
//
// Test List:
// 1. valid config with a thresholds section -> Some(AnalysisConfig) with
//    the right threshold values
// 2. no config file anywhere (no --config, nothing in search dirs) -> Ok(None)
// 3. explicit --config pointing to a nonexistent file -> Err, no silent
//    fallback to auto-discovery
// 4. explicit --config pointing to a symlink -> Err (refuses non-regular)
// 5. oversized config file -> Err
// 6. malformed JSON -> Err naming the line/position
// 7. invalid threshold value in the file (negative) -> Err
// 8. config file present but no thresholds section -> Ok(Some(..)) with
//    both metrics None (absent section is not an error)
// 9. a genuinely unknown/typo top-level key is now REJECTED
//    (deny_unknown_fields, US31 — a change from US8's tolerant schema)
// 10. reserved forward-compat keys (languages, sourceRoots, extensions,
//     parser, ioSignatures) are tolerated (parsed, not wired)
// 11. auto-discovery: target dir is tried before cwd
// 12. error messages never leak the absolute path (ADR-0006)
// 13. explicit --config pointing to a FIFO -> Err, without hanging
// 14. include/exclude/respectGitignore are parsed into the FileFilter
// 15. respectGitignore defaults to true when the file is present but the
//     key is absent (D4 — distinct from "no file at all", which is
//     AnalysisConfig::defaults() with respect_gitignore=false)
// 16. an include pattern attempting path traversal ("../etc/**") is
//     rejected at the FileFilter boundary (AC4/D1)
// 17. an invalid glob syntax in include/exclude is rejected

fn isolated_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "codeimpact_config_reader_test_{}_{}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated test dir");
    dir
}

#[test]
fn valid_config_with_thresholds_section_is_read() {
    let dir = isolated_dir("valid");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(
        &config_path,
        r#"{"thresholds":{"max_energy_kwh":12.5,"max_co2_grams":30.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("read should succeed")
        .expect("a thresholds section was present");
    assert_eq!(config.thresholds().max_energy_kwh(), Some(12.5));
    assert_eq!(config.thresholds().max_co2_grams(), Some(30.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_config_file_anywhere_returns_ok_none() {
    let dir = isolated_dir("no_config");
    let reader = FileSystemConfigReader::new();

    let result = reader.read_config(None, &[&dir]);

    assert_eq!(
        result.expect("no config file is not an error"),
        None,
        "AC6: the config file is optional"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explicit_nonexistent_config_path_errors_without_silent_fallback() {
    let dir = isolated_dir("explicit_missing");
    let fallback_config = dir.join(".codeimpact.json");
    std::fs::write(&fallback_config, r#"{"thresholds":{"max_energy_kwh":1.0}}"#).unwrap();
    let bogus = dir.join("does_not_exist.json");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&bogus), &[&dir]);

    assert!(
        result.is_err(),
        "an explicit --config path that doesn't exist must error, not silently fall back to the \
         auto-discovered file next to it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn explicit_symlink_config_path_is_refused() {
    use std::os::unix::fs::symlink;

    let dir = isolated_dir("symlink");
    let real_target = dir.join("real.json");
    std::fs::write(&real_target, r#"{"thresholds":{"max_energy_kwh":1.0}}"#).unwrap();
    let link = dir.join(".codeimpact.json");
    symlink(&real_target, &link).expect("create symlink");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&link), &[]);

    assert!(
        result.is_err(),
        "a symlinked config path must be refused (ADR-0006), got {:?}",
        result
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Review-barrier sweep (Security + Dev-B, issue #8) — completes the
// security mirror against write_report_file_tests::
// refuses_fifo_target_without_hanging (primaries/src/main.rs): a FIFO
// config target must be refused, and refused WITHOUT hanging (a naive
// `fs::read_to_string` on a FIFO with no writer blocks forever; the
// `symlink_metadata` non-regular-file check in `read_and_validate` must
// reject it before `read_to_string` is ever reached).
#[cfg(unix)]
#[test]
fn refuses_fifo_config_target_without_hanging() {
    let dir = isolated_dir("fifo");
    let fifo = dir.join(".codeimpact.json");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("failed to spawn mkfifo");
    assert!(status.success(), "mkfifo must succeed to set up the test");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&fifo), &[]);

    assert!(
        result.is_err(),
        "a FIFO config target must be refused, got {:?}",
        result
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn oversized_config_file_is_refused() {
    let dir = isolated_dir("oversized");
    let config_path = dir.join(".codeimpact.json");
    // 1 MiB cap + 1 extra byte, padded inside a JSON string so it is still
    // syntactically valid JSON (proves the SIZE check runs before parsing).
    let padding = "a".repeat(1024 * 1024 + 1);
    std::fs::write(&config_path, format!(r#"{{"padding":"{}"}}"#, padding)).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    assert!(result.is_err(), "an oversized config file must be refused");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_json_error_names_the_line() {
    let dir = isolated_dir("malformed");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, "not json at all @@@").unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let err = result.expect_err("malformed JSON must error");
    let message = err.to_string();
    assert!(
        message.contains("line"),
        "the error must name the offending line (AC4): {}",
        message
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn negative_threshold_in_file_is_rejected() {
    let dir = isolated_dir("negative");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{"thresholds":{"max_energy_kwh":-5.0}}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    assert!(
        result.is_err(),
        "a negative threshold in the config file must be rejected, same as a CLI one"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_without_thresholds_section_yields_both_metrics_none() {
    let dir = isolated_dir("no_thresholds_section");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("empty config is valid")
        .expect("file was present");
    assert_eq!(config.thresholds().max_energy_kwh(), None);
    assert_eq!(config.thresholds().max_co2_grams(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_top_level_key_is_rejected() {
    let dir = isolated_dir("unknown_key");
    let config_path = dir.join(".codeimpact.json");
    // "includ" is a typo of "include" — must be REJECTED (deny_unknown_fields,
    // US31), not silently tolerated the way US8's schema tolerated it.
    std::fs::write(
        &config_path,
        r#"{"includ":["src/**"],"thresholds":{"max_co2_grams":7.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let err = result.expect_err("a typo'd/unknown key must now be rejected");
    let message = err.to_string();
    assert!(
        message.contains("includ"),
        "the error should name the offending key: {}",
        message
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reserved_forward_compat_keys_are_tolerated() {
    let dir = isolated_dir("forward_compat");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(
        &config_path,
        r#"{
            "$schema": "https://example.test/schema.json",
            "languages": ["rust"],
            "sourceRoots": ["src"],
            "extensions": ["rs"],
            "parser": {"engine": "syn"},
            "ioSignatures": [],
            "thresholds": {"max_co2_grams": 7.0}
        }"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("reserved forward-compat keys must not fail parsing")
        .expect("file was present");
    assert_eq!(config.thresholds().max_co2_grams(), Some(7.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_discovery_tries_target_dir_before_cwd() {
    let target_dir = isolated_dir("auto_discovery_target");
    let cwd_dir = isolated_dir("auto_discovery_cwd");
    std::fs::write(
        target_dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_energy_kwh":42.0}}"#,
    )
    .unwrap();
    std::fs::write(
        cwd_dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_energy_kwh":999.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(None, &[&target_dir, &cwd_dir]);

    let config = result
        .expect("read should succeed")
        .expect("a file was found");
    assert_eq!(
        config.thresholds().max_energy_kwh(),
        Some(42.0),
        "the target dir's config must win over cwd's"
    );
    let _ = std::fs::remove_dir_all(&target_dir);
    let _ = std::fs::remove_dir_all(&cwd_dir);
}

#[test]
fn error_messages_never_leak_the_absolute_path() {
    let dir = isolated_dir("no_path_leak");
    let bogus = dir.join("does_not_exist.json");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&bogus), &[]);

    let err = result.expect_err("expected an error for a nonexistent explicit path");
    let err_message = err.to_string();
    assert!(
        !err_message.contains(dir.to_str().unwrap()),
        "error message must not leak the absolute path (ADR-0006): {}",
        err_message
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── US31 (#31) — include/exclude/respectGitignore parsed into the
// FileFilter carried by AnalysisConfig ──

#[test]
fn include_exclude_and_respect_gitignore_are_parsed_into_the_filter() {
    let dir = isolated_dir("filter_fields");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(
        &config_path,
        r#"{"include":["src/**"],"exclude":["target/**"],"respectGitignore":false}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("read should succeed")
        .expect("file was present");
    assert_eq!(config.file_filter().include(), &["src/**".to_string()]);
    assert_eq!(config.file_filter().exclude(), &["target/**".to_string()]);
    assert!(!config.file_filter().respect_gitignore());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn respect_gitignore_defaults_to_true_when_file_present_but_key_absent() {
    let dir = isolated_dir("gitignore_default");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("read should succeed")
        .expect("file was present");
    assert!(
        config.file_filter().respect_gitignore(),
        "D4: a present file without the key defaults respect_gitignore to true"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn include_pattern_attempting_path_traversal_is_rejected() {
    let dir = isolated_dir("traversal");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{"include":["../etc/**"]}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    assert!(
        result.is_err(),
        "a path-traversal include pattern must be rejected (AC4/D1), got {:?}",
        result
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// D1: glob SYNTAX validation is deliberately NOT this reader's job — it
// only builds a validated `FileFilter` of raw patterns (rejecting
// traversal/absolute/empty/etc. shape issues above). Compiling the glob
// happens later, in `FileSystemCodeReader::list_source_files` (already
// pinned in file_system_code_reader_test.rs's
// `invalid_glob_syntax_in_filter_errors_instead_of_panicking`), and the
// full CLI path (config read -> walk) is pinned end-to-end in
// main_test.rs. A syntactically odd but shape-valid pattern like `src/[`
// must therefore still parse successfully here.
#[test]
fn syntactically_odd_but_shape_valid_glob_parses_successfully_here() {
    let dir = isolated_dir("odd_glob_shape");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{"exclude":["src/["]}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_config(Some(&config_path), &[]);

    let config = result
        .expect("read should succeed — glob syntax is validated at walk time, not here")
        .expect("file was present");
    assert_eq!(config.file_filter().exclude(), &["src/[".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}
