# CodeImpact — Project Context

## Architecture

Hexagonal (ports & adapters) + DDD tactical. Single bounded context.

```
codeimpact/
├── hexagon/          # domain + application + ports. Zero external deps.
├── primaries/        # driving adapters (CLI, LSP, CI)
├── secondaries/      # driven adapters (FS, profilers, reports)
└── tests/            # unit, integration, e2e
```

## Stack

- **Core:** Rust (zero-dep hexagon)
- **CLI:** `clap` derive
- **Async:** `tokio` (filesystem I/O only)
- **JSON:** `serde` / `serde_json`
- **Cross-langage:** FFI (`extern "C"` for .NET/Node.js/Java adapters — future)

## Dependency Rule

```
primaries → hexagon + secondaries
secondaries → hexagon
hexagon → rien
```

## Naming Conventions

| Element | Convention | Example |
|---|---|---|
| Port trait | `{Noun}Port` | `CodeReaderPort` |
| Adapter | `{Technology}{Noun}` | `FileSystemCodeReader` |
| Stub | `{Noun}Stub` | `CodeReaderStub` |
| Use case | `{Verb}{Noun}` | `RunAnalysis` |
| VO | `{Noun}` | `CodeMetrics` |
| Test project | `{Context}.{Level}Test` | `hexagon.unit_test` |

## US Roadmap

| # | Title | Depends on | Priority |
|---|---|---|---|
| 1 | Analyse complexité cyclomatique | — | P0 |
| 2 | Estimation impact économique (CPU/mem) | US1 | P0 |
| 3 | Estimation impact écologique (CO2) | US2 | P0 |
| 4 | Rapport JSON | US1 | P0 |
| 5 | Détection I/O dans boucles | US1 | P1 |
| 6 | Stress test instrumenté | US1, US2 | P1 |
| 7 | Rapport HTML | US4 | P1 |
| 8 | Seuils d'alerte personnalisés | US1, US2, US3 | P1 |

## Workspace Structure

```
Cargo.toml (workspace)
codeimpact/
├── hexagon/Cargo.toml          # zero external deps
├── primaries/Cargo.toml        # depends on hexagon + secondaries
├── secondaries/Cargo.toml      # depends on hexagon
└── tests/
    ├── hexagon.unit_test/      # unit tests with stubs
    ├── secondaries.integration_test/
    └── primaries.e2e_test/
```

## Key ADRs

- ADR-0001: Rust core, zero-dep hexagon
- ADR-0002: 1 bounded context (YAGNI)
- ADR-0003: No Stryker — direct execution + measurement
- ADR-0004: Heuristics P0 → real profiling P2
- ADR-0005: Package-by-context, layers inside