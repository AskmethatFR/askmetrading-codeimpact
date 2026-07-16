/// A fake canary (#63 T3) that never terminates on its own — stands in for
/// a pathological source hanging `syn::parse_file` without ever aborting,
/// so `SynCodeParser::probe_source`'s 10s timeout-and-kill path can be
/// proven deterministically (the kill is a difference of *nature*, not a
/// timing margin — ADR-0010's lesson applies to the test too: this probe
/// sleeps far longer than the timeout, not "just barely" past it).
fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
