# Cahier des charges « finitions avant v1.0.0 » — PlannerCraft

Référence : ADR-0022 (`docs/decisions.md`). Demande du propriétaire (2026-11-05) après comparaison des onglets Tâche, Projet et Affichage avec des captures de MS Project 2013 : **pas de clonage**, seulement les manques qui servent vraiment (Pareto) à quelqu'un qui fait quelques plannings de chantier par an, et un **lifting du ruban** (« pas cheap, pas LibreOffice 2010 »).

Lots **séquentiels** (un seul agent à la fois : machine à mémoire partagée), chacun sur sa branche depuis `main` à jour, sa PR, fusionnée avant le lot suivant. Chaque lot : commits `type(scope): …` testés, fiche `H-UI-xx` / `R-xxx` et tests **dans le même commit**, messages terminés par `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Règles communes

- Règles du `CLAUDE.md` du dépôt (clean-room, ne jamais planter, le LLM ne calcule jamais une date, couches, déterminisme). Conventions MS Project tirées des seules pages d'aide Microsoft relues (titre exact, phrase citée) ; sinon `hypothèse`.
- Ruban (ADR-0017) : toute commande nouvelle a son jumeau au menu (`h_ui_70_every_item_has_menu_twin`), une action, un prédicat ; **un geste = une entrée d'annulation** ; un geste sans effet = aucune entrée, aucun message ; refus en langage d'utilisateur (H-UI-105 : chaque code nouveau a sa phrase FR et EN, espace insécable avant « : ; ? ! »).
- Toute écriture passe par le registre de commandes (même chemin pour l'interface, la CLI et le MCP) ; outil MCP quand la fonction a un sens pour un agent.
- **Mémoire (serveurdvp, pot de 6 Go partagé par 3 sessions)** : `export PATH=$HOME/.cargo/bin:$PATH` ; avant chaque commande cargo lourde `ramstatus` puis `ramwait 3` ; lancer par `memlimit 4 cargo …` ; **une seule commande cargo à la fois, jamais en tâche de fond** ; pas de `cargo test --workspace` ni de suite complète de `plannercraft-ui-egui` en local : tests ciblés par filtre (`cargo test -p plannercraft-ui-egui --test ui <filtre>`, `--lib <filtre>`), la CI de la PR passe la suite complète sur Linux, macOS et Windows. `CARGO_TARGET_DIR` propre au worktree.
- Checks locaux de chaque lot : `cargo fmt --check`, `cargo clippy -p <crates touchées> --all-targets -- -D warnings`, tests ciblés, `cargo xtask panics`, `layers`, `rules`, `text-ids`, `icons --check`, `cargo check -p plannercraft-core --target wasm32-unknown-unknown` si le cœur est touché.
- Captures (`tests/snapshots/*.png`) : régénérées seulement par le lot qui change le rendu, relues image par image ; aucune capture Microsoft dans le dépôt.

## Lots

| Lot | Contenu | Décision |
|---|---|---|
| **P1** | Décaler des tâches ; colonne Indicateurs ; bouton Inspecter ; Zoom sur les tâches sélectionnées ; deux libellés | D1, D2, D5 |
| **P2** | Mise en forme du texte des lignes (gras, italique, couleur, surlignage, pinceau, effacer) | D3 |
| **P3** | Regrouper par un champ | D4 |
| **P4** | Lifting du ruban et de la chronologie, captures | D6 |

### P1 — Décaler, Indicateurs, petits gains

- **Décaler** (D1) : commande `task.shift { tasks, by }` (`by` : durée signée en jours ouvrés, `DurationJson`), MCP `shift_tasks`. Règle `R-xxx` (hypothèse) : tâche automatique ⇒ contrainte « Début au plus tôt le » (SNET) à la date de début actuelle décalée de `by` sur le calendrier de la tâche (une contrainte autre que « Dès que possible » / SNET est remplacée, remarque dans le compte rendu) ; tâche manuelle ⇒ début et fin décalés, durée gardée ; récapitulative sélectionnée ⇒ ses feuilles ; tâche inactive ou achevée ⇒ ignorée avec remarque ; décalage vers l'arrière bloqué par un prédécesseur ⇒ effectué quand même (contrainte posée) et remarque « limitée par ses prédécesseurs ». Une annulation pour tout le lot. Ruban : Tâche → Planification → **Décaler ▾** (+1 jour, +1 semaine, +4 semaines, −1 jour, −1 semaine, Personnalisé… avec saisie d'une durée lue par `plannercraft-core`), jumeau au menu Tâche. Tests de la règle dans `plannercraft-automation` (et le cœur si un calcul y vit).
- **Indicateurs** (D2) : colonne `Indicateurs` (titre ⓘ, 28 px, non éditable) insérable par le catalogue de champs et **présente par défaut** après N° dans le Gantt des nouveaux états d'interface (états mémorisés existants : migration qui l'ajoute une fois). Icônes, dans cet ordre : note, contrainte autre que « Dès que possible » / « Au plus tard possible » selon l'ancrage, échéance dépassée, tâche manuelle, tâche inactive, achevée (100 %). Infobulle : une ligne par indicateur, en langage d'utilisateur, avec la date de la contrainte ou de l'échéance. Clic sur l'icône de note ⇒ ouvre les notes de la tâche. Icônes Lucide ajoutées par `cargo xtask icons` (liste vendorisée) ; lecture seule du modèle et du `Schedule` (règle 3). Mode impression : colonne non proposée.
- **Inspecter** (D5) : bouton Tâche → Tâches → **Inspecter** (et menu Tâche) : ouvre l'onglet « Pourquoi cette date ? » sur la tâche active (volet déjà existant).
- **Zoom → Tâches sélectionnées** (D5) : Affichage → Zoom, ajuste l'échelle pour que les barres de la sélection tiennent dans la largeur du Gantt et y défile.
- **Libellés** (D5) : « Défiler jusqu'à la tâche » ⇒ **« Atteindre la tâche »** ; « Définir la référence » ⇒ **« Définir le planning de référence »** (FR seulement ; menus et ruban ; `docs/user/ribbon.md` à jour).

### P2 — Mise en forme du texte des lignes (D3)

- Par tâche : gras, italique, couleur du texte, couleur de surlignage de la ligne (fond des cellules de la table ; le Gantt n'est pas touché). Palette : celle du volet de mise en forme des barres (contraste contrôlé sur le fond du thème et à l'impression, proposition de teinte corrigée comme pour les barres).
- Stockage : style du document (même instruction de traitement que les autres réglages de style, écrite seulement si elle a été choisie ; fichier sans style relu et écrit octet pour octet comme avant). Pas de champ MSPDI (le format XML ne porte pas la mise en forme de texte par tâche).
- Commande `task.set_text_style { tasks, bold?, italic?, color?, fill? }` (`Patch` : absent = inchangé, `null` = retiré), MCP `set_task_text_style` ; une annulation par geste.
- Ruban : Tâche, groupe **Texte** (après Presse-papiers) : **G** (Ctrl/Cmd+B), **I** (Ctrl/Cmd+I), Couleur du texte ▾, Surlignage ▾, **Reproduire la mise en forme** (pinceau : copie le style de la tâche active, un clic l'applique à la sélection suivante ; Échap annule), **Effacer la mise en forme**. Boutons G et I enfoncés selon la tâche active. Jumeaux au menu Format. Vérifier que Ctrl+B / Ctrl+I ne sont pas déjà pris (`keymap.rs`).
- Rendu : table à l'écran, aperçu, PDF et SVG ; thème sombre (couleur gardée si contrastée, sinon variante claire de la même teinte).

### P3 — Regrouper par (D4)

- Affichage → Données → **Regrouper par ▾** : Aucun groupe ; champs personnalisés texte renommés ou non vides (Texte1 à Texte30, affichés sous leur nom : « Lot ») ; Ressources (nom de la ressource ; une tâche à plusieurs ressources apparaît sous chacune) ; Critique / non critique ; Jalons. Jumeau au menu Affichage.
- Affichage groupé : ligne d'en-tête par valeur (valeur ou « (sans valeur) », nombre de tâches, début et fin du groupe lus du `Schedule`, barre récapitulative de groupe dans le Gantt), tâches feuilles du groupe en ordre des ID, récapitulatives masquées ; groupes repliables ; ordre des groupes : texte (ordre naturel, « (sans valeur) » en dernier), Critique avant non critique.
- Pendant le groupement : saisie dans les cellules permise ; gestes de structure (insérer, indenter, déplacer des lignes, couper/coller des lignes) grisés avec la raison « Retirez le regroupement », comme sous tri ou filtre. Le filtre s'applique avant le groupement.
- Mémorisé avec le document (style), imprimé tel quel (aperçu, PDF, SVG). Rien dans le MSPDI.
- MCP : `render_gantt` et `export_pdf` suivent le style du document ; `get_style` l'expose.

### P4 — Lifting du ruban (D6)

Constat : ruban plat gris, onglet actif en pilule bleu clair, séparateurs et libellés de groupe ternes, icônes toutes au même gris, chronologie bleu pâle sans lien avec la marque ; impression « suite bureautique libre de 2010 ».

Direction : sobre et contemporain, la marque (violet `#7719AA`, ADR-0021) en **accent seulement** ; jamais de dégradé, d'ombre portée lourde ni d'arrondi partout.
- **Onglets** : texte seul, onglet actif en violet avec un trait de 2 px sous le libellé (pas de pilule) ; survol : fond très léger.
- **Surface** : ruban sur la couleur de fond de la fenêtre (pas de bloc gris), un filet fin en bas ; barre d'accès rapide alignée sur les onglets, précédée du petit symbole de l'app (bloc violet + carré blanc, dessiné, pas une image).
- **Boutons** : coins 6 px, survol et appui en fond teinté ; bouton enfoncé (bascule active) en fond violet très clair et icône violette ; grisé lisible (pas un gris délavé illisible).
- **Icônes** : grandes 24 px, petites 16 px, trait un peu plus fin qu'aujourd'hui ; à la couleur du texte, violet pour les bascules actives.
- **Groupes** : séparateurs verticaux courts (marge haute et basse), libellé de groupe petit, gris moyen, centré ; grille d'espacement de 4 px, hauteurs identiques entre onglets.
- **Chronologie** : fond neutre, barres et losanges dans l'accent, trait de date d'état violet, cadre de la partie visible en violet transparent.
- Clair et sombre conçus tous les deux (accent `#B57BE0` environ en sombre, contraste ≥ 4,5:1 pour le texte, ≥ 3:1 pour les icônes), jetons dans `theme.rs` ; rien codé en dur ailleurs.
- Captures régénérées et relues ; avant/après montré au propriétaire (session principale) avant fusion.

## Hors périmètre (ADR-0022)

Groupe Police complet (police, taille), Avancement conforme, Respecter les liaisons, mode des nouvelles tâches, vue Utilisation des tâches, Planificateur d'équipe, Tables ▾, ligne 0 du projet, Mettre en surbrillance, sous-projets et liaisons inter-projets, Réseau, Calendrier, fenêtres multiples, orthographe, Livrable, durée estimée « ? ».
