# Architecture — CodeImpact

## Stack

- **Core:** Rust (zero-dep hexagon, `async-trait` ports)
- **CLI:** `clap` derive
- **JSON:** `serde` / `serde_json`
- **Async:** `tokio` (filesystem, profiling)
- **Cross-langage:** FFI (`extern "C"` pour adapters .NET/Node.js/Java)

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

- **Value Objects:** EconomicImpact, EcologicalImpact, CodeMetrics, CodeLocation, AnalysisTarget
- **Domain Services:** ProactiveAnalyzer (statique), ReactiveAnalyzer (dynamique)
- **Pas d'Entity / Aggregate** dans le MVP (pas de persistence, pas de cycle de vie)

### Ports & Adapters

| Port (hexagon) | Adapter P0 (secondaries) | Adapter futur |
|---|---|---|
| CodeReaderPort | FileSystemCodeReader | — |
| ProfilerPort | *heuristiques* (pas d'implémentation réelle P0) | ClrMdProfiler, V8Profiler, JvmtiProfiler |
| TestRunnerPort | CargoTestRunner | — |
| ReportWriterPort | ConsoleReportWriter, JsonReportWriter | — |

### Naming conventions

| Élément | Convention | Exemple |
|---|---|---|
| Port trait | `{Noun}Port` | `CodeReaderPort` |
| Adapter réel | `{Technology}{Noun}` | `FileSystemCodeReader` |
| Stub test | `{Noun}Stub` | `CodeReaderStub` |
| Use case | `{Verb}{Noun}` | `RunAnalysis` |
| VO | `{Noun}` | `EconomicImpact` |
| Projet test | `{Context}.{Level}Test` | `hexagon.unit_test` |

## Bounded Context

Un seul bounded context pour le MVP: **CodeImpact**.

**Ubiquitous Language:**

| Terme | Définition |
|---|---|
| AnalysisTarget | Fichier ou projet soumis à l'analyse |
| CodeMetrics | Mesures extraites du code source (complexité, patterns I/O, etc.) |
| EconomicImpact | Coût CPU/mem/network estimé |
| EcologicalImpact | CO2/énergie dérivé de l'impact économique |
| StressTestRun | Exécution d'un test existant avec instrumentation |
| ProactiveAnalysis | Analyse statique (linter) |
| ReactiveAnalysis | Analyse dynamique (stress test) |

## User Stories

| ID | Priorité | Titre | Slice |
|---|---|---|---|
| US1 | P0 | Analyse proactive d'un fichier | 1 |
| US2 | P0 | Rapport console + JSON | 1 |
| US3 | P1 | Détection I/O dans boucles | 2 |
| US4 | P1 | Stress test sur tests existants | 3 |
| US5 | P2 | Support .NET (CLR MD) | 4 |
| US6 | P2 | Seuil d'alerte CI | 4 |

## Décisions enregistrées (ADR)

- **ADR-0001:** Rust core, zero-dep hexagon
- **ADR-0002:** 1 seul bounded context (YAGNI)
- **ADR-0003:** Pas de Stryker — exécution directe + mesure
- **ADR-0004:** Heuristiques P0 → profiling réel P2
- **ADR-0005:** Package-by-context, package-by-layer à l'intérieur