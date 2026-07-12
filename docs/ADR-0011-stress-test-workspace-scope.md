# ADR-0011 — Stress test : portée workspace, agrégation des binaires, et le 0-test honnête

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-12
> **Decided in:** Issue #39
> **Links:** [[architecture-overview]], [[ADR-0010]], [[ADR-0006]], [[ADR-0009]]

## Contexte

Sur ce dépôt même — un workspace multi-crates, comme tout projet Rust sérieux — `codeimpact stress-test` affichait :

```
Tests: 0/0 passés
```

…accompagné d'un **rapport économique complet et parfaitement assuré**.

[[ADR-0010]] avait fait cesser le mensonge « je n'ai pas su mesurer, donc c'est gratuit ». Celui-ci est sa maladie voisine : les chiffres ne sont pas faux au sens arithmétique, ils sont **mesurés sur un sujet qui n'est pas celui que l'utilisateur croit**. Un rapport confiant sur un binaire vide est aussi trompeur qu'un zéro.

**La cause racine n'était pas celle que le ticket annonçait.** Le parser retenait bien le *dernier* `compiler-artifact` du build, mais le défaut structurant était en amont : `build_cmd` passait `--lib`, qui ne compile que les cibles **lib**. Or tous les tests réels de ce dépôt vivent dans des cibles d'**intégration** (`tests/*.rs`). Corriger le seul parser aurait laissé un compte quasi nul. **Les deux devaient changer.**

## Décision

### 1. Portée workspace + agrégation

`build_cmd` passe `--workspace` (et non plus `--lib`). Le parser collecte **tous** les exécutables de test, pas le dernier. Chacun est mesuré individuellement, puis les N `StressTestRun` sont pliés par une loi de domaine, `StressTestRun::aggregate` :

| Champ | Loi | Pourquoi |
|---|---|---|
| `duration_ms` | **somme** | les binaires tournent en séquence — c'est le temps réellement dépensé |
| `tests_passed` / `tests_total` | **somme** | |
| `cpu_time_ms` | **somme**, mais `Unmeasurable` si **un seul** run l'est | une somme partielle rendue comme un total, c'est le mensonge d'[[ADR-0010]] en costume neuf |
| `memory_kb` | **max**, `Unmeasurable` si **un seul** run l'est | un pic RSS de processus qui **ne coexistent jamais** est un maximum, jamais une somme — sommer inventerait de la mémoire qui n'a jamais été résidente simultanément |
| liste vide | `Err` | on ne synthétise pas un run tout-à-zéro : rien construit ⇒ rien à rapporter |

**Alternatives rejetées.** *Sélectionner un binaire et exiger `--package` quand N > 1* : le critère d'acceptation exigeait la cohérence avec `cargo test --workspace` ; la sélection ne peut pas la tenir, et elle rendrait un flag obligatoire dans le cas nominal (un workspace). *Échouer bruyamment quand N > 1* : l'outil refuserait de tourner sur son propre dépôt — « explicite » n'oblige pas à abdiquer. *Ajouter `--package` pour restreindre* : YAGNI, `--filter` restreint déjà.

### 2. Le 0-test cesse de produire un rapport confiant

Nouveau `UnmeasurableReason::NoTestsExecuted`. `ReactiveAnalyzer::analyze` garde `tests_total == 0` **en premier**, avant tout calcul : un run qui n'a rien exercé n'a aucun coût honnête à rapporter, **si bien échantillonné que soit son processus**.

L'invariant vit **dans l'hexagone**, pas dans le renderer. Conséquence vérifiable : aucun writer (console, JSON, HTML) n'a eu **une seule ligne** à changer — ils héritent de la règle par le `match` exhaustif. C'est la preuve que l'invariant est au bon étage. Placé dans le renderer, chaque adaptateur l'aurait ré-implémenté, et chacun aurait pu l'oublier.

> **Ce n'est pas un invariant de construction.** « 0 test a tourné » reste un fait vrai et rapportable. Ce qui ne doit jamais exister, c'est le **rapport économique confiant** qui en dérive. La précondition appartient à `analyze`, pas à `StressTestRun::new`.

### 3. Un binaire qui a crashé n'est pas un binaire à 0 test

Un membre du workspace dont le binaire de test meurt (SIGSEGV, `abort()`, panique du harness) émettait `(0, 0)` — **indistinguable** d'« aucun test » — et ce zéro se noyait dans la somme des autres. `tests_total` restait gros, le garde du §2 ne se déclenchait pas, CPU et mémoire restaient `Available` (le processus *a* tourné et *a* été échantillonné — il est juste mort), et le rapport ressortait confiant. **#39 régénéré par son propre correctif** : avant `--workspace`, un crash faisait tomber le total visible à zéro, signal évident ; désormais il se diluait.

**Le discriminant est la ligne de résumé `test result:`, pas le code de sortie.** Un binaire libtest qui va au bout l'imprime *toujours* — succès, échec, ou zéro test. Un binaire qui meurt en cours n'en imprime aucune.

> **Le code de sortie serait un piège.** Un binaire dont des tests **échouent** sort aussi en non-zéro : c'est son chemin **nominal**. S'en servir comme discriminant ferait échouer `stress-test` sur tout projet ayant un seul test rouge — or on mesure des stress tests, les tests rouges doivent rester mesurables. Un test dédié épingle ce garde-fou pour empêcher un futur « correctif » par `status.success()`.

Absence de ligne de résumé ⇒ `Err`, tout le run échoue (fail-closed, comme `confine_all`). Pas de sauvetage d'un agrégat partiel.

## Conséquences — sécurité

### Surface d'exécution élargie : le modèle de menace

Avant : **un** binaire de test lib était exécuté. Après : **tous** les binaires de test de tous les membres du workspace.

**Aucune nouvelle frontière de confiance n'est franchie.** `cargo test --no-run` exécutait déjà les `build.rs` du dépôt analysé — du code arbitraire, sous l'identité de l'utilisateur. Exécuter ensuite les binaires de test compilés depuis ce même dépôt ne demande de faire confiance à rien de plus que ce que le *compiler* exigeait déjà. `--workspace` ne tire d'ailleurs que les membres que l'auteur du dépôt a lui-même déclarés dans `[workspace] members` — pas des dépendances git/registry. La frontière est, et reste : **« le projet analysé est assez fiable pour être compilé »**.

Ce qui **change**, en revanche, c'est le **rayon de souffle en temps et en ressources** — et c'est de là que viennent les contrôles ci-dessous. « Pas de nouvelle frontière » n'est pas « pas de nouveau risque ». Les contrôles suivants protègent **l'hôte qui exécute codeimpact** (disponibilité, hygiène), ils ne sont pas des barrières de confidentialité contre le dépôt analysé.

### Confinement de N binaires

`confine_all` applique `confine_to_target_dir` ([[ADR-0006]] : canonicalize-then-confine) à **chaque** candidat, et **échoue en bloc au premier rejet** — il n'écarte pas le candidat hostile pour exécuter les autres. Idiome : `.map(...).collect::<Result<Vec<_>, _>>()`, qui court-circuite. Un `.filter_map(|c| confine(c).ok())` aurait été un **bypass silencieux**.

L'ordre est **confine-tout-puis-exécute-tout**, jamais entrelacé : sinon le binaire 1 s'exécuterait avant qu'on découvre que le binaire 2 est hostile.

**Compromis TOCTOU assumé.** Ce choix allonge la fenêtre entre la vérification et l'exécution du dernier binaire (les précédents ont déjà tourné entre-temps, donc du code arbitraire du dépôt a pu s'exécuter). Ce n'est **pas** une escalade : un attaquant capable de réécrire le binaire N a déjà l'exécution de code arbitraire via le binaire en cours — strictement plus puissant. Le compromis est accepté, et il est écrit ici pour qu'aucun relecteur futur n'ait à le re-déduire de la source.

### Fenêtre de mesure : drain, kill, bornes

Trois défauts de la machinerie timeout/drain, **latents** et réveillés par `--workspace`, ont dû être corrigés dans le même cycle :

1. **Deadlock de pipe.** On attendait la sortie du processus avant de drainer stdout/stderr. Le tampon de pipe OS (~64 Ko) saturait sur les **70 Ko de JSON** du build workspace : l'enfant bloquait en écriture, ne sortait jamais, on attendait 300 s puis on échouait. `--lib` le masquait (sortie minuscule). Les pipes sont désormais drainés **concurremment** par des threads dédiés.
2. **Buffer non borné.** `MAX_CHILD_OUTPUT_BYTES = 64 Mio`. Au dépassement : **`Err`, jamais un `Ok` silencieusement tronqué** — un flux JSON tronqué alimenterait le parser et produirait une liste de binaires **silencieusement incomplète** : exactement la classe de mensonge que ce ticket tue.
3. **Kill de groupe, et borne sur le drain.** L'enfant est placé dans son propre groupe (`process_group(0)`, donc pgid = son pid), ce qui rend `libc::kill(-pgid, SIGKILL)` sûr : il ne peut viser que ce groupe, jamais celui de codeimpact ni du shell. Le drain est borné (`DRAIN_JOIN_TIMEOUT = 5 s` par pipe, via `mpsc::recv_timeout` — `JoinHandle::join()` n'offre aucune borne).

> **Pourquoi la borne sur le drain est indispensable.** Un petit-fils qui appelle `setsid()` **s'échappe du groupe** et survit au kill. S'il a hérité du pipe et le garde ouvert, l'EOF n'arrive jamais : sans borne, `join()` bloque **pour toujours** et tout le process codeimpact pend — le budget de 300 s est silencieusement annulé. Aucune malveillance requise : une suite d'intégration qui lance un serveur détaché sans rediriger sa sortie suffit.

**Risque résiduel accepté** : le petit-fils échappé fuit en processus orphelin (réabsorbé par init/PID 1), et son thread lecteur est abandonné, bloqué, jusqu'à la fin du process. On rapporte alors une erreur honnête. **Un thread fuité dans un process borné est un compromis acceptable ; un outil d'analyse qui ne rend jamais la main ne l'est pas.** Fermer complètement la brèche demanderait un confinement OS (cgroups / job objects) — hors périmètre.

### FFI : `libc` plutôt qu'un binding fait maison

Le kill de groupe passe par `libc::kill` / `libc::pid_t` / `libc::SIGKILL`, **pas** par un `unsafe extern "C"` écrit à la main. Un binding maison est correct le jour où on l'écrit, puis devient un passif : chaque édition future (ordre des arguments, constante de signal, dérive d'ABI sur une nouvelle cible) échappe au compilateur et n'est re-vérifiée que par une relecture humaine. Les bindings de `libc` sont générés et testés contre les en-têtes réels.

La règle **zéro-dépendance ne contraint que l'hexagone** ([[ADR-0001]], [[ADR-0005]]) — `secondaries` dépend déjà de `serde_json` et `tempfile`. `libc` est maintenu par rust-lang, sans dépendance transitive, et était déjà présent dans `Cargo.lock` : le coût supply-chain marginal ([[ADR-0009]]) est nul.

## Hors périmètre

- Un flag `--package` / `-p` de sélection (`--filter` restreint déjà — YAGNI).
- L'exécution **parallèle** des N binaires (elle invaliderait les lois d'agrégation : le pic mémoire ne serait plus un max de processus disjoints).
- Un budget de temps **global** sur les N binaires (le timeout de 300 s reste **par binaire**).
- Un **détail par binaire** dans le rapport (l'agrégat est un seul `StressTestRun` ; montrer le coût par crate est une vraie fonctionnalité, donc un autre ticket).
- Le confinement OS (cgroups / job objects) contre l'échappement `setsid()`.
