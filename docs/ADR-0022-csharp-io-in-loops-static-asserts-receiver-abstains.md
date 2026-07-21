# ADR-0022 — Classification des I/O en boucle pour le C# : le qualificatif statique affirme, le nom de récepteur s'abstient

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-20
> **Decided in:** Issue #33 (US14-T4 — classification I/O-en-boucle C#), sur `feat/33-t4-csharp-io`
> **Links:** [[architecture-overview]], [[ADR-0016]], [[ADR-0020]], [[ADR-0019]], [[ADR-0021]], [[ADR-0010]], [[glossary]]

## Contexte

[[ADR-0016]] a fixé la règle d'or de l'I/O-en-boucle : **le type affirme, le nom s'abstient**, trois états (`Io` / `NotIo` / `Unknown`), l'abstention comptée jamais criée. Cette règle a été écrite pour `SynCodeParser` (Rust), qui **résout le type du récepteur intra-fichier** (§1 d'[[ADR-0016]]) : l'affirmation `Io` repose sur une **preuve de type**.

[[ADR-0020]] (US14-T4… en réalité T2) a introduit `TreeSitterCodeParser` pour le C# — mais tree-sitter est un moteur **purement syntaxique** : il n'y a **aucune résolution de type**. En T2, chaque appel-en-boucle C# était donc classé `IoClassification::Unknown` — un **placeholder honnête** (on ne savait pas encore classer), mais aveugle : un authentique `File.ReadAllText(path)` dans un `foreach` remontait comme abstention, pas comme I/O.

Ce cycle (T4) **ferme le placeholder** : `TreeSitterCodeParser` classe désormais chaque appel-en-boucle C#. La question centrale : **comment tenir la règle « le type affirme » quand l'adaptateur n'a pas de type à offrir ?** La réponse est l'extension d'[[ADR-0016]] à un adaptateur **sans résolution de type** — sans jamais amender sa règle d'or.

> **Le détecteur de l'hexagone est resté inchangé.** `IoInLoopsDetector` délègue déjà entièrement le jugement « est-ce de l'I/O ? » à l'adaptateur (le champ `io: IoClassification` du `LoopCall`, [[ADR-0016]] §2, [[ADR-0020]] frontière agnostique). T4 ne touche **que** l'adaptateur C# : c'est la preuve que le contrat port/adaptateur d'[[ADR-0016]] était correct dès l'origine.

## Décision

### D1 — Le **qualificatif statique** est la preuve de type ; le **nom de récepteur** ne produit que de l'abstention

Transposition exacte d'[[ADR-0016]] §1 à un adaptateur sans inférence de type. La preuve de type que `Io` exige, tree-sitter ne peut la fournir que dans **un seul cas syntaxique** : l'appel **statiquement qualifié**, où le type est **dans la syntaxe même**.

| Forme syntaxique C# | Preuve de type ? | Classification |
|---|---|---|
| Appel **statiquement qualifié** — `File.ReadAllText`, `Directory.GetFiles` | Oui — le type est **dans la syntaxe** | **`Io`** (warned) |
| Appel **d'instance / sur récepteur** — `_httpClient.GetAsync(...)`, `reader.ReadLine()` | Non — un nom de récepteur n'est **pas** une preuve de type | **`Unknown`** (abstention comptée) |

- `IO_PREFIXES` est **restreint au statique** : `File.` / `Directory.` (les types BCL dont le nom qualifié **est** la preuve). Un préfixe statique matché → `Io`.
- Les récepteurs d'instance sont reconnus par des **marqueurs de nommage idiomatiques** (underscore-camelCase de champ privé) : `_httpClient.`, `_sqlCommand.`, `_stream.`, `_dbContext.`, plus les marqueurs EF `_context.` / `_db.`. Un marqueur matché → `Unknown`, **jamais `Io`** : parce que tree-sitter est syntaxique, un nom de récepteur seul n'est pas une preuve de type — exactement le rôle « le nom s'abstient » d'[[ADR-0016]].

**Un appel d'instance n'affirme jamais.** C'est le prix assumé du zéro-faux-positif d'[[ADR-0016]] §1, tenu à l'identique côté C# : plutôt qu'un warning fabriqué sur `results.Write()`, une abstention comptée.

### D2 — Les marqueurs d'abstention **doivent** épouser le nommage C# idiomatique (la leçon du faux négatif silencieux)

Un tour de revue a rattrapé un défaut de conception exact : les marqueurs de récepteur avaient été écrits en **PascalCase**, si bien qu'ils ne matchaient **jamais** `_httpClient.` (underscore-camelCase, le nommage idiomatique d'un champ privé C#). Conséquence : un appel HTTP en boucle tombait en `NotIo` **confiant et faux** — précisément le **« faux négatif silencieux tue l'honnêteté »** que [[ADR-0016]] §3 (et [[ADR-0010]]) proscrit.

**Décision : les marqueurs d'abstention sont épinglés sur le nommage idiomatique C# (`_camelCase` de champ privé), et l'appariement est verrouillé par un test dédié.** Réintroduire un marqueur qui ne matche pas la convention réelle exigera de réfuter le test, pas un oubli. C'est la discipline « exclusion épinglée par un test » d'[[ADR-0016]] §3, appliquée en sens inverse : ici c'est l'**inclusion** de l'abstention qui doit être prouvée présente.

### D3 — EF / N+1 (`IQueryable` matérialisé en `foreach`) = `Unknown` compté, **pas** `Io` (décision humaine, fidèle à [[ADR-0016]], non amendée)

Un `IQueryable` matérialisé dans un `foreach` (une requête EF exécutée par itération) est un **N+1** — de la **vraie I/O**, souvent le pire coût caché d'un code C#. La tentation est de l'affirmer `Io`.

**Décision (humaine) : `Unknown`, pas `Io`.** La reconnaissance d'un N+1 repose sur une **heuristique de nom de récepteur** (`_context.`, `_db.`) **sans preuve de type** : l'affirmer `Io` violerait la règle d'or « le type affirme » d'[[ADR-0016]] §1. Le N+1 atterrit donc :

- dans le **compteur d'abstention** (`Unknown` agrégé — [[ADR-0016]] §3), et
- **nommé explicitement dans la raison de dégradation** (D4) : l'utilisateur est prévenu que les récepteurs d'instance / EF ont été **abstenus, pas asseverés**.

C'est un choix **fidèle à [[ADR-0016]], PAS un amendement** : le N+1 est visible (compté + nommé) sans fabriquer une affirmation non prouvée. La résolution de type qui permettrait de l'affirmer `Io` reste hors scope (P2, trajectoire heuristiques-d'abord d'[[ADR-0016]] / [[ADR-0004]]).

### D4 — La capacité `io_in_loops` du C# = `Degraded`, rendue par le canal d'honnêteté de T3

En T2, `TreeSitterCodeParser` déclarait `io_in_loops` comme `Supported` (placeholder, tout `Supported` — [[ADR-0020]] D3). T4 la **bascule en `Degraded`** (le VO `MetricSupport` d'[[ADR-0020]]), avec la raison :

> `"syntactic only; instance/EF receivers abstained, not asserted"`

Le rendu de cette dégradation est le **canal d'honnêteté de T3** ([[ADR-0021]]) : la console **appose `[dégradé: <raison>]`** sur la ligne I/O. La couture capacité/dégradation câblée à vide en T2 ([[ADR-0020]] D3) reçoit ici son **premier `Degraded` réel** — la couture était le prérequis, T4 est le premier retour.

> **Note d'inter-branches.** [[ADR-0021]] (T3, canal de dégradation honnête) vit sur une branche sœur non encore mergée : ce lien est un **forward-ref** qui se résout au merge (pattern « collision de numéro ADR »). La bascule `Degraded` de T4 est correcte indépendamment ; seul son **rendu** dépend de T3.

### D5 — `ioSignatures` de la config câblé **additivement**, borné 256/256 (fermeture d'une DoS)

[[ADR-0019]] §6 avait **réservé** la clé `ioSignatures` du `.codeimpact.json` (parsée-mais-inerte, forward-compat). T4 encaisse la réserve, **sans rouvrir le contrat du fichier** :

- **Sémantique additive** : les préfixes d'I/O fournis par l'utilisateur **promeuvent en `Io`** les appels qui les matchent. C'est un **ajout** au chemin statique confiant (D1), jamais un retrait — un `ioSignatures` vide/absent laisse le comportement de D1 byte-identique.
- **Bornes anti-DoS** : `MAX_IO_SIGNATURE_COUNT = 256` et `MAX_IO_SIGNATURE_LENGTH = 256` — **miroir exact** des bornes de `FileFilter` ([[ADR-0019]] §1, `MAX_PATTERN_COUNT` / `MAX_PATTERN_LENGTH`). **Auto-validées dans le VO `AnalysisConfig`** ([[ADR-0019]] §1), **fail-fast au chargement** : une liste non bornée gonflerait le coût de parse par fichier (chaque signature testée sur chaque appel) — un état capable de cette DoS est **inconstructible**, refusé à la frontière du VO, jamais au moment du walk.

C'est la discipline d'[[ADR-0019]] (motifs bruts validés à la frontière du VO, hexagone zéro-dep — [[ADR-0001]]) appliquée à une nouvelle liste de motifs.

### D6 — Différé, documenté : LINQ-to-Objects → **boucle CPU** (`has_loop`), une tranche US1/US11 distincte

Un `.Where(...).Select(...)` LINQ-to-Objects sur une collection en mémoire est une **boucle CPU** (itération), pas de l'I/O. Le classer relèverait de `has_loop = true` — donc des **champs cyclomatiques / de boucle** (US1 / US11), **pas** du canal I/O que T4 câble.

**Décision : politique enregistrée ici, implémentation différée.** Traiter LINQ-to-Objects comme une boucle CPU est une **tranche séparée** (touche `has_loop` et le comptage cyclomatique, pas `IoClassification`). L'agréger à T4 mélangerait deux surfaces distinctes. Différé à une future tranche US1/US11 ; la politique est actée pour ne pas être re-décidée.

## Calibration (T4.4) — eShopOnWeb, un vrai vrai-négatif

Discipline *freeze-then-measure* d'[[ADR-0016]] §4 : les listes (préfixes statiques, marqueurs d'abstention) gelées, **puis** mesurées.

| Corpus | Fichiers | Hits `Io`-en-boucle (chemin statique) | Interprétation |
|---|---|---|---|
| eShopOnWeb | 209 | **0** | **Vrai vrai-négatif** sur corpus clairsemé |

- **0 hit statique est correct, pas un manque.** eShopOnWeb n'a pas de `File.ReadAllText` / `Directory.GetFiles` dans une boucle : le chemin statique confiant **n'a rien à affirmer**, et il **s'abstient de fabriquer**. Son accès données passe par EF (récepteurs d'instance) → comptés en abstention, nommés dans la raison `Degraded` (D3/D4), jamais faussement asseverés.
- **Le chemin statique confiant fonctionne** : vérifié **de bout en bout** sur un `File.ReadAllText`-en-`foreach` réel (hors corpus) → `Io`. Le zéro d'eShopOnWeb est donc un vrai-négatif mesuré, pas un détecteur muet.
- **Documenté honnêtement** ([[ADR-0010]]) : un corpus sur lequel l'outil ne trouve rien de statique et le dit est plus crédible qu'un corpus « ajusté pour faire des hits ».

## Conséquences

- **(+)** Le placeholder `Unknown` de T2 ([[ADR-0020]]) est **fermé** : le C# classe enfin ses I/O-en-boucle, sur le seul chemin honnête (qualificatif statique = preuve de type).
- **(+)** **Zéro faux positif par construction** : aucun récepteur d'instance n'affirme `Io` — la règle d'or d'[[ADR-0016]] tenue sur un adaptateur sans résolution de type.
- **(+)** Le N+1 EF est **visible sans être fabriqué** : compté en abstention + nommé dans la raison `Degraded`.
- **(+)** `ioSignatures` donne à l'utilisateur un levier d'affirmation **borné et sûr** (256/256, fail-fast VO), sans rouvrir le contrat de config ([[ADR-0019]]).
- **(−)** L'I/O d'instance / EF réelle passe en abstention (non prouvée) — **prix assumé** du zéro-faux-positif, rendu visible par le compteur + la raison `Degraded`.
- **(−)** La capacité `io_in_loops` du C# est `Degraded`, pas `Supported` : honnête, mais moins que le Rust (qui résout les types). La parité exigerait une résolution de type C# — hors scope, P2.

## Dette connue, explicitement non traitée

- **N+1 EF affirmable `Io`** — exigerait une résolution de type C# (le type de `_context.Orders` est un `DbSet<Order>`). Hors scope, P2 ([[ADR-0004]]).
- **LINQ-to-Objects → boucle CPU (`has_loop`)** — politique enregistrée (D6), implémentation différée à une tranche US1/US11 distincte (touche les champs cyclomatiques, pas l'I/O).
- **Marqueurs de récepteur codés par convention de nommage** — l'abstention d'instance repose sur `_camelCase` idiomatique (D1/D2). Un code C# non idiomatique (champ public `PascalCase`, variable locale) échappe au marqueur et retombe en `NotIo`. Comportement heuristique documenté, borné par le test d'appariement (D2), non garanti par le type.
