# Architecture — CodeImpact

## Stack

- **Core:** Rust (zero-dep hexagon)
- **CLI:** `clap` derive
- **JSON:** `serde` / `serde_json`
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

- **Value Objects:** CodeMetrics, AnalysisTarget, EconomicImpact, EcologicalImpact, CodeLocation, OutputFormat, AlertThresholds, **FileFilter**, **AnalysisConfig** (VO composite thresholds + filter — pas un Aggregate DDD, voir [[ADR-0019]])
- **Domain Services:** ProactiveAnalyzer (statique), ReactiveAnalyzer (dynamique), EconomicImpactEstimator
- **Pas d'Entity / Aggregate** dans le MVP (pas de persistence, pas de cycle de vie)

### Ports & Adapters

| Port (hexagon) | Méthodes (signatures agnostiques du langage) | Adapter P0 (secondaries) | Adapter futur |
|---|---|---|---|
| CodeReader | `read_source(target)` · `list_source_files(dir, extensions: &[&str], filter: &FileFilter)` | FileSystemCodeReader (`&["rs"]` fourni par la racine ; walk `ignore` + `globset`, [[ADR-0019]]) | RoslynCodeReader (`&["cs"]`), TsCodeReader (`&["ts","tsx"]`) |
| CodeParser | `parse(source)` · `resolve_dependencies(source, &DependencyContext) -> Vec<PathBuf>` | SynCodeParser (sémantique modules `crate::`/`super::`/`mod.rs` **privée à l'adaptateur**) | RoslynCodeParser, TsCodeParser |
| ProfilerPort | — | *heuristiques* (EconomicImpactEstimator) | ClrMdProfiler, V8Profiler, JvmtiProfiler |
| TestRunnerPort | — | CargoTestRunner | — |
| ReportWriterPort | — | ConsoleReportWriter, JsonReportWriter, HtmlReportWriter | — |
| ConfigReaderPort | `read_config(explicit_path, search_dirs) -> Option<AnalysisConfig>` ([[ADR-0019]]) | FileSystemConfigReader (`.codeimpact.json`, serde_json, `deny_unknown_fields` + schéma forward-compat) | — |

> **Frontière agnostique du langage ([[ADR-0018]]).** L'hexagone est ~100 % agnostique du langage : la sémantique par-langage — résolution de modules/namespaces, extensions de fichiers, signatures d'I/O — vit **entièrement** dans l'adaptateur pilote. `CodeParser::resolve_dependencies` rend des `PathBuf` **déjà résolus** (le protocole `"mod:"`/`"use:"` a disparu) via le VO neutre `DependencyContext`; `CodeReader::list_source_files` filtre sur un ensemble d'extensions passé par la racine de composition. Un adaptateur C#/TS s'ajoute **sans toucher `hexagon/`** — invariant zéro-dép d'[[ADR-0001]] renforcé, pas seulement préservé.

> **Frontière de processus dans `SynCodeParser`.** Le parsing risqué est isolé dans un sous-processus canari dédié (`codeimpact-parse-probe`, une cible binaire Cargo dédiée de `secondaries/`) pour contenir un débordement de pile de `syn` sans tuer le scan. Voir [[ADR-0015]].

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

## JSON Report Format

`JsonReportWriter` (P0 adapter) produces JSON output via `write_json` on the `ReportWriter` port. See [[json-report-schema]] for full schema and [[ADR-0007]] for architecture decisions.

## Alert Thresholds (US8)

Porte de domaine pure `AlertThresholds::evaluate` dans l'hexagone zéro-dep : gate l'énergie (kWh) et le CO2 (g) agrégés projet contre des seuils venus de la CLI (`--max-kwh`/`--max-co2`) et/ou d'un fichier `.codeimpact.json` (lu derrière `ConfigReaderPort`). `--strict` mappe un dépassement sur exit 3. Une métrique non mesurée (`None`) ne franchit jamais un seuil ([[ADR-0010]]). Design courant : [[alert-thresholds]] ; décision : [[ADR-0017]].

## Configuration file — `AnalysisConfig` (US15)

La config projet lue depuis `.codeimpact.json` est désormais un VO composite `AnalysisConfig { thresholds: AlertThresholds, filter: FileFilter }` (pas un Aggregate DDD — VO immuable, voir [[ADR-0019]]). `ConfigReaderPort::read_config(explicit_path, search_dirs) -> Option<AnalysisConfig>` (ex-`read_thresholds`) rend le VO déjà validé ; `Ok(None)` = fichier absent ⇒ `AnalysisConfig::defaults()` (comportement byte-identique au pré-US15). `FileFilter { include, exclude, respect_gitignore }` porte les **motifs bruts validés** ; la **compilation des globs vit dans l'adaptateur** (`globset`), l'hexagone restant zéro-dep ([[ADR-0001]]). Le walk migre de `walkdir` vers la crate `ignore` (`exclude` l'emporte sur `include` ; les 4 sources gitignore gatent ensemble ; `.parents(false)` borne le walk à la racine). Le DTO adaptateur déclare le **schéma forward-compat complet** (`languages`/`sourceRoots`/`extensions`/`parser`/`ioSignatures` parsés-mais-inertes) sous `#[serde(deny_unknown_fields)]`. Décision : [[ADR-0019]].

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
│       │   ├── run_stress_test.rs      # RunStressTest use case
│       │   ├── output_format.rs        # OutputFormat enum (Console, Json)
│       │   └── report_writer.rs        # ReportWriter port trait (write_console, write_json)
│       ├── gateways-secondary_ports/
│       │   ├── code_reader_port.rs     # trait
│       │   └── report_writer_port.rs   # trait (inclut write_stress_test)
│       └── use_cases-application_services/
│           └── run_analysis.rs         # use case (handle, handle_json, handle_project_json)
├── secondaries/
│   └── src/
│       ├── lib.rs
│       └── gateways/
│           ├── code_readers/
│           │   ├── file_system_code_reader.rs
│           │   └── code_reader_stub.rs
│           └── report_writers/
│               ├── console_report_writer.rs
│               ├── json_report_writer.rs    # DTOs sérialisés + JsonReportWriter
│               └── report_writer_stub.rs
│           └── test_runners/
│               ├── cargo_test_runner.rs
│               └── test_runner_stub.rs
├── primaries/
│   └── src/main.rs                     # clap CLI (--format console|json)
└── tests/
    ├── fixtures/sample.rs
    ├── hexagon.unit_test/              # 35+ tests (VOs, analyzer, use case, economic impact, stress test, handle_json)
    ├── secondaries.integration_test/   # 6+ tests (reader, writer, JSON report writer, cargo test runner)
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
| OutputFormat | Enum de format de sortie: Console, Json |
| StressTestRun | Exécution d'un test existant avec instrumentation |
| ProactiveAnalysis | Analyse statique (linter) |
| ReactiveAnalysis | Analyse dynamique (stress test) |

## User Stories

| ID | Priorité | Titre | Statut |
|---|---|---|---|
| US1 | P0 | Analyse complexité cyclomatique | ✅ Livré |
| US2 | P0 | Estimation impact économique (CPU/mem) | ✅ Livré |
| US3 | P0 | Estimation impact écologique (CO2) | ✅ Livré |
| US4 | P0 | Rapport JSON | ✅ Livré |
| US5 | P1 | Détection I/O dans boucles | ✅ Livré |
| US6 | P1 | Stress test instrumenté | ✅ Livré |
| US7 | P1 | Rapport HTML | ✅ Livré |
| US8 | P1 | Seuils d'alerte personnalisés | ✅ Livré |

## Décisions enregistrées (ADR)

| # | Titre | Statut |
|---|---|---|
| 0001 | Rust core, zero-dep hexagon | ✅ Accepté |
| 0002 | 1 seul bounded context (YAGNI) | ✅ Accepté |
| 0003 | Pas de Stryker — exécution directe + mesure | ✅ Accepté |
| 0004 | Heuristiques P0 → profiling réel P2 | ✅ Accepté |
| 0005 | Package-by-context, package-by-layer à l'intérieur | ✅ Accepté |
| 0006 | Sécurité: canonicalize, limite taille fichier, pas de fuite de path | ✅ Appliqué dans US1 |
| 0007 | JSON Report Format — Output Format & Schema | ✅ Appliqué dans US4 |
| … | (0008–0017 — voir docs/INDEX.md, spine canonique) | — |
| 0018 | Hexagone dé-rustifié — sémantique par-langage dans les adaptateurs (US14-T1) | ✅ Appliqué dans #32 |
| 0019 | Fichier de config — agrégat `AnalysisConfig`, globs compilés dans l'adaptateur, schéma forward-compat (US15) | ✅ Appliqué dans #31 |
