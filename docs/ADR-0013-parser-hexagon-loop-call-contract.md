# ADR-0013 — Le contrat parser ↔ hexagone : le domaine nomme le concept, l'adaptateur nomme la syntaxe

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-14
> **Decided in:** Issue #47
> **Links:** [[architecture-overview]], [[ADR-0010]], [[ADR-0012]], [[ADR-0001]], [[glossary]]

## Contexte

`detect_quadratic_loops` levait deux alertes **CRITICAL fausses** sur `html/view_model.rs` — le fichier le plus complexe du projet, donc le premier qu'un utilisateur regarde. `aggregate` est une récursion postordre sur un arbre : la boucle itère les **enfants**, la récursion **descend**, chaque nœud est visité **une fois** — c'est du O(n), pas du O(n²).

Un faux positif CRITICAL coûte cher : il envoie un développeur refactorer du code linéaire et correct. À grande échelle, il apprend à ignorer les alertes — et le jour où une vraie boucle quadratique apparaît, plus personne ne regarde.

## Ce que l'enquête a trouvé — un champ dont le nom mentait

La cause immédiate : **le détecteur ne vérifiait jamais si l'appel était *dans* la boucle.** Il lisait `ParsedFunction.calls` — tous les appels, où qu'ils soient — et demandait « cette fonction a-t-elle une boucle quelque part, et appelle-t-elle quelque chose qui a une boucle quelque part ? ». La position lui était indifférente. C'est pourquoi `build_tree` se déclenchait alors que son appel à `sort_children` est **séquentiel, après la boucle**.

Le champ nécessaire existait déjà : `calls_in_loops`. **Mais son nom mentait.** Il prétendait contenir *les appels dans les boucles* et ne contenait que *les appels d'I/O dans les boucles* — le parseur filtrait la capture derrière `is_io_call`.

Un détecteur branché sur ce champ ne détectait donc **plus rien** : les deux faux positifs disparaissaient, mais **par accident** — un champ vide exclut tout. Un détecteur qui annonce « aucune boucle quadratique » parce que son entrée est structurellement vide fabrique un **`0` confiant, silencieux et faux** — précisément ce qu'[[ADR-0010]] proscrit, et **pire que les faux positifs qu'il remplaçait** :

> **Un faux positif est auditable. Un faux négatif est invisible.**

## Décision

### 1. Un fait, deux interprétations

**« Un appel niché dans une boucle » est UN fait. « Une I/O dans une boucle » et « une boucle quadratique » sont DEUX interprétations de ce fait.**

Le parseur rapporte le fait. Chaque détecteur l'interprète.

```rust
// hexagon/src/analysis/code_parser.rs
pub struct LoopCall {
    pub name: String,
    pub line: usize,
    pub col: usize,
    pub is_io: bool,
}
```

`ParsedFunction.calls_in_loops: Vec<LoopCall>` enregistre désormais **tout** appel à `loop_depth > 0`, quel qu'il soit. Le nom cesse de mentir.

- `io_in_loops_detector` filtre : `.filter(|c| c.is_io)` — une ligne, comportement identique à l'existant.
- `detect_quadratic_loops` itère l'ensemble, et n'exclut un appelé niché que si **cet appelé précis** est cyclique (`has_cycle`).

`is_io: bool`, **pas** une enum `CallKind` : deux détecteurs, une classification. Une enum à une seule variante utile est de la généralité spéculative.

### 2. Le précédent — où vit la classification

> **Le domaine nomme le concept. L'adaptateur nomme la syntaxe.**

- **`is_io` est du vocabulaire métier.** US5 s'intitule littéralement « Détection I/O dans boucles ». L'hexagone a tout droit à un champ `is_io`.
- **`IO_PREFIXES = ["std::fs::", "tokio::fs::", "std::net::", "reqwest::"]` est de la syntaxe Rust.** Ça ne veut rien dire pour un domaine qui doit aussi décrire du C# et du TypeScript.

Déplacer la classification dans l'hexagone exigerait une table de préfixes **par langage** (`match language { Rust => …, DotNet => … }`) : un domaine zéro-dépendance, agnostique du langage, qui se retrouverait avec `reqwest` dans son vocabulaire — plus une violation d'OCP forçant une édition de l'hexagone à chaque nouvel adaptateur. **Rejeté.**

Traduire le vocabulaire de l'infrastructure en vocabulaire du domaine **est la définition du travail d'un adaptateur** (c'est un ACL).

**Instruction pour l'auteur d'un futur adaptateur .NET / Node.js / Java :** vous implémentez `CodeParser`. Vous parcourez votre propre AST. Pour chaque appel trouvé à une profondeur de boucle > 0, vous émettez un `LoopCall` et vous répondez à la question du domaine — *cet appel est-il une I/O ?* — avec le vocabulaire de **votre** langage (`File.*`/`HttpClient` en .NET, `fs.*`/`fetch` en Node). **Vous ne touchez jamais à l'hexagone.** Si vous vous surprenez à vouloir ajouter une liste de préfixes dans `hexagon/`, vous avez mal lu cet ADR.

### 3. Le parseur doit voir les appels de méthode

Le bras `Expr::MethodCall` du parseur **ne touchait pas du tout** au suivi de boucle — seul `Expr::Call` le faisait. Or **en Rust, la plupart des appels intra-type sont des appels de méthode** (`aggregate`/`sort_children` en sont la preuve). Sans cela, le détecteur restait aveugle, une couche plus bas.

Un helper `record_call` unique, appelé depuis **les deux** bras.

### 4. `Recursion` passe de `Critical` à `Warning`

L'analyse statique **ne peut pas établir** si une récursion est bornée — la profondeur réelle dépend des données à l'exécution. Crier `CRITICAL` sur toute récursion est le même excès de zèle que celui corrigé ici pour `QuadraticLoop`. Le motif reste émis (le vrai positif sur `aggregate` est préservé) ; seule sa sévérité change. Vérifié : aucune CI ni aucun code de sortie ne s'appuie sur `Critical`.

### 5. La détection de cycles doit être un invariant de graphe

L'ancienne `detect_cycles` utilisait `HashMap::keys()` — **dont l'ordre est randomisé par processus** — comme ensemble de racines de sa DFS. Sur un graphe de confluence (`a→b, a→c, b→d, c→d, d→a`), elle **perdait silencieusement un membre de cycle 21 fois sur 40**.

Ce n'était pas un test instable : `has_cycle` conditionne l'exclusion du détecteur quadratique, donc un cycle manqué produit un **verdict faux**. Remplacée par **Tarjan (SCC)**, dont la décomposition est un invariant de graphe, indépendant de l'ordre de parcours.

## Conséquences

- **(+)** `QuadraticLoop` détecte enfin ce qu'il prétend détecter : 0 faux positif sur notre code, 1 vrai positif sur une vraie boucle quadratique.
- **(+)** Le contrat parseur ↔ hexagone survit au multi-langage : un adaptateur .NET/Node/Java répond à la question du domaine sans jamais y injecter sa syntaxe.
- **(+)** La détection de cycles est déterministe.
- **(−)** Les sévérités publiées changent : une `Recursion` n'est plus `Critical`.

## Dette connue, explicitement non traitée

- **`is_io` est structurellement toujours `false` pour un appel de méthode.** Le nom enregistré est l'identifiant nu (`read_to_string`), qui ne peut jamais commencer par `std::fs::`. Donc `file.read_to_string(&mut s)` dans une boucle **passe inaperçu** — et en Rust idiomatique, l'I/O est massivement en forme de méthode. **US5 ne détecte qu'une minorité des I/O réelles.** Défaut **préexistant** (avant #47, `MethodCall` n'atteignait même pas `calls_in_loops`), donc pas une régression — mais c'est le même zéro confiant qu'[[ADR-0010]] combat. → **Issue #56**.
- Les trois parcours du graphe d'appel (`dfs_reachable`, `tarjan_scc`, `compute_depth`) sont **récursifs** : débordement de pile vers 10-20k de profondeur linéaire. → **Issue #52**.
