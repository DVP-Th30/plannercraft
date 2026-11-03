# PlannerCraft

Logiciel de planification de projet open source, équivalent de Microsoft Project Desktop pour l'usage courant, écrit en Rust pur :

- un **moteur de planification déterministe** (chemin critique, calendriers, contraintes, ressources), distribué comme un seul binaire ;
- un **serveur MCP** pour qu'un agent IA pilote le planning par des opérations métier, sans jamais calculer une date lui-même ;
- une **application** avec diagramme de Gantt éditable (egui), en natif et dans le navigateur (WebAssembly) ;
- le format **MSPDI XML** pour l'échange avec MS Project.

## État

Pré-alpha : rien d'utilisable. Le dépôt ne contient encore que le socle technique (workspace, CI, outillage).

## Documentation

- [`docs/brief.md`](docs/brief.md) — cahier des charges
- [`docs/decisions.md`](docs/decisions.md) — décisions d'architecture
- [`docs/scheduling-rules.md`](docs/scheduling-rules.md) — règles de calcul `R-xxx`
- [`docs/scorecard.md`](docs/scorecard.md) — parité mesurée avec MS Project
- [`docs/user/ms-project-export.md`](docs/user/ms-project-export.md) — échanger avec MS Project (XML et CSV) / exchanging with MS Project

Nom choisi le 2026-11-03 (anciennement `planrs`, nom provisoire) : `docs/rename.md`.

## Licence

Au choix, sous licence [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE).

Sauf mention contraire explicite, toute contribution soumise pour inclusion dans ce projet, au sens de la licence Apache 2.0, est placée sous cette double licence, sans condition supplémentaire.
