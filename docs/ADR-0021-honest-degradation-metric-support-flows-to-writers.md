# ADR-0021 — Dégradation honnête : `MetricSupport` circule jusqu'aux writers — `n/a`, jamais `0`

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-20
> **Decided in:** Issue #33 (US16 — US14-T3, dégradation honnête C#)
> **Links:** [[architecture-overview]], [[ADR-0020]], [[ADR-0010]], [[ADR-0007]], [[ADR-0008]], [[json-report-schema]], [[html-report]], [[console-report-enriched]], [[glossary]]

## Contexte

[[ADR-0020]] (US14-T2) a **posé la couture** de capacité par-métrique par-langage — `MetricSupport ∈ Supported / Degraded / Unsupported`, le VO `LanguageCapabilities`, la méthode de port `CodeParser::capabilities()` — mais l'a laissée **inerte** : en T2, tout langage rendait `Supported`, et **le rendu** d'un état non-`Supported` était explicitement différé à T3 (D3 d'[[ADR-0020]], « dette connue »). Ce cycle (#33, T3) **allume la couture** : la capacité déclarée par un parseur **circule de bout en bout** et est **rendue** dans les trois formats.

Le principe qui gouverne ce cycle vient d'[[ADR-0010]] : *« un `0` se lit "gratuit" / "code propre" — c'est la pire sortie que ce produit puisse produire »*. [[ADR-0010]] a rendu cette honnêteté **structurelle** pour la *mesure* (le type somme `Measurement<T>` : un `0` mesuré est légitime, un `Unmeasurable` s'affiche `n/a`). T3 étend **exactement la même discipline** à la *capacité d'un langage* : quand le parseur C# ne sait pas produire une métrique, le rapport doit dire `n/a`, **jamais `0`**. Un `0` affiché pour `io_in_loops` sur du C# mentirait « aucune I/O en boucle détectée » alors que la vérité est « rien n'a été mesuré ».

État réel du C# à la sortie de T3 :

| Métrique | `MetricSupport` C# | Pourquoi |
|---|---|---|
| cyclomatique | `Supported` | Extraite par la requête `.scm` ([[ADR-0020]]) |
| impact économique | `Supported` | Dérivée de la complexité (heuristiques, [[ADR-0004]]) |
| impact écologique | `Supported` | Dérivée de l'économique |
| `call_graph` | `Degraded("name-based resolution; ambiguous edges dropped")` | Résolution par nom, arêtes ambiguës abandonnées |
| `io_in_loops` | `Unsupported` | Rien n'est mesuré pour le C# tant que T4 n'a pas livré la table d'I/O |

Le Rust reste **tout-`Supported`** : sa sortie est **byte-inchangée** (aucun objet capacité rendu là où tout est supporté — voir D3).

## Décision

### D1 — Le porteur de la capacité : un champ `Option` sur `CodeMetrics`, **pas** un changement de port

La capacité déclarée par un parseur doit **voyager avec les métriques** jusqu'aux writers. Deux options :

1. Re-consulter `CodeParser::capabilities()` dans chaque writer → recouple les writers au parseur, alors que les writers ne connaissent (et ne doivent connaître) que `CodeMetrics`.
2. **Faire porter la capacité par `CodeMetrics` lui-même**, en tant que **nouveau champ `capabilities: Option<LanguageCapabilities>`**.

**Retenu : l'option 2.** `RunAnalysis` interroge `capabilities()` une seule fois (là où le parseur est déjà en main) et pose le résultat dans `CodeMetrics`. Les writers lisent un champ des métriques qu'ils reçoivent **déjà** — aucun nouveau port, aucun nouvel argument de méthode, aucun re-couplage.

**Pourquoi `Option`, et pas un `LanguageCapabilities` toujours présent :** `None` = « tout supporté, rien à signaler » (le cas Rust). Rendre `None` équivalent à « aucune annotation » garantit que **la sortie Rust reste byte-identique au pré-T3** — la migration ne touche que les langages qui ont réellement quelque chose à dégrader. C'est le pendant, au niveau capacité, de la règle d'[[ADR-0010]] : ne rien changer là où il n'y a rien à corriger. (`Some(caps)` où *toutes* les entrées sont `Supported` se rend aussi sans annotation — le discriminant est l'état par-métrique, jamais la présence de l'`Option`.)

> **[[ADR-0001]] tenu.** `LanguageCapabilities` et `MetricSupport` sont les types zéro-dép de l'hexagone déjà introduits par [[ADR-0020]] ; T3 n'ajoute qu'un champ `Option` à un VO existant. Aucune dépendance ne franchit la frontière de l'hexagone.

### D2 — Les writers branchent sur `MetricSupport`, **jamais sur l'identité du langage**

La propriété centrale de ce cycle, et celle qui fait **composer** T4/T5 sans rouvrir les writers :

> **Un writer rend une métrique non-`Supported` en `n/a` en lisant *dynamiquement* son `MetricSupport` — il ne teste jamais "si C#".**

Aucun writer ne contient `if language == CSharp`. Chacun lit l'état `MetricSupport` de la métrique et rend en conséquence. Conséquence directe :

- **T4** fera passer `io_in_loops` C# de `Unsupported` à `Degraded(...)` — **zéro changement de writer** : la même branche `Degraded` s'allume.
- **T5** ajoutera `cross_file_dependencies` à la carte des capacités — les writers, qui itèrent sur les entrées présentes, la rendront **sans modification**.

C'est OCP appliqué au rendu : le comportement varie par la **donnée** (`MetricSupport`), pas par un `match` sur le langage réécrit à chaque slice. Même esprit que « le comportement par-langage est de la donnée, pas du code » d'[[ADR-0020]] D1, transposé du parsing au rendu.

### D3 — Le rendu par format

Trois amendements, un par writer. La **présence de l'annotation est pilotée par l'état** : `Supported` (ou `None`) → rendu nominal inchangé ; `Degraded`/`Unsupported` → `n/a` + note.

| Format | `Unsupported` | `Degraded(reason)` | Détail |
|---|---|---|---|
| **Console** | `n/a — non supporté pour C#` (français) | valeur + note `[dégradé: reason]` | Voir [[console-report-enriched]] |
| **JSON** | le champ sérialise `null` (jamais `[]` ni `0`) + objet `metric_support` | `null`/valeur + `metric_support` (`"degraded: reason"`) | Valeurs **anglaises** `"supported"` / `"degraded: reason"` / `"unsupported"`. Voir amendement [[ADR-0007]] |
| **HTML** | `MetricVm.support` = `na` + `MetricVm.note` | `support` = `degraded` + note | Whitelist JS **fermée** `SUP` → classes CSS `sup-ok` / `sup-degraded` / `sup-na` ; **pas d'`innerHTML`**, discipline [[ADR-0008]] §8.10 préservée. Voir amendement [[ADR-0008]] |

**JSON — `null`, jamais `[]`/`0` (le cœur d'[[ADR-0010]] appliqué à la sérialisation).** `io_in_loops` et `unclassifiable_io_in_loops_count` sérialisent `null` quand la métrique est `Unsupported` — **jamais** un tableau vide (qui lirait « analysé, aucune I/O ») ni `0` (idem). Le nouvel objet `metric_support` porte l'état déclaré, pour qu'un consommateur CI distingue « mesuré à vide » de « non mesuré ». Les valeurs y sont en **anglais** (contrat machine stable, cohérent avec le reste du schéma JSON), là où la Console rend en **français** (sortie humaine).

**HTML — la whitelist `SUP` est la forme applicable d'[[ADR-0008]] §8.10.** Un état de support ne devient **jamais** une chaîne de style ou de classe construite à partir de la donnée : le JS mappe l'énuméré, via une whitelist fermée `SUP` (`hasOwnProperty` + fallback), vers l'une des trois classes CSS figées `sup-ok`/`sup-degraded`/`sup-na`. Zéro `innerHTML`, zéro `.style` piloté par la capacité — exactement la discipline « couleurs par classes résolues par whitelist fermée » d'[[ADR-0008]] §8.10, étendue à la dimension support.

### D4 — Le chemin temporel `Unsupported` (T3) → `Degraded` (T4)

L'`Unsupported` du `io_in_loops` C# est **transitoire par conception**. T3 le rend `n/a` honnêtement *aujourd'hui* ; T4 livrera la table de signatures d'I/O C# ([[ADR-0016]], [[ADR-0020]] `LanguageProfile`) et fera basculer l'état à `Degraded(...)`. Parce que les writers branchent sur `MetricSupport` (D2), **ce basculement est une donnée, pas une réécriture** : la métrique quitte la branche `n/a — non supporté` pour la branche `[dégradé]`, sans qu'un writer change.

## Conséquences

- **(+)** **L'outil ne peut plus imprimer `0` là où il n'a pas su analyser.** La promesse d'[[ADR-0010]] — « jamais "gratuit" par défaut » — est tenue pour la *capacité d'un langage*, pas seulement pour la *mesure d'un run*.
- **(+)** **T4/T5 composent sans rouvrir les writers** : le rendu branche sur `MetricSupport` (D2), pas sur le langage. T4 = flip d'un état, T5 = ajout d'une entrée de carte.
- **(+)** **Sortie Rust byte-inchangée** : `capabilities: None` (ou tout-`Supported`) ne rend aucune annotation (D1). Aucune régression sur le langage historique.
- **(+)** **Contrat JSON honnête et lisible-machine** : `null` (jamais `[]`/`0`) + `metric_support` distinguent « mesuré à vide » de « non mesuré » pour la CI ([[ADR-0007]] amendé).
- **(=)** **[[ADR-0008]] §8.10 préservé** : la whitelist fermée `SUP` n'ouvre aucun nouveau sink XSS ; la capacité rejoint le DOM par classe CSS figée, jamais par chaîne construite.
- **(=)** **[[ADR-0001]] tenu** : un simple champ `Option` sur un VO existant, sur des types hexagone déjà zéro-dép d'[[ADR-0020]].
- **(−)** **`CodeMetrics` gagne un champ `Option`** propagé dans les trois writers — coût de migration payé une fois, comme le `Measurement<T>` d'[[ADR-0010]].

## Dette connue, explicitement non traitée

- **Honnêteté de l'agrégat projet (tuile stat / agrégat JSON projet) — différée en T3b.** La tuile de niveau *top-level* d'un projet **purement C#** (et l'agrégat JSON projet) ne porte pas encore l'annotation de dégradation : la rendre honnête exige une **agrégation de `MetricSupport` en présence de langages mixtes** (que devient le support d'une métrique quand un projet mêle Rust tout-`Supported` et C# `Unsupported` ?). Cette règle d'agrégation est le sujet du follow-up **T3b** ; ce cycle rend honnête le **détail par-fichier / par-fonction**, pas l'agrégat projet.
- **`io_in_loops` C# reste `Unsupported`** jusqu'à T4 (D4). Comportement honnête *aujourd'hui* (`n/a`), pas un `0` trompeur — mais l'utilisateur C# ne voit pas encore d'I/O-en-boucle.
