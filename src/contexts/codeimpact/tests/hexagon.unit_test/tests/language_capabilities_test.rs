use codeimpact_hexagon::analysis::AggregateMetricSupport;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::LanguageCapabilities;
use codeimpact_hexagon::analysis::MetricSupport;

// Test List (AggregateMetricSupport::fold — #89 S1, ADR-0021 T3b follow-up):
// 1. all_supported_or_none_files_fold_to_supported — every file Supported
//    (or capabilities: None, treated as Supported) -> Supported.
// 2. all_unsupported_folds_to_unsupported — every file Unsupported for the
//    axis -> Unsupported.
// 3. mixed_supported_and_unsupported_folds_to_degraded — the headline
//    lattice arm: a naive "any supported -> Supported" AND a naive "any
//    unsupported -> Unsupported" both fail this case.
// 4. any_degraded_folds_to_degraded — a single per-file Degraded state
//    dominates even when every other file is Supported.
// 5. degraded_and_unsupported_folds_to_degraded — Degraded still wins when
//    an Unsupported file is also present (the any_degraded arm is checked
//    first in the lattice).
// 6. empty_iterator_folds_to_supported — vacuous truth: no files means
//    nothing to report as degraded, not an artificial Unsupported/panic.
// 7. axes_fold_independently — narrowing io_in_loops on a file must not
//    drag cyclomatic_complexity (left Supported) down with it: each axis
//    folds on its own per-file state, never on a shared verdict.

fn csharp_with_io(support: MetricSupport) -> LanguageCapabilities {
    LanguageCapabilities::all_supported(Language::CSharp).with_io_in_loops(support)
}

#[test]
fn all_supported_or_none_files_fold_to_supported() {
    let rust = LanguageCapabilities::all_supported(Language::Rust);
    let aggregate = AggregateMetricSupport::fold(vec![Some(&rust), None].into_iter());

    assert_eq!(*aggregate.io_in_loops(), MetricSupport::Supported);
}

#[test]
fn all_unsupported_folds_to_unsupported() {
    let a = csharp_with_io(MetricSupport::Unsupported);
    let b = csharp_with_io(MetricSupport::Unsupported);
    let aggregate = AggregateMetricSupport::fold(vec![Some(&a), Some(&b)].into_iter());

    assert_eq!(*aggregate.io_in_loops(), MetricSupport::Unsupported);
}

#[test]
fn mixed_supported_and_unsupported_folds_to_degraded() {
    let rust = LanguageCapabilities::all_supported(Language::Rust);
    let csharp = csharp_with_io(MetricSupport::Unsupported);
    let aggregate = AggregateMetricSupport::fold(vec![Some(&rust), Some(&csharp)].into_iter());

    match aggregate.io_in_loops() {
        MetricSupport::Degraded(reason) => {
            assert_eq!(reason, "partial: 1/2 files measured this metric");
        }
        other => panic!(
            "expected Degraded for a mixed Supported+Unsupported fold, got {:?}",
            other
        ),
    }
}

#[test]
fn any_degraded_folds_to_degraded() {
    let rust = LanguageCapabilities::all_supported(Language::Rust);
    let degraded = csharp_with_io(MetricSupport::Degraded("syntactic only".to_string()));
    let aggregate = AggregateMetricSupport::fold(vec![Some(&rust), Some(&degraded)].into_iter());

    match aggregate.io_in_loops() {
        MetricSupport::Degraded(reason) => {
            assert_eq!(reason, "partial: 1/2 files measured this metric");
        }
        other => panic!(
            "expected Degraded when any file is Degraded, got {:?}",
            other
        ),
    }
}

#[test]
fn degraded_and_unsupported_folds_to_degraded() {
    let degraded = csharp_with_io(MetricSupport::Degraded("syntactic only".to_string()));
    let unsupported = csharp_with_io(MetricSupport::Unsupported);
    let aggregate =
        AggregateMetricSupport::fold(vec![Some(&degraded), Some(&unsupported)].into_iter());

    match aggregate.io_in_loops() {
        MetricSupport::Degraded(reason) => {
            assert_eq!(reason, "partial: 0/2 files measured this metric");
        }
        other => panic!(
            "expected Degraded to win over Unsupported (any_degraded arm), got {:?}",
            other
        ),
    }
}

#[test]
fn empty_iterator_folds_to_supported() {
    let aggregate = AggregateMetricSupport::fold(std::iter::empty());

    assert_eq!(*aggregate.io_in_loops(), MetricSupport::Supported);
    assert_eq!(*aggregate.cyclomatic_complexity(), MetricSupport::Supported);
    assert_eq!(*aggregate.economic_impact(), MetricSupport::Supported);
    assert_eq!(*aggregate.ecological_impact(), MetricSupport::Supported);
}

#[test]
fn axes_fold_independently() {
    let csharp = csharp_with_io(MetricSupport::Unsupported);
    let aggregate = AggregateMetricSupport::fold(vec![Some(&csharp)].into_iter());

    assert_eq!(*aggregate.io_in_loops(), MetricSupport::Unsupported);
    assert_eq!(
        *aggregate.cyclomatic_complexity(),
        MetricSupport::Supported,
        "narrowing io_in_loops must not drag cyclomatic_complexity (left at its Supported \
         default) down with it — each axis folds independently"
    );
}
