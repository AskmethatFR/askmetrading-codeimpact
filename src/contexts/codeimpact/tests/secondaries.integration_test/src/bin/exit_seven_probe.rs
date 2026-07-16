use std::io::Read;

/// A fake canary (#63 T3) that always exits with an arbitrary unknown code
/// — neither `EXIT_ADMISSIBLE` (0) nor `EXIT_SYNTAX_ERROR` (2) — proving
/// `verdict_from`'s "anything but 0 or 2 is refused" rule end-to-end
/// through the real subprocess wiring, not just the pure mapping function.
fn main() {
    let mut discard = String::new();
    let _ = std::io::stdin().read_to_string(&mut discard);
    std::process::exit(7);
}
