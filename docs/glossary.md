# Glossaire — Ubiquitous Language

> Termes du bounded context **CodeImpact**, utilisés dans le code, les docs, et les conversations.

| Terme | Nature | Définition |
|---|---|---|
| AnalysisTarget | VO | Fichier ou projet soumis à l'analyse. Porte un `path` et un `TargetType` (File / Project). |
| CodeLocation | VO | Position précise dans le code: `file_path`, `line`, `column`. Validée au constructeur (line >= 1, column >= 1). |
| CodeMetrics | VO | Résultat de l'analyse proactive: complexité cyclomatique, complexité transitive, complexité cachée (somme additive des `hidden` par fonction — voir [[ADR-0012]]), profondeur d'appel, warnings, impact économique. |
| FunctionDetail | VO | Métriques d'**une** fonction. Champs **privés**, construit par `new()`. Stocke `direct` et `hidden` ; **dérive** `transitive() = direct + hidden`. L'état illégal (`transitive < direct`) est donc **inconstructible**, y compris depuis un futur adaptateur FFI — impossibilité structurelle plutôt que garde-fou runtime. Voir [[ADR-0012]]. |
| EconomicImpact | VO | Coût estimé: CPU (μ$), mémoire (bytes), coût total (μ$), niveau (low/moderate/high/critical). |
| EconomicImpactEstimator | Domain Service | Calcule EconomicImpact à partir de CodeMetrics, ParsedFunction, et CallGraph via formules heuristiques. |
| MicroDollars | Unité | 1 μ$ = 10⁻⁶ $. Unité de coût CPU basée sur le pricing cloud (~$0.10/CPU-heure). 1 μ$ ≈ 10⁷ cycles CPU. |
| EcologicalImpact | VO | Impact CO2 (g) et énergie (J) dérivé de l'impact économique via un facteur régional (gCO2/kWh). |
| EfficiencyClass | Enum | Label A–G basé sur le CO2 (A = très faible, G = extrême). |
| AnalysisRule | Enum | Règle d'analyse proactive: CyclomaticComplexity, IoInLoops, NestedDepth, AllocationHotspots. |
| AnalysisReport | DTO | Regroupe les métriques, impacts, et recommandations. Sortie du use case. |
| ProactiveAnalyzer | Domain Service | Analyse statique du code source (AST, patterns). Orchestre l'estimation d'impact économique. |
| ReactiveAnalyzer | Domain Service | Analyse dynamique via exécution instrumentée des tests. |
| StressTestRun | VO | Résultat d'un run de tests instrumenté: durée, CPU, mémoire (ces deux-là en `Measurement`), tests passés/total, filtre. Sur un workspace, c'est l'**agrégat** des N binaires de test (voir Agrégation). |
| Measurement | VO | `Available(T)` ou `Unmeasurable(raison)`. Type somme qui rend **structurellement impossible** de confondre « mesuré à 0 » et « pas su mesurer » — un `Available(0)` honnête reste légitime (voir [[ADR-0010]]). |
| UnmeasurableReason | Enum | Pourquoi une mesure manque. `NoSampler`: pas de sondeur (`/usr/bin/time`) sur l'hôte. `NoTestsExecuted`: aucun test n'a tourné — un run qui n'a rien exercé n'a aucun coût honnête à rapporter, si bien échantillonné que soit son processus (voir [[ADR-0011]]). |
| Agrégation | Loi de domaine | Pliage des N `StressTestRun` d'un workspace en un seul (`StressTestRun::aggregate`). Durée et tests: **somme**. CPU: somme, mais `Unmeasurable` si **un seul** run l'est. Mémoire: **max** — un pic RSS de processus qui ne coexistent jamais n'est pas une somme. Voir [[ADR-0011]]. |
| Ligne de résumé | Concept | La ligne `test result:` qu'un binaire libtest imprime **toujours** s'il va au bout — succès, échec, ou zéro test. Son **absence** est le discriminant d'un binaire **crashé**. Le code de sortie ne l'est pas: un binaire dont des tests échouent sort en non-zéro sur son chemin **nominal**, et les tests rouges doivent rester mesurables (voir [[ADR-0011]]). |
| CodeReaderPort | Port | Interface pour lire le code source depuis le filesystem. |
| ProfilerPort | Port | Interface pour mesurer l'impact réel (CPU/mem/IO). L'implémentation P0 utilise des heuristiques. |
| TestRunnerPort | Port | Interface pour exécuter les tests avec instrumentation. |
| ReportWriterPort | Port | Interface pour produire le rapport (console, JSON, HTML). Méthode `write_html` retourne le document HTML en `String`; l'écriture fichier vit dans les primaries. |
| HtmlReport | Concept | Rapport visuel self-contained: un seul `.html` (CSS/JS/data JSON inline, fonts système, zéro asset externe) ouvrable en `file://`. Produit par `HtmlReportWriter`. Vue projet (liste fichiers) en T1; vue node détail en T2. |
| ImpactScore | Concept | Score d'impact affiché dans le rapport HTML = `transitive_complexity()`. Heuristique de présentation, PAS une métrique domaine (voir [[html-report]] ADR-8.8). Barres normalisées au max du projet. |
| DataIsland | Concept | Bloc `<script id="ci-data" type="application/json">` portant le view-model sérialisé. Échappé via `json_island_escape` (breakout `</script>`); rendu côté client par `textContent`/`createElement` (défense XSS structurelle). |
| CallGraph | VO | Graphe d'appels entre fonctions, dérivé du parsing AST. Utilisé pour la complexité transitive et la profondeur. |
| CyclomaticComplexity | Concept | Nombre de chemins linéairement indépendants dans le code. Mesure statique de complexité structurelle. |
| TransitiveComplexity | Concept | Somme des complexités de toutes les fonctions appelées (directement ou indirectement). |
| HiddenComplexity | Concept | Complexité atteignable **à travers les appels** d'une fonction : `hidden(f) = Σ direct(g)` pour chaque `g` du **sous-graphe atteignable** depuis `f`, chaque fonction distincte comptée **une seule fois** (lire `g` deux fois n'est pas deux fois le travail). Toujours ≥ 0 par construction. **C'est une propriété de la FONCTION**, qui s'agrège **additivement** : `hidden(fichier) = Σ_f hidden(f)`, `hidden(projet) = Σ_fichiers hidden(fichier)`. Elle ne se calcule **jamais** en soustrayant deux agrégats (`ΣT − ΣC`) : `C` (fichier) et `T` (fonction) ne sont pas dans la même unité, et la soustraction exige un clamp qui masque les violations d'invariant. Voir [[ADR-0012]]. |
| TransitiveComplexity | Concept | Coût de **compréhension** d'une fonction : `transitive(f) = direct(f) + hidden(f)`. Dérivée, jamais stockée — d'où `transitive ≥ direct` comme vérité arithmétique, sans garde-fou runtime. Bornée par la somme des complexités directes du fichier, donc non débordable par construction. Ce n'est **pas** une somme sur les chemins d'exécution (c'était le défaut corrigé par [[ADR-0012]]). |
| ProjectMetrics | VO | **Unique source de vérité** des nombres de niveau-rapport (fichiers, complexités directe/transitive/cachée, profondeur, cycles, warnings, critiques, I/O-en-boucle, hotspots, impacts). Calculée une seule fois par `FileConsumptionGraph::aggregated_metrics()`. Les writers console, JSON et HTML la **rendent** — aucun n'a le droit de recalculer un agrégat. Voir [[ADR-0012]]. |
| ComplexityWarning | VO | Avertissement de complexité **porteur d'une sévérité** (`Critical` / `Warning`). Compté par la tuile `Warnings`. |
| IoInLoopWarning | VO | I/O détectée dans une boucle. **N'a PAS de sévérité** — l'agréger à un décompte de « critiques » est un non-sens (cause du défaut #49). Classe distincte, tuile distincte. Voir [[ADR-0012]]. |
