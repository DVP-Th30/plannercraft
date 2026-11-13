# Installer PlannerCraft

Chaque release (`https://github.com/DVP-Th30/plannercraft/releases`) porte cinq fichiers : un par plateforme et `SHA256SUMS`. Chaque archive contient deux programmes :

- `plannercraft-desktop` : l'application (fenêtre) ;
- `plannercraft` : la ligne de commande et le serveur MCP (`plannercraft mcp`), pour piloter un planning depuis un agent IA.

`plannercraft --version` et `plannercraft-desktop --version` impriment `PlannerCraft <version>`.

La première version publique est la pré-version `1.0.0-rc.1` (marquée « Pre-release » sur GitHub) : `<version>` vaut alors `1.0.0-rc.1` dans les noms de fichiers ci-dessous.

Sous Windows, les binaires ne sont pas signés par un certificat. Sous macOS, le DMG est soit signé par l'éditeur (Developer ID) et notarisé par Apple, soit seulement signé « ad hoc » (préversion construite sans le certificat de l'éditeur). Windows, et macOS pour un DMG ad hoc, demandent une confirmation au premier lancement, détaillée ci-dessous.

## Vérifier le téléchargement

Placer les fichiers téléchargés et `SHA256SUMS` dans un même dossier, puis :

- macOS : `shasum -a 256 -c SHA256SUMS --ignore-missing`
- Linux : `sha256sum -c SHA256SUMS --ignore-missing`
- Windows (PowerShell) : `Get-FileHash plannercraft-<version>-windows-x86_64.zip -Algorithm SHA256`, à comparer à la ligne correspondante de `SHA256SUMS`.

Avec la CLI GitHub, la provenance (fichier construit par le workflow `release.yml` du dépôt) se vérifie par `gh attestation verify <fichier> --repo DVP-Th30/plannercraft`.

## macOS (11 Big Sur ou plus récent, Apple Silicon et Intel)

Fichier : `plannercraft-<version>-macos-universal.dmg`.

1. Ouvrir le DMG et glisser `PlannerCraft.app` dans `Applications`.
2. Premier lancement, DMG signé et notarisé : `PlannerCraft.app` s'ouvre après la confirmation habituelle d'une application téléchargée ; rien d'autre à faire.
   Premier lancement, DMG ad hoc (macOS 15 et plus récents) : ouvrir `PlannerCraft.app` ; macOS refuse, car l'application n'est pas notarisée. Ouvrir **Réglages Système → Confidentialité et sécurité**, descendre jusqu'au message sur PlannerCraft et cliquer **« Ouvrir quand même »**, puis confirmer. Les lancements suivants sont normaux. (Sur les versions plus anciennes, clic droit sur l'application puis « Ouvrir » suffit.)
3. Ligne de commande : copier `cli/plannercraft` du DMG, par exemple dans `/usr/local/bin`, puis, pour un DMG ad hoc, lever la quarantaine : `xattr -d com.apple.quarantine /usr/local/bin/plannercraft`.

Pour savoir lequel des deux on a : `codesign -dv --verbose=2 /Applications/PlannerCraft.app` affiche une ligne `Authority=Developer ID Application: …` pour un DMG signé par l'éditeur, et `Signature=adhoc` sinon. Une application signée « ad hoc » (signature locale, sans identité Apple) est intègre, mais Gatekeeper ne connaît pas son auteur ; macOS lui redemande aussi l'accès au trousseau (clé ou session de l'assistant) après chaque mise à jour, sa signature changeant à chaque build.

## Windows (10 et 11, x86_64)

Fichier : `plannercraft-<version>-windows-x86_64.zip`.

1. Extraire le zip (clic droit → « Extraire tout… »).
2. Lancer `plannercraft-desktop.exe`. Microsoft Defender SmartScreen affiche « Windows a protégé votre ordinateur » : cliquer **« Informations complémentaires »**, puis **« Exécuter quand même »**.
3. `plannercraft.exe` se lance depuis un terminal (PowerShell ou Invite de commandes).

`plannercraft-desktop.exe --version` n'affiche rien dans un terminal (programme fenêtré) : utiliser `plannercraft.exe --version`, ou `plannercraft-desktop.exe --version | more`.

## Linux (x86_64)

Fichier : `plannercraft-<version>-linux-x86_64.tar.gz`.

Prérequis :
- **glibc 2.36 ou plus récente** (Debian 12, Ubuntu 22.10, Fedora 37 et suivantes) ; `ldd --version` l'indique ;
- une session Wayland, ou X11 avec `libxkbcommon-x11` ;
- un pilote **Vulkan ou OpenGL** (paquets Mesa de la distribution) ;
- pour « Ouvrir… » et « Enregistrer sous… » : le **portail XDG** (`xdg-desktop-portal` et l'implémentation du bureau, présents sur GNOME et KDE).

Installation pour l'utilisateur courant :

```sh
tar xzf plannercraft-<version>-linux-x86_64.tar.gz
cd plannercraft-<version>-linux-x86_64
install -Dm755 plannercraft-desktop plannercraft -t ~/.local/bin
install -Dm644 plannercraft.desktop -t ~/.local/share/applications
install -Dm644 plannercraft.png ~/.local/share/icons/hicolor/256x256/apps/plannercraft.png
```

`~/.local/bin` doit être dans le `PATH` pour que le lanceur trouve `plannercraft-desktop`.

## Web

Fichier : `plannercraft-<version>-web.tar.gz` (dossier `dist` : `index.html`, `.wasm` et le JS de colle généré).

Les fichiers se servent **en HTTP** (un navigateur refuse de charger le `.wasm` depuis `file://`) :

```sh
tar xzf plannercraft-<version>-web.tar.gz
python3 -m http.server -d dist 8080
```

puis ouvrir `http://localhost:8080` dans un navigateur récent (WebGPU ou WebGL2). La version web ne relie pas `plannercraft mcp` et ne garde pas de fichiers récents.

Le planning en cours est gardé dans le navigateur (stockage local du site, jamais envoyé à un serveur) et rouvert automatiquement au retour, modifications non enregistrées comprises ; Fichier → Fermer repart d'une page vide. Un seul onglet à la fois est sauvegardé : un second onglet ouvert en même temps le dit. Dans Chrome et Edge, « Enregistrer » écrit dans le fichier ouvert ou choisi ; dans Safari et Firefox, il télécharge le fichier. Ces deux fonctions exigent une page servie en HTTPS ou depuis `localhost` : servie en HTTP sous un autre nom, la page télécharge et ne garde rien (un message le dit). Les données gardées par le navigateur s'effacent avec les données du site.
