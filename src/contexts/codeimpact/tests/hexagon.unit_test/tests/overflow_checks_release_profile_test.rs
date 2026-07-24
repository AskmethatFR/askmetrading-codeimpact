// Ticket #51 — the workspace defines no `[profile.release]` section, so
// `overflow-checks` and `debug-assertions` are OFF in `--release`. A u32/u64
// overflow there WRAPS silently instead of panicking: exactly the "plausible
// but false" number ADR-0010/ADR-0012 forbid.
//
// This is config, not application logic — there is no unit to drive through
// its own API. The empirical proof is a `--release` test run itself (same
// lesson as ADR-0010 §"différence de nature vs marge": mutate and RUN, don't
// reason about a build flag's effect).
//
// Test List:
// 1. overflow_panics_under_release_when_checks_enabled — a runtime (not
//    const-folded) u32 addition that overflows MUST panic under
//    `cargo test --release` once `[profile.release] overflow-checks = true`
//    is set. Before that Cargo.toml change, this same test FAILS under
//    `cargo test --release` (the addition wraps silently, `#[should_panic]`
//    never fires). Under plain `cargo test` (dev profile) it was already
//    green — the dev profile has `overflow-checks = true` by default — so
//    this test's discriminating power lives entirely in the `--release`
//    run, which is exactly the gap this ticket closes.

#[inline(never)]
fn add_one(x: u32) -> u32 {
    x + 1
}

#[test]
#[should_panic(expected = "attempt to add with overflow")]
fn overflow_panics_under_release_when_checks_enabled() {
    // `black_box` defeats compile-time const-folding of the overflow, so
    // this exercises the RUNTIME overflow check the release profile flag
    // controls, not a `deny(arithmetic_overflow)` compile-time error.
    let max = std::hint::black_box(u32::MAX);
    let _ = add_one(max);
}
