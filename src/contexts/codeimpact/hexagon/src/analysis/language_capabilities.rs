use super::language::Language;

/// Whether a `CodeParser` adapter can produce a given metric for its
/// language, at whatever fidelity T2 built (US16). `Degraded`/`Unsupported`
/// are real cases a future adapter WILL construct (a language whose grammar
/// cannot express loop-nesting detection, say) — T2 itself only ever
/// constructs `Supported` (human-approved Q1: the seam exists now so
/// `CodeParser`'s trait is not re-opened in T3, but nothing renders a
/// degraded/unsupported state yet).
#[derive(Clone, Debug, PartialEq)]
pub enum MetricSupport {
    Supported,
    Degraded(String),
    Unsupported,
}

/// What a language adapter can measure, per metric (US16 T2 seam, `call_graph`
/// added T3). Plain data — no rendering, no behavior beyond construction;
/// covered transitively through each adapter's own test (`SynCodeParser`,
/// `TreeSitterCodeParser`), never given a standalone test of its own (Test
/// Surface Map: a record-shaped type is not a unit).
#[derive(Clone, Debug, PartialEq)]
pub struct LanguageCapabilities {
    language: Language,
    cyclomatic_complexity: MetricSupport,
    io_in_loops: MetricSupport,
    economic_impact: MetricSupport,
    ecological_impact: MetricSupport,
    /// Whether the call graph (transitive/hidden complexity, call depth,
    /// cycles) is built from real resolution or a weaker heuristic — T3's
    /// C# adapter reports `Degraded` here (name-based resolution;
    /// unresolved-receiver calls may merge — T5 corrected this message to
    /// describe what happens TODAY, the precise dropping of ambiguous
    /// edges is deferred to T5.3), never a fabricated `Supported`.
    call_graph: MetricSupport,
    /// Whether the cross-file dependency graph (`resolve_dependencies`) is
    /// built from exact resolution or a coarser heuristic (US16 T5) — T5's
    /// C# adapter reports `Degraded` here (namespace-level resolution: a
    /// file links to every declarer of a used namespace, not necessarily
    /// the one it actually needed), never a fabricated `Supported`.
    cross_file_dependencies: MetricSupport,
}

impl LanguageCapabilities {
    /// The only constructor T2 needs (human-approved Q1: minimal until
    /// T3) — every metric `Supported`, for whichever `language` the caller
    /// names. T3 adapters narrow individual metrics via the `with_*`
    /// builders below.
    pub fn all_supported(language: Language) -> Self {
        Self {
            language,
            cyclomatic_complexity: MetricSupport::Supported,
            io_in_loops: MetricSupport::Supported,
            economic_impact: MetricSupport::Supported,
            ecological_impact: MetricSupport::Supported,
            call_graph: MetricSupport::Supported,
            cross_file_dependencies: MetricSupport::Supported,
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn cyclomatic_complexity(&self) -> &MetricSupport {
        &self.cyclomatic_complexity
    }

    pub fn io_in_loops(&self) -> &MetricSupport {
        &self.io_in_loops
    }

    pub fn economic_impact(&self) -> &MetricSupport {
        &self.economic_impact
    }

    pub fn ecological_impact(&self) -> &MetricSupport {
        &self.ecological_impact
    }

    pub fn call_graph(&self) -> &MetricSupport {
        &self.call_graph
    }

    pub fn cross_file_dependencies(&self) -> &MetricSupport {
        &self.cross_file_dependencies
    }

    /// Builder-style override (mirrors `CodeMetrics::with_economic_impact`)
    /// — an adapter starts from `all_supported` and narrows only the
    /// metrics it cannot honestly claim.
    pub fn with_io_in_loops(mut self, support: MetricSupport) -> Self {
        self.io_in_loops = support;
        self
    }

    pub fn with_call_graph(mut self, support: MetricSupport) -> Self {
        self.call_graph = support;
        self
    }

    pub fn with_cross_file_dependencies(mut self, support: MetricSupport) -> Self {
        self.cross_file_dependencies = support;
        self
    }
}

/// A project-level `MetricSupport`, one per metric axis, folded from every
/// analyzed file's `LanguageCapabilities` (#89 S1, ADR-0021 "dette connue"
/// T3b follow-up). ADR-0021 rendered the honest `n/a`/`Degraded` signal
/// per-file; this VO extends the same honesty to the project aggregate
/// (banner stat tiles + project JSON) — instead of a fabricated "0".
///
/// The aggregate reads `n/a` (Unsupported) only when EVERY analyzed file
/// declares that metric `Unsupported`. Note: no shipped adapter emits
/// `MetricSupport::Unsupported` today — the C# adapter reports `io_in_loops`
/// as `Degraded` (T4 already flipped Unsupported -> Degraded, ADR-0021 D4),
/// so a real pure-C# project currently reads "degraded", not "n/a". The
/// `Unsupported` -> "n/a" path is correct and forward-compatible for
/// whenever a future axis/adapter does declare `Unsupported`, but is not
/// reachable end-to-end via a shipped adapter as of #89.
///
/// Four axes only (human-approved Q3: wire ALL tiles to their axis) — the
/// ones an S1 calling use case (`build_stats`, HTML writer) actually
/// consumes. `call_graph`/`cross_file_dependencies` have no stat tile yet,
/// so they are not folded here (YAGNI: no calling use case, no VO field).
#[derive(Clone, Debug, PartialEq)]
pub struct AggregateMetricSupport {
    cyclomatic_complexity: MetricSupport,
    io_in_loops: MetricSupport,
    economic_impact: MetricSupport,
    ecological_impact: MetricSupport,
}

impl AggregateMetricSupport {
    /// Folds one project-level `MetricSupport` per axis from every file's
    /// declared capabilities. A `None` (no capabilities attached — the Rust
    /// case, ADR-0021 D1) contributes `Supported` to every axis, so a
    /// Rust-only project folds to all-`Supported` and its tiles stay
    /// unchanged.
    pub fn fold<'a>(capabilities: impl Iterator<Item = Option<&'a LanguageCapabilities>>) -> Self {
        let mut cyclomatic_complexity = AxisTally::default();
        let mut io_in_loops = AxisTally::default();
        let mut economic_impact = AxisTally::default();
        let mut ecological_impact = AxisTally::default();
        let all_supported = MetricSupport::Supported;

        for file_capabilities in capabilities {
            match file_capabilities {
                Some(caps) => {
                    cyclomatic_complexity.record(caps.cyclomatic_complexity());
                    io_in_loops.record(caps.io_in_loops());
                    economic_impact.record(caps.economic_impact());
                    ecological_impact.record(caps.ecological_impact());
                }
                None => {
                    cyclomatic_complexity.record(&all_supported);
                    io_in_loops.record(&all_supported);
                    economic_impact.record(&all_supported);
                    ecological_impact.record(&all_supported);
                }
            }
        }

        Self {
            cyclomatic_complexity: cyclomatic_complexity.resolve(),
            io_in_loops: io_in_loops.resolve(),
            economic_impact: economic_impact.resolve(),
            ecological_impact: ecological_impact.resolve(),
        }
    }

    pub fn cyclomatic_complexity(&self) -> &MetricSupport {
        &self.cyclomatic_complexity
    }

    pub fn io_in_loops(&self) -> &MetricSupport {
        &self.io_in_loops
    }

    pub fn economic_impact(&self) -> &MetricSupport {
        &self.economic_impact
    }

    pub fn ecological_impact(&self) -> &MetricSupport {
        &self.ecological_impact
    }
}

/// One axis' running tally across the files folded so far — private, `fold`
/// runs one per axis. Resolves to the approved lattice: `Degraded` wins over
/// everything (a single per-file `Degraded`, OR a mix of `Supported` and
/// `Unsupported` files with no per-file `Degraded` at all); `Unsupported`
/// only when nothing at all was measured; `Supported` otherwise, including
/// the empty/vacuous case. The `Degraded` reason is always the precise
/// coverage count (human-approved Q2), never a concatenation of individual
/// per-file reasons — `supported` counts only fully-`Supported` files, so it
/// reads as "how many files gave a clean measurement" regardless of whether
/// the rest were `Degraded` or `Unsupported`.
#[derive(Default)]
struct AxisTally {
    total: usize,
    supported: usize,
    any_degraded: bool,
    any_unsupported: bool,
}

impl AxisTally {
    fn record(&mut self, support: &MetricSupport) {
        self.total += 1;
        match support {
            MetricSupport::Supported => self.supported += 1,
            MetricSupport::Degraded(_) => self.any_degraded = true,
            MetricSupport::Unsupported => self.any_unsupported = true,
        }
    }

    fn resolve(&self) -> MetricSupport {
        if self.any_degraded || (self.supported > 0 && self.any_unsupported) {
            MetricSupport::Degraded(format!(
                "partial: {}/{} files measured this metric",
                self.supported, self.total
            ))
        } else if self.any_unsupported {
            MetricSupport::Unsupported
        } else {
            MetricSupport::Supported
        }
    }
}
