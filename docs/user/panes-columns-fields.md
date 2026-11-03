# Volets, colonnes et champs personnalisés

Ce guide s'adresse à l'utilisateur de MS Project : il dit où PlannerCraft range la chronologie, le formulaire de la tâche, les colonnes et les champs personnalisés, et ce qui diffère. Ctrl désigne Cmd sur macOS. Les conventions sont celles des pages d'aide de Microsoft quand elles en parlent, des hypothèses de PlannerCraft sinon (ADR-0019) ; rien n'a été vérifié dans MS Project.

## 1. Les volets de la vue Gantt

La vue Gantt se partage, de haut en bas, entre trois volets :

| Volet | Afficher, masquer | Taille |
|---|---|---|
| **Chronologie** (frise) | Affichage → Chronologie ; clic droit n'importe où dans la frise → « Masquer la chronologie » ; ou le **×** qui apparaît en haut à droite au survol | Tirer le trait sous la frise (de 48 points à 40 % de la hauteur) ; double-clic sur le trait : hauteur par défaut |
| **Table et Gantt** | toujours là (au moins 120 points) | — |
| **Détails** (Formulaire tâche) | Affichage → Détails (case du groupe Fractionner, comme MS Project) ; **×** de son en-tête | Tirer le trait au-dessus du volet (de 120 points à 60 %) ; double-clic : hauteur par défaut |

F6 et Maj+F6 passent le clavier d'un volet à l'autre. Tailles, volets ouverts et forme du formulaire sont gardés d'une session à l'autre.

Différence avec MS Project : un double-clic sur le trait du volet de détails remet sa hauteur par défaut au lieu de le fermer (on le ferme par son ×).

## 2. Le formulaire de la tâche (volet Détails)

Le volet suit la **cellule active** : cliquer une autre ligne montre une autre tâche. En haut : Nom, Durée, Début, Fin, % achevé, Contrainte et sa date, Type de tâche, Pilotée par l'effort, Planifiée manuellement. En bas, deux petites grilles, choisies par **clic droit dans le volet** :

- Ressources et prédécesseurs (par défaut) ;
- Ressources et successeurs ;
- Prédécesseurs et successeurs.

Prédécesseurs et successeurs : N°, Nom (lu), Type (FD, DD, FF, DF), Décalage. Ressources : Nom, Unités, Travail. La dernière ligne de chaque grille, vide, sert à ajouter ; vider le N° d'une ligne retire le lien.

Comme dans MS Project, rien n'est écrit avant **OK** (ou Entrée dans un champ du haut) : tout ce qui a changé part en **une seule modification**, qu'un Ctrl+Z annule d'un coup. **Annuler** (ou Échap) relit la tâche. Passer à une autre tâche avec un formulaire modifié vaut OK ; un refus (« Il n'y a pas de tâche 99. ») vous laisse sur la tâche. Une récapitulative montre ses champs calculés grisés.

## 3. Colonnes

| Geste | Résultat |
|---|---|
| Clic droit sur un titre de colonne → **Insérer une colonne** | Liste des champs ; le champ choisi s'insère **à gauche** de la colonne cliquée |
| Clic sur **Ajouter une nouvelle colonne** (titre grisé, en bout de table) | Même liste ; la colonne s'ajoute à la fin |
| Format → Insérer une colonne, ou le bouton du groupe Colonnes de l'onglet Format | Insertion à gauche de la colonne active |
| Clic droit sur un titre → **Masquer la colonne** | La colonne disparaît (la valeur reste dans le projet) |
| Clic droit → **Paramètres de champ…** (Alt+F3) | Champ, titre, alignements du titre et des données, largeur |
| Clic sur un titre pour sélectionner la colonne, puis glisser ce titre | Déplace la colonne (un trait montre la place ; Échap annule) ; la colonne N° reste en tête |

La liste des champs s'ouvre sur une **zone de recherche** : tapez quelques lettres, sans vous soucier des accents ni des majuscules (« trav » trouve Travail, Travail réel, Travail restant…) ; ↑ / ↓ puis Entrée insère, Échap ferme. Les champs sont rangés par catégorie : Général, Planification, Avancement, Ligne de base (0 à 10), Écarts, Marges, Ressources, Coûts, Personnalisés. Un champ déjà affiché est marqué : le choisir le déplace. Les **coûts** sont listés mais grisés : PlannerCraft ne les calcule pas encore. Les champs calculés (marges, dates au plus tôt et au plus tard, écarts, lignes de base) se lisent mais ne se saisissent pas (« Ce champ est calculé. »).

Ces réglages de colonnes sont des préférences d'affichage : ils ne créent aucune entrée d'annulation et ne changent pas le fichier.

## 4. Champs personnalisés

PlannerCraft connaît les champs personnalisés simples de MS Project : **Texte1 à Texte30**, **Nombre1 à Nombre20**, **Date1 à Date10** et **Indicateur1 à Indicateur20** (catégorie Personnalisés de la liste des champs). Ils se saisissent comme les autres cellules (copier, coller, Ctrl+D compris) : un texte de 255 caractères au plus, un nombre écrit dans la langue de l'interface (« 12,5 » en français), une date comme le Début, Oui ou Non pour un indicateur. Vider la cellule efface la valeur.

**Projet → Champs personnalisés…** (ou le bouton du groupe Propriétés de l'onglet Projet) les renomme : le nouveau nom (50 caractères au plus) devient le titre de la colonne, par exemple « Lot (Texte1) ». Tous les renommages d'un OK forment une seule modification.

Noms et valeurs sont lus et écrits dans le fichier MSPDI (XML) de MS Project. Un champ défini par une **formule** ou une **table de choix** est gardé tel quel dans le fichier mais n'est pas proposé : PlannerCraft ne sait pas encore les calculer.

## 5. Messages

Les comptes rendus (un collage, par exemple) et les refus s'affichent en **notifications** en bas à droite, au-dessus de la barre d'état, sans déplacer la vue ; elles s'effacent seules (5 secondes, 8 pour un avertissement, 10 pour un refus), restent tant que le pointeur est dessus, et le × les ferme. « Détails » déplie le code technique d'un refus. Seul ce qui attend une décision (enregistrer avant de quitter, fichier illisible) prend un bandeau sous le ruban. Le bouton **Messages** de la barre d'état rouvre les 20 derniers.

Le volet des **diagnostics** n'apparaît que si le projet en a : cliquer la pastille « 2 avertissements » de la barre d'état l'ouvre sur les seuls avertissements (« Tout afficher » retire le filtre) ; la même pastille le referme.
