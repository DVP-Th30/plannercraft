# PlannerCraft — consignes du dépôt

Clone open source de MS Project en Rust pur, pilotable par IA (MCP). Brief complet : `docs/brief.md`. Lire au démarrage :
- `docs/brief.md` — mission, architecture, modèle, jalons (référence, ne pas modifier)
- `docs/decisions.md` — ADR courts, dont les écarts assumés par rapport au brief
- `docs/scheduling-rules.md` — règles `R-xxx` de calcul, source et statut
- `docs/scorecard.md` — parité mesurée (« implémenté » / « testé » / « vérifié contre MS Project »)

Langue : documentation, commentaires et messages en français ; identifiants de code en anglais. Commits `type(scope): description` en anglais.

## Règles non négociables

1. **Clean-room strict.** Implémenter à partir des spécifications publiques (schéma MSPDI XML de Microsoft, documentation d'aide de MS Project) et du comportement observé sur des fichiers réels. Ne jamais lire, traduire ni s'inspirer du code source de MPXJ (LGPL), ProjectLibre (CPAL), GanttProject (GPL) ou de tout autre logiciel sous licence incompatible. **Leur documentation est interdite aussi** (elle décrit le code). En cas de doute sur une règle, l'écrire comme hypothèse (règle 4) ; ne pas aller chercher la réponse dans ces projets. MPXJ peut servir d'outil externe pour convertir des `.mpp` du corpus, jamais plus.
2. **Licence** `MIT OR Apache-2.0` pour tout le code. Toute dépendance a une licence compatible, vérifiée par `cargo deny check`.
3. **Le LLM ne calcule jamais une date.** Toute date, durée ou marge vient du moteur. Le serveur MCP expose des opérations ; il ne renvoie jamais de résultat estimé.
4. **Chaque règle de calcul est tracée** dans `docs/scheduling-rules.md` : identifiant `R-xxx`, formulation, source (page Microsoft ou « hypothèse »), statut (`hypothèse` / `vérifié contre MS Project`). Chaque règle a au moins un test portant son identifiant dans son nom.
5. **Déterminisme.** Mêmes entrées, mêmes sorties, sur toutes les plateformes. Durées en minutes entières (`i64`) ; unités, pourcentages et coûts en point fixe entier (politique dans `docs/decisions.md`) ; jamais de `f32`/`f64` dans le moteur. Pas d'itération sur `HashMap`/`HashSet` dans le calcul (`BTreeMap`, `Vec` indexés). Aucune horloge système dans `plannercraft-core` : « maintenant » est injecté.
6. **Le cœur ne dépend d'aucune interface.** `plannercraft-core` ne connaît ni MCP, ni egui, ni le système de fichiers, ni l'heure.
7. **Pas de fonctionnalité sans test.** Un commit qui ajoute une règle de calcul ajoute ses tests dans le même commit.
8. **Rust uniquement.** Aucun JavaScript ni TypeScript écrit à la main. Pas de Tauri, d'Electron ni de webview : desktop en egui/eframe, web = même code compilé en WebAssembly (trunk + wasm-bindgen). Le JS de colle généré par wasm-bindgen et l'`index.html` minimal de trunk sont tolérés. Jamais de framework d'interface web comme dépendance, étape de build ou cible de packaging.
9. **Couches strictes.** Aucune crate en dessous de `plannercraft-ui-egui` ne dépend d'egui, eframe, winit ou rfd. `plannercraft-core` compile en `wasm32-unknown-unknown` (vérifié en CI).
10. **Ne jamais planter.** Prioritaire sur toute nouvelle fonctionnalité. `#![forbid(unsafe_code)]` partout ; `unwrap()`, `expect()`, `panic!`, `unreachable!`, indexation pouvant paniquer et arithmétique débordante interdits hors tests (clippy en mode strict dans la CI). Toute erreur remonte proprement et s'affiche.

## Méthode

- Code écrit par des agents (Opus 5.5) en workflow, cadrés par un cahier des charges écrit ; la session principale relit, fait tourner les checks et fusionne.
- Avant chaque jalon : plan court dans `docs/decisions.md`, choix structurants signalés au propriétaire. Règle MS Project incertaine : hypothèse consignée, on avance.
- Commits petits et testés. Jamais `git push --force` sur `main`. Chaque jalon finit par `docs/scorecard.md` à jour et un tag `jN`.
- Checks avant tout commit : `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`, `cargo check -p plannercraft-core --target wasm32-unknown-unknown`, `cargo xtask layers`, `cargo xtask panics`. Lister les checks exécutés et leur résultat dans chaque compte-rendu.
- `docs/scorecard.md` distingue « implémenté », « testé » et « vérifié contre MS Project » ; le câblage n'est pas le comportement.
- Nom définitif PlannerCraft (minuscules techniques `plannercraft`) depuis le 2026-11-03 (ADR-0020) : les endroits où il apparaît et l'historique du renommage (ancien nom provisoire `planrs`) sont dans `docs/rename.md`.
