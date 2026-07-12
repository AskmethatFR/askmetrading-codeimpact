# Glossaire — Ubiquitous Language

> Termes du bounded context **CodeImpact**, utilisés dans le code, les docs, et les conversations.

| Terme | Nature | Définition |
|---|---|---|
| AnalysisTarget | VO | Fichier ou projet soumis à l'analyse. Porte un `path` et un `TargetType` (File / Project). |
| CodeLocation | VO | Position précise dans le code: `file_path`, `line`, `column`. Validée au constructeur (line >= 1, column >= 1). |
| CodeMetrics | VO | Résultat de l'analyse proactive: complexité cyclomatique, complexité transitive, profondeur d'appel, warnings, impact économique. |
| EconomicImpact | VO | Coût estimé: CPU (μ$), mémoire (bytes), coût total (μ$), niveau (low/moderate/high/critical). |
| EconomicImpactEstimator | Domain Service | Calcule EconomicImpact à partir de CodeMetrics, ParsedFunction, et CallGraph via formules heuristiques. |
| MicroDollars | Unité | 1 μ$ = 10⁻⁶ $. Unité de coût CPU basée sur le pricing cloud (~$0.10/CPU-heure). 1 μ$ ≈ 10⁷ cycles CPU. |
| EcologicalImpact | VO | Impact CO2 (g) et énergie (J) dérivé de l'impact économique via un facteur régional (gCO2/kWh). |
| EfficiencyClass | Enum | Label A–G basé sur le CO2 (A = très faible, G = extrême). |
| AnalysisRule | Enum | Règle d'analyse proactive: CyclomaticComplexity, IoInLoops, NestedDepth, AllocationHotspots. |
| AnalysisReport | DTO | Regroupe les métriques, impacts, et recommandations. Sortie du use case. |
| ProactiveAnalyzer | Domain Service | Analyse statique du code source (AST, patterns). Orchestre l'estimation d'impact économique. |
| ReactiveAnalyzer | Domain Service | Analyse dynamique via exécution instrumentée des tests. |
| StressTestRun | VO | Résultat d'un run de tests avec instrumentation: durée, delta CPU/mem, tests passés/échoués. |
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
| HiddenComplexity | Concept | Différence entre complexité transitive et directe. Complexité qui "se cache" dans les appels. |
