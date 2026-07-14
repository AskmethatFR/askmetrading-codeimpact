use super::code_location::CodeLocation;
use super::code_parser::ParsedFunction;
use super::io_in_loop_warning::IoInLoopWarning;

/// Stateless domain service that detects I/O calls inside loops.
///
/// Iterates each `ParsedFunction` and checks its `calls_in_loops` field.
/// The parser (secondaries) is responsible for identifying which calls
/// are I/O — the domain only maps them to warnings.
pub struct IoInLoopsDetector;

impl IoInLoopsDetector {
    pub fn detect(functions: &[ParsedFunction]) -> Vec<IoInLoopWarning> {
        let mut warnings = Vec::new();
        for f in functions {
            for call in &f.calls_in_loops {
                warnings.push(IoInLoopWarning {
                    function: f.name.clone(),
                    io_call: call.name.clone(),
                    location: CodeLocation::new("".into(), call.line, call.col),
                });
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::super::code_parser::{LoopCall, ParsedFunction};
    use super::*;

    // Test List:
    // 1. detects single IO call in loop — warning with function, io_call, location
    // 2. detects multiple IO calls in same function — multiple warnings
    // 3. skips functions with no calls_in_loops — empty result
    // 4. handles multiple functions — each produces its own warnings
    // 5. warning fields match the parsed data
    // 6. (#47 retry 2) a non-I/O call in a loop produces ZERO warnings — the
    //    parser now records every nested call, IO or not; the detector must
    //    filter on is_io itself rather than trust the field to hold IO-only
    //    entries.

    fn make_fn(name: &str, calls_in_loops: Vec<(&str, usize, usize, bool)>) -> ParsedFunction {
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
                .map(|(call, line, col, is_io)| LoopCall {
                    name: call.to_string(),
                    line,
                    col,
                    is_io,
                })
                .collect(),
        }
    }

    #[test]
    fn single_io_call_in_loop_detected() {
        let fns = vec![make_fn("read_file", vec![("std::fs::read", 5, 9, true)])];
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
                ("std::fs::read", 5, 9, true),
                ("std::fs::write", 10, 5, true),
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
            make_fn("read_file", vec![("std::fs::read", 5, 9, true)]),
            make_fn("write_file", vec![("std::fs::write", 3, 7, true)]),
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
            vec![("std::fs::read_to_string", 42, 13, true)],
        )];
        let warnings = IoInLoopsDetector::detect(&fns);

        assert_eq!(warnings[0].function, "my_func");
        assert_eq!(warnings[0].io_call, "std::fs::read_to_string");
        assert_eq!(warnings[0].location.line(), 42);
        assert_eq!(warnings[0].location.col(), 13);
    }
}
