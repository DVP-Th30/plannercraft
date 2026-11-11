# Le ruban et la chronologie : où est… ?

Guide pour l'utilisateur occasionnel de MS Project qui ouvre PlannerCraft. Référence technique : ADR-0017 (`docs/decisions.md`) et `docs/features/ui.md` § 1.32 et § 1.33.

## 1. Ce qui ressemble à MS Project

- **Barre du haut** (depuis l'interface adoucie, ADR-0026) : le symbole de PlannerCraft, le bouton **Menu** (☰ : Fichier, Édition, Affichage, Tâche, Projet, Format, Aide), Enregistrer, Annuler, Rétablir, puis les onglets ; à droite, le champ **Rechercher une commande…** (palette de commandes, voir plus bas) et le bouton **Assistant IA**.
- **Cinq onglets** dans la barre du haut, dans l'ordre de MS Project : **Tâche, Ressource, Projet, Affichage, Format** (en anglais : Task, Resource, Project, View, Format ; page Microsoft « Learn the Project 2010 Ribbon »). L'onglet Tâche est ouvert au premier lancement, puis PlannerCraft rouvre le dernier choisi.
- **Groupes nommés** sous chaque onglet (Presse-papiers, Planification, Insérer, Propriétés, Édition…), grands boutons à icône et libellé, petits boutons empilés, listes déroulantes (▾), cases à cocher.
- **Info-bulles** : survoler un bouton affiche son nom, une phrase d'aide et le raccourci clavier (« Lier les tâches (Ctrl+F2) ») ; un bouton grisé dit pourquoi (« Sélectionnez au moins une tâche », « Retirez le tri ou le filtre »).
- **Barre d'accès rapide** à gauche des onglets, après le symbole de PlannerCraft et le bouton Menu : Enregistrer, Annuler, Rétablir.
- **Ruban compact** (par défaut) : une seule ligne avec les commandes courantes ; le reste de chaque groupe est dans son menu **…** (rien n'est perdu), et l'avancement 0 % à 100 % est un groupe de cinq boutons accolés. Le **chevron** à droite du ruban affiche le **ruban complet** (grands boutons, noms des groupes) ; le même chevron revient au compact. Le choix est retrouvé au lancement suivant.
- **Apparence** : le ruban est une carte aux coins arrondis sur le fond de la fenêtre, l'onglet ouvert est en pastille claire et en gras ; un bouton enfoncé (Gras, Planifier manuellement, Sur la chronologie…) a un fond violet très clair et une icône violette ; un bouton grisé reste lisible. Clair et sombre suivent le thème choisi (Affichage → Thème). Avec les modèles Classique et Sobre, la ligne sélectionnée et le cadre de la cellule active sont violets eux aussi (les autres modèles gardent leurs couleurs).
- **Ruban repliable** : double-clic sur un onglet, ou **Ctrl+F1** (Cmd+F1 sur macOS), ou Affichage → Réduire le ruban. Replié, un clic sur un onglet ouvre sa ligne compacte par-dessus la vue ; Échap ou un clic ailleurs la referme. Déplié de nouveau, le ruban est compact.
- **Chronologie** : frise au-dessus du diagramme de Gantt (page Microsoft « Create a timeline in Project »), affichée par défaut.

Les libellés français des onglets et des groupes ont été choisis pour ce guide ; ils n'ont pas été comparés à l'interface française de MS Project.

Les **menus** (Fichier, Édition, Affichage, Tâche, Projet, Format, Aide) restent, dans le bouton **Menu** de la barre du haut : chaque bouton du ruban y a son entrée jumelle, qui fait exactement la même chose. Rien n'est accessible par le ruban seul.

### La palette de commandes

Pour lancer une commande sans savoir dans quel onglet ou quel menu elle se trouve :

- **Ouvrir** : **Ctrl+K** (**Cmd+K** sur macOS), ou un clic sur le champ **Rechercher une commande…** de la barre du haut (une simple loupe quand la fenêtre est étroite). Le raccourci marche aussi sans projet ouvert, mais pas pendant que vous tapez dans une cellule ou un champ.
- **Chercher** : tapez quelques lettres ; les accents et les majuscules ne comptent pas (« echelle » trouve « Échelle de temps »), les débuts de mots suffisent (« litac » trouve « Lier les tâches »), les mots peuvent être dans le désordre. Chaque ligne montre la commande, l'onglet ou le menu d'où elle vient et son raccourci clavier. Toutes les commandes du ruban et des menus y sont, entrées des listes comprises (échelle de temps, thème, langue, filtres, tri, niveaux du plan, regroupement, colonnes affichées) ; pas les couleurs du texte ni les fichiers récents.
- **Lancer** : **↑** et **↓** pour choisir, **Entrée** (ou un clic) pour lancer : la commande fait exactement ce que fait son bouton du ruban, et s'annule de la même façon. Une commande grisée dit pourquoi (« Sélectionnez au moins une tâche ») et ne se lance pas.
- **Fermer** : **Échap**, Ctrl+K de nouveau, ou un clic à côté ; le clavier revient au planning.
- **Demander à l'assistant** : si l'assistant est activé, la dernière ligne propose « Demander à l'assistant : « … » » avec ce que vous avez tapé ; **Entrée** sur elle (ou **Tab**) ouvre le volet de l'assistant avec ce texte dans la question, sans l'envoyer : relisez, puis envoyez. La question qui s'y trouvait est remplacée.
- **Version web** : Ctrl+K ouvre la palette et non la recherche du navigateur (si le navigateur le garde quand même, cliquez le champ de la barre du haut).

## 2. Les cinq onglets

| Onglet | Groupes et boutons |
|---|---|
| **Tâche** | Affichage (Diagramme de Gantt ▾) · Presse-papiers (Coller, Couper, Copier, Collage spécial…) · Texte (Gras, Italique, Couleur du texte ▾, Surlignage ▾, Reproduire la mise en forme, Effacer la mise en forme : icônes seules) · Mise en forme (Mettre en forme les barres, Modèles…) · Planification (0 % à 100 % ; Lier, Délier ; Indenter, Désindenter ; Désactiver, Décaler ▾) · Tâches (Planifier manuellement, Inspecter) · Insérer (Tâche, Récapitulative, Jalon) · Propriétés (Informations, Notes, Sur la chronologie) · Édition (Rechercher, Atteindre la tâche, Remplir ▾ : Vers le bas, À droite ; Effacer ▾) |
| **Ressource** | Affichage (Feuille des ressources, Utilisation des ressources) · Affectations (Affecter les ressources) |
| **Projet** | Propriétés (Informations sur le projet, Champs personnalisés, Modifier le temps de travail) · Planification (Définir le planning de référence) · État (Mettre à jour le projet) · Impression (Mode impression, Mise en page…, Exporter en PDF…) |
| **Affichage** | Affichages des tâches (Diagramme de Gantt, Gantt de suivi) · Affichages des ressources · Données (Trier ▾, Mode plan ▾, Filtre ▾, Regrouper par ▾) · Zoom (Échelle de temps ▾, Zoom avant, Zoom arrière, Projet entier, Tâches sélectionnées) · Fractionner l'affichage (Chronologie, Détails, Volet Opérations IA, Assistant IA, Diagnostics) · Interface (Thème ▾, Langue ▾, Réduire le ruban) |
| **Format** | Colonnes (Insérer une colonne, Paramètres de champ, Colonnes ▾) · Styles des barres (Mettre en forme les barres, Tâches critiques, Mois relatifs) · Styles du diagramme (Modèles de mise en forme…) · Afficher/Masquer (Graphique seul) |

Ce tableau décrit le ruban complet. Dans le ruban compact, chaque groupe garde ses commandes courantes (onglet Tâche : Diagramme de Gantt ▾ ; Coller, Couper, Copier ; Gras, Italique, Pinceau ; Mettre en forme les barres ; 0 % à 100 %, Lier, Délier, Indenter, Désindenter, Décaler ▾ ; Planifier manuellement ; **Tâche** ; Informations, Notes ; Rechercher, Atteindre la tâche) et range les autres dans son menu **…**. Survoler un bouton sans libellé affiche son nom.

Fenêtre étroite : les groupes de droite perdent d'abord les libellés de leurs petits boutons (l'info-bulle reste), puis se replient en un seul bouton à menu portant le nom du groupe. Onglet Tâche, ruban complet : à 1 280 px, Presse-papiers, Texte, Planification et Insérer restent entiers (Propriétés peut perdre ses libellés, Mise en forme et Édition sont repliés) ; à 1 024 px, Presse-papiers et Texte restent entiers, Planification n'est jamais repliée ; à 800 px, la plupart des groupes deviennent des boutons à menu. Le ruban compact se réduit de la même façon (icônes seules, puis un bouton par groupe).

## 3. Où est… ?

| Je cherche… | Dans PlannerCraft |
|---|---|
| Insérer une tâche | Tâche → Insérer → **Tâche** (Inser, ou Ctrl+Maj+I ; Cmd+Maj+I sur macOS) |
| Insérer un jalon | Tâche → Insérer → **Jalon** (avant la tâche sélectionnée, en fin de projet sans sélection) |
| Créer une récapitulative au-dessus de tâches | Sélectionner des tâches voisines, Tâche → Insérer → **Récapitulative** (une seule annulation) |
| Lier, délier | Tâche → Planification → **Lier** (Ctrl+F2), **Délier** (Ctrl+Maj+F2) |
| Indenter, désindenter | Tâche → Planification (Alt+Maj+→, Alt+Maj+←) |
| Marquer l'avancement | Tâche → Planification → **0 %** à **100 %** |
| Planification manuelle ou automatique | Tâche → Tâches → **Planifier manuellement** (bouton enfoncé si la tâche active est manuelle) |
| Désactiver une tâche | Tâche → Planification → **Désactiver** |
| Décaler des tâches de quelques jours | Tâche → Planification → **Décaler ▾** (+1 jour, +1 semaine, +4 semaines, −1 jour, −1 semaine, Personnalisé… : « 3j », « -2sem ») ; menu Tâche → Décaler. Une tâche automatique reçoit la contrainte « Début au plus tôt le » ; une tâche manuelle garde sa durée ; une récapitulative décale ses tâches ; les tâches achevées ou inactives ne bougent pas. Une seule annulation, et un message dit ce qui a été fait |
| Comprendre pourquoi une tâche commence à cette date | Tâche → Tâches → **Inspecter** (volet « Pourquoi cette date ? ») |
| Assistant IA | Bouton **✦ Assistant IA** en haut à droite de la barre du haut (toujours visible, ruban réduit compris), **Ctrl+J** (Cmd+J sur macOS), ou Affichage → Fractionner l'affichage → **Assistant IA** : ouvre ou ferme le volet de l'assistant ; une pastille signale une réponse terminée ou une proposition arrivée volet fermé |
| Informations sur la tâche, notes | Tâche → Propriétés → **Informations** (Maj+F2), **Notes** |
| Retrouver la barre d'une tâche | Tâche → Édition → **Atteindre la tâche** |
| Rechercher | Tâche → Édition → **Rechercher** (Ctrl+F ; suivant Ctrl+G) |
| Recopier vers le bas | Tâche → Édition → **Remplir ▾** → Vers le bas (Ctrl+D) |
| Recopier vers la droite | Tâche → Édition → **Remplir ▾** → À droite (Ctrl+R), ou Édition → Remplir à droite |
| Vider des cellules sans supprimer les tâches | Tâche → Édition → **Effacer ▾** → Contenu (Ctrl+Suppr) |
| Gantt de suivi, feuille des ressources | Affichage → Affichages des tâches / des ressources, ou Tâche → **Diagramme de Gantt ▾** |
| Trier, filtrer, niveau de plan | Affichage → Données (effacer le filtre : F3 ; trier par N° : Maj+F3) |
| Regrouper les tâches par lot, ressource, critique ou jalon | Affichage → Données → Regrouper par ▾ (ou menu Affichage → Regrouper par) : une ligne d'en-tête par valeur (nombre de tâches, début et fin du groupe), un clic sur l'en-tête replie le groupe ; gardé dans le document et imprimé tel quel ; « Aucun groupe » pour revenir. Pendant le regroupement, insérer, indenter et déplacer des lignes sont grisés (« Retirez le regroupement… ») ; la saisie dans les cellules reste possible |
| Changer l'échelle de temps | Affichage → Zoom → **Échelle de temps ▾**, **Zoom avant/arrière** (Ctrl+/ et Ctrl+-), **Projet entier** |
| Voir en entier les barres des tâches sélectionnées | Affichage → Zoom → **Tâches sélectionnées** |
| Repérer les notes, contraintes, échéances dépassées | Colonne **ⓘ** (Indicateurs), après N° : survoler l'icône pour le détail ; cliquer sur l'icône de note ouvre les notes |
| Afficher ou masquer la chronologie | Affichage → Fractionner l'affichage → **Chronologie** (ou clic droit dans la frise → Masquer, ou son ×) |
| Formulaire de la tâche (prédécesseurs, successeurs, ressources) | Affichage → Fractionner l'affichage → **Détails** (voir `panes-columns-fields.md`) |
| Ajouter des tâches à la chronologie | Tâche → Propriétés → **Sur la chronologie** (bouton enfoncé si la tâche active y est), ou clic droit sur la tâche → « Ajouter à la chronologie » |
| Calendrier, jours fériés | Projet → Propriétés → **Modifier le temps de travail** |
| Ligne de base | Projet → Planification → **Définir le planning de référence** |
| Imprimer, PDF | Projet → Impression → **Mode impression**, **Mise en page…**, **Exporter en PDF…** |
| Colonnes de la table | Format → **Insérer une colonne**, **Paramètres de champ** (Alt+F3), **Colonnes ▾** ; clic droit sur un titre ; colonne « Ajouter une nouvelle colonne » en bout de table |
| Renommer Texte1, Nombre1… | Projet → Propriétés → **Champs personnalisés** |
| Couleurs et styles des barres | Format → Styles des barres, Styles du diagramme |
| Mettre une ligne en gras, en italique, en couleur | Tâche → **Texte** : **Gras** (Ctrl+B, Cmd+B sur macOS), **Italique** (Ctrl+I, Cmd+I sur macOS), **Couleur du texte ▾** ; ou menu Format. Le bouton est enfoncé quand la tâche active est en gras ou en italique. Seule la table change, pas le Gantt |
| Surligner une ligne | Tâche → Texte → **Surlignage ▾** (fond des cellules de la ligne dans la table ; « Aucun surlignage » le retire). Une couleur peu lisible est signalée, avec une teinte corrigée à cliquer ; en thème sombre, les couleurs trop sombres sont éclaircies d'elles-mêmes |
| Recopier la mise en forme d'une ligne sur d'autres | Tâche → Texte → **Reproduire la mise en forme** (pinceau) : sélectionner la ligne modèle, cliquer le pinceau, puis cliquer (ou sélectionner) les lignes à mettre en forme ; Échap annule |
| Retirer gras, italique, couleur et surlignage | Tâche → Texte → **Effacer la mise en forme** |
| Agrandir la vue | Ctrl+F1 (replie le ruban : la vue gagne 92 points) |

## 4. La chronologie

- Tant qu'aucune tâche n'y est ajoutée, la frise reste basse (une ligne : dates de début et de fin, graduations, et la phrase d'aide) ; elle reprend sa hauteur dès qu'une tâche y est.
- Frise de la largeur de la fenêtre, au-dessus de la table et du Gantt : **Début** et date à gauche, **Fin** et date à droite, graduations des mois (trimestres ou années pour un long projet), date d'état en trait tireté « État JJ/MM ». Absente des vues des ressources.
- **Ajouter** : sélectionner des tâches, Tâche → Propriétés → **Sur la chronologie** (ou clic droit → « Ajouter à la chronologie ») ; si toutes y sont déjà, le même bouton les retire. Une tâche devient une barre, un jalon un losange. Au plus 3 rangées de barres ; les tâches qui ne tiennent pas sont comptées « +n » à droite. Au plus 200 tâches.
- **Cliquer** une barre ou un losange sélectionne la tâche et fait défiler la table et le Gantt jusqu'à elle ; clic droit → « Retirer de la chronologie ».
- Le cadre violet montre la partie du projet visible dans le Gantt. À l'écran, la frise prend les couleurs de PlannerCraft (barres lavande cerclées de violet) ; imprimée, elle prend celles du modèle de mise en forme du projet.
- La liste des tâches de la frise est enregistrée avec le projet (dans le style du document) ; avec la préférence « ne jamais écrire le style », elle ne l'est pas.
- **Impression** : case **Chronologie** de Mise en page ; le Mode impression la coche d'office quand la frise est affichée. Elle s'imprime en tête de la première page (aperçu, PDF, SVG).

## 5. Ce qui n'existe pas (encore)

Pas de choix de police ni de taille (le groupe Texte ne fait que gras, italique, couleur et surlignage, enregistrés avec le projet dans le style du document, jamais dans le XML de MS Project), pas d'onglet « Format de la chronologie » (légendes, plusieurs barres, plages de dates), pas de personnalisation du ruban ni de la barre d'accès rapide, pas d'info-touches Alt, pas d'écran Fichier en coulisses (le menu Fichier le remplace), ni nivellement, Planificateur d'équipe, vues Réseau, Calendrier ou Détails (ADR-0017, reportés après la v1.0.0).

Sur macOS, Cmd+F1 peut être pris par le système (luminosité, recopie vidéo) selon les réglages du clavier : le double-clic sur un onglet et le menu Affichage restent toujours disponibles.
