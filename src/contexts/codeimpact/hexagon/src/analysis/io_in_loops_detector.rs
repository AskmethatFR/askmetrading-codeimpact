use super::code_location::CodeLocation;
use super::code_parser::ParsedFunction;
use super::io_classification::IoClassification;
use super::io_in_loop_warning::IoInLoopWarning;

/// Stateless domain service that detects I/O calls inside loops.
///
/// Iterates each `ParsedFunction`'s `calls_in_loops` field and interprets
/// the fact the parser (secondaries) recorded there — the parser records
/// every nested call unconditionally (`io` classifies, it does not filter),
/// this detector is the one deciding which facts it cares about. Two
/// interpretations of the same fact (#56 T2, mirrors ADR-0013 §1's "un fait,
/// deux interprétations"): `detect` surfaces `Io` calls as warnings,
/// `count_unclassifiable` counts `Unknown` calls as an aggregate — never a
/// per-line pseudo-warning (ADR-0010).
pub struct IoInLoopsDetector;

impl IoInLoopsDetector {
    pub fn detect(functions: &[ParsedFunction]) -> Vec<IoInLoopWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            for call in &f.calls_in_loops {
                match call.io {
                    IoClassification::Io => {
                        warnings.push(IoInLoopWarning {
                            function: f.name.clone(),
                            io_call: call.name.clone(),
                            location: CodeLocation::new("".into(), call.line, call.col),
                        });
                    }
                    IoClassification::NotIo | IoClassification::Unknown => {}
                }
            }
        }
        warnings
    }

    /// Counts calls whose receiver could not be classified at all —
    /// abstention as a NUMBER (ADR-0010), never as a per-line detail. A file
    /// with zero unclassifiable calls reports `0`, an honest and meaningful
    /// answer, not an omitted signal.
    pub fn count_unclassifiable(functions: &[ParsedFunction]) -> usize {
        let mut count = 0;
        for f in functions {
            for call in &f.calls_in_loops {
                match call.io {
                    IoClassification::Unknown => count += 1,
                    IoClassification::Io | IoClassification::NotIo => {}
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::super::code_parser::{LoopCall, ParsedFunction};
    use super::*;

    // Test List — detect() (unchanged since #47 retry 2, now driven by
    // IoClassification::Io instead of is_io == true):
    // 1. detects single IO call in loop — warning with function, io_call, location
    // 2. detects multiple IO calls in same function — multiple warnings
    // 3. skips functions with no calls_in_loops — empty result
    // 4. handles multiple functions — each produces its own warnings
    // 5. warning fields match the parsed data
    // 6. a NotIo call in a loop produces ZERO warnings — the parser records
    //    every nested call, IO or not; the detector must match on
    //    IoClassification::Io itself rather than trust the field to hold
    //    Io-only entries.
    //
    // Test List — count_unclassifiable() (#56 T2, new):
    // 7. a single Unknown call in a loop counts as 1
    // 8. multiple Unknown calls across functions all count
    // 9. Io and NotIo calls do not contribute to the count
    // 10. an Unknown call produces ZERO warnings via detect() (the split:
    //     one fact, two interpretations — Unknown is counted, never warned)

    fn make_fn(
        name: &str,
        calls_in_loops: Vec<(&str, usize, usize, IoClassification)>,
    ) -> ParsedFunction {
        ParsedFunction {
            name: name.to_string(),
            start_line: 1,
            calls: vec![],
            has_loop: false,
            has_nested_loop: false,
            decision_points: 0,
            depth: 0,
            match_arms: 0,
            calls_in_loops: calls_in_loops
                .into_iter()
                .map(|(call, line, col, io)| LoopCall {
                    name: call.to_string(),
                    line,
                    col,
                    io,
                })
                .collect(),
        }
    }

    #[test]
    fn single_io_call_in_loop_detected() {
        let fns = vec![make_fn(
            "read_file",
            vec![("std::fs::read", 5, 9, IoClassification::Io)],
        )];
        let warnings = IoInLoopsDetector::detect(&fns);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].function, "read_file");
        assert_eq!(warnings[0].io_call, "std::fs::read");
        assert_eq!(warnings[0].location.to_string(), ":5:9");
    }

    #[test]
    fn multiple_io_calls_in_same_function() {
        let fns = vec![make_fn(
            "process",
            vec![
                ("std::fs::read", 5, 9, IoClassification::Io),
                ("std::fs::write", 10, 5, IoClassification::Io),
            ],
        )];
        let warnings = IoInLoopsDetector::detect(&fns);

        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].io_call, "std::fs::read");
        assert_eq!(warnings[1].io_call, "std::fs::write");
    }

    #[test]
    fn skips_functions_with_no_io_in_loops() {
        let fns = vec![make_fn("clean", vec![])];
        let warnings = IoInLoopsDetector::detect(&fns);
        assert!(warnings.is_empty());
    }

    #[test]
    fn multiple_functions_each_produce_warnings() {
        let fns = vec![
            make_fn(
                "read_file",
                vec![("std::fs::read", 5, 9, IoClassification::Io)],
            ),
            make_fn(
                "write_file",
                vec![("std::fs::write", 3, 7, IoClassification::Io)],
            ),
        ];
        let warnings = IoInLoopsDetector::detect(&fns);

        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].function, "read_file");
        assert_eq!(warnings[1].function, "write_file");
    }

    #[test]
    fn warning_fields_match_parsed_data() {
        let fns = vec![make_fn(
            "my_func",
            vec![("std::fs::read_to_string", 42, 13, IoClassification::Io)],
        )];
        let warnings = IoInLoopsDetector::detect(&fns);

        assert_eq!(warnings[0].function, "my_func");
        assert_eq!(warnings[0].io_call, "std::fs::read_to_string");
        assert_eq!(warnings[0].location.line(), 42);
        assert_eq!(warnings[0].location.col(), 13);
    }

    #[test]
    fn non_io_call_in_loop_produces_zero_warnings() {
        // (#47 retry 2) The parser now records EVERY nested call, IO or
        // not — calls_in_loops is no longer IO-only despite its name. The
        // detector must match on IoClassification::Io; trusting the field
        // to hold only Io entries (as it did before) would leak every plain
        // nested call as a fabricated I/O warning.
        let fns = vec![make_fn(
            "process",
            vec![("validate", 2, 5, IoClassification::NotIo)],
        )];
        let warnings = IoInLoopsDetector::detect(&fns);
        assert!(
            warnings.is_empty(),
            "a NotIo call recorded in calls_in_loops must not surface as an IoInLoopWarning"
        );
    }

    #[test]
    fn single_unknown_call_counts_as_one() {
        let fns = vec![make_fn(
            "process",
            vec![("read", 2, 5, IoClassification::Unknown)],
        )];
        assert_eq!(IoInLoopsDetector::count_unclassifiable(&fns), 1);
    }

    #[test]
    fn multiple_unknown_calls_across_functions_all_count() {
        let fns = vec![
            make_fn("a", vec![("read", 2, 5, IoClassification::Unknown)]),
            make_fn(
                "b",
                vec![
                    ("write", 3, 1, IoClassification::Unknown),
                    ("connect", 4, 1, IoClassification::Unknown),
                ],
            ),
        ];
        assert_eq!(IoInLoopsDetector::count_unclassifiable(&fns), 3);
    }

    #[test]
    fn io_and_not_io_calls_do_not_contribute_to_unclassifiable_count() {
        let fns = vec![make_fn(
            "process",
            vec![
                ("std::fs::read", 2, 5, IoClassification::Io),
                ("validate", 3, 1, IoClassification::NotIo),
            ],
        )];
        assert_eq!(IoInLoopsDetector::count_unclassifiable(&fns), 0);
    }

    #[test]
    fn unknown_call_produces_zero_warnings_but_is_counted() {
        // The split (#56 T2): one fact (`calls_in_loops`), two
        // interpretations. An Unknown call must never surface via detect()
        // — that would turn an abstention into a pseudo-warning (ADR-0010)
        // — but it must still be visible via count_unclassifiable().
        let fns = vec![make_fn(
            "process",
            vec![("read", 2, 5, IoClassification::Unknown)],
        )];
        assert!(IoInLoopsDetector::detect(&fns).is_empty());
        assert_eq!(IoInLoopsDetector::count_unclassifiable(&fns), 1);
    }
}
