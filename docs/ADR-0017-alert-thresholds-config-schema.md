# ADR-0017 — Seuils d'alerte : une porte de domaine pure, un fichier `.codeimpact.json` partagé, un code de sortie qui gate la CI

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-17
> **Decided in:** Issue #8 (US8), mergée par PR #81 sur `feat/8-alert-thresholds`
> **Links:** [[architecture-overview]], [[alert-thresholds]], [[ADR-0001]], [[ADR-0004]], [[ADR-0006]], [[ADR-0009]], [[ADR-0010]], [[json-report-schema]], [[html-report]], [[glossary]]

## Contexte

US8 veut donner à l'utilisateur un moyen de dire *« au-delà de tel coût CPU ou de tel CO2, échoue »* — un garde-fou activable en CI. Trois forces cadrent la décision :

1. **L'hexagone est zéro-dépendance** ([[ADR-0001]]). Lire un fichier de config (serde_json) ne peut pas entrer dans le domaine.
2. **L'honnêteté de la mesure est non négociable** ([[ADR-0010]]). Une métrique *non mesurée* (`Unmeasurable` / absente) ne doit **jamais** déclencher une alerte — sinon on fabrique un dépassement confiant à partir d'un trou de mesure.
3. **US15 (#31 — filtres include/exclude) viendra écrire dans le même fichier de config.** Choisir un format aujourd'hui sans réserver la place de US15 forcerait une migration cassante demain.

## Décision

### 1. L'évaluation du seuil est une porte de domaine dans l'hexagone zéro-dep

Un Value Object `AlertThresholds` porte deux seuils optionnels et une fonction pure `evaluate(cpu: Option<f64>, co2: Option<f64>) -> ThresholdReport`. Aucune I/O, aucune dépendance externe — l'hexagone reste zéro-dep ([[ADR-0001]]). Le VO est auto-validant (`ddd-value-object`) : `AlertThresholds::new` rejette à la construction tout seuil non-fini ou négatif (`ThresholdError`), de sorte qu'aucun code du système ne peut détenir un seuil capable de produire un dépassement absurde.

`src/contexts/codeimpact/hexagon/src/analysis/alert_thresholds.rs`.

### 2. `None` ne déclenche jamais un dépassement — l'honnêteté d'[[ADR-0010]] étendue au gate

Les entrées d'`evaluate` sont des `Option<f64>`, et la comparaison n'a lieu **que sur `(Some, Some)`** :

```rust
if let (Some(limit), Some(actual)) = (self.max_cpu_microdollars, cpu) {
    if actual > limit { /* breach */ }
}
```

Une métrique absente (CPU ou CO2 `Unmeasurable`, [[ADR-0010]]) ne franchit aucun seuil, si bas soit-il : l'absence n'est pas un zéro confiant. L'invariant est épinglé à **trois niveaux** — au VO (`evaluate`), à l'usage (`RunAnalysis` sur cible fichier et projet, `RunStressTest`), et au stress test (un run `Unmeasurable` dérive `(None, None)`, jamais un `0` qui passerait sous le seuil). C'est la transposition exacte d'[[ADR-0010]] au nouveau gate : ne rien affirmer quand on n'a pas mesuré.

### 3. Format du fichier = `.codeimpact.json`, lu derrière un port — schéma partagé réservé pour US15

Le fichier de config est du **JSON** (`.codeimpact.json`), lu par serde_json dans `secondaries/` derrière `ConfigReaderPort` (DIP, `ca-ports-adapters`) — l'hexagone ne voit qu'`AlertThresholds` déjà validé, jamais serde. Le schéma **réserve délibérément la place de US15** :

```json
{ "thresholds": { "max_cpu_microdollars": 50, "max_co2_grams": 12 } }
```

- Le désérialiseur ne lit **que** la section `thresholds` ; il est `#[serde(default)]` de bout en bout (fichier vide, section absente, champ absent → tous tolérés) et **tolérant aux clés inconnues** (pas de `deny_unknown_fields`).
- US15 (#31) pourra donc ajouter une section `include`/`exclude` au **même fichier** sans collision ni migration — la coordination est actée ici, pas découverte au moment de #31.

`ConfigReaderPort` : `hexagon/src/analysis/config_reader.rs`. Adaptateur : `secondaries/src/gateways/config_readers/file_system_config_reader.rs`.

### 4. `--strict` mappe un dépassement sur le **code de sortie 3**

En mode `--strict`, un dépassement fait sortir le process avec **exit code 3** — délibérément distinct de **1** (erreur d'entrée / runtime) et de **2** (code réservé par clap pour une erreur de parsing d'argument). La **décision** appartient au domaine (`ThresholdReport::has_breach`) ; `main.rs::gated_exit_code` ne fait que la **mapper** sur un code process, jamais re-dériver une comparaison. Sans `--strict`, un dépassement est rapporté mais l'exit reste 0.

**Finding épinglé (parsing CLI negative).** `--max-cpu -5` (séparé par une espace) est avalé par clap comme un flag inconnu → exit **2**, la validation VO n'est jamais atteinte. La forme qui atteint `AlertThresholds::new` (et se fait rejeter proprement en exit 1) est `--max-cpu=-5`. Documenté pour que le comportement ne soit pas pris pour un bug.

`gated_exit_code` : `primaries/src/main.rs`.

### 5. La CLI l'emporte sur le fichier, par métrique

`AlertThresholds::from_sources(file, cli)` fusionne les deux sources : pour chaque métrique, la valeur CLI gagne quand elle est présente (`cli.or(file)`), sinon la valeur fichier passe. Composition de domaine pure — les deux entrées sont déjà validées, la fusion ne peut donc pas produire un résultat invalide. Conséquence opérationnelle (voir [[ADR-0006]] §6) : passer `--max-cpu`/`--max-co2` explicitement en CI **surclasse** le fichier auto-configurable du dépôt.

### 6. Discipline de sécurité du lecteur de config — miroir de `write_report_file` ([[ADR-0006]])

L'adaptateur `FileSystemConfigReader` applique la même défense que l'écriture de rapport ([[ADR-0006]] §4) :

- **canonicalize du parent seul** puis re-jointure du nom de fichier — jamais `canonicalize(path)` direct, qui suivrait un symlink jusqu'à sa cible et ferait inspecter les métadonnées de la **cible** au lieu du symlink ;
- **`symlink_metadata` + `!is_file()`** refuse symlink / FIFO / socket / répertoire **avant** toute lecture ;
- **plafond de taille (1 MiB)** avant lecture/parse ;
- **aucune fuite de path** dans les messages d'erreur (identifiants anonymes) ;
- `AlertThresholds::new` rejette non-fini/négatif à la frontière du VO.

Le garde de profondeur de récursion par défaut de serde_json (128) borne le DoS par JSON profondément imbriqué, à l'intérieur du plafond 1 MiB.

### 7. Périmètre des métriques et de l'agrégation

Métriques gatées : **coût CPU (µ$) et CO2 (g) uniquement**. Le gate évalue l'**agrégat au niveau projet** (une cible mono-fichier utilise l'impact de ce fichier) — **jamais par fonction**. La cible projet passe par `aggregated_metrics`, la cible fichier par l'impact du fichier ; les deux alimentent `evaluate` en `Option<f64>`.

### 8. Un seul renderer partagé, un porteur `GatedOutput<T>`

`humanize::render_threshold_warning` est le renderer **unique** du message de dépassement, réutilisé sur les quatre surfaces (console, JSON, HTML, stderr en `--strict`). Le résultat du gate voyage dans `GatedOutput<T>` : le use case retourne son payload normal **plus** le `ThresholdReport` ; la décision d'exit est prise dans le domaine, mappée dans `main.rs`.

`humanize` : `secondaries/src/gateways/report_writers/humanize.rs`. `GatedOutput` : `hexagon/src/analysis/gated_output.rs`.

## Conséquences

- **(+)** L'hexagone reste zéro-dep : serde_json vit derrière `ConfigReaderPort`, le domaine ne connaît qu'`AlertThresholds`.
- **(+)** Le gate hérite de l'honnêteté d'[[ADR-0010]] : aucune métrique non mesurée ne peut faire échouer un build, à trois niveaux.
- **(+)** Le format `.codeimpact.json` accueillera US15 (#31) sans migration cassante — schéma partagé, section réservée, tolérant.
- **(+)** Trois codes de sortie distincts (1 erreur, 2 clap, 3 breach strict) rendent le gate scriptable en CI ([[ADR-0009]]).
- **(−)** Le fichier de config est self-configurable par le dépôt analysé — vecteur de confiance documenté et remédié en [[ADR-0006]] §6 (opérationnel : CODEOWNERS / flags CLI explicites).
- **(−)** `--max-cpu -5` (espace) est intercepté par clap avant notre validation — comportement de clap, documenté (§4), non corrigé.

## Dette connue, explicitement non traitée

- **US15 (#31)** — sections `include`/`exclude` dans le même fichier : place réservée, non implémentée.
- **Seuils par fonction / par fichier** — hors scope ; le gate est projet-agrégat par conception.
- **Autres métriques** (mémoire, énergie brute) non gatées — CPU et CO2 seulement pour US8.
