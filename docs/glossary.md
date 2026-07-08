# Glossaire — Ubiquitous Language

> Termes du bounded context **CodeImpact**, utilisés dans le code, les docs, et les conversations.

| Terme | Nature | Définition |
|---|---|---|
| AnalysisTarget | VO | Fichier ou projet soumis à l'analyse. Porte un `path` et un `TargetType` (File / Project). |
| CodeLocation | VO | Position précise dans le code: `file_path`, `line`, `column`. Validée au constructeur (line >= 1, column >= 1). |
| CodeMetrics | VO | Résultat de l'analyse proactive: complexité cyclomatique, I/O dans boucles, profondeur, hotspots d'allocation. |
| EconomicImpact | VO | Coût estimé: CPU (μ$), mémoire (bytes), réseau (bytes), coût total. |
| EcologicalImpact | VO | Impact CO2 (g) et énergie (J) dérivé de l'impact économique via un facteur régional (gCO2/kWh). |
| EfficiencyClass | Enum | Label A–G basé sur le CO2 (A = très faible, G = extrême). |
| AnalysisRule | Enum | Règle d'analyse proactive: CyclomaticComplexity, IoInLoops, NestedDepth, AllocationHotspots. |
| AnalysisReport | DTO | Regroupe les métriques, impacts, et recommandations. Sortie du use case. |
| ProactiveAnalyzer | Domain Service | Analyse statique du code source (AST, patterns). |
| ReactiveAnalyzer | Domain Service | Analyse dynamique via exécution instrumentée des tests. |
| StressTestRun | VO | Résultat d'un run de tests avec instrumentation: durée, delta CPU/mem, tests passés/échoués. |
| CodeReaderPort | Port | Interface pour lire le code source depuis le filesystem. |
| ProfilerPort | Port | Interface pour mesurer l'impact réel (CPU/mem/IO). |
| TestRunnerPort | Port | Interface pour exécuter les tests avec instrumentation. |
| ReportWriterPort | Port | Interface pour produire le rapport (console, JSON). |