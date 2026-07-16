use std::io::Read;

/// The canary's own parse stack — deliberately smaller than the parent's
/// re-parse budget (`PARENT_REPARSE_STACK_BYTES` in `syn_code_parser.rs`),
/// so a successful probe (exit 0) *proves* the parent's larger budget will
/// also succeed (stack dominance, D2, #63) rather than merely hoping so.
const PROBE_STACK_BYTES: usize = 16 * 1024 * 1024;

/// The only two exit codes the parent (`syn_code_parser::verdict_from`)
/// treats as proof the parse thread terminated cleanly. Every other code —
/// including the ones below — is deliberately generic: the parent's
/// mapping rule is "anything but 0 or 2 is refused", so no other exit path
/// in this binary needs its own reserved number.
const EXIT_ADMISSIBLE: i32 = 0;
const EXIT_SYNTAX_ERROR: i32 = 2;
/// Any abnormal termination that is NOT a proven syntax error — stdin
/// unreadable, the defense-in-depth size guard tripping, or the parse
/// thread panicking without overflowing the stack. Never reused as a
/// signal for "safe to re-parse": only `EXIT_SYNTAX_ERROR` carries that
/// guarantee, because only that path actually ran `syn::parse_file` to
/// completion inside the stack-bounded thread.
const EXIT_NOT_PROVEN_SAFE: i32 = 101;

/// Best-effort address-space cap (D4, #63): caps worst-case RSS on a
/// pathological input that parses without overflowing the stack but
/// allocates unreasonably. Unix-only this cycle — Windows equivalent (Job
/// Objects) is deferred; the size guard (`source_guard`, #62) is the
/// primary defense on every OS.
#[cfg(unix)]
fn apply_memory_limit() {
    const RLIMIT_AS_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    let limit = libc::rlimit {
        rlim_cur: RLIMIT_AS_BYTES as libc::rlim_t,
        rlim_max: RLIMIT_AS_BYTES as libc::rlim_t,
    };
    // Best-effort: a failure here (e.g. a stricter limit already imposed by
    // the caller) is not fatal — the size guard remains the primary bound.
    unsafe {
        libc::setrlimit(libc::RLIMIT_AS, &limit);
    }
}

fn main() {
    #[cfg(unix)]
    apply_memory_limit();

    let mut source = String::new();
    if std::io::stdin().read_to_string(&mut source).is_err() {
        std::process::exit(EXIT_NOT_PROVEN_SAFE);
    }

    // Defense in depth: the parent already refused an oversized source
    // before ever spawning this process, but a probe invoked directly
    // (tests, `CODEIMPACT_PARSE_PROBE` override) gets the same guard.
    if codeimpact_hexagon::analysis::check_admissible(&source).is_err() {
        std::process::exit(EXIT_NOT_PROVEN_SAFE);
    }

    // Runs the SAME parse-and-walk pipeline the parent will run on
    // success (Security finding retry 1, CWE-674) — not a bare
    // `syn::parse_file` — so this canary's stack budget is measured
    // against the actual recursion the parent re-parse performs, not just
    // its first stage.
    let exit_code = std::thread::Builder::new()
        .stack_size(PROBE_STACK_BYTES)
        .spawn(move || {
            match codeimpact_secondaries::gateways::code_parsers::syn_code_parser::exercise_full_pipeline(
                &source,
            ) {
                Ok(()) => EXIT_ADMISSIBLE,
                Err(_) => EXIT_SYNTAX_ERROR,
            }
        })
        .expect("failed to spawn probe thread")
        .join()
        // A genuine stack overflow aborts the whole process before this
        // join could ever return Err — this only guards a plain panic
        // inside the parse thread, which did NOT run the pipeline to
        // completion and therefore cannot claim EXIT_SYNTAX_ERROR's
        // safe-to-reparse guarantee.
        .unwrap_or(EXIT_NOT_PROVEN_SAFE);

    std::process::exit(exit_code);
}
