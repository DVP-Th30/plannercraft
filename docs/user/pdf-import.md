# Importer un planning PDF

Guide court de **Fichier → Importer un PDF…**. Référence technique : `docs/plans/pdf-import.md`, ADR-0032 et ADR-0033 (`docs/decisions.md`), `docs/features/ui.md` § 1.53.

## 1. Ce qui marche

Le planning qu'un maître d'œuvre ou un OPC joint à un dossier de consultation est souvent une vue Gantt **imprimée en PDF** depuis MS Project : un tableau à gauche (N°, nom, durée, début, fin, parfois prédécesseurs), les barres à droite. PlannerCraft relit ce tableau et en fait un planning modifiable :

- tâches, hiérarchie (par le retrait des noms ou la colonne WBS), durées, jalons, liens si la colonne Prédécesseurs est imprimée ;
- en français ou en anglais, sur une ou plusieurs pages, dates en chiffres (« lun 21/01/19 », « 1/21/19 ») ou en lettres (« 21 janv. 2019 ») ;
- **les dates ne sont jamais recopiées** : le moteur de PlannerCraft les calcule à partir des durées, des liens et du calendrier, puis les compare aux dates lues. Les écarts restent visibles.

Le fichier est lu **sur votre appareil** : rien n'est envoyé, ni sur l'ordinateur ni dans la version web.

## 2. Pas à pas

1. **Fichier → Importer un PDF…** (aussi sans planning ouvert, ou par la palette de commandes) et choisissez le fichier.
2. La fenêtre d'aperçu montre le **résumé** (tâches lues, niveaux, liens, colonnes reconnues), les **avertissements** (N° absents, nom coupé à l'impression, date illisible…) et le **tableau** des tâches : début et fin lus, **fin calculée**, **écart**. Cochez **Écarts seulement** pour ne voir que les tâches dont une date diffère du PDF.
3. Réglez au besoin (seuls les réglages utiles apparaissent) :
   - **Ordre des dates** : quand aucune date ne permet de trancher entre 03/04 = 3 avril et 03/04 = 4 mars ;
   - **Importer sans hiérarchie** : quand les retraits du PDF ne sont pas lisibles (toutes les tâches au niveau 1, à remettre en retrait ensuite) ;
   - **Contraindre les tâches liées à leur début lu** : quand le PDF a des liens et que des tâches liées démarrent plus tôt que dans le PDF (décalage ou contrainte non imprimés) ; pose « Début au plus tôt le » sur ces tâches.
4. Réglez le **calendrier** (§ 3) et regardez le compteur **Dates identiques au PDF : n sur m**.
5. **Importer** ouvre le planning comme un **nouveau document non enregistré**, nommé d'après le fichier ; enregistrez-le ensuite (Fichier → Enregistrer). **Annuler** (ou Échap) ne change rien.

## 3. À quoi sert le bloc « Calendrier »

Le PDF n'imprime pas le calendrier de son auteur. Une tâche de 40 jours qui enjambe la Toussaint et le 11 novembre finit, dans le PDF, deux jours ouvrés plus tard que sur un calendrier sans fériés : c'est l'essentiel des écarts de fin.

- **Jours fériés français** : les 11 fêtes légales de métropole (1er janvier, lundi de Pâques, 1er et 8 mai, Ascension, lundi de Pentecôte, 14 juillet, 15 août, 1er et 11 novembre, 25 décembre), cochées d'office pour un PDF en français. Pas d'Alsace-Moselle (Vendredi saint, 26 décembre) ni d'outre-mer : ajoutez-les ensuite dans Projet → Modifier le temps de travail. Si le compteur baisse quand la case est cochée, l'auteur ne chômait pas les fériés : décochez.
- **Périodes chômées déduites des écarts** : PlannerCraft cherche les jours chômés (congés d'été, de fin d'année, ponts) qui expliquent les fins trop précoces. C'est un **calcul du moteur**, pas une IA : une tâche de 10 jours lue du 27 juillet au 28 août a forcément chômé 15 jours ouvrés dans cet intervalle ; les tâches sans écart prouvent les jours travaillés. Chaque période est proposée **décochée** : cochez celles qui vous semblent justes.
  - « Du … au … : n jours ouvrés, explique k tâches » : la période ;
  - **À placer** : le PDF dit combien de jours ont été chômés et entre quelles dates, pas lesquels ; PlannerCraft propose un placement collé aux week-ends et fériés voisins, que les flèches **◀ ▶** déplacent ;
  - les demi-journées (une tâche de 5 jours qui court sur 6 jours ouvrés) ne sont pas des congés : leur écart reste, la fenêtre le dit.

Les jours cochés deviennent des **exceptions du calendrier du projet** (« Noël », « Congés (déduits de l'import PDF) »), modifiables ensuite dans Projet → Modifier le temps de travail.

## 4. Ce qui est refusé

Plutôt que d'importer de travers, PlannerCraft refuse, dit pourquoi et, quand c'est utile, quoi faire :

| Cas | Que faire |
|---|---|
| PDF numérisé (image) ou sans texte | demander l'export PDF depuis le logiciel de planning, ou ressaisir |
| PDF protégé | enregistrer une copie sans protection depuis votre lecteur PDF |
| Pas de tableau de planning (autre logiciel, autre document) | l'import lit les vues Gantt de MS Project ou d'un logiciel semblable |
| Tableau sans dates | réimprimer avec les colonnes Début et Fin |
| Hiérarchie illisible | cocher « Importer sans hiérarchie » |
| Fichier abîmé | le rouvrir dans un lecteur PDF, le réimprimer en PDF, importer la copie |
| Trop volumineux | imprimer seulement les pages du planning |

## 5. Version web

Même fenêtre, même calcul, dans l'onglet du navigateur : PDF de 16 Mio et 50 pages au plus (64 Mio et 500 pages dans l'application). Pendant la lecture, la page affiche « Lecture du PDF en cours… » et ne répond pas quelques instants. Un PDF abîmé peut arrêter le lecteur et la page : rechargez-la ; si la sauvegarde automatique du navigateur est active, le planning ouvert est retrouvé.

## 6. Limites

- Couleurs des barres, format de page et colonnes du PDF : pas encore repris (lot P2).
- Liens seulement s'ils sont imprimés dans une colonne Prédécesseurs (les flèches du graphique ne sont pas relues).
- Calendrier : semaine de 5 jours de 8 h à 17 h ; une semaine de 4 jours ou des horaires différents ne sont pas déduits.
