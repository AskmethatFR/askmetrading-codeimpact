# AskMeTrading CodeImpact

Analyse d'impact économique (CPU/mem/network) et écologique (CO2/énergie) du code source.

## Architecture

Hexagonal (ports & adapters) + DDD tactical, core Rust.

```
codeimpact/           ← bounded context unique
├── hexagon/          ← domain + application + ports (zero dep)
├── primaries/        ← driving adapters (CLI, LSP, CI)
├── secondaries/      ← driven adapters (FS, profilers, reports)
└── tests/            ← unit, integration, e2e
```

Voir [docs/architecture.md](docs/architecture.md) pour le détail.

## Roadmap

| Slice | US | Description |
|-------|----|-------------|
| 1 (P0) | US1, US2 | Analyse proactive + rapport console/JSON |
| 2 (P1) | US3 | Détection I/O dans boucles |
| 3 (P1) | US4 | Stress test sur tests existants |
| 4 (P2) | US5, US6 | Multi-langage + seuils CI |

## Utilisation

```bash
# Analyse proactive d'un fichier
codeimpact analyze file.rs

# Rapport JSON
codeimpact analyze file.rs --format json

# Stress test sur les tests existants
codeimpact stress-test --filter "tests::*"
```

## Licence

MIT