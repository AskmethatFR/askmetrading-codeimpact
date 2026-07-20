# ADR-0006: Sécurité — Canonicalize, Limite Taille, Pas de Fuite de Path

**Status:** Applied  
**Date:** 2026-07-08  
**Applied in:** US1  
**Relations:**  
  depends-on: ["architecture-overview"]  
  related-to: ["ADR-0015", "ADR-0017", "ADR-0019", "ADR-0020", "ADR-0023"]  

## Context

Le CLI lit des fichiers sur le filesystem. Risques: path traversal, fichier immense, fuite de chemin absolu dans les messages d'erreur.

## Decision

1. **Canonicalize** tous les chemins en entrée (`fs::canonicalize`) pour résoudre les symlinks et `..`.
2. **Limite de taille — deux étages** :
   - **Plafond lecture** : l'adaptateur `FileSystemCodeReader` refuse de slurper un fichier au-delà de `MAX_FILE_SIZE` = **10 MB** (garde d'allocation brute).
   - **Borne d'admissibilité mesure** (ajouté #62) : le domaine refuse une source > `MAX_MEASURABLE_SOURCE_BYTES` = **1 MB** via `source_guard::check_admissible`, surfacée `Unmeasurable(SourceTooLarge)` (ADR-0010), avant `syn::parse_file` — borne le RSS pire-cas (~900 MB à 1 MB) sous le plus petit conteneur CI. Une source hostile de 1–10 MB est lue puis refusée à la mesure, jamais un crash ni un `0` silencieux.
   - *(Correction : ce point énonçait « 1 MB (const `MAX_FILE_SIZE`) » ; la constante réelle valait 10 MB — le drift doc/code est ici réconcilié, les deux étages distingués.)*
3. **Pas de fuite de path**: les messages d'erreur utilisent des identifiants anonymes. Le chemin absolu n'est jamais affiché à l'utilisateur.
4. **Garde d'écriture du rapport** (ajouté #53) : `write_report_file` refuse une cible de sortie qui n'est pas un fichier régulier via `std::fs::symlink_metadata` (ne suit pas le composant final) + `!file_type().is_file()` — bloque symlink / FIFO / socket / device / répertoire **avant** `fs::write`. Ferme l'écrasement arbitraire à travers un symlink planté en CI (démontré) et le hang sur FIFO ; s'applique aux formats JSON et HTML (fonction partagée). Erreur path-free (point 3). Résiduel accepté : la fenêtre check→write (TOCTOU) est adéquate pour la menace CI (symlink pré-planté, pas d'attaquant en course concurrente), même classe que le résiduel #40.
5. **Confinement défense-en-profondeur au point d'exécution** (ajouté #40) : le binaire de test compilé est confiné au `target/` du projet par `confine_to_target_dir`. Ce check est **rejoué à l'intérieur de `measure_cmd`** (la fonction qui construit et lance `Command::new`), pas seulement chez l'appelant `build_test_binaries`. `measure_cmd` retourne `Result<Command, _>` et exécute le **chemin canonique retourné** (validate-then-use-the-validated-value), pas l'argument brut. Erreur dure (pas `debug_assert!`) → l'invariant tient **par construction dans tout profil de build**, pas par convention de l'appelant.
6. **Lecteur de config `.codeimpact.json`** (ajouté US8, #8 — voir [[ADR-0017]]) : `FileSystemConfigReader` applique la **même discipline que `write_report_file`** (point 4) — canonicalize du parent seul puis re-jointure du nom (jamais `canonicalize(path)` direct qui suivrait un symlink vers sa cible), `symlink_metadata` + `!is_file()` refuse symlink/FIFO/socket/répertoire **avant** toute lecture, plafond de taille (1 MiB) avant lecture/parse, erreurs path-free, et `AlertThresholds::new` rejette non-fini/négatif à la frontière du VO. Le garde de profondeur de récursion par défaut de serde_json (128) borne le DoS par JSON imbriqué, dans le plafond 1 MiB.

## Frontière de confiance — `.codeimpact.json` comme gate CI dur (finding Security A04, US8)

Quand `.codeimpact.json` sert de **gate CI dur** (`--strict`, exit 3, [[ADR-0017]]), le fichier est **self-configurable par le dépôt analysé** : un contributeur peut, dans la même PR, relever son propre seuil pour faire passer le build.

- **Rayon d'action étroit.** Le fichier n'expose que **deux champs f64 validés** (`max_energy_kwh`, `max_co2_grams`) ; aucun path traversal, aucune injection, aucune exécution possible via ce fichier (§ point 6 ci-dessus). La seule « attaque » est de desserrer son propre garde-fou.
- **Remédiation opérationnelle, pas code.** Couvrir `.codeimpact.json` par **CODEOWNERS / branch protection**, ou faire passer la CI des flags **`--max-kwh`/`--max-co2` explicites** — qui **surclassent** le fichier par métrique ([[ADR-0017]] §5). Même schéma accepté que `.eslintrc` / `codecov.yml` : un fichier de config versionné est de confiance au niveau du contrôle d'accès au dépôt, pas au niveau du code.
- **Résiduel accepté.** Aucune contre-mesure code n'est ajoutée : la surface est trop étroite et la remédiation opérationnelle est le pattern standard de l'écosystème.

## Menace *agrégée* — consommation de ressource projet-globale (finding Security, US14-T5 #33, voir [[ADR-0023]])

Jusqu'à US14-T5, le modèle de menace ressource ne bornait qu'une **unité isolée** : le RSS d'**un** fichier (`MAX_MEASURABLE_SOURCE_BYTES` 1 Mo, `MAX_FILE_SIZE` 10 Mo, point 2). La résolution de dépendances inter-fichiers C# ([[ADR-0023]]) a introduit une **dimension nouvelle** : une pré-passe qui **lit tout le projet en mémoire d'un coup** pour construire l'index namespace→fichiers. Deux classes de coût, invisibles d'une garde par-unité, apparaissent alors :

7. **Plafond mémoire *agrégé* `MAX_PROJECT_SOURCE_BYTES` = 100 Mo (ajouté US14-T5, #33).** Mille fichiers de 1 Mo passent **chacun** la garde par-fichier (point 2) mais somment à ~1 Go en RAM. `source_guard::check_project_admissible` refuse le projet **avant** de charger l'ensemble des sources — **fail-fast**, jamais un OOM ni un swap. C'est la **borne agrégée** complémentaire de la borne par-fichier : la première protège contre *beaucoup de fichiers admissibles*, la seconde contre *un fichier hostile*. Les deux coexistent.

8. **Classe de complexité exponentielle latente — `FileConsumptionGraph::compute_depth` (mémoïsé, #33).** Le comptage de profondeur du graphe était **exponentiel** (comptage de chemins non mémoïsé). **Dormant pour Rust** (faible fan-out des modules), il devenait un **DoS algorithmique atteignable par du C# ordinaire** : la résolution de grain namespace produit des arêtes **denses** ([[ADR-0023]] D5), et un graphe dense fait exploser le nombre de chemins. Fermé par **mémoïsation** (cache `HashMap`, `O(V+E)`), calquée sur `detect_cycles`. Leçon : une borne de *taille d'entrée* ne couvre pas une *classe de complexité* — un petit projet à graphe dense suffit à déclencher un coût super-linéaire.

## Consequences

- Path traversal impossible (canonicalize résout les `../../`).
- (#33, [[ADR-0023]]) La lecture *projet-globale* est bornée (`MAX_PROJECT_SOURCE_BYTES`, fail-fast) et le coût de profondeur du graphe est ramené de exponentiel à `O(V+E)` : la surface de résolution inter-fichiers C# n'ouvre ni OOM agrégé ni DoS algorithmique. Résiduel accepté : la borne 100 Mo est fixe (un très gros monorepo légitime est refusé net plutôt que mesuré — configurable si un cas réel l'exige).
- Fichier de 100 MB ne fait pas planter l'analyse.
- Les logs utilisateur ne contiennent pas de chemins sensibles.
- (#40) Un futur refactor ne peut plus contourner en silence le confinement du binaire exécuté : `measure_cmd` re-valide au point d'exécution. Résiduel accepté : la fenêtre TOCTOU canonicalize→exec (déjà documentée, non aggravée) et le confinement OS-level (sandbox/seccomp) restent hors scope.
