# Historique du renommage : `planrs` → PlannerCraft

Le projet s'est appelé `planrs` (nom provisoire) jusqu'au 2026-11-03, date à laquelle le propriétaire a choisi **PlannerCraft** (ADR-0020). Nom affiché : « PlannerCraft » ; nom technique, en minuscules : `plannercraft`.

## Où le nom apparaît (à tenir à jour)

- **nom affiché** : `plannercraft_core::PRODUCT_NAME` = « PlannerCraft » (`crates/plannercraft-core/src/lib.rs`), seul endroit du code qui le définit ; CLI (`--version` : `PlannerCraft <version>`), fenêtre desktop, titre de la page web, menu natif de macOS, producteur des PDF le lisent ; **nom technique** `PRODUCT_ID` = `plannercraft` (usage de la CLI, nom du serveur MCP, dossier d'état, réponse `hello` du canal) ; **ancien nom** `LEGACY_PRODUCT_ID` = `planrs`, lu seulement par les reprises ci-dessous
- crates `crates/plannercraft-*`, `apps/plannercraft`, `apps/plannercraft-cli`, `apps/plannercraft-web`, `tests/` (`plannercraft-tests`) : `[package] name`, `[workspace.dependencies]` du `Cargo.toml` racine, chemins, identifiants Rust `plannercraft_*` et `PlannerCraft*` (`PlannerCraftApp`, `PlannerCraftServer`), champs `description` et commentaires `//!`
- binaires : `plannercraft` (CLI et serveur MCP) et `plannercraft-desktop`
- `repository` du `Cargo.toml` racine (`https://github.com/DVP-Th30/plannercraft`)
- `xtask/src/layers.rs` (`GUI_ALLOWED`), `xtask/src/panics.rs` (`EXEMPT_PACKAGES`), `deny.toml`, `.gitignore`, `tests/corpus/manifest.toml`
- variables d'environnement `PLANNERCRAFT_*` (`PLANNERCRAFT_CORPUS`, `PLANNERCRAFT_UI_SNAPSHOTS`, `PLANNERCRAFT_BLESS`, `PLANNERCRAFT_CONTROL_DIR`, `PLANNERCRAFT_RECOVERY_DIR`, `PLANNERCRAFT_LOG_KEYS`, `PLANNERCRAFT_NOW`…) : code, tests, `.github/workflows/*`, docs ; les anciennes `PLANRS_*` ne sont plus lues (variables de développement et de test)
- web : `apps/plannercraft-web/index.html` (titre initial, `data-bin`, `plannercraft_canvas`, `plannercraft_loading`) et `apps/plannercraft-web/src/main.rs` (`CANVAS_ID`, attribut `data-plannercraft-started`)
- `tools/oracle/generate.bas` (module `PlannerCraftOracle`, titres, chemin d'exemple)
- `LICENSE-MIT` (« The PlannerCraft contributors »), `NOTICE`, `README.md`, `CLAUDE.md`, `docs/`, READMEs de `tests/`
- nom passé à `eframe::run_native` (`PRODUCT_NAME`) : il fixe le dossier des préférences du desktop (`eframe::storage_dir`) ; clé de l'état d'interface `plannercraft.ui_state` (`STATE_KEY`, `crates/plannercraft-ui-egui/src/state.rs` ; fichier `app.ron` sur le desktop, `localStorage` sur le web)
- répertoire d'état : `<base>/plannercraft/control` (rendez-vous du canal) et `<base>/plannercraft/recovery` (journaux de reprise), `<base>` = `$XDG_STATE_HOME` ou `~/.local/state`, `~/Library/Application Support`, `%LOCALAPPDATA%`
- instruction de style des fichiers MSPDI `<?plannercraft-style …?>` et origine `plannercraft` de l'entrée de style dans `Project.extras` (`crates/plannercraft-mspdi/src/style_pi.rs`)
- fichier compagnon `<nom>.plannercraft.json` : prévu par ADR-0005, **pas encore implémenté** (aucun code ne le lit ni ne l'écrit) ; quand il le sera, lire aussi `<nom>.planrs.json` en l'absence du nouveau
- serveur MCP : nom `plannercraft` (`Implementation` de `get_info`, `claude mcp add plannercraft -- plannercraft mcp`), instructions et `docs/features/mcp.md`
- release : `packaging/macos/Info.plist` (`PlannerCraft.app`, identifiant `io.github.dvp-th30.plannercraft`, `CFBundleName` et `CFBundleDisplayName` « PlannerCraft », exécutable `plannercraft-desktop`, icône `plannercraft.icns`), `packaging/macos/LISEZ-MOI.txt`, `packaging/windows/README.txt`, `packaging/linux/plannercraft.desktop`, icônes `assets/icon/plannercraft-*.png` ; workflows `desktop-build.yml` et `release.yml` (artefacts `plannercraft-<version>-*`, volume du DMG « PlannerCraft <version> », `--version` attendu `PlannerCraft <version>`, titre de la release) ; `CHANGELOG.md` et `docs/user/install.md`
- exemple `cargo run -p plannercraft-ui-egui --example snapshot` (fichiers `plannercraft-snapshot-<vue>.png`)

## Reprise sans perte de l'état de `planrs`

Au premier lancement de PlannerCraft, ce que `planrs` a laissé est repris, jamais écrasé :

- **préférences d'interface, desktop** (`apps/plannercraft/src/main.rs`, `adopt_legacy_preferences`) : le contenu du dossier `eframe::storage_dir("planrs")` (`app.ron`) passe dans `eframe::storage_dir("PlannerCraft")` s'il n'y est pas déjà ; test `desktop_adopts_legacy_preferences` ;
- **clé de l'état d'interface, desktop et web** (`state::stored_state`) : `plannercraft.ui_state`, à défaut `planrs.ui_state` (`LEGACY_STATE_KEY`, `localStorage` compris) ; la sauvegarde suivante écrit la nouvelle clé ; test `ui_state_legacy_key_adopted` ;
- **répertoire d'état** (`control::adopt_legacy_state_dir`, appelé au démarrage du desktop après les préférences) : chaque entrée de `<base>/planrs` (journaux de reprise `recovery`, rendez-vous `control`) passe dans `<base>/plannercraft` si elle n'y est pas ; rien si `PLANNERCRAFT_CONTROL_DIR` impose le dossier ; tests `crates/plannercraft-automation/tests/legacy_migration.rs`. Fusion entrée par entrée (`files::adopt_legacy_dir`) parce que, sur macOS (système de fichiers insensible à la casse), `Application Support/planrs` était à la fois le dossier d'eframe et le dossier d'état, et que `PlannerCraft` et `plannercraft` y sont le même dossier ; les préférences sont donc reprises d'abord ;
- **style des fichiers** : `<?planrs-style …?>` est lu comme `<?plannercraft-style …?>` (`LEGACY_TARGET`), et une entrée de style d'origine `planrs` (projet JSON d'un journal de reprise) est reconnue (`LEGACY_ORIGIN`) ; la réécriture utilise la nouvelle cible ; tests `h_r12_legacy_planrs_style_pi_read` et `h_r12_legacy_planrs_style_origin` (`crates/plannercraft-mspdi/tests/style_pi.rs`).

Limites : un échec de reprise (droits, autre volume) est signalé sur la sortie d'erreur et laisse l'ancien dossier en place ; les variables d'environnement `PLANRS_*` ne sont pas reprises ; une application `planrs` encore ouverte pendant la reprise perd son rendez-vous (relancer l'une ou l'autre).

## Ce qui garde `planrs`

L'historique, qui raconte le passé : ADR-0001 à ADR-0019 de `docs/decisions.md`, cahiers `docs/plans/`, `docs/brief.md` (référence non modifiée), sections publiées du `CHANGELOG.md` (`1.0.0-rc.1`, `0.1.0`), ce fichier ; et les constantes de reprise ci-dessus (`LEGACY_PRODUCT_ID`, `LEGACY_STATE_KEY`, `LEGACY_TARGET`, `LEGACY_ORIGIN`) avec leurs tests.
