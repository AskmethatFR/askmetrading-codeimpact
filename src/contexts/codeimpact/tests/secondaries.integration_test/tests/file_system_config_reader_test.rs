use std::path::PathBuf;

use codeimpact_hexagon::analysis::ConfigReaderPort;
use codeimpact_secondaries::gateways::config_readers::file_system_config_reader::FileSystemConfigReader;

// US8 slice 4 (AD-5) — FileSystemConfigReader, SECURITY SURFACE (ADR-0006
// discipline mirrored from write_report_file/FileSystemCodeReader):
// canonicalize, size cap, refuse non-regular files, no absolute-path leak
// in error messages.
//
// Test List:
// 1. valid config with a thresholds section -> Some(AlertThresholds) with
//    the right values
// 2. no config file anywhere (no --config, nothing in search dirs) -> Ok(None)
// 3. explicit --config pointing to a nonexistent file -> Err, no silent
//    fallback to auto-discovery
// 4. explicit --config pointing to a symlink -> Err (refuses non-regular)
// 5. oversized config file -> Err
// 6. malformed JSON -> Err
// 7. invalid threshold value in the file (negative) -> Err
// 8. config file present but no thresholds section -> Ok(Some(..)) with
//    both metrics None (US15 reserves the schema; absent section is not
//    an error)
// 9. unknown top-level keys are tolerated (future include/exclude section)
// 10. auto-discovery: target dir is tried before cwd
// 11. error messages never leak the absolute path (ADR-0006)

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
        r#"{"thresholds":{"max_cpu_microdollars":12.5,"max_co2_grams":30.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&config_path), &[]);

    let thresholds = result
        .expect("read should succeed")
        .expect("a thresholds section was present");
    assert_eq!(thresholds.max_cpu_microdollars(), Some(12.5));
    assert_eq!(thresholds.max_co2_grams(), Some(30.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_config_file_anywhere_returns_ok_none() {
    let dir = isolated_dir("no_config");
    let reader = FileSystemConfigReader::new();

    let result = reader.read_thresholds(None, &[&dir]);

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
    std::fs::write(&fallback_config, r#"{"thresholds":{"max_cpu_microdollars":1.0}}"#).unwrap();
    let bogus = dir.join("does_not_exist.json");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&bogus), &[&dir]);

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
    std::fs::write(&real_target, r#"{"thresholds":{"max_cpu_microdollars":1.0}}"#).unwrap();
    let link = dir.join(".codeimpact.json");
    symlink(&real_target, &link).expect("create symlink");

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&link), &[]);

    assert!(
        result.is_err(),
        "a symlinked config path must be refused (ADR-0006), got {:?}",
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
    let result = reader.read_thresholds(Some(&config_path), &[]);

    assert!(result.is_err(), "an oversized config file must be refused");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_json_errors() {
    let dir = isolated_dir("malformed");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, "not json at all @@@").unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&config_path), &[]);

    assert!(result.is_err(), "malformed JSON must error");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn negative_threshold_in_file_is_rejected() {
    let dir = isolated_dir("negative");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(&config_path, r#"{"thresholds":{"max_cpu_microdollars":-5.0}}"#).unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&config_path), &[]);

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
    let result = reader.read_thresholds(Some(&config_path), &[]);

    let thresholds = result.expect("empty config is valid").expect("file was present");
    assert_eq!(thresholds.max_cpu_microdollars(), None);
    assert_eq!(thresholds.max_co2_grams(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_top_level_keys_are_tolerated() {
    let dir = isolated_dir("unknown_keys");
    let config_path = dir.join(".codeimpact.json");
    std::fs::write(
        &config_path,
        r#"{"include":["src/**"],"exclude":["target/**"],"thresholds":{"max_co2_grams":7.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(Some(&config_path), &[]);

    let thresholds = result
        .expect("unknown keys must not fail parsing")
        .expect("file was present");
    assert_eq!(thresholds.max_co2_grams(), Some(7.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn auto_discovery_tries_target_dir_before_cwd() {
    let target_dir = isolated_dir("auto_discovery_target");
    let cwd_dir = isolated_dir("auto_discovery_cwd");
    std::fs::write(
        target_dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_cpu_microdollars":42.0}}"#,
    )
    .unwrap();
    std::fs::write(
        cwd_dir.join(".codeimpact.json"),
        r#"{"thresholds":{"max_cpu_microdollars":999.0}}"#,
    )
    .unwrap();

    let reader = FileSystemConfigReader::new();
    let result = reader.read_thresholds(None, &[&target_dir, &cwd_dir]);

    let thresholds = result.expect("read should succeed").expect("a file was found");
    assert_eq!(
        thresholds.max_cpu_microdollars(),
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
    let result = reader.read_thresholds(Some(&bogus), &[]);

    let err = result.expect_err("expected an error for a nonexistent explicit path");
    let err_message = err.to_string();
    assert!(
        !err_message.contains(dir.to_str().unwrap()),
        "error message must not leak the absolute path (ADR-0006): {}",
        err_message
    );
    let _ = std::fs::remove_dir_all(&dir);
}
