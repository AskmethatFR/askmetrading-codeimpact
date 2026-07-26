# ADR-0028 — Recette de mutation testing : la suite externe doit être câblée, et trois faux-verts doivent être fermés explicitement

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-26
> **Decided in:** Issue #114 / PR #116
> **Links:** [[architecture-overview]], [[ADR-0003]], [[ADR-0010]], [[ADR-0025]], [[ADR-0009]]

## Contexte

La Definition of Done de l'équipe inclut désormais un **gate de mutation** (`~/.claude/lib/mutation_gate.py`), qui lance `cargo mutants` et analyse `mutants.out/outcomes.json`. Ce dépôt n'avait aucun outillage de mutation : ni `cargo-mutants` installé, ni `mutants.toml`, ni recette.

**Le piège est la disposition des tests.** Chaque test vit dans un crate binaire `[[test]]` séparé (`tests/hexagon.unit_test`, `tests/secondaries.integration_test`, `tests/primaries.e2e_test`), chacun avec `[lib] test = false`. Un `cargo mutants` par défaut mute un paquet et lance *les tests de ce paquet* — vides ici. Sans configuration, l'outil rapporte un run vert contre **zéro test exécuté** : exactement le mode de défaillance que le gate existe pour empêcher. C'est la même racine que #110 (`tdd-run.sh --lib` ne trouve aucun test dans cette disposition).

Trois chemins de **faux vert** ont été découverts pendant la construction, chacun reproduit empiriquement contre le binaire réel `cargo-mutants 27.1.0`. Ils sont documentés ici parce qu'aucun n'est déductible de la documentation de l'outil, et que les trois se reproduiraient à l'identique dans tout dépôt reprenant cette recette.

## Décision

Une seule recette, source de vérité pour le local et pour toute CI future : `scripts/mutation.sh` + `.cargo/mutants.toml` — même principe que `scripts/coverage.sh` ([[ADR-0025]]).

### 1. Câbler la suite externe — `test_workspace = true`

`.cargo/mutants.toml` pose `test_workspace = true`, ce qui fait exécuter `cargo test --workspace` à chaque mutant : « quel paquet porte les tests » cesse de compter. `exclude_globs = ["src/contexts/codeimpact/tests/**"]` sort les crates de test de la surface mutée (mesuré : 1636 mutants sans config → 1540 avec, dont 96 issus de `codeimpact/tests/`).

Emplacement `.cargo/mutants.toml` et non `mutants.toml` à la racine : c'est le défaut réel de l'outil. Vérifié qu'il est bien lu (`--list` vs `--list --no-config` diffèrent), et invisible pour `mutation_gate.py`, qui ne référence aucun chemin de config.

### 2. Faux-vert n°1 — le rapport périmé blanchi

`cargo mutants --in-diff <patch-sans-Rust>` sort **0** et ne touche, ni ne fait tourner, ni ne supprime `mutants.out/`. Un garde `[[ ! -f outcomes.json ]]` ne se déclenche donc que si le répertoire est vide par hasard.

Reproduit : un `outcomes.json` préexistant contenant `missed: 1` a été relu comme le résultat du run courant, produisant `exit 0` et « 15 mutant(s) validly examined » — un **succès annoncé par-dessus l'échec d'un run antérieur**, sans qu'un seul mutant ait tourné.

**Fermeture :** `rm -f mutants.out/outcomes.json` en tête de script, avant toute invocation. Rendre le garde exact plutôt que d'ajouter un test de fraîcheur : `[[ fichier -nt marqueur ]]` est inutilisable, `-nt` est à la seconde sous bash 3.2 et un run rapide fait égalité.

`mutation_gate.py` est immunisé contre ce chemin par son propre `_locate_report(..., not_before=run_started_at)`. La recette était le seul appelant non protégé.

### 3. Faux-vert n°2 — la baseline ne peut pas jouer son rôle

Sous `test_workspace = true`, la **baseline** tourne `cargo test --package=<muté>` alors que **chaque mutant** tourne `cargo test --workspace`. Asymétrie lue dans les logs de phase de l'outil, et confirmée indépendamment par l'argv que cargo-mutants écrit lui-même dans `outcomes.json`.

Conséquence, démontrée à code de production **identique**, seul l'état de la suite variant :

| Suite workspace | Résultat |
|---|---|
| ROUGE | `10 caught / 0 missed`, exit 0, `verdict: "pass"` |
| VERTE | `5 caught / 5 missed`, exit 2 |

`ok Unmutated baseline` dans les deux cas — la baseline ne voit jamais le rouge workspace.

**C'est le faux vert le plus grave du lot**, parce que le triple `ran==true && blocking==true && verdict=="pass"` est précisément ce qui **relâche la cérémonie de provenance TDD** de l'équipe. Une suite cassée allègerait donc silencieusement le gate TDD sur tous les tickets suivants : une suite pourrie achèterait *moins* de vérification, pas plus.

Aucune clé de configuration ne corrige la baseline — trois combinaisons testées (`test_workspace` seul ; `test_workspace` + `--workspace` en CLI ; `test_workspace` + `test_package`), la baseline reste `--package` dans les trois.

**Fermeture :** la recette fournit sa **propre** baseline — `cargo test --workspace --quiet -- --test-threads=4` — et refuse de muter sur rouge, avant toute invocation de cargo-mutants (prouvé par l'absence de `mutants.out` après refus).

### 4. Faux-vert n°3 — la contention réintroduite par le correctif précédent

Le gate du §3 lance la suite complète. Or [[ADR-0025]] établit que les `SourceTooComplex` fantômes de `syn_code_parser` sont de la contention wall-clock affamant le sous-processus sonde au-delà de son `PROBE_TIMEOUT` de 10 s, et la ferme *précisément* par `--test-threads=4`. Le gate était la recette d'[[ADR-0025]] **moins sa mitigation**.

Deux conséquences, la seconde étant décisive : un flake fait refuser la recette en désignant le mauvais coupable (« workspace test suite is RED »), ce qui pousse à contourner le gate et rouvre le §3 ; et surtout, **sous `test_workspace = true` cette suite tourne pour chaque mutant** — un flake de contention pendant un run de mutant se lit alors comme un `caught`. Même classe de faux vert, autre porte.

**Fermeture :** plafond des deux côtés — `-- --test-threads=4` sur le gate, et `additional_cargo_test_args = ["--", "--test-threads=4"]` dans `.cargo/mutants.toml`. Vérifié dans l'argv écrit par cargo-mutants : le plafond est présent sur la baseline **et** sur chaque mutant, en `--workspace` comme en `--in-diff`.

`cargo mutants ... -- --test-threads=4` **ne fonctionne pas** (casse la baseline, exit 4) : `--test-threads` est un argument de harness, pas de cargo. La clé de config est la seule voie propre — et elle a le mérite de s'appliquer aussi à l'invocation directe de `mutation_gate.py`.

## Limite assumée — le gate Python contourne la recette

`mutation_gate.py` invoque `cargo mutants` **directement** et ne passe jamais par `scripts/mutation.sh`. Le garde-fou de suite rouge du §3 **ne le protège donc pas** : lancé sur une suite rouge, il rendrait `verdict: "pass"`.

Le plafond du §4 le couvre, lui, par ricochet — il vit dans `.cargo/mutants.toml`, que cargo-mutants lit quel que soit l'appelant. Mais l'écart de fond demeure et relève de l'outillage global, hors de ce dépôt. Consigné ici pour que la limite soit connue plutôt que redécouverte.

## Ce que cette ADR ne dit pas — désambiguïsation de [[ADR-0003]]

[[ADR-0003]] (« Pas de Stryker ») est une décision **produit** : CodeImpact remplace Stryker comme fonctionnalité en mesurant l'exécution réelle au lieu de générer des mutants. Elle ne dit rien de la façon dont la suite de tests de CodeImpact est elle-même vérifiée.

Les deux coexistent sans contradiction : le produit ne fait pas de mutation, l'outillage de test du dépôt en fait. Un lecteur pressé pourrait lire « pas de mutation testing » là où [[ADR-0003]] écrit « pas de mutation testing *comme fonctionnalité produit* ».

## Conséquences

- **(+)** Le gate de mutation est utilisable dans ce dépôt, et sur une suite réellement exécutée : preuve empirique d'un mutant `caught` par un test vivant dans un crate `[[test]]` externe, alors que le fichier muté n'a aucun test in-lib.
- **(+)** Trois classes de faux vert fermées, chacune avec sa reproduction et son test de non-régression (19 assertions dans `scripts/mutation_test.sh`).
- **(+)** La première campagne réelle a exposé deux vrais trous de test dans du code de production préexistant, suivis en **#115** — dont une asymétrie invisible à la lecture : la branche énergie a son test « exactement à la limite », la branche CO2 non.
- **(−)** Un `cargo test --workspace` complet est ajouté à chaque invocation de `scripts/mutation.sh` (~45 s ici). Coût assumé face au faux vert qu'il ferme.
- **(−)** La détection d'override `--timeout` est vérifiée exhaustive contre cargo-mutants **27.1.0**, pas contre les versions futures. C'est une constatation datée, pas une promesse permanente.
- **(−)** Aucun job CI (décision opérateur) : la recette est locale, et appelée par le gate.

## Non couvert

Le verrouillage inter-processus de cargo (`Blocking waiting for file lock on package cache`), observé en direct lors des vérifications, est un **phénomène distinct** de la contention du §4 : il oppose des invocations `cargo` concurrentes, là où `--test-threads` plafonne le parallélisme *interne* d'une seule invocation. Il persiste après le correctif, n'est ni causé ni traité par cette décision, et se manifeste quand plusieurs agents travaillent sur le même checkout.
