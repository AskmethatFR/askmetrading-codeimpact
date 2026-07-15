# ADR-0010 — Honnêteté de la mesure : `Unmeasurable` plutôt que `0`

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-12
> **Decided in:** Issue #36 (slice S0 de l'étude #35 / US18)
> **Links:** [[architecture-overview]], [[ADR-0004]], [[ADR-0006]], [[economic-impact-estimator]]

## Contexte

La promesse de CodeImpact est : *« ce code te coûte de l'argent et du CO₂ ».*

Deux défauts vivants dans `main` la contredisaient :

1. Quand `/usr/bin/time` était absent, le runner renvoyait `(0, 0)` pour le CPU et la mémoire. L'outil affichait alors **« 0 CPU, 0 mémoire, level: low »** — il **imprimait « gratuit »** sur toute machine dépourvue du sondeur. Un zéro se lit « code propre ». C'est la pire sortie que ce produit puisse produire.
2. `/usr/bin/time cargo test` enveloppait **le build ET le run**. Le RSS pic était dominé par `rustc` en train de compiler, et le temps CPU incluait la compilation. **Le stress test chronométrait `rustc`** — tous les chiffres US6 publiés jusqu'ici étaient faux.

## Décision

### 1. `0` cesse d'être représentable comme « non mesuré »

Le primitif de mesure devient un type somme dans l'hexagone (zéro dépendance, ADR-0001) :

```rust
pub enum Measurement<T> { Available(T), Unmeasurable(UnmeasurableReason) }
pub enum UnmeasurableReason { NoSampler }
```

`StressTestRun.cpu_time_ms` / `memory_kb` passent de `u64` à `Measurement<u64>`.

**Ce n'est pas un raffinement de style, c'est le cœur de l'ADR.** `Option<u64>` + un champ `reason` séparé aurait laissé constructible l'état illégal *(valeur = 0, censée signifier « manquant »)*. Le type somme le rend **structurellement impossible**, et le `match` exhaustif **force chaque consommateur à décider explicitement** quoi faire de l'absence — au lieu de pouvoir l'oublier.

`ReactiveAnalyzer::analyze` renvoie désormais `Measurement<EconomicImpact>` : si le CPU **ou** la mémoire est `Unmeasurable`, tout le calcul économique l'est aussi. **Pas de coût fantôme à 0 $ calculé sur une entrée manquante.**

La règle de l'operator — *« il n'existe aucun `f64` qui puisse valoir `0.0` par défaut »* — devient **structurelle** plutôt qu'aspirationnelle.

> **Un `0` mesuré reste légitime.** `Available(0)` (un test trivial consomme réellement 0 ms de CPU arrondi) s'affiche `0 ms` et c'est honnête. Seul `Unmeasurable` s'affiche `n/a` + sa raison. La distinction est exactement le sujet de cet ADR.

### 2. Le build sort de la fenêtre de mesure

Deux commandes distinctes :
- **build** — `cargo test --no-run --message-format=json`, **non mesuré** ;
- **mesure** — exécution **directe du binaire de test compilé**, seule chronométrée.

`parse_cpu_time` somme désormais **`user` + `sys`** (le temps noyau était invisible, dans un outil dont une des features est justement la détection d'I/O).

### 3. Confinement du binaire exécuté (exigé par l'audit Security)

Le nouveau flux **parse un chemin dans la sortie de cargo, puis l'exécute**. `confine_to_target_dir` canonicalise **les deux côtés** et vérifie par `starts_with` que le binaire est un descendant du `target/` canonicalisé du projet. Sinon : erreur générique, **sans chemin dans le message** (ADR-0006).

Deux propriétés vérifiées empiriquement, pas seulement raisonnées :
- `Path::starts_with` compare des **composants**, pas un préfixe de chaîne → un voisin `target-evil/` ne peut pas passer ;
- un `.cargo/config.toml` hostile (`[build] target-dir = <hors projet>`) **échoue fermé** : `canonicalize` du `target/` inexistant échoue → erreur générique.

## RISQUE RÉSIDUEL ACCEPTÉ — TOCTOU entre la vérification et l'exécution

> L'écart *check-then-execute* entre la canonicalisation de `confine_to_target_dir` et le `Command::new(binary)` qui suit est une **véritable fenêtre TOCTOU** : un `build.rs` malveillant pourrait remplacer le fichier au chemin canonique validé, après la vérification et avant l'exécution.
>
> **Ce risque est accepté, pas fermé.** Gagner cette course exige un `build.rs` qui **s'exécute déjà avec tous les privilèges de l'utilisateur** pendant `cargo test --no-run` — autrement dit, l'attaquant dispose *déjà* de l'exécution de code arbitraire, prix de base de l'analyse d'une source Rust non fiable avec CodeImpact. Échanger le binaire par TOCTOU ne lui accorde **aucune capacité supplémentaire**.
>
> **Aucune remédiation n'est requise tant que le modèle de menace n'évolue pas.** Si CodeImpact acquiert un jour un mode « analyser un dépôt non fiable **sans exécuter ses build scripts** » (bac à sable), cette fenêtre devrait alors être fermée par un épinglage sur descripteur de fichier (`open`-puis-`exec`-par-fd) plutôt que par une re-vérification de chemin.

## Ce que ce cycle a appris sur les tests — et qui a coûté trois retries

Le test de non-régression du défaut n°2 (« le build n'est plus dans la mesure ») a été **rejeté trois fois par QA**, et c'est l'enseignement le plus transférable du cycle :

1. **Borne absolue** (`duration_ms < 2000`) → **flaky**. Réfutée non par un raisonnement mais par une exécution sous charge : `14674 ms`, `8067 ms`.
2. **Ratio froid/chaud** (`cold/warm < 30`) → **aveugle**. Le run « chaud » shelle quand même `cargo test --no-run`, donc les deux termes sont dominés par le **coût fixe de démarrage des process**, pas par la compilation. Numérateur et dénominateur écrasés par la même constante : **aucune valeur de borne ne pouvait le sauver.** Pire que le flake — le flake échouait bruyamment, celui-ci passait en silence.
3. **Plancher de `sleep`** → **déterministe**. Le `build.rs` de la crate de fixture dort **20 s**. Un sleep est **immunisé contre la contention** : la charge ne peut que le rendre *plus long*, jamais plus court. Le code muté (build replié dans la mesure) est donc borné **par en dessous** à 20 s ; le code correct reste à quelques secondes. Le seuil (8 s) tombe dans un intervalle qu'aucun des deux camps ne peut traverser.

> **La leçon :** séparer les deux camps par une **différence de nature**, pas par une **marge**. Une marge se fait toujours franchir par la contention ; un plancher de `sleep`, jamais. Et : ne jamais *raisonner* sur la robustesse d'un test de timing — **le muter et l'exécuter sous charge**. Deux relecteurs ont validé par raisonnement une borne que QA a démolie en l'exécutant.

**Coût assumé :** la suite passe de 2,4 s à **24,9 s**, et la CI exécute les tests dans les jobs `test` **et** `coverage` (~+45 s sur une base de 1m38). Arbitrage : **sur une branche protégée, un test lent qui mord bat un test rapide et aveugle.** La constante de 20 s ne sera pas rognée — « optimiser le seuil » est exactement le jeu qui a échoué deux fois. Optimisation éventuelle suivie en #41.

**Résolu par #41 :** le test est marqué `#[ignore]` et exécuté par un job CI dédié `slow-tests` sur **chaque** PR (`cargo test ... -- --ignored --exact`). La boucle locale `cargo test --workspace` et les jobs `test`/`coverage` ne paient plus les +20 s ; la protection reste effective sur chaque PR (la mutation de référence — replier le build dans `duration_ms` — rougit toujours le job). La constante de 20 s n'a **pas** été touchée : le test a été *déplacé*, pas *optimisé*.

## Conséquences

- **(+)** L'outil ne peut plus imprimer « gratuit » quand il n'a pas su mesurer. La garantie est portée par le **type**, pas par la discipline.
- **(+)** Les chiffres du stress test mesurent enfin le code de l'utilisateur, pas `rustc`.
- **(+)** Le binaire exécuté est confiné au `target/` du projet ; un `.cargo/config.toml` hostile échoue fermé.
- **(−→résolu #41)** CI et boucle locale ne paient plus les +20 s : le test lent vit dans le job CI dédié `slow-tests` (exécuté sur chaque PR), hors de la boucle locale et des jobs `test`/`coverage`.
- **(−)** `Measurement<T>` se propage dans tous les `ReportWriter` — coût de migration payé une fois.

## Dette connue, explicitement non traitée

- **Les deux échelles de seuils divergent toujours d'un facteur 100** — `economic_impact.rs` (10/20/40 μ$) vs `reactive_analyzer.rs` (1000/10000/100000 μ$) construisent le **même** champ `level`. `impl Add` permet toujours d'additionner une **estimation** et une **mesure** : un non-sens arithmétique qui compile. **Épinglé par un test de caractérisation** (`known_defect_static_and_measured_level_scales_diverge_36_bug3`), volontairement **non corrigé** — la refonte est la slice **S3** de l'étude #35, qui unifie les deux vues sous un primitif physique unique (`CostPerInvocation`).
- **Résolu (#60) :** même famille de dette, un cran plus loin. `serialize_project_metrics` appliquait `complexity_level_for` (l'échelle **par fichier**, plafonds 10/20/40) au **total projet** — un nombre qui n'est sur cette échelle que par accident, et qui lit « critical » pour quasi tout projet non trivial (571 sur ce dépôt). Le champ `complexity_level` du JSON projet lit désormais la **MÉDIANE** des complexités par fichier (`ProjectMetrics::median_file_cyclomatic_complexity` / `complexity_level()`) : la médiane EST une valeur d'un seul fichier, donc rester sur l'échelle que `complexity_level_for` a été calibrée pour est légitime — le total ne l'est pas. Sur ce dépôt : médiane 2 → « low » (au lieu du « critical » fabriqué). Le champ signifie désormais *« niveau du fichier médian (typique) »*, pas *« niveau du projet entier »* — projet vide (zéro fichier mesuré) → `"none"`, jamais le « low » trompeur que `complexity_level_for(0)` donnerait. `complexity_level_for` elle-même reste inchangée.
- **#39 (P0)** — sur un workspace multi-crates, `parse_test_binary_path` retient le **dernier** artefact de test, ici un binaire e2e à **zéro test** → `Tests: 0/0 passés` accompagné d'un rapport économique confiant. L'outil ne ment plus par `0`, mais il mesure **le mauvais sujet**. Découvert en dogfoodant sur ce dépôt même.
- **#40 (P1)** — buffering non borné de la sortie des sous-processus ; invariant de confinement porté par l'appelant plutôt que par la fonction qui exécute.
- **ADR-0004** affirmait qu'un `ProfilerPort` existait dans l'hexagone. **Il n'existe pas** (`grep` → zéro résultat). L'ADR est amendé. `docs/architecture.md`, `docs/glossary.md` et `docs/technical/economic-impact.md` répètent encore cette affirmation fausse — à corriger.
