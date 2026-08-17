# ADR-0033 — `--strict` ne sort plus 0 sur ce qu'il n'a pas mesuré — un code de sortie **4** dédié, `GateCoverage` porté par `GatedOutput`, et quatre gardes de même forme dont une reste ouverte

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-08-17
> **Decided in:** Issue #128, PR #150 sur `feat/128-strict-never-passes-on-unmeasured`
> **Links:** [[architecture-overview]], [[alert-thresholds]], [[ADR-0017]], [[ADR-0010]], [[ADR-0032]], [[ADR-0006]], [[ADR-0019]], [[ADR-0009]], [[json-report-schema]], [[html-report]], [[console-report-enriched]], [[glossary]]

> Chemins abrégés dans tout ce document : préfixe `src/contexts/codeimpact/`.

## Contexte

`--strict` est un **gate CI** ([[ADR-0017]] §4) : il existe pour faire échouer un build. Il sortait **0** sur des projets dont une partie du code n'avait **jamais été mesurée**.

Security l'a démontré sur binaires release, pas en théorie : avec `--max-kwh 0.0009 --strict` contre une consommation réelle de **0,0013 kWh**, il suffit de gonfler **un** fichier au-delà de la garde de taille pour le faire tomber hors de la somme gatée — l'énergie affichée redescend à **0,0006 kWh** et l'outil sort **0**. Le dépassement est réel, le gate le rate.

La cause n'est **aucun bug** : ce sont **deux décisions individuellement correctes** qui se composent mal.

1. `AlertThresholds::evaluate` (`hexagon/src/analysis/alert_thresholds.rs:95-112`) applique délibérément *« une métrique absente ne franchit jamais un seuil : l'absence n'est pas un zéro confiant »* ([[ADR-0010]], repris en [[ADR-0017]] §2). Correct, et non négociable.
2. `gated_exit_code` (`primaries/src/main.rs`) n'observait qu'une seule chose : `report.has_breach()`. Correct aussi — la décision appartient au domaine, le mapping ne re-dérive rien.

Composées, elles rendent **le même exit 0** pour *« rien n'a dépassé »* et pour *« rien de ce que j'ai réussi à mesurer n'a dépassé »*. Le point important pour la suite : **le rapport, lui, était déjà honnête** — `unmeasurable_files` / `unmeasurable_files_count` nommaient les fichiers perdus. C'est **le code de sortie seul** qui perdait l'information, sur le seul canal qu'une CI lit.

## L'option retenue, et les trois écartées

**Retenue (approuvée en GATE 1.5) : un code de sortie dédié, `4`, et non une réutilisation de `3`.**

- **Un flag `--fail-on-unmeasurable` seul — écarté.** Le défaut resterait ouvert *par défaut* ; ne serait protégé que celui qui lit la documentation. Un gate qui ment tant qu'on ne l'a pas configuré n'est pas un gate.
- **Statu quo documenté — écarté.** Il laisse la porte malhonnête et se contente de l'avouer ailleurs que là où elle est lue.
- **Rabattre le cas sur `3` — écarté, et c'est l'écart le plus instructif.** Cela aurait satisfait **la lettre** du critère d'acceptation (via le message affiché) tout en **perdant l'information sur le seul canal qu'une CI lit** : le code de sortie. C'est-à-dire reproduire le défaut même du ticket, un étage plus bas.

## Décision AD-1 — le contrat de codes de sortie gagne un **4**, `3` reste inchangé et l'emporte

| Exit code | Signification |
|---|---|
| 0 | Rien n'a dépassé **et** tout était couvert |
| 1 | Erreur d'entrée / runtime (inchangé) |
| 2 | Réservé clap (inchangé) |
| 3 | Dépassement en `--strict` (**inchangé**, et **l'emporte sur 4**) |
| **4** | **Nouveau** — `--strict`, rien de **mesuré** n'a dépassé, mais le gate n'a **pas pu s'appliquer en totalité** |

Le `4` se déclenche **si et seulement si** : `strict` **∧** au moins un seuil configuré **∧** couverture ≠ `Complete` **∧** aucun dépassement. Hors `--strict`, l'exit reste **0** (US8 AC3, inchangé). Un seul message est imprimé — celui du code gagnant, **jamais les deux** (Q3, arbitrage humain).

```rust
if report.has_breach() { eprintln!(...); return 3; }
if !coverage.is_complete() { eprintln!(...); return 4; }
0
```

`primaries/src/main.rs:351-378`. Le mapping ne re-dérive **aucune** comparaison : il lit `has_breach()` et `is_complete()`, tous deux décidés dans le domaine — la discipline d'[[ADR-0017]] §4 est conservée telle quelle, elle gagne une seconde entrée.

**Le changement de contrat est assumé.** Un dépôt hébergeant un gros fichier généré verra la couleur de sa CI changer. C'est exactement ce que `--strict` promettait déjà : *échoue si le seuil ne tient pas*. Sortir 0 parce qu'on n'a pas su mesurer n'a jamais fait partie de la promesse.

## Décision AD-2 — deux faits, deux porteurs ; `evaluate` n'est pas touché

`ThresholdReport` répond à **« qu'est-ce qui a dépassé ? »**. `GateCoverage` répond à **« sur quoi ai-je pu décider ? »** (`hexagon/src/analysis/gate_coverage.rs:14-47`). Les fusionner obligerait `evaluate` à connaître les **échecs de lecture de fichier** — un concept d'adaptateur que la porte de domaine pure n'a aucune raison d'atteindre.

Conséquence vérifiée à chacune des quatre passes : **`alert_thresholds.rs` a un diff nul** (`git diff main...HEAD -- '*alert_thresholds.rs'` → vide). Le VO le plus délicat du gate n'a pas bougé d'un octet ; toute la tranche vit **autour** de lui.

Le porteur est `GatedOutput<T>` (`hexagon/src/analysis/gated_output.rs`), dont la raison d'être documentée est précisément *« ce dont la CLI a besoin pour décider du code de sortie »* — la couverture y appartient donc de plein droit, sans nouveau canal.

**`GatedOutput::new` prend la couverture en troisième argument positionnel obligatoire, délibérément pas un builder `with_coverage`** (`gated_output.rs:12-32`) — en rupture assumée avec l'idiome builder de ce dépôt. Un builder rend le fait **oubliable**, et un site d'appel qui l'oublie retombe **silencieusement** sur `Complete` : c'est-à-dire exactement sur le mensonge que ce ticket corrige. Ici le compilateur force chaque site d'appel à prendre position.

**Effet de bord digne d'être consigné : cette forme a rendu 34 mutants `unviable`.** La conception a **supprimé la classe de bug** au lieu de tester autour d'elle — le mutant « oublie la couverture » ne compile plus.

La dérivation reste dans le domaine, en un point unique par use case : `RunAnalysis::derive_gate_coverage` (`hexagon/src/analysis/run_analysis.rs:306-321`) et son jumeau `RunStressTest::derive_gate_coverage` (`run_stress_test.rs:35`). Sans seuil configuré, la couverture est `Complete` quel que soit le nombre de fichiers perdus : un projet non gaté n'a rien que le gate aurait pu manquer.

## Décision AD-3 — quatre gardes de **même forme**, trouvées l'une après l'autre

Toutes les quatre ont la même forme : **l'adaptateur laisse tomber un fichier et le domaine ne l'apprend jamais.**

| # | Garde | Emplacement | État |
|---|---|---|---|
| 1 | `MAX_MEASURABLE_SOURCE_BYTES` (1 Mio) | domaine — `hexagon/src/analysis/source_guard.rs:9` | **fermée** (passe initiale) |
| 2 | `MAX_FILE_SIZE` (10 Mio) | adaptateur, au parcours — `secondaries/.../file_system_code_reader.rs:13` | **fermée** (retry 1) via `SourceFileListing.dropped_files` (`file_system_code_reader.rs:332`) |
| 3 | `MAX_WALK_DEPTH` (128) | adaptateur — `file_system_code_reader.rs:14`, détection `:402` | **fermée** (retry 2) via `unexplored_subtree` |
| 4 | `hidden(true)` | adaptateur — `file_system_code_reader.rs:346` | **OUVERTE — #147** |

Deux choses à retenir, et c'est la **forme** qui est la leçon, pas la liste :

- **Aucune des trois dernières n'a été trouvée en lisant le diff.** Chacune l'a été par **sondage adverse** sur binaire release — construire l'arborescence hostile et regarder le code de sortie. Une garde d'adaptateur est invisible à la relecture d'un diff qui ne la touche pas.
- **Règle générale qui en sort : tout abandon de fichier au niveau adaptateur doit atteindre le domaine.** Avant #128, chaque abandon ne faisait qu'un `eprintln!` ; chaque `push` dans `dropped_files` est aujourd'hui apparié 1:1 avec le `eprintln!` voisin — jamais une *nouvelle* raison d'abandon, seulement une raison **nommée**.

## Décision AD-4 — une absence **non dénombrable**, nommée sans inventer sa taille

La troncature en profondeur n'émet **ni `Ok` ni `Err`** : `WalkBuilder::max_depth` livre la dernière entrée de répertoire dans laquelle il descendra, puis s'arrête — en silence. **Aucun mécanisme indexé par chemin ne pouvait la voir.** Et personne ne peut dire combien de fichiers vivent dans un sous-arbre jamais parcouru : les **compter** serait exactement la fabrication qu'[[ADR-0010]] interdit.

D'où la forme du variant :

```rust
Partial { unmeasurable_files: usize, unexplored_subtree: bool }
```

Un **compte** pour ce qui a pu être nommé, un fait **non quantifié** pour ce qui ne pouvait pas l'être — rendus en **deux clauses séparées**, jamais fusionnées (`humanize.rs:93-145` : les fusionner en « N+1 fichiers » fabriquerait un compte pour le sous-arbre). Les deux faits sont **indépendants et cohabitent dans le MÊME variant** : un projet peut avoir N fichiers nommés-mais-non-mesurables **et** un sous-arbre inexploré sans rapport.

C'est la porte ouverte par `Absent` en [[ADR-0032]] AD-5 : *un adaptateur qui ne peut pas lire un signal propage l'absence, il ne fabrique jamais une valeur plausible*. `GateCoverage::Absent` en est l'application au cas dégénéré — il n'y avait aucun fichier sur lequel être partiel, c'est la mesure unique du run qui n'a pas pu être prise.

Le `bool` est alimenté par **deux prédicats exacts**, pas des estimations :

1. une entrée de répertoire à `entry.depth() == MAX_WALK_DEPTH` (`file_system_code_reader.rs:402`) — le seul signal honnête disponible : *« le marcheur n'ira pas au-delà »* ;
2. un `Err` extérieur dont le chemin résout vers un **répertoire**, via `classify_walk_error_path` (`file_system_code_reader.rs:243-256`), dont la branche `Err(_)` (TOCTOU, retry 3) replie prudemment sur `UnexploredSubtree` plutôt que sur le silence.

Security a vérifié que l'alignement est **exact** : `128` est précisément la première profondeur à laquelle un fichier cesse d'être livré — **aucune profondeur-trou** entre la dernière mesurée et la première tronquée.

## Décision AD-5 — le signal honnête doit atteindre **le canal que le consommateur lit**

[[ADR-0010]] demande au *rapport* de dire ce qu'il n'a pas mesuré. #128 montre la limite de cette formulation : **le rapport ne suffit pas quand le consommateur est une CI**, qui ne lit que le code de sortie. Le principe se généralise donc — l'honnêteté doit atterrir sur le canal que le consommateur lit réellement, pas sur celui qui nous arrange.

Corollaire appliqué **à l'intérieur même du ticket** : le retry 2 a créé un état `Partial` **invisible sur JSON et HTML**. Un tableau de bord parsant le rapport voyait un projet en pleine santé. Ce trou a été **replié dans la tranche** plutôt que transformé en ticket — c'était **notre propre dette**, créée par cette tranche, pas une trouvaille préexistante. `unexplored_subtree` est aujourd'hui un booléen jamais omis sur les deux surfaces : `secondaries/.../json_report_writer.rs:80` et `secondaries/.../html/view_model.rs:32-38`.

## Décision AD-6 — une trouvaille **retirée sur preuves**

Une demande de revue proposait de ré-appliquer le filtre d'extension dans la branche `Err` du marcheur, par symétrie. Le correctif a été écrit, puis **reverté**. Deux raisons indépendantes, toutes deux mesurées :

1. **Il était intestable.** La campagne de mutation a produit **4 survivants, zéro `killed`** sur la branche ajoutée : aucun test vivant ne l'atteignait. Puis **six sondages sur système de fichiers réel** (un `.gitignore` illisible, un `.gitignore` malformé, un répertoire sans droit d'exécution contenant un fichier d'extension enregistrée et un d'extension inconnue, un nom de fichier UTF-8 invalide) ont montré que **tout `WithPath` atteignable sur cette pile nomme un RÉPERTOIRE, jamais un fichier** — `ignore` parse les gitignore avec indulgence, APFS refuse les noms non-UTF-8, et la construction de forme fichier vit dans le marcheur **parallèle**, que ce code ne construit jamais. Détail complet en doc de `matches_extension_and_include` (`file_system_code_reader.rs:85-100`).
2. **Security a nommé une seconde raison, dirimante :** le filtre ne pouvait que transformer un `Partial` en `Complete` — c'est-à-dire **un exit 4 en exit 0**. Il échangeait un comportement *fail-closed* contre une sortie plus silencieuse.

**Le revert est consigné comme correct.** Livrer du branchement inatteignable et non vérifié est pire que l'asymétrie qu'il prétendait fermer (`cc-yagni`). **Toute réintroduction exige une preuve d'atteignabilité, pas une dérogation de gate de mutation** — un survivant qu'on déclare équivalent ne prouve pas qu'un chemin existe.

## Ce que `--strict` ne couvre pas

[[ADR-0010]] demande à l'outil de dire ce qu'il n'a pas mesuré ; la même discipline s'applique à la **porte**. Ce que `--strict` ne couvre **pas**, au terme de #128 :

- **#147 (HIGH) — un chemin commençant par `.` sort entièrement du gate.** `hidden(true)` (`file_system_code_reader.rs:346`) écarte l'entrée **sans aucune trace, sur aucune surface** : ni `dropped_files`, ni `unexplored_subtree`, ni JSON, ni HTML, ni console. Security a démontré une charge réellement dépassante de **1 600 fonctions** s'échappant par un simple renommage de répertoire : `heavy/` → complexité 3 241, exit **3** ; `.heavy/` → complexité 1, exit **0 silencieux**. Représentable en git, **survivant à un clone frais**. Préexistant (#83), pas introduit ici. Le correctif est **une décision d'architecture** — compter les entrées écartées-car-cachées dans le signal de couverture, ou rendre `hidden` configurable **et** rapporté — **pas** un rustine, et surtout **pas** un `hidden(false)` sec, qui ingérerait `.git/`, `.venv/` et consorts.
- **#145 — `exclude` réduit l'ensemble mesuré sans trace exploitable par machine.** `.codeimpact.json` ([[ADR-0019]]) peut rétrécir le périmètre ; `default_excluded_count` ne compte que le sous-ensemble **par défaut**, pas les motifs de l'utilisateur. Cela **défait la remédiation documentée d'[[ADR-0006]]** § *Frontière de confiance* : les flags `--max-kwh`/`--max-co2` surclassent bien les **seuils**, mais `exclude` **n'a aucun équivalent CLI et aucun surclassement**. Cause de fond : deux niveaux de confiance distincts — motifs écrits par l'**opérateur** vs motifs écrits par le **dépôt analysé** — effondrés sur un unique `FileFilter`.
- **#146 — un nom de fichier hostile peut masquer visuellement l'avertissement.** Un attribut SGR laissé actif peut dissimuler la ligne de couverture incomplète. L'hexagone ne peut pas atteindre `sanitize_console_text` (`secondaries/.../humanize.rs:213`) sans inverser la règle de dépendance — le correctif demande donc une décision de placement, pas une ligne de plus.
- **`ignore::Error` sans chemin — fermé par analyse, pas de la dette.** Sondé et trouvé **structurellement inatteignable** sur le marcheur séquentiel : `Error::from_walkdir` attache `WithPath` en position la plus externe, et les deux constructions sans chemin exigent soit `follow_links(true)` (fixé à `false`, `file_system_code_reader.rs`), soit le marcheur **parallèle**, jamais construit ici. Consigné comme **clos**, pour qu'une relecture future ne le rouvre pas par prudence.

## Conséquences

- **(+)** Le gate CI cesse de mentir par défaut : *« rien n'a dépassé »* et *« rien de ce que j'ai su mesurer n'a dépassé »* ont désormais deux codes distincts, sur le canal qu'une CI lit ([[ADR-0009]]).
- **(+)** `AlertThresholds::evaluate` — la porte de domaine pure — sort de la tranche **avec un diff nul** ; l'honnêteté d'[[ADR-0010]] y est préservée intacte, elle n'est pas rognée pour faire de la place au nouveau cas.
- **(+)** L'obligation constructeur de `GatedOutput::new` rend l'oubli de couverture **non compilable** : 34 mutants deviennent `unviable` — classe de bug supprimée, pas contournée.
- **(+)** L'état `Partial` atteint les **quatre** surfaces (console, stderr strict, JSON, HTML) : un tableau de bord ne peut plus lire « projet sain » sur une mesure incomplète.
- **(−)** **Changement de contrat assumé** : un dépôt avec un gros fichier généré, ou une arborescence profonde, voit sa CI passer de verte à rouge (exit 4). C'est le comportement voulu, mais il demande une note de migration côté opérateur.
- **(−)** `unexplored_subtree` est un booléen : l'opérateur apprend *qu'*un sous-arbre a été perdu, jamais *lequel* ni *combien*. Limite délibérée ([[ADR-0010]]), pas une omission.
- **(−)** La couverture reste **aveugle aux chemins cachés** (#147) : un exit 0 sous `--strict` ne garantit toujours pas que tout a été vu tant que #147 n'est pas fermé.

## Dette connue, explicitement non traitée

- **#147 (HIGH)** — `hidden(true)` hors du signal de couverture. Décision d'architecture requise, préexistant (#83).
- **#145** — `exclude` de `.codeimpact.json` sans trace machine ni surclassement CLI ; deux niveaux de confiance effondrés sur un `FileFilter`.
- **#146** — assainissement console inatteignable depuis l'hexagone sans inverser la règle de dépendance.
- **Seuils par fonction / par fichier** — toujours hors scope ([[ADR-0017]] §7), inchangé.
