# ADR-0016 — Classification des I/O dans les boucles : le type affirme, le nom s'abstient, et l'abstention se compte

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-16
> **Decided in:** Issue #56
> **Links:** [[architecture-overview]], [[ADR-0004]], [[ADR-0010]], [[ADR-0013]], [[ADR-0014]], [[ADR-0015]], [[glossary]]

## Contexte

La dette la plus ancienne du détecteur US5, tracée dans [[ADR-0013]] et reconduite par [[ADR-0014]] : **`is_io` était structurellement `false` pour tout appel de méthode.** Le nom enregistré est l'identifiant nu (`read_to_string`), qui ne peut jamais commencer par `std::fs::` — donc `file.read_to_string(&mut s)` dans une boucle passait inaperçu. En Rust idiomatique, l'I/O est massivement en forme de méthode : **US5 ne détectait qu'une minorité des I/O réelles** — le zéro confiant, silencieux et faux qu'[[ADR-0010]] proscrit.

Le ticket opposait deux mécanismes : une **liste de noms de méthodes suspects** (`read`, `write`, `send`…) contre un **suivi syntaxique du type du récepteur**. Deux forces rendaient chaque mécanisme seul inacceptable :

1. **Le faux positif tue la crédibilité** ([[ADR-0013]]) : un warning I/O sur `results.write()` où `results` est un `Vec` apprend à l'utilisateur à ignorer les alertes.
2. **Le faux négatif silencieux tue l'honnêteté** ([[ADR-0010]]) : ne rien dire quand on ne sait pas classifier, c'est fabriquer un `NotIo` confiant.

La décision refuse le dilemme : **les deux mécanismes sont retenus, chacun dans le seul rôle où il est honnête.**

## Décision

### 1. L'affirmation exige une preuve de type ; le nom seul ne produit que de l'abstention

Un appel de méthode n'est classé **`Io`** que si le **type du récepteur est prouvé** et appartient à la liste des types I/O connus. Un nom suspect **sans preuve de type ne produit jamais un warning** — seulement une abstention (§3). La liste de noms n'affirme rien ; le suivi de type seul affirme.

**La résolution de type est intra-fichier et purement syntaxique** — aucune inférence :

| Source de preuve | Exemple |
|---|---|
| Paramètre de signature typé | `fn f(file: &mut File)` |
| `let` annoté | `let sock: TcpStream = …` |
| Liaison de constructeur | `let f = File::open(p)` |
| … à travers une chaîne bornée | `?` / `.unwrap()` / `.expect()` / `.await` |
| Shadowing | par écrasement — la dernière liaison gagne |

**Types I/O connus** (std / tokio / reqwest) : `File`, `TcpStream`, `TcpListener`, `UdpSocket`, `Client`, `Response`, `BufReader`, `BufWriter`, `Stdin`, `Stdout`, `Stderr`.

### 2. Trois états, pas deux — parce qu'un `bool` rend l'abstention non représentable

```rust
// hexagon — value object
pub enum IoClassification { Io, NotIo, Unknown }
```

C'est la transposition exacte d'[[ADR-0010]] §1 : avec `is_io: bool`, « je ne sais pas » devait s'écrire `false` — un `NotIo` fabriqué, inconstructible en trois états. Le **`match` exhaustif est obligatoire, aucun bras `_` fourre-tout** — règle vérifiée par revue et par grep : introduire un quatrième état demain forcera chaque consommateur à décider, au lieu de tomber silencieusement dans un défaut.

La répartition des responsabilités suit [[ADR-0013]] §2 — *le domaine nomme le concept, l'adaptateur nomme la syntaxe* : `IoClassification` est un VO de l'hexagone ; les **listes** (types I/O connus, noms suspects) vivent dans l'adaptateur `secondaries/` (ACL).

**Garde anti-collision locale** : un type déclaré **dans le fichier parsé** (`struct Client { … }`) n'affirme jamais `Io`, même homonyme d'un type de la liste. Le `Client` de l'utilisateur n'est pas celui de `reqwest`.

### 3. L'abstention est bornée par la liste de noms suspects — sinon elle redevient du bruit

Marquer `Unknown` **tout** appel au récepteur non résolu noierait le signal : l'écrasante majorité (`push`, `len`, `iter`…) n'est pas de l'I/O. L'abstention n'est émise que si le nom de méthode appartient à la liste des **16 noms suspects** :

`read`, `read_to_string`, `read_to_end`, `read_exact`, `read_line`, `write`, `write_all`, `flush`, `send`, `recv`, `query`, `execute`, `connect`, `accept`, `sync_all`, `copy`

**Exclusions délibérées** : `get`, `post`, `fetch`, `open`, `create` — collisions massives avec du code non-I/O (`HashMap::get`, constructeurs…). L'exclusion est **épinglée par un test dédié** : la réintroduire exigera de réfuter le test, pas un oubli.

**L'abstention se publie comme un nombre, jamais comme une ligne.** Compteur **agrégé** uniquement — par fichier + total projet — sur les **trois surfaces** (JSON, additif conformément à [[ADR-0007]] ; console ; HTML). Jamais de pseudo-warning par ligne : une abstention n'est pas une alerte dégradée, c'est l'aveu comptable de ce qu'on n'a pas su prouver *(arbitrage humain, Q2)*.

### 4. Rien n'est shippé non mesuré — calibration sur deux corpus

Discipline *freeze-then-measure* d'[[ADR-0014]] §5 : les listes ont été gelées, puis mesurées — jamais ajustées dans le même mouvement. Corpus tiers : **ripgrep** *(arbitrage humain, Q1)*. Harness jetable appelant le **vrai** `SynCodeParser::parse` via path deps, `CODEIMPACT_PARSE_PROBE` positionné explicitement pour la découverte de la sonde canari ([[ADR-0015]]). Mesures du 2026-07-16 (T3) :

| Corpus | Fichiers | Fonctions | Appels en boucle | `Io` | `NotIo` | `Unknown` | Faux `Io` (vérifiés main) | Abstention |
|---|---|---|---|---|---|---|---|---|
| codeimpact (self) | 91 (0 non mesurés) | 746 | 478 | 5 | 472 | 1 | **0/5** | 0,2 % |
| ripgrep (clone shallow, 100 fichiers, 0 crash) | 100 (0 non mesurés) | 2373 | 1178 | 3 | 1158 | 17 | **0/3** | 1,4 % (6/100 fichiers) |

- **8/8 hits `Io` vérifiés à la main sont vrais.** Côté codeimpact : 4 dans des fixtures de test, 1 authentique — `FileSystemCodeReader::list_rust_files` appelant `std::fs::metadata` dans la boucle du walker. **Taux de faux positifs : 0 % sur les deux corpus.**
- **Principaux contributeurs d'abstention** (ripgrep) : `write_all` ×6, `StandardImpl::write` ×5, `write` ×3, `send` ×2, `Worker::recv` ×1 — plausiblement de vraies I/O, non prouvées intra-fichier.
- **Une collision de nom observée, absorbée sans dégât** : `RwLock::write()` (un verrou, pas une I/O) → +1 au compteur agrégé, **zéro fausse alerte**. Exactement le comportement voulu : la collision atterrit dans l'abstention, jamais dans l'affirmation.
- **Décision de pruning : aucune.** Zéro faux `Io` vérifié sur 8/8, volume d'abstention plausible et faible. Les listes sont conservées telles quelles — élaguer sans défaut mesuré serait de l'élagage pour l'élagage (lean).

## Conséquences

- **(+)** `file.read_to_string(&mut s)` dans une boucle est enfin détecté quand le type est prouvé — la dette « `is_io` structurellement `false` » d'[[ADR-0013]] / [[ADR-0014]] est **fermée**.
- **(+)** L'affirmation est irréprochable : **0 % de faux positifs mesurés** sur deux corpus (8/8 vrais).
- **(+)** Le « je ne sais pas » devient comptable : un compteur d'abstention agrégé sur trois surfaces, jamais un `NotIo` fabriqué ni un pseudo-warning.
- **(−)** L'abstention laisse passer des I/O réelles non prouvées (les `write_all` de ripgrep en sont probablement) — **prix assumé** du zéro-faux-positif ; le compteur en rend le volume visible.
- **(−)** Un champ additif de plus sur chaque surface ([[ADR-0007]] respecté, rien de retiré ni renommé).

## Dette connue, explicitement non traitée

- **#73 — appartenance à la boucle du `for` : l'expression d'itérateur est comptée *dans* la boucle.** Découvert pendant la calibration T3, hors scope de #56. `Expr::ForLoop` visite l'expression d'itérateur avec `loop_depth` déjà incrémenté : dans ripgrep `gitignore.rs:412`, `rdr.lines()` — évalué **une fois**, avant l'itération — est compté comme appel en boucle. La **classification** est correcte, l'**appartenance** est fausse (faux positif de membership, pas de classification). → **Issue #73**.
- **Récepteurs `self` / accès de champ non résolus — par conception.** `self.file.read(…)` s'abstient : suivre les types des champs exigerait une résolution inter-item que la §1 refuse. L'abstention est le comportement voulu, pas un manque.
- **Résolution inter-fonction / inter-fichier hors scope** — un type qui traverse une frontière de fonction n'est pas suivi. P2, conformément à la trajectoire heuristiques-d'abord d'[[ADR-0004]].
- **Les closures héritent du `type_env` englobant** — incidemment, non spécifié : le comportement est correct sur les corpus mesurés mais n'est pas une garantie du contrat.
