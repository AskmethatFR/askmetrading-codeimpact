# Architecture — CodeImpact

## Stack

- **Core:** Rust (zero-dep hexagon)
- **CLI:** `clap` derive
- **JSON:** `serde` / `serde_json` (futur)
- **Cross-langage:** FFI (`extern "C"` pour adapters .NET/Node.js/Java — futur)

## Principes architecturaux

### Clean Architecture / Hexagonal

```
Primaries (driving)         Secondaries (driven)
     │                            ▲
     ▼                            │
     └────── Hexagon ─────────────┘
            ├── Domain Model
            ├── Ports (traits)
            └── Use Cases (orchestration)
```

### Dependency Rule

```
primaries → hexagon + secondaries
secondaries → hexagon
hexagon → rien
```

### DDD Tactical

- **Value Objects:** CodeMetrics, AnalysisTarget, EconomicImpact, EcologicalImpact, CodeLocation
- **Domain Services:** ProactiveAnalyzer (statique), ReactiveAnalyzer (dynamique), EconomicImpactEstimator
- **Pas d'Entity / Aggregate** dans le MVP (pas de persistence, pas de cycle de vie)

### Ports & Adapters

| Port (hexagon) | Adapter P0 (secondaries) | Adapter futur |
|---|---|---|
| CodeReaderPort | FileSystemCodeReader | — |
| ProfilerPort | *heuristiques* (EconomicImpactEstimator) | ClrMdProfiler, V8Profiler, JvmtiProfiler |
| TestRunnerPort | CargoTestRunner | — |
| ReportWriterPort | ConsoleReportWriter | JsonReportWriter |

### Naming conventions

| Élément | Convention | Exemple |
|---|---|---|
| Port trait | `{Noun}Port` | `CodeReaderPort` |
| Adapter réel | `{Technology}{Noun}` | `FileSystemCodeReader` |
| Stub test | `{Noun}Stub` | `CodeReaderStub` |
| Use case | `{Verb}{Noun}` | `RunAnalysis` |
| VO | `{Noun}` | `CodeMetrics` |
| Projet test | `{Context}.{Level}Test` | `hexagon.unit_test` |

## Economic Impact Estimation

`EconomicImpactEstimator` (domain service) derives CPU cost and memory from static complexity metrics. See [[economic-impact-estimator]] for full technical rationale.

### Formulas (summary)

| Measure | Formula | Unit |
|---|---|---|
| CPU cost | `direct × 0.5 + transitive × 0.3 + max_call_depth × 1.0 + warnings × 2.0` | μ$ |
| Memory | `direct × 100 + hidden × 200 + loops × 1024` | bytes |
| Total cost | `cpu_cost + memory_bytes × 0.0001` | μ$ |

### Levels

| Range (μ$) | Level |
|---|---|
| 0–10 | low |
| 10.01–20 | moderate |
| 20.01–40 | high |
| 40.01+ | critical |

### Key Design Decisions

1. **Heuristics P0 → profiling P2** ([[ADR-0004]]). Real profiling deferred; heuristics are the first ProfilerPort adapter.
2. **Coefficients are provisional.** Recalibrate when real profiler data exists (MAPE > 50% triggers update).
3. **Memory scaled by 0.0001** in total cost. Memory is cheap relative to CPU; this factor bridges the magnitude gap.
4. **Levels mirror complexity thresholds.** Same 0–10/11–20/21–40/41+ scheme as `CodeMetrics::complexity_level()` for user consistency.

## Module structure (actuelle)

```
codeimpact/
├── Cargo.toml                          # workspace
├── hexagon/                            # zero deps (std only)
│   └── src/
│       ├── lib.rs
│       ├── domain_model/
│       │   ├── code_metrics.rs         # VO — complexité + niveau + impact économique
│       │   ├── analysis_target.rs      # VO — fichier/projet cible
│       │   ├── analysis_rule.rs        # enum — règles d'analyse
│       │   ├── proactive_analyzer.rs   # domain service — calcul complexité + impact éco
│       │   └── errors.rs               # AnalysisError
│       ├── analysis/
│       │   ├── economic_impact.rs      # EconomicImpact VO + EconomicImpactEstimator
│       │   ├── call_graph.rs           # CallGraph + analyse transitive
│       │   ├── code_parser.rs          # ParsedFunction, analyse AST
│       │   ├── complexity_detector.rs  # ComplexityWarning, patterns
│       │   ├── reactive_analyzer.rs    # ReactiveAnalyzer — impact réel via stress test
│       │   ├── stress_test_run.rs      # StressTestRun VO + TestRunnerPort trait
│       │   └── run_stress_test.rs      # RunStressTest use case
│       ├── gateways-secondary_ports/
│       │   ├── code_reader_port.rs     # trait
│       │   └── report_writer_port.rs   # trait (inclut write_stress_test)
│       └── use_cases-application_services/
│           └── run_analysis.rs         # use case
├── secondaries/
│   └── src/
│       ├── lib.rs
│       └── gateways/
│           ├── code_readers/
│           │   ├── file_system_code_reader.rs
│           │   └── code_reader_stub.rs
│           └── report_writers/
│               ├── console_report_writer.rs
│               └── report_writer_stub.rs
│           └── test_runners/
│               ├── cargo_test_runner.rs
│               └── test_runner_stub.rs
├── primaries/
│   └── src/main.rs                     # clap CLI
└── tests/
    ├── fixtures/sample.rs
    ├── hexagon.unit_test/              # 35 tests (VOs, analyzer, use case, economic impact, stress test)
    ├── secondaries.integration_test/   # 6 tests (reader, writer, cargo test runner)
    └── primaries.e2e_test/             # 8 tests (CLI)
```

## Bounded Context

Un seul bounded context pour le MVP: **CodeImpact**.

**Ubiquitous Language:**

| Terme | Définition |
|---|---|
| AnalysisTarget | Fichier ou projet soumis à l'analyse |
| CodeMetrics | Mesures extraites du code source (complexité, patterns I/O, etc.) |
| EconomicImpact | Coût CPU/mem estimé (μ$, bytes) |
| MicroDollars | Unité de coût CPU: 1 μ$ = 10⁻⁶ $. Base: ~0.10 $/CPU-heure cloud |
| EconomicImpactEstimator | Domain service qui calcule EconomicImpact à partir de métriques statiques |
| EcologicalImpact | CO2/énergie dérivé de l'impact économique |
| StressTestRun | Exécution d'un test existant avec instrumentation |
| ProactiveAnalysis | Analyse statique (linter) |
| ReactiveAnalysis | Analyse dynamique (stress test) |

## User Stories

| ID | Priorité | Titre | Statut |
|---|---|---|---|
| US1 | P0 | Analyse complexité cyclomatique | ✅ Livré |
| US2 | P0 | Estimation impact économique (CPU/mem) | ✅ Livré |
| US3 | P0 | Estimation impact écologique (CO2) | ✅ Livré |
| US4 | P0 | Rapport JSON | 📋 En attente |
| US5 | P1 | Détection I/O dans boucles | ✅ Livré |
| US6 | P1 | Stress test instrumenté | ✅ Livré |
| US7 | P1 | Rapport HTML | 📋 En attente |
| US8 | P1 | Seuils d'alerte personnalisés | 📋 En attente |

## Décisions enregistrées (ADR)

| # | Titre | Statut |
|---|---|---|
| 0001 | Rust core, zero-dep hexagon | ✅ Accepté |
| 0002 | 1 seul bounded context (YAGNI) | ✅ Accepté |
| 0003 | Pas de Stryker — exécution directe + mesure | ✅ Accepté |
| 0004 | Heuristiques P0 → profiling réel P2 | ✅ Accepté |
| 0005 | Package-by-context, package-by-layer à l'intérieur | ✅ Accepté |
| 0006 | Sécurité: canonicalize, limite taille fichier, pas de fuite de path | ✅ Appliqué dans US1 |
