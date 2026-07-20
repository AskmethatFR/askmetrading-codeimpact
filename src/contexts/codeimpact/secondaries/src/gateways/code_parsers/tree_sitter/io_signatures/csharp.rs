/// Confident (type-proof) C# I/O prefixes — US16 T4.1, ADR-0016 §1 applied
/// to C#: a call is classified `Io` only when the syntax ITSELF proves the
/// receiver's type, never from a name/variable alone. `File.` and
/// `Directory.` are the two BCL classes whose I/O members are called
/// statically — the class name literally appears in the call text
/// (`File.ReadAllText(...)`), so no type resolution is needed to trust it.
///
/// `HttpClient`/`SqlCommand`/`Stream`/`DbContext` were T2's provisional
/// guess for this table but are normally INSTANCE-typed (`_client.GetAsync
/// (...)`, `_context.Users`) — a literal `HttpClient.` prefix match on an
/// instance receiver would be a name-only assertion ADR-0016 §1 forbids.
/// T4.2's `SUSPICIOUS_RECEIVER_MARKERS` (`classifier.rs`) carries them
/// forward instead, as abstention markers (`Unknown`), never confident
/// `Io` assertions — human-approved Q1.
pub const IO_PREFIXES: &[&str] = &["File.", "Directory."];
