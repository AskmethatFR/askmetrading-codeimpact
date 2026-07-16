# ADR-0015 — Isolation du parsing par sous-processus canari : contenir un débordement de pile sans le prédire

> **Type:** technical (ADR)
> **Status:** Accepted
> **Date:** 2026-07-16
> **Decided in:** Issue #63
> **Links:** [[architecture-overview]], [[ADR-0006]], [[ADR-0010]]

## Contexte

`syn::parse_file` déborde la pile — et **abandonne le processus** (`SIGABRT` « stack overflow ») — sur un fichier hostile de ~12 Ko contenant ~1700 `mod` imbriqués (dette connue d'[[ADR-0014]], cousine de #52). Le débordement n'est pas une lenteur : c'est la mort du processus **entier**. Or CodeImpact scanne un `--path` fichier par fichier dans un seul processus : **un seul fichier pathologique tue tout le scan**, y compris les résultats déjà calculés pour les centaines de fichiers sains. Reproduit sur le binaire réel : une DoS, pas un cas théorique.

Trois forces cadrent la décision :

1. **Un thread ne peut pas contenir un débordement de pile.** Le débordement déclenche `abort()` au niveau du processus ; il ne remonte pas comme une panique attrapable. Faire tourner `parse_file` dans un `std::thread` à grande pile **retarde** le seuil sans jamais le fermer — et quand il déborde, l'`abort` du thread **emporte le parent**. Contenir un `abort` exige une frontière de processus, pas de thread.

2. **On ne peut pas prédire le débordement en lisant la source.** Le cycle-1 avait tenté un **pré-scan par énumération de caractères** (compter la profondeur d'imbrication avant de parser). Il a échoué de trois façons, documentées dans la leçon d'[[ADR-0010]] : trois contournements structurels (imbrication via génériques, via opérateurs unaires, via références — chacun débordait `syn` sans déclencher le compteur) ; une partie de **whack-a-mole** sans fin de fin ; et pire, des **faux positifs** qui refusaient de mesurer du code parfaitement sain — exactement l'inversion d'[[ADR-0010]] (« ne jamais afficher un `0` confiant sur ce qu'on n'a pas regardé » devenait « refuser de regarder ce qui est mesurable »). Prédire la profondeur avant de parser est indécidable en pratique. **On laisse `syn` déborder — mais dans un processus jetable.**

3. **[[ADR-0006]] borne déjà l'entrée.** La borne de taille à deux étages d'[[ADR-0006]] (input ≤ 1 Mo) reste le **garde-fou primaire sur les trois OS** : elle plafonne l'amplification. L'isolation par sous-processus est la **seconde ligne** contre la classe résiduelle — un petit fichier sous la borne qui déborde quand même par imbrication.

> **Note d'arbitrage.** L'Architecte avait d'abord rejeté l'isolation par sous-processus comme YAGNI et lourde. Security a **surchargé** cet avis sur une DoS **reproduite** — la borne d'entrée seule ne ferme pas la classe « petit fichier, pile débordée ». La règle de priorité (Security critique > QA critique) a tranché. Cet ADR enregistre la décision retenue, pas l'hésitation.

## Décision

### 1. Un sous-processus canari dédié, `codeimpact-parse-probe`

Le parsing risqué — `syn::parse_file` **et** la marche d'extraction qui descend l'AST — est isolé dans un **binaire auxiliaire distinct**, `codeimpact-parse-probe`, déclaré comme cible binaire Cargo dédiée de `secondaries/` (section `bin` du manifeste) (`secondaries/src/bin/parse_probe.rs`). L'adaptateur `SynCodeParser` (`exercise_full_pipeline` / `probe_source`) le lance en enfant, une fois par fichier candidat.

### 2. Protocole canari : l'enfant ne rapporte qu'un verdict, le parent re-parse

L'enfant **ne sérialise jamais l'AST**. Il exécute le pipeline complet sur la source reçue sur `stdin` et **ne renvoie qu'un code de sortie** :

| Code de sortie enfant | Signification | Action parent |
|---|---|---|
| `0` | pipeline complet réussi (canari vivant) | **re-parser la source dans le parent** et produire les métriques |
| `2` | parse échoué proprement (`syn::Error`) | `Unmeasurable` avec la raison de parse existante |
| non-{0,2} | crash (débordement de pile, `abort`, signal) | `Unmeasurable { reason: SourceTooComplex }` |

Le parent ne fait **jamais confiance à des données** venant de l'enfant — l'enfant est un **canari** : sa seule fonction est de **mourir à la place du parent**. S'il survit (`0`), le parent re-parse en sécurité, sachant que cette source **ne déborde pas** à cette profondeur.

### 3. Dominance de pile : une différence de nature, pas une marge

L'enfant parse avec une pile de **16 MiB** ; le parent re-parse avec **32 MiB**. Ce n'est **pas une marge de sécurité** — c'est une **garantie par dominance** : toute source que l'enfant survit à 16 MiB, le parent la survit *a fortiori* à 32 MiB (le double). Le re-parse du parent ne peut donc **jamais** déborder sur une entrée que le canari a validée. Sans cette dominance, le parent pourrait déborder sur une source que l'enfant a acceptée — et on aurait déplacé la DoS, pas fermée.

### 4. Mapping par code de sortie seul → agnostique de la plateforme

La classification « crash » est faite **uniquement sur le code de sortie**, jamais sur le contenu de `stderr` ni sur un texte de signal. `verdict_from(exit_status)` mappe `non-{0,2}` → `SourceTooComplex`. C'est ce qui rend le mécanisme **cross-platform par construction** (voir la note de plateforme).

### 5. Cache de verdict clé par égalité de source complète — jamais par hash

Le verdict (Admissible / SourceTooComplex / parse-error) est mémoïsé pour éviter un fork+exec par fichier répété. La clé du cache est la **source complète, comparée par égalité** — **pas un hash**. Raison : une collision de hash déterministe rejouerait un verdict `Admissible` **périmé** sur une source différente qui, elle, déborde — **rouvrant la DoS silencieusement**. L'égalité de source complète ne peut pas collisionner. Le coût mémoire est borné par [[ADR-0006]] (sources ≤ 1 Mo).

### 6. Garde-fous d'exécution

- **Timeout 10 s, tué par le parent** : un enfant qui boucle (sans déborder) est tué et classé `SourceTooComplex`.
- **`RLIMIT_AS` 2 GiB, `cfg(unix)`** : plafonne l'espace d'adressage de l'enfant, fermant l'amplification mémoire (#62) côté enfant sur Unix.

## Alternatives rejetées

- **Thread à grande pile.** Ne contient pas un `abort` : le débordement du thread emporte le parent (force n°1). Retarde le seuil sans le fermer.
- **Mode caché `parse-probe` du binaire principal** (au lieu d'un binaire séparé). Ferait dépendre `secondaries/` d'un adaptateur *primaire* (le CLI) pour se sonder lui-même — **inversion de la règle de dépendance** (`primaries → secondaries`, jamais l'inverse), invérifiable en test. Rejeté.
- **Enfant sérialisant l'AST** (au lieu d'un simple code de sortie). Exigerait un DTO miroir de l'arbre `syn` traversant la frontière de processus — une dépendance de sérialisation lourde et un couplage qui **violerait la règle zéro-dépendance de l'hexagone** ([[ADR-0001]]). Le protocole canari (code de sortie seul + re-parse parent) l'évite entièrement.
- **Pré-scan par énumération de caractères.** La leçon du cycle-1 (force n°2, tracée dans [[ADR-0010]]) : trois contournements, whack-a-mole, faux positifs inversant [[ADR-0010]]. Rejeté définitivement.
- **Windows Job Objects ce cycle.** Différé : la borne d'entrée à 1 Mo d'[[ADR-0006]] est le garde-fou primaire sur les **trois** OS, et le mapping par code de sortie couvre déjà `STATUS_STACK_OVERFLOW` sur Windows. Les Job Objects (plafond mémoire natif Windows) sont une amélioration future, pas un bloqueur.

## Conséquences

- **(+)** Un fichier pathologique ne tue plus le scan : il devient une ligne `NON MESURÉ` (`SourceTooComplex`), fidèle à [[ADR-0010]] — jamais un `0` confiant, jamais un scan interrompu.
- **(+)** Cross-platform **prouvé** par un job de matrice CI `cross-platform` (Linux / macOS / Windows) : le crash de l'enfant est classé identiquement sur les trois.
- **(−) Deux binaires livrés côte à côte.** `codeimpact` **et** `codeimpact-parse-probe` (~+2-3 Mo). Le parent **échoue bruyamment** si la sonde est absente (pas de dégradation silencieuse) ; l'override `CODEIMPACT_PARSE_PROBE` permet de pointer une sonde explicite.
- **(−) Un fork+exec par fichier** (un seul, amorti par le cache du §5). Surcoût mesuré ~**6,3 ms/fichier**, dépendant de la machine.
- **(−) Résidu D2 — sonde étrangère ou périmée.** Si `discover_probe_path` résout une sonde d'un **autre build** (versions de `syn` divergentes, tailles de pile différentes), la garantie de dominance (§3) ne tient plus. Pire cas : **réouverture de la DoS** — **jamais une fausse mesure** (le parent re-parse toujours ; il ne fait jamais confiance aux octets de l'enfant). Mitigé par la **découverte de la sonde sœur du même build** (`discover_probe_path` cherche d'abord le sibling adjacent au binaire courant).

## Note de plateforme

Le mécanisme est **agnostique de la plateforme par construction** :

- **Mapping par code de sortie** (§4) : sur Unix, un signal (débordement → `SIGSEGV`/`SIGABRT`) donne `code() == None` → non-{0,2}. Sur Windows, `STATUS_STACK_OVERFLOW` (`0xC00000FD`) ou un `abort` donne un code non-{0,2}. Même verdict, sans lire aucun texte.

Seuls **deux** points sont spécifiques à la plateforme :
- `RLIMIT_AS` 2 GiB → `cfg(unix)` uniquement (§6).
- Le nom de la sonde utilise `EXE_SUFFIX` (`.exe` sur Windows) dans `discover_probe_path`.

## Dette connue, explicitement non traitée

- **#52** — les trois parcours récursifs du graphe d'appel (`dfs_reachable`, `tarjan_scc`, `compute_depth`) peuvent déborder à leur tour vers 10-20k de profondeur linéaire. **Orthogonal à #63** (qui isole le parsing, pas les parcours de graphe) et **différé** : la trace #63-vs-#52 est enregistrée dans [[ADR-0010]] § Dette connue.
- **Windows Job Objects** — plafond mémoire natif Windows, différé (voir Alternatives rejetées).
