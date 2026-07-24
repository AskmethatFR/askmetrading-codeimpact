# ADR-0027 — `overflow-checks` + `debug-assertions` activés en release : le dépassement d'entier panique, il ne wrappe pas

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-24
> **Decided in:** Issue #51 / PR #108
> **Links:** [[architecture-overview]], [[ADR-0010]], [[ADR-0012]], [[ADR-0015]]

## Contexte

L'audit sécurité de #46/#49 avait établi que le workspace ne définissait aucun `[profile.release] overflow-checks` ni `debug-assertions`. En release, tout dépassement d'entier **wrappe silencieusement** et produit un nombre plausible mais faux — le mécanisme même que proscrit [[ADR-0010]] (« une marge, pas une différence de nature » ; le pire mode d'échec est le chiffre faux mais crédible). Symétriquement, un `debug_assert!` protégeant un invariant est **absent de l'artefact que les utilisateurs exécutent** (prouvé pendant l'audit : un `#[should_panic]` sur l'invariant échoue en `--release`).

#46/#49 avait rendu la lane proactive non-débordable **par construction** ([[ADR-0012]] révisé — `transitive` borné par la somme des complexités directes). Restait ouverte la question du reste de la base (agrégation projet, lane réactive, impacts éco/éco, futurs adaptateurs FFI) et un **débat** : un outil de reporting doit-il *paniquer* sur un dépassement, ou remonter un `Measurement::Unmeasurable` ?

## Décision

**Activer `overflow-checks = true` et `debug-assertions = true` dans `[profile.release]` à la racine du workspace** (portée décidée par l'humain : tout le workspace). Tout dépassement résiduel devient une **panique franche** plutôt qu'un chiffre mensonger, et les `debug_assert!` d'invariant sont présents dans l'artefact expédié.

**Pas d'activation à l'aveugle** : les sites arithmétiques nommés ont été audités site par site avant activation. Conclusion de l'audit : aucun ne franchit la barre « panique clairement hostile sur une entrée légitime plausible » —
- l'agrégation projet (`file_consumption_graph.rs`) utilise déjà `saturating_add` (#46/#49) ;
- les sommes `u64` d'impact économique sont bornées ~7 ordres de grandeur sous `u64::MAX` par les gardes `MAX_MEASURABLE_SOURCE_BYTES` (1 Mo) / `MAX_PROJECT_SOURCE_BYTES` (100 Mo, [[ADR-0006]]) ;
- lane réactive et stress-test opèrent à des échelles réalistes ;
- l'impact écologique est en `f64` (jamais concerné par `overflow-checks`) ;
- aucun adaptateur FFI n'existe encore (roadmap).

Là où un débordement peut *réellement* survenir dans ce périmètre, c'est déjà le signe d'une corruption/donnée aberrante — y paniquer fort est le comportement **honnête** voulu par [[ADR-0010]], pas un crash hostile. Le débat panic-vs-`Unmeasurable` est donc tranché **en faveur de la panique** pour ces lanes ; le vocabulaire `Measurement::Unmeasurable` reste réservé à la non-mesurabilité *externe* réelle de la lane réactive. Zéro `debug_assert!` vivant en production aujourd'hui → activer `debug-assertions` ne réveille aucune panique atteignable.

**Coût mesuré** : CPU ~0 (écart dans le bruit machine), taille binaire +12 %.

## Piège corrigé — `cfg!(debug_assertions)` n'est pas « suis-je en release »

Activer `debug-assertions = true` **aussi** en release rend `cfg!(debug_assertions)` **`true` dans les deux profils**. Le helper de test `ensure_bin_built` (`secondaries.integration_test/src/lib.rs`) détectait le profil via `!cfg!(debug_assertions)` — devenu **toujours `false`** → il construisait la sonde `codeimpact-parse-probe` dans `target/debug` et ne passait jamais `--release`, tandis que `discover_probe_path` la cherchait dans `target/release` ([[ADR-0015]]) → `AnalysisFailed("sonde d'analyse introuvable")` sous `cargo test --release`. Régression **invisible en CI** (la CI ne lance que le profil debug), attrapée par QA en rejouant le scénario `--release`.

**Correctif** : `is_release_exe_path()` lit le composant de chemin de `current_exe()` (`target/<profil>/deps/…`) — le **même signal** que `discover_probe_path` — au lieu de `cfg!(debug_assertions)`. **Leçon** : `cfg!(debug_assertions)` reflète un *réglage de profil*, jamais « dev vs release » ; toute détection de profil doit s'appuyer sur un signal que ce ticket ne réécrit pas lui-même.

## Conséquences

- **(+)** Un dépassement d'entier en release panique (échec franc) au lieu de produire un nombre faux — la garantie d'honnêteté d'[[ADR-0010]] s'étend à tout le workspace, pas seulement à la lane proactive fermée par construction.
- **(+)** Les `debug_assert!` d'invariant sont présents dans l'artefact expédié.
- **(−)** `debug-assertions` en release interagit avec toute détection de profil basée sur `cfg!(debug_assertions)` (piège ci-dessus) — désormais documenté.
- **(−)** Coût binaire +12 % ; CPU négligeable.
- **Dette ouverte** : les futurs adaptateurs FFI (.NET/Node.js/Java) devront ré-auditer leurs sites arithmétiques à leur arrivée — la décision « panique » vaut pour le code présent, pas pour une frontière FFI non encore écrite.
