# ADR-0009 — CI GitHub Actions & posture supply-chain d'un dépôt public

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-12
> **Decided in:** Issue #29 / PR #37
> **Links:** [[architecture-overview]], [[ADR-0006]]

## Contexte

Le dépôt `AskmethatFR/askmetrading-codeimpact` est **public** et s'ouvre aux contributions externes. Avant ce cycle il n'avait **ni CI, ni protection de branche**. `main` n'avait jamais été confronté à `cargo fmt --check` ni à `cargo clippy -D warnings` : la dette de conformité était invisible faute de porte pour la révéler.

Une CI sur un dépôt public n'est pas seulement un outil de qualité : c'est **une surface d'exécution que n'importe quel inconnu peut déclencher via une pull request depuis un fork**. Le modèle de menace n'est donc pas « un contributeur maladroit » mais « un contributeur hostile ».

## Décision

### 1. Le workflow

`.github/workflows/ci.yml` — quatre jobs, sur `push` (toute branche) et `pull_request` vers `main` :

| Job | Commande | Rôle |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | dérive de formatage |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | lints, en erreur |
| `test` | `cargo test --workspace` | 367 tests, 23 suites |
| `coverage` | `cargo-llvm-cov` → lcov + HTML en artefact | couverture |

**Aucun affaiblissement des portes n'est admis pour les faire passer** : pas de `continue-on-error`, pas d'abandon de `-D warnings`, pas de `#[allow]` de complaisance. La dette pré-existante se corrige, elle ne se contourne pas.

### 2. Posture supply-chain (non négociable sur un dépôt public)

- **Toute action tierce est épinglée à un SHA de commit complet**, jamais à un tag. Un tag est mutable : un mainteneur compromis peut le repointer. Un SHA, non. Le commentaire de version en fin de ligne est une commodité de lecture, pas la référence.
- **`permissions:` déclaré au niveau du workflow, au moindre privilège** (`contents: read`). Aucun job n'a besoin d'écriture.
- **`pull_request_target` est interdit.** C'est le vecteur classique de RCE sur dépôt public : il exécute le workflow de la branche *de base* avec un token privilégié, dans le contexte du code de la PR.
- **Aucun `${{ secrets.* }}`**, et **aucune interpolation de `${{ github.event.* }}` dans un bloc `run:`** — titre de PR, nom de branche et corps de message sont des chaînes contrôlées par l'attaquant.
- **Les outils installés depuis crates.io sont épinglés à une version explicite.** `cargo install --locked` n'épingle **que** le `Cargo.lock` de l'outil, **pas la version installée** : sans `--version`, chaque run récupère et exécute le dernier paquet publié.

### 3. Réglages du dépôt (hors dépôt versionné — consignés ici car ce sont des décisions)

| Réglage | Valeur | Raison |
|---|---|---|
| Protection de `main` | PR obligatoire, 4 checks verts requis, `strict` (à jour avant merge) | pas de push direct |
| `enforce_admins` | **`true`** | le mainteneur est soumis à ses propres règles. Coût assumé : plus d'échappatoire en urgence |
| Review approuvée requise | **non** | mainteneur solo — il ne peut pas approuver sa propre PR sans se bloquer |
| Force-push / suppression de `main` | interdits | — |
| Approbation CI des PR de fork | **`all_external_contributors`** | **le levier le plus important du lot** (voir ci-dessous) |
| `sha_pinning_required` | **`true`** | empêche une future PR de revenir à un tag mutable |

## Le point le plus important

Le réglage par défaut de GitHub (`first_time_contributors`) n'exige une approbation humaine que pour la **première** PR d'un compte. Ensuite, ses PR déclenchent la CI sans validation.

Or le déclencheur `pull_request` exécute **le workflow tel qu'il est défini dans la branche de la PR**. Un compte devenu « de confiance » après une PR anodine peut donc **modifier `ci.yml` lui-même** et le faire exécuter sur le runner, sans qu'un humain ne regarde.

`all_external_contributors` place une **porte humaine devant chaque exécution de CI issue d'un fork**. Sur un dépôt à mainteneur solo qui a renoncé à la review obligatoire, **c'est le contrôle compensatoire qui rend ce renoncement acceptable** : le mainteneur est forcé de lire le code *avant* qu'il ne s'exécute.

## Conséquences

- **(+)** Le dépôt peut accepter des contributions externes sans exposer son runner à du code non relu.
- **(+)** La dette `fmt`/`clippy` de `main` est purgée, et ne peut plus se reconstituer silencieusement.
- **(+)** Une compromission en amont d'une action ou de `cargo-llvm-cov` ne peut plus atterrir dans la CI sans un changement de SHA/version visible en revue.
- **(−)** `enforce_admins: true` supprime le bypass du mainteneur : même un hotfix passe par une PR et attend la CI (~1m40s).
- **(−)** `all_external_contributors` impose une action manuelle du mainteneur à chaque PR externe. C'est le prix du contrôle, et il est voulu.
- **(−)** Le job `coverage` recompile `cargo-llvm-cov` à chaque run (non caché). Si le temps devient gênant, passer à un installeur binaire épinglé (`taiki-e/install-action`) — sans jamais perdre l'épinglage.

## Dette connue (non traitée par ce cycle)

`.gitignore` ne couvre que `target/` et `Cargo.lock`. `.DS_Store`, `report.html` et `reports/` ne sont pas ignorés — le premier contributeur macOS commitera un `.DS_Store`. À corriger avant d'annoncer l'ouverture du dépôt.

## Addendum #86 (PR #99) — porte `cargo-deny` : advisories RUSTSEC en CI

La posture supply-chain de la §2 imposait l'épinglage SHA/version des actions et outils, mais **rien n'automatisait la garantie « aucune CVE connue » sur les dépendances elles-mêmes** — elle reposait sur la mémoire du relecteur. #86 ferme ce trou.

- **`deny.toml`** (racine) — sections `[advisories]` / `[licenses]` / `[bans]` / `[sources]`. Les advisories RUSTSEC échouent le build (fail-closed, `ignore = []`) ; vérifié empiriquement en injectant `time =0.1.42` (RUSTSEC-2020-0071) → `advisories FAILED`, exit 1.
- **Job CI `cargo-deny`** (`.github/workflows/ci.yml`, après `clippy`) — action `EmbarkStudios/cargo-deny-action` épinglée au **SHA de commit** de `v2.1.1` (§2 respectée), `arguments: ""` pour ne pas forcer `--all-features` (cohérent avec `[graph] all-features = false`). C'est le **5ᵉ check requis** de la protection de branche, aux côtés de `fmt`/`clippy`/`test`/`coverage`/`slow-tests`.
- **Effet de bord assumé** : `cargo deny check licenses` a révélé qu'**aucun crate du workspace ne déclarait de `license`** alors que le dépôt porte un `LICENSE` Apache-2.0. Corrigé en déclarant `license = "Apache-2.0"` au niveau `[workspace.package]` + `license.workspace = true` sur les 6 membres — la licence était déjà tranchée par le fichier `LICENSE`, aucune décision nouvelle. Choix de **corriger, pas masquer** (`[licenses.private] ignore = false`), conformément à l'interdiction de supprimer un finding réel.

La dette « recompilation de `cargo-llvm-cov` à chaque run » (§Conséquences) reste ouverte ; `cargo-deny` est installé via l'action épinglée, pas recompilé.

## Note de numérotation

Ce cycle occupe **ADR-0009**. Les études #30 (multi-langage) et #35 (modèle d'impact unifié) proposent chacune des ADR encore non écrits : ils prendront **ADR-0010 et suivants** au moment de leur phase Documentation. Les numéros cités dans le corps de ces deux études sont des brouillons et doivent être réalloués.
