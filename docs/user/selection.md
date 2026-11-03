# Sélectionner, copier, remplir

La table des tâches de PlannerCraft se manie comme une feuille Excel et comme la table de MS Project : on sélectionne à la souris ou au clavier, puis on copie, colle, recopie ou efface **des cellules**, sans toucher aux tâches elles-mêmes. Ctrl désigne Cmd sur macOS (sauf Ctrl+Espace). Toutes ces conventions sont des hypothèses de PlannerCraft (ADR-0018) : rien n'a été vérifié dans MS Project.

## 1. Sélectionner

| Geste | Résultat |
|---|---|
| Appui sur une cellule | La cellule seule ; elle devient la **cellule active** (cadre de 2 px). La sélection se fait à l'appui, sans attendre le relâcher. |
| Cliquer-glisser | Un **bloc** de cellules, de la cellule appuyée à celle sous le pointeur. Un léger tremblement (moins de 6 points) garde la cellule appuyée. Au bord haut ou bas de la table, la vue défile toute seule ; tout contre le bord gauche ou droit, ou au-delà, elle défile de côté. |
| Maj+clic | Étend le bloc jusqu'à la cellule cliquée. |
| Ctrl+clic | Ajoute ou retire une ligne (jamais la dernière) ; glisser après un Ctrl+appui n'étend rien. |
| Clic, glisser sur les **n°** (colonne ID) | Une ou plusieurs **lignes entières**. Attention : un glisser qui part du n° d'une ligne **déjà** sélectionnée en lignes entières la **déplace** (section 4, comme MS Project ; Ctrl+Z l'annule) ; pour étendre la sélection depuis elle, **Maj+clic** sur le dernier n°. |
| Clic sur un **titre de colonne** | La colonne entière ; glisser sur les titres ou Maj+clic : plusieurs colonnes. Le titre « N° » sélectionne tout. |
| Ctrl+A (ou Ctrl+Maj+Espace) | Tout, en lignes entières. |
| Maj+← / → / Origine / Fin | Étend le bloc d'une colonne, ou jusqu'au bout de la ligne. |
| Maj+↑ / ↓ | Étend le bloc d'une ligne. |
| Maj+Espace / Ctrl+Espace | Lignes entières / colonnes entières de la sélection. |
| Maj+Retour arrière | Revient à la cellule active seule. |
| Entrée, Tab (et Maj) | Dans un bloc, passent à la cellule suivante **du bloc** sans le défaire. |

Ce que l'on voit :

- le bloc est **teinté en bleu**, entouré d'une bordure de 2 px ; la cellule active reste **blanche** (fond de sa ligne) dans son cadre ;
- les **n° des lignes** de la sélection sont « appuyés » (fond bleu pâle), et la ligne correspondante du Gantt aussi ;
- des lignes entières sont teintées sur toute la largeur de la table ;
- la ligne sous le pointeur est légèrement surlignée (jamais pendant un glisser).

Ces couleurs suivent le modèle de mise en forme et le thème (clair ou sombre) : la teinte est assez marquée pour se voir d'un coup d'œil, et le texte reste lisible dessus.

## 2. Copier, couper, coller

- **Copier** (Ctrl+C) un bloc écrit ses cellules en texte tabulé, une ligne par tâche : il se colle tel quel dans Excel ou un tableur.
- **Coller** (Ctrl+V) du texte venu d'Excel le pose à partir du coin haut-gauche du bloc ; une seule valeur collée sur un bloc remplit tout le bloc. Les cellules calculées (Fin d'une récapitulative, Marge…) sont sautées et comptées dans le bandeau. Une valeur illisible refuse tout le collage : rien n'est modifié.
- **Couper** (Ctrl+X) un bloc = copier, puis effacer son contenu.
- Avec des **lignes entières** sélectionnées, copier-coller copie et insère des **tâches** (comme MS Project).

## 3. Remplir et effacer

| Je veux… | Geste |
|---|---|
| Recopier la première cellule de chaque colonne du bloc vers le bas | **Ctrl+D**, ou Tâche → Édition → **Remplir ▾ → Vers le bas**, ou Édition → Remplir vers le bas, ou clic droit |
| Écrire la même valeur dans tout le bloc | Taper la valeur puis **Ctrl+Entrée** |
| Vider des cellules sans supprimer les tâches | **Ctrl+Suppr**, ou Tâche → Édition → **Effacer ▾ → Contenu** ; Suppr fait de même sur une cellule hors N° et Nom, et sur tout bloc de plusieurs cellules (colonne entière comprise) |
| Supprimer des tâches | **Suppr** avec des lignes entières (clic sur le N°), ou une seule cellule du Nom sélectionnée (un message rappelle Ctrl+Z) ; **Ctrl+Retour arrière** supprime toujours les tâches des lignes sélectionnées, quel que soit le bloc (le menu « Supprimer la tâche » affiche Suppr, qui efface un bloc de plusieurs cellules) |

Chaque geste (coller, remplir, effacer, déplacer) est **une seule** entrée d'annulation : Ctrl+Z le défait d'un coup.

## 4. Lignes et colonnes

- **Déplacer des tâches** : sélectionner des lignes entières (par leurs n°), puis glisser l'un de ces n° ; un trait bleu montre où elles iront. Échap annule. Impossible sous un tri ou un filtre (le pointeur l'indique).
- **Élargir une colonne** : glisser le bord droit de son titre ; **double-clic** sur ce bord : largeur ajustée au contenu.
- **Clic droit** sur un titre : masquer, ajuster, trier ; sur une cellule : le même menu que le ruban (Couper, Copier, Coller, Remplir, Effacer, Insérer, Supprimer…).
