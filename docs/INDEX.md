# Project Knowledge Graph — CodeImpact

> **Type:** Index  
> **Owner:** Architect + PM (shared)  
> **Updated:** 2026-07-11

## Nodes

| ID | Type | Title | Status | Updated | Links | Path |
|---|---|---|---|---|---|---|
| architecture-overview | technical | Architecture — CodeImpact | Applied | 2026-07-11 | [[ADR-0001]], [[ADR-0002]], [[ADR-0003]], [[ADR-0004]], [[ADR-0005]], [[ADR-0006]], [[ADR-0007]], [[economic-impact-estimator]], [[json-report-schema]] | docs/architecture.md |
| economic-impact-estimator | technical | Economic Impact Estimator — Technical Rationale | Applied | 2026-07-08 | [[ADR-0004]], [[architecture-overview]] | docs/technical/economic-impact.md |
| json-report-schema | technical | JSON Report Schema — CodeImpact | Applied | 2026-07-11 | [[ADR-0007]], [[architecture-overview]] | docs/technical/json-report-schema.md |
| ADR-0001 | technical | Core Rust, zero-dep hexagon | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0001-rust-core-hexagon.md |
| ADR-0002 | technical | Bounded context unique (YAGNI) | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0002-single-bounded-context.md |
| ADR-0003 | technical | Pas de Stryker — exécution directe + mesure | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0003-no-stryker.md |
| ADR-0004 | technical | Economic Impact — Heuristics P0, Profiling P2 | Accepted | 2026-07-08 | [[architecture-overview]], [[economic-impact-estimator]] | docs/ADR-0004-economic-impact-heuristics.md |
| ADR-0005 | technical | Package-by-Context, Package-by-Layer à l'intérieur | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0005-package-by-context.md |
| ADR-0006 | technical | Sécurité — Canonicalize, Limite Taille, Pas de Fuite de Path | Applied | 2026-07-08 | [[architecture-overview]] | docs/ADR-0006-security-measures.md |
| ADR-0007 | technical | JSON Report Format — Output Format & Schema | Applied | 2026-07-11 | [[architecture-overview]], [[json-report-schema]] | docs/ADR-0007-json-report-format.md |
| glossary | functional | Glossaire — Ubiquitous Language | Live | 2026-07-08 | [[architecture-overview]] | docs/glossary.md |

## Graph Health

- **Total nodes:** 11
- **Dangling [[id]]:** 0
- **Orphan nodes:** 0
- **All rows map to existing files:** ✅
