use std::io::Read;
use std::io::Write;

/// A fake canary (#63) that always reports "admissible" (exit 0) but first
/// appends one line to the file named by `PROBE_CALL_LOG` — so a test can
/// count how many times `SynCodeParser` actually spawned the probe, e.g. to
/// prove its single-entry verdict cache avoids a second fork+exec for the
/// same source (T2).
fn main() {
    let mut discard = String::new();
    let _ = std::io::stdin().read_to_string(&mut discard);

    if let Ok(log_path) = std::env::var("PROBE_CALL_LOG") {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = writeln!(file, "call");
        }
    }

    std::process::exit(0);
}
