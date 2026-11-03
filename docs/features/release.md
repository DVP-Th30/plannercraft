# Publication des versions (J8, ADR-0016 D11)

Contrats posés par le lot L0 ; pipeline `.github/workflows/release.yml` et `cargo xtask release-check` écrits par le lot L1 (« à blanc » dès la vague 2), porte du web en L13, vérification finale en L16.

## 1. Artefacts

Une release `vX.Y.Z[-pre]` porte exactement cinq fichiers (C-REL-08) :

| Fichier | Contenu |
|---|---|
| `plannercraft-<v>-macos-universal.dmg` | `PlannerCraft.app` universel (x86_64 + arm64, signé ad hoc) avec `plannercraft-desktop` ; la CLI `plannercraft` (CLI et serveur MCP) dans `cli/plannercraft` du DMG |
| `plannercraft-<v>-windows-x86_64.zip` | `plannercraft-desktop.exe` (sous-système fenêtré), `plannercraft.exe` (console), `README.txt` ; non signé (SmartScreen) |
| `plannercraft-<v>-linux-x86_64.tar.gz` | `plannercraft-desktop`, `plannercraft`, `plannercraft.desktop`, icône ; construit en conteneur `debian:bookworm` (glibc ≤ 2.36) |
| `plannercraft-<v>-web.tar.gz` | `dist` de trunk (`index.html`, `.wasm`, JS de colle généré par wasm-bindgen) |
| `SHA256SUMS` | Empreintes SHA-256 des quatre fichiers ci-dessus |

Chacune des quatre archives porte aussi un dossier `licenses/` (ADR-0017, lot L6) : `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, et les licences des composants tiers embarqués aux chemins que cite `NOTICE` (`assets/fonts/OFL.txt` pour Inter, `assets/icons/lucide/LICENSE` pour Lucide, ISC et MIT de Feather). Les jobs `verify-*` échouent si l'un manque ou est vide.

Tags `v*-test.*` ⇒ brouillon (non public) ; un tag d'essai `vX.Y.Z-test.N` éprouve la version du workspace qu'il prépare (même noyau `X.Y.Z`, par exemple `v1.0.0-test.1` pour `1.0.0-rc.1` : C-REL-01, notes et noms des artefacts sont ceux de cette version, lot L16) ; autres pré-versions ⇒ pré-version ; jetons `contents: write`, `id-token: write`, `attestations: write` au seul job `publish`.

## 2. Contrats (C-REL)

| Id | Contrat | Lot |
|---|---|---|
| C-REL-01 | Tag `vX.Y.Z[-pre]` = `version` du workspace = première section de `CHANGELOG.md` | L1 |
| C-REL-02 | macOS : `lipo -archs` des deux binaires = « x86_64 arm64 » ; `codesign --verify --deep --strict` vert ; `hdiutil verify` vert | L1 |
| C-REL-03 | `plannercraft --version` et `plannercraft-desktop --version` impriment `PlannerCraft <version>` sans fenêtre, sur les 3 OS | L1 |
| C-REL-04 | Linux : plus haute version `GLIBC_` requise (`objdump -T`) ≤ 2.36 | L1 |
| C-REL-05 | Web : `.wasm` compressé gzip -9 ≤ 4 Mio (3 Mio jusqu'au 2026-10-27, ADR-0016) | L1 (mesure), L13 (porte) |
| C-REL-06 | Web : `chromium --headless` sur `dist` servi en HTTP ⇒ attribut `data-plannercraft-started="1"` dans le DOM (posé par Rust au démarrage) | L1, L13 |
| C-REL-07 | `sha256sum -c SHA256SUMS` vert sur les artefacts téléchargés de la release | L1 |
| C-REL-08 | `gh release view --json assets` liste exactement les cinq fichiers du § 1 ; `gh attestation verify` vert sur l'un d'eux | L1 |

## 3. Budget du web

Mesure du lot L0 (sonde `cdylib` wasm32, `opt-level = "s"`, LTO) : krilla seule 509 Kio gzip -9, `raster-images` + 73 Kio ; polices Inter du dépôt 2 × 87 Kio (non compressées). Le budget de C-REL-05 décide du logo du PDF (L7).

Mesure de la release d'essai (2026-10-27, `v1.0.0-test.1`) : 9 897 306 octets, 3 783 052 gzip -9, au-delà des 3 Mio. Après SIMD 128 (`.cargo/config.toml`) : 8 534 454 octets, 3 518 979 gzip -9 ; budget relevé à 4 Mio (détail et options écartées : ADR-0016, budget du web).

## 4. Pipeline (lot L1)

`.github/workflows/release.yml`, sur tag `v*` ou lancement manuel :

| Job | Rôle | Contrats |
|---|---|---|
| `check` | `cargo xtask release-check <tag>` puis `cargo xtask release-notes <tag>` (tag seulement) | C-REL-01 |
| `build` | appelle `desktop-build.yml` (`workflow_call`) : `build-macos` (deux cibles, `lipo`, `PlannerCraft.app` avec `packaging/macos/Info.plist`, `.icns` par `iconutil` depuis `assets/icon`, `codesign --force --deep -s -`, DMG UDZO avec `PlannerCraft.app`, `cli/plannercraft`, `LISEZ-MOI.txt`), `build-windows` (`x86_64-pc-windows-msvc`, zip), `build-linux` (conteneur `debian:bookworm`, rustup installé dans le job), `build-web` (trunk 0.21.14) | — |
| `verify-macos` | `hdiutil verify`, `codesign --verify --deep --strict`, `lipo -archs`, `--version` des deux binaires (tranche x86_64 aussi si Rosetta est présent) | C-REL-02, C-REL-03 |
| `verify-windows` | contenu du zip, `--version` des deux `.exe` (sortie redirigée) | C-REL-03 |
| `verify-linux` | dans `debian:bookworm` : `objdump -T` (plus haute `GLIBC_` ≤ 2.36), `--version`, `desktop-file-validate` | C-REL-03, C-REL-04 |
| `verify-web` | taille du `.wasm` gzip -9 publiée dans le résumé du run, **échec** au-delà de 4 Mio (porte depuis le lot L13, relevée de 3 à 4 Mio le 2026-10-27) ; `dist` servi en HTTP, Chromium headless, `data-plannercraft-started="1"` cherché dans le DOM | C-REL-05, C-REL-06 |
| `publish` | tag `v*` seulement : `SHA256SUMS`, `actions/attest-build-provenance`, `gh release create --verify-tag --notes-file` (`--draft` si `-test.`, `--prerelease` si `-`), puis contrôle de la release publiée : téléchargement, `sha256sum -c`, liste exacte des cinq fichiers, `gh attestation verify` | C-REL-07, C-REL-08 |

Le contrôle après publication est fait dans le job `publish` et non dans un job séparé : un brouillon n'est visible que d'un jeton en écriture, et ce job est le seul à en avoir un. Aucun secret n'est ajouté (seul `github.token`).

`desktop-build.yml` est la seule définition des builds : appelé par `release.yml`, ou lancé seul (tag `ui-*`, lancement manuel) pour une préversion, sans vérification ni publication (artefacts d'Actions).

Marqueur du web : `apps/plannercraft-web` pose `data-plannercraft-started="1"` sur `<html>` dès que le wasm s'exécute, **avant** l'initialisation graphique (le runner n'a pas de GPU) : C-REL-06 prouve que le wasm a tourné, pas que l'interface s'affiche (message « GPU indisponible » : H-UI-57, lot L13).

Icône : `cargo xtask icon` (PNG 16 à 1 024 px dans `assets/icon/`, dessin déterministe, test de fraîcheur `icon_files_fresh`), choisie par le propriétaire (ADR-0021) ; même PNG (256 px) en icône de fenêtre du desktop, favicon SVG inline pour le web. Le `.exe` Windows ne porte pas d'icône dans l'Explorateur (ressource Windows non embarquée).

## 5. Publier une version

1. Version : `[workspace.package] version` du `Cargo.toml` racine **et** les exigences `version = "…"` des crates internes de `[workspace.dependencies]` (Cargo refuse une exigence `0.1.0` face à une pré-version `0.1.0-test.1`) ; `Cargo.lock` régénéré.
2. `CHANGELOG.md` : contenu de « Non publié » déplacé sous `## [X.Y.Z[-pré]] - AAAA-MM-JJ`, « Non publié » laissé vide en tête.
3. `cargo xtask release-check vX.Y.Z[-pré]` vert en local, commit, puis tag `vX.Y.Z[-pré]` poussé sur ce commit.
4. Tag de test `vX.Y.Z-test.N` sur le commit de la version (aucun changement de version : `release-check` le rapporte à la version du workspace de même noyau) : brouillon non public, vérifié de bout en bout, puis supprimé (`gh release delete <tag> --cleanup-tag`).

Limite : sous Windows, `plannercraft-desktop.exe --version` n'imprime que si sa sortie est redirigée (sous-système fenêtré ; attacher la console du parent demanderait du code `unsafe`, interdit par la règle 10).
