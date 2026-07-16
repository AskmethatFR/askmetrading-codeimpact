use std::io::Read;

/// A fake canary (#63 test infra) whose verdict is DETERMINISTIC and keyed
/// on stdin content rather than on real recursion/stack behavior: any
/// source containing the marker below is treated as refused (an arbitrary
/// unknown exit code, mapped by `verdict_from` to `TooComplex`); anything
/// else is admissible (exit 0). Used to decouple cache-correctness tests
/// from `syn`'s real stack threshold, which shifts across build profiles
/// (Security finding, retry 2) — this probe never runs `syn::parse_file`
/// at all, so its behavior is identical in debug and release.
const REFUSE_MARKER: &str = "CODEIMPACT_TEST_REFUSE_MARKER";

fn main() {
    let mut source = String::new();
    let _ = std::io::stdin().read_to_string(&mut source);

    if source.contains(REFUSE_MARKER) {
        std::process::exit(9);
    }

    std::process::exit(0);
}
