# AskMeTrading CodeImpact

Analyse statique de code Rust — complexité cyclomatique, transitive, cachée, impact économique (CPU/mem) et écologique (CO₂/énergie), détection des patterns problématiques (O(n²), récursion, I/O dans boucles).

## Architecture

Hexagonal (ports & adapters) + DDD tactical, core Rust zero-dép.

```
codeimpact/
├── hexagon/          ← domain + application + ports (zero dep)
├── primaries/        ← driving adapters (CLI)
├── secondaries/      ← driven adapters (FS, parsers, reports)
└── tests/            ← unit, integration, e2e
```

Voir [docs/architecture.md](docs/architecture.md) pour le détail.

## Roadmap

| Statut | US | Description |
|--------|----|-------------|
| ✅ | US1 | Analyse complexité cyclomatique |
| ✅ | US2 | Estimation impact économique (CPU/mem) |
| ✅ | US3 | Estimation impact écologique (CO₂) |
| ✅ | US4 | Rapport JSON |
| ✅ | US5 | Détection I/O dans boucles |
| ✅ | US6 | Stress test instrumenté |
| ✅ | US9 | Parser AST avec `syn` |
| ✅ | US10 | Graphe d'appels et complexité transitive |
| ✅ | US11 | Détection des patterns de complexité |
| ✅ | US12 | Rapport enrichi (pattern, warnings par fichier) |
| ✅ | US13 | Graphe de consommation des fichiers |
| 🔲 | US7 | Rapport HTML |
| 🔲 | US8 | Seuils d'alerte personnalisés |

## Installation

```bash
cargo build --release
./target/release/codeimpact --help
```

`cargo build --release` produces **two** binaries side by side in
`target/release/`: `codeimpact` (the CLI above) and
`codeimpact-parse-probe` — an internal canary process the parser spawns
per file to isolate a pathologically-nested source from crashing the
whole scan (#63). It is never invoked directly; keep both binaries
together when distributing or installing `codeimpact` (`CODEIMPACT_PARSE_PROBE`
overrides its location if they cannot ship side by side).

## Utilisation

### Analyser un fichier

```bash
codeimpact analyze src/main.rs
```

Sortie console :

```
=== Rapport d'analyse ===
Complexité directe: 19
Complexité transitive: 18 (dont 0 cachée dans les appels)
Profondeur d'appels max: 2
Fonctions avec cycle: 0
Niveau: moderate

=== Détails par fonction ===
  main — directe: 18, transitive: 18, profondeur: 2 (src/main.rs:38:1)

=== Impact économique estimé ===
Coût CPU: $0.000017
Mémoire: 1.9 KB
Coût total: $0.000017
Niveau: moderate

=== Impact écologique estimé ===
CO₂: 0.0 g
Énergie: 60.8 J (0.000017 kWh)
Classe: A
========================
```

### Rapport JSON

```bash
codeimpact analyze src/main.rs --format json
```

```json
{
  "tool": { "name": "codeimpact", "version": "0.1.0" },
  "timestamp": "2026-07-11T15:48:28Z",
  "target": "src/main.rs",
  "target_type": "file",
  "metrics": {
    "cyclomatic_complexity": 19,
    "transitive_complexity": 18,
    "hidden_complexity": 0,
    "max_call_depth": 2,
    "complexity_level": "moderate",
    "functions_with_cycles": [],
    "function_details": [
      {
        "name": "main",
        "direct": 18,
        "transitive": 18,
        "call_depth": 2,
        "in_cycle": false,
        "location": { "file": "src/main.rs", "line": 38, "col": 1 }
      }
    ],
    "economic_impact": {
      "cpu_cost_microdollars": 16.9,
      "memory_bytes": 1900,
      "total_cost_microdollars": 17.09,
      "level": "moderate"
    },
    "ecological_impact": {
      "co2_grams": 0.007,
      "energy_joules": 60.84,
      "efficiency_class": "A"
    }
  }
}
```

### Analyser un projet complet

```bash
codeimpact analyze --path .
```

Sortie console multi-fichier :

```
=== Métriques par fichier ===
src/main.rs — complexité directe: 19, complexité transitive: 18, niveau: moderate
    main — directe: 18, transitive: 18, profondeur: 2 (src/main.rs:38:1)
    complexité cachée dans les appels: 0
src/lib.rs — complexité directe: 5, complexité transitive: 8, niveau: low
    parse — directe: 3, transitive: 5, profondeur: 1 (src/lib.rs:15:1)
    process — directe: 2, transitive: 3, profondeur: 1 (src/lib.rs:30:1)
    complexité cachée dans les appels: 3
    avertissements:
      [CRITICAL][QuadraticLoop] process → O(n²) probable: appelle validate (src/lib.rs:30:1)

=== Chaînes de consommation ===
  src/main.rs → parse → process

=== Cycles ===
  (aucun cycle détecté)

=== Résumé du projet ===
Fichiers analysés: 2
Dépendances totales: 3
Complexité directe totale: 24
Complexité transitive totale: 26
Profondeur max de chaîne: 3
Fichiers en cycle: 0

=== Impact économique total ===
Coût CPU: $0.000025
Mémoire: 3.2 KB
Coût total: $0.000026
Niveau: moderate
```

### Stress test

```bash
codeimpact stress-test --filter "tests::*"
```

Sortie :

```
=== Stress Test ===
Tests: 298/298 passés (filtre: tests::*)
Durée: 1542 ms
Temps CPU: 1430 ms
Mémoire: 45.3 MB

=== Impact économique réel ===
Coût CPU: $0.000013
Mémoire: 0.0 MB
Coût total: $0.000013
Niveau: low
==============================
```

## Types de complexité détectés

| Type | Description | Exemple |
|------|-------------|---------|
| **Cyclomatique** | Complexité directe (décisions + 1) | `if`, `for`, `while`, `match` |
| **Transitive** | Directe + somme des appelés | `a → b → c` compte la complexité de `b` et `c` |
| **Cachée** | Transitive − directe | Complexité masquée dans les appels |
| **QuadraticLoop** | O(n²) probable | Boucle externe appelle une fonction contenant une boucle |
| **NestedLoops** | Boucles imbriquées | `for` dans `for` |
| **DeepCallChain** | Chaîne d'appels trop profonde | > 5 niveaux d'appels |
| **HiddenComplexity** | Callee bien plus complexe que caller | Callee > 5x la complexité du caller |
| **Recursion** | Appel direct ou indirect à soi-même | `a → b → a` |
| **LargeMatch** | Match avec trop de branches | > 10 arms |
| **DeepConditional** | Imbrication conditionnelle trop profonde | > 5 niveaux de `if` |
| **I/O dans boucles** | Appel I/O dans une boucle | `std::fs::read` dans `for` |

## Licence

MIT