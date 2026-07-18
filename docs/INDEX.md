# Project Knowledge Graph — CodeImpact

> **Type:** Index  
> **Owner:** Architect + PM (shared)  
> **Updated:** 2026-07-18

## Nodes

| ID | Type | Title | Status | Updated | Links | Path |
|---|---|---|---|---|---|---|
| architecture-overview | technical | Architecture — CodeImpact | Applied | 2026-07-18 | [[ADR-0001]], [[ADR-0002]], [[ADR-0003]], [[ADR-0004]], [[ADR-0005]], [[ADR-0006]], [[ADR-0007]], [[ADR-0008]], [[ADR-0017]], [[ADR-0018]], [[economic-impact-estimator]], [[json-report-schema]], [[html-report]], [[alert-thresholds]] | docs/architecture.md |
| console-report-enriched | technical | Console Report Format — CodeImpact (Enriched) | Applied | 2026-07-11 | [[architecture-overview]], [[json-report-schema]], [[html-report]] | docs/technical/console-report-enriched.md |
| economic-impact-estimator | technical | Economic Impact Estimator — Technical Rationale | Applied | 2026-07-08 | [[ADR-0004]], [[architecture-overview]] | docs/technical/economic-impact.md |
| json-report-schema | technical | JSON Report Schema — CodeImpact | Applied | 2026-07-11 | [[ADR-0007]], [[architecture-overview]], [[html-report]] | docs/technical/json-report-schema.md |
| html-report | technical | HTML Report — Self-Contained Visual Report | Applied | 2026-07-12 | [[ADR-0008]], [[architecture-overview]], [[json-report-schema]], [[console-report-enriched]] | docs/technical/html-report.md |
| alert-thresholds | technical | Alert Thresholds — Threshold Gate Design (US8) | Applied | 2026-07-17 | [[architecture-overview]], [[ADR-0017]], [[ADR-0010]], [[ADR-0006]], [[json-report-schema]], [[html-report]], [[glossary]] | docs/technical/alert-thresholds.md |
| ADR-0001 | technical | Core Rust, zero-dep hexagon | Accepted | 2026-07-08 | [[architecture-overview]], [[ADR-0017]], [[ADR-0018]] | docs/ADR-0001-rust-core-hexagon.md |
| ADR-0002 | technical | Bounded context unique (YAGNI) | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0002-single-bounded-context.md |
| ADR-0003 | technical | Pas de Stryker — exécution directe + mesure | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0003-no-stryker.md |
| ADR-0004 | technical | Economic Impact — Heuristics P0, Profiling P2 | Accepted | 2026-07-08 | [[architecture-overview]], [[economic-impact-estimator]] | docs/ADR-0004-economic-impact-heuristics.md |
| ADR-0005 | technical | Package-by-Context, Package-by-Layer à l'intérieur | Accepted | 2026-07-08 | [[architecture-overview]] | docs/ADR-0005-package-by-context.md |
| ADR-0006 | technical | Sécurité — Canonicalize, Limite Taille, Pas de Fuite de Path | Applied | 2026-07-17 | [[architecture-overview]], [[ADR-0015]], [[ADR-0017]] | docs/ADR-0006-security-measures.md |
| ADR-0007 | technical | JSON Report Format — Output Format & Schema | Applied | 2026-07-11 | [[architecture-overview]], [[json-report-schema]] | docs/ADR-0007-json-report-format.md |
| ADR-0008 | technical | HTML Report Format — Self-Contained Output & XSS Defense | Applied | 2026-07-12 | [[architecture-overview]], [[html-report]] | docs/ADR-0008-html-report-format.md |
| ADR-0009 | technical | CI GitHub Actions & posture supply-chain (dépôt public) | Applied | 2026-07-12 | [[architecture-overview]], [[ADR-0006]] | docs/ADR-0009-ci-supply-chain.md |
| ADR-0010 | technical | Honnêteté de la mesure — `Unmeasurable` plutôt que `0` | Applied | 2026-07-17 | [[architecture-overview]], [[ADR-0004]], [[ADR-0006]], [[economic-impact-estimator]], [[ADR-0015]], [[ADR-0016]], [[ADR-0017]] | docs/ADR-0010-measurement-honesty.md |
| ADR-0011 | technical | Stress test — portée workspace, agrégation, 0-test honnête | Applied | 2026-07-12 | [[architecture-overview]], [[ADR-0010]], [[ADR-0006]], [[ADR-0009]] | docs/ADR-0011-stress-test-workspace-scope.md |
| ADR-0012 | technical | `hidden_complexity` — mesurée à l'atome, jamais dérivée d'agrégats | Applied | 2026-07-14 | [[architecture-overview]], [[ADR-0010]], [[ADR-0007]], [[glossary]], [[json-report-schema]], [[console-report-enriched]] | docs/ADR-0012-hidden-complexity-single-source.md |
| ADR-0013 | technical | Contrat parser ↔ hexagone — le domaine nomme le concept, l'adaptateur nomme la syntaxe | Applied | 2026-07-14 | [[architecture-overview]], [[ADR-0010]], [[ADR-0012]], [[ADR-0001]], [[glossary]], [[ADR-0016]], [[ADR-0018]] | docs/ADR-0013-parser-hexagon-loop-call-contract.md |
| ADR-0014 | technical | Le parseur voit enfin les méthodes — nom qualifié, résolution intra-type, trois états de mesure | Applied | 2026-07-14 | [[architecture-overview]], [[ADR-0010]], [[ADR-0013]], [[ADR-0007]], [[ADR-0008]], [[glossary]], [[json-report-schema]], [[console-report-enriched]], [[html-report]], [[ADR-0016]], [[ADR-0018]] | docs/ADR-0014-parser-impl-methods-qualified-names.md |
| ADR-0015 | technical | Isolation du parsing par sous-processus canari — contenir un débordement de pile sans le prédire | Accepted | 2026-07-16 | [[architecture-overview]], [[ADR-0006]], [[ADR-0010]] | docs/ADR-0015-subprocess-canary-parse-isolation.md |
| ADR-0016 | technical | Classification des I/O en boucle — le type affirme, le nom s'abstient, trois états, calibration mesurée | Applied | 2026-07-16 | [[architecture-overview]], [[ADR-0004]], [[ADR-0010]], [[ADR-0013]], [[ADR-0014]], [[ADR-0015]], [[glossary]] | docs/ADR-0016-io-in-loops-type-asserts-name-abstains.md |
| ADR-0017 | technical | Seuils d'alerte — porte domaine pure, `.codeimpact.json` partagé, exit 3 CI | Applied | 2026-07-17 | [[architecture-overview]], [[alert-thresholds]], [[ADR-0001]], [[ADR-0004]], [[ADR-0006]], [[ADR-0009]], [[ADR-0010]], [[json-report-schema]], [[html-report]], [[glossary]] | docs/ADR-0017-alert-thresholds-config-schema.md |
| ADR-0018 | technical | Hexagone dé-rustifié — sémantique par-langage dans les adaptateurs (US14-T1) | Applied | 2026-07-18 | [[architecture-overview]], [[ADR-0001]], [[ADR-0013]], [[ADR-0014]], [[glossary]] | docs/ADR-0018-de-rustified-hexagon-language-agnostic.md |
| glossary | functional | Glossaire — Ubiquitous Language | Live | 2026-07-18 | [[architecture-overview]], [[ADR-0001]], [[ADR-0006]], [[ADR-0010]], [[ADR-0011]], [[ADR-0012]], [[ADR-0013]], [[ADR-0014]], [[ADR-0016]], [[ADR-0017]], [[ADR-0018]] | docs/glossary.md |

## Graph Health

- **Total nodes:** 25
- **Dangling [[id]]:** 0
- **Orphan nodes:** 0
- **All rows map to existing files:** ✅
