# Import des fichiers `.mpp` de MS Project (lots M1 à M4)

Décision : ADR-0036. Règles : R-281 à R-292 (`docs/scheduling-rules.md`). Faits de format, chacun avec sa preuve : `docs/formats/mpp.md`. Code : `crates/plannercraft-mpp` (lecture seule, sans interface, compile en `wasm32-unknown-unknown`), CLI `plannercraft import-mpp`.

## 1. Besoin

Les plannings échangés entre entreprises du bâtiment circulent souvent en `.mpp`, le format binaire de MS Project, et non en XML. Le brief mettait `.mpp` hors périmètre ; le propriétaire a demandé la compatibilité le 2026-11-14 et autorisé l'analyse du format (ADR-0036 D1). On ne lit que ; on n'écrira jamais de `.mpp`.

## 2. Lots

| Lot | Contenu | État |
|---|---|---|
| **M1** | Lecteur MPP14 (Project 2010 à 2021, Microsoft 365) et gabarits `.mpt` : tâches (avec réels, reprise, retard de nivellement), liens, calendriers de base (exceptions périodiques comprises), propriétés et options de calcul du projet ; refus clair des autres formats et des tables incohérentes ; CLI `import-mpp` ; corpus public, fichiers réels et mesure contre un corrigé | livré (ce document) |
| **M2** | Ressources, affectations, calendriers de ressource ; MPP12 (Project 2007), puis MPP9 et MPP8 si la demande existe | à faire |
| **M3** | Ouverture d'un `.mpp` depuis l'application bureau et la version web (fichier déposé dans le navigateur, lu localement) : même chemin que l'ouverture d'un XML, rapport dans les diagnostics | à faire |
| **M4** | Présentation : affichages, tables, styles de barres (`   214`), champs personnalisés | à faire |

## 3. Chaîne de lecture (M1)

Entrée : des octets (aucun système de fichiers). Sortie : `MppDocument` (projet du cœur avec les **entrées** seules, dates **enregistrées** à part, provenance, diagnostics), puis comparaison au calcul du moteur.

1. **Conteneur** (`cfb.rs`) : fichier composé lu depuis `[MS-CFB]` (en-tête, DIFAT, FAT, répertoire, mini FAT, mini flux), chaînes bornées, boucles détectées (R-281).
2. **Format** (`detect.rs`) : dossier `   114`, `   112`, `   19`/`   29`, `   1`/`   2` ; version d'enregistrement dans `Props14`, qui choisit la disposition Project 2010 ou Project 2013 (R-282).
3. **Tables** (`tables.rs`) : `FixedMeta`/`FixedData`, `Fixed2Meta`/`Fixed2Data`, `VarMeta`/`Var2Data`, `Props`.
4. **Projet** (`project.rs`) : propriétés (R-288), calendriers de base (`calendar.rs`, R-287), tâches (`task.rs`, R-283 à R-285, R-289, R-290), garde de cohérence (R-292), liens (R-286). Décalages par disposition dans `layout.rs`.
5. **Comparaison** (`compare.rs`, R-291) : le moteur calcule, chaque tâche est comparée à la minute aux dates enregistrées.
6. **Rapport** (`report.rs`) : texte et JSON (comptes, non repris, diagnostics par code, comparaison).

CLI : `plannercraft import-mpp projet.mpp -o projet.xml [--json] [--dry-run]` ; MSPDI écrit par la session (dates du moteur), écriture atomique ; refus (format non pris en charge, conteneur endommagé, disposition non reconnue) : code de sortie 4 ; fichier illisible : 3.

## 4. Corpus, fichiers réels et corrigé

- **Corpus de test** (hors dépôt) : 609 fichiers `.mpp` de test publics du dépôt `joniles/mpxj` (commit `4982974b2594ffc7146cdb9ebdf2ae46186737ca`), extraits seuls (aucun code) : 164 MPP14 (80 enregistrés par Project 2010, 84 par Project 2013 et suivants), 178 MPP12, 238 MPP9, 29 MPP8. Le sous-dossier `generated/` fait varier un champ par dossier, un fichier par version de MS Project. Ce sont de petits projets (2 225 tâches en tout).
- **Fichiers réels** (hors dépôt, 2026-11-15) : 41 fichiers publics de 41 dépôts GitHub (40 `.mpp`, 1 `.mpt`), trouvés par la liste des arbres de dépôts publics et téléchargés seuls, sans ouvrir de page sur le format ni de dépôt de lecteur de `.mpp` ; source, commit, taille, SHA-256 et date dans `SOURCES.tsv` à côté des fichiers. Projets d'étudiants et de cours de gestion de projet, un livre de formation (Packt) : construction résidentielle, logiciel, hôtellerie, banque, industrie pharmaceutique, déploiement informatique. 12 à 330 tâches (2 966 en tout, 8 fichiers de plus de 100 tâches), 52,9 Mo ; enregistrés par 34 versions différentes : 40 par la version 16 (Project 2016 à Microsoft 365, versions 16,0,4266 à 16,0,20026), 1 par Project 2010 (14,0,7162) ; aucun par Project 2013 (15). Aucun fichier d'entreprise ou d'administration : les sites publics essayés refusaient les téléchargements automatiques (HTTP 403). S'y ajoutent les 7 fichiers d'un dépôt de formation fournis à la relecture (commit noté hors dépôt, URL inconnue, donc pas au manifeste).
- **Corrigé** : chaque fichier converti en MSPDI par MPXJ **lancé comme outil externe** (son code et sa documentation ne sont pas lus, ADR-0036 D1), XML relu par `plannercraft-mspdi`. **Limite** : c'est la lecture d'un autre programme, pas celle de MS Project ; un écart peut venir de lui (exemples au § 7). Rien n'est vérifié contre MS Project lui-même (aucune licence).
- **Corpus versionné** : `tests/corpus-mpp/manifest.toml`, 57 fichiers : 45 du corpus de test (40 MPP14 des deux dispositions : dates, liens, durées, indicateurs, textes, calendriers, exceptions simples et périodiques, propriétés, lignes vides, suppressions, notes, échéance, tâches manuelles ; 5 refus MPP12, MPP9, MPP8) et 12 fichiers réels (les 8 de plus de 100 tâches sauf un, le seul enregistré par Project 2010, le `.mpt`, et ceux qui ont appris un fait : emplacements en surnombre, sens de planification, réels), URL brute figée sur le commit, taille, SHA-256 ; `cargo xtask corpus fetch-mpp` ; test `crates/plannercraft-mpp/tests/corpus.rs` (comptes et début du projet lus, plancher des tâches identiques au calcul) ; job CI `corpus-mpp`. Valeurs attendues lues par ce lecteur après vérification contre le corrigé, jamais copiées du XML du corrigé.

## 5. Mesure sur le corpus de test (2026-11-15)

Commande : `MPP_MEASURE_DIR=… MPP_ORACLE_DIR=… cargo test -p plannercraft-mpp --test measure -- --ignored --nocapture` (test ignoré, hors CI ; `MPP_MEASURE_LAYOUT=Project2010` ou `Project2013` pour une disposition), sur les 609 fichiers.

**Fichiers** : 164 MPP14 lus sur 164 (0 refusé, aucun enregistrement incohérent au sens de R-292) ; 445 refusés avec la version nommée (`mpp.unsupported_version` : 178 MPP12, 238 MPP9, 29 MPP8). MPP12 n'est pas lu : ses enregistrements de tâche font 264 octets (`docs/formats/mpp.md` § 2), il demande sa propre analyse (lot M2).

**Champs, contre le corrigé** (A = enregistrés par Project 2010, 80 fichiers ; B = par Project 2013 et suivants, 84 fichiers) :

| Champ | Égaux / comparés | Taux | A | B |
|---|---|---|---|---|
| Tâche présente (UID) | 2 238 / 2 239 | 100,0 % | 859 / 860 | 1 379 / 1 379 |
| Nombre de tâches par fichier | 163 / 164 | 99,4 % | 79 / 80 | 84 / 84 |
| Ordre des tâches (UID par ID) | 162 / 164 | 98,8 % | 78 / 80 | 84 / 84 |
| Ligne vide | 2 238 / 2 238 | 100 % | 859 / 859 | 1 379 / 1 379 |
| Nom, niveau, unité de durée, jalon, % achevé, active, priorité, type, calendrier, contrainte, échéance, liens (chacun) | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Durée (minutes) | 2 224 / 2 225 | 100,0 % | 850 / 851 | 1 374 / 1 374 |
| Début et fin enregistrés (chacun) | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Début réel, fin réelle, durée réelle (chacun) | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Reprise (tâche commencée) | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Retard de nivellement | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Planification manuelle | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Valeurs saisies d'une tâche manuelle | 2 225 / 2 225 | 100 % | 851 / 851 | 1 374 / 1 374 |
| Notes | 2 223 / 2 225 | 99,9 % | 849 / 851 | 1 374 / 1 374 |
| Projet : sens et date de planification, titre, facteurs, heure de début par défaut, date d'état, calendrier (chacun) | 164 / 164 | 100 % | 80 / 80 | 84 / 84 |
| Calendrier de base présent | 207 / 224 | 92,4 % | 108 / 112 | 99 / 112 |
| Calendrier : nom | 205 / 207 | 99,0 % | 106 / 108 | 99 / 99 |
| Calendrier : semaine | 207 / 207 | 100 % | 108 / 108 | 99 / 99 |
| Calendrier : exceptions (jours couverts, récurrence dépliée) | 207 / 207 | 100 % | 108 / 108 | 99 / 99 |
| Non repris : nombre de ressources annoncé | 161 / 164 | 98,2 % | 77 / 80 | 84 / 84 |
| Non repris : nombre d'affectations annoncé | 143 / 164 | 87,2 % | 64 / 80 | 79 / 84 |

Les écarts de tâches, de durée, de notes et d'ordre viennent tous de `sf248.mpp` et de `task-deletion2-project2010-mpp14.mpp` (§ 7). Peu d'exemples positifs pour certains champs (le taux de 100 % ne dit pas tout) : 23 jalons, 4 tâches inactives (2 fichiers), 2 échéances, 31 contraintes datées, 15 calendriers de tâche, 62 liens, 13 lignes vides, 5 retards de nivellement.

**Moteur** : tâches dont le début et la fin recalculés égalent, à la minute, ceux enregistrés par MS Project : **2 179 sur 2 221** (98,1 %) ; A 819 / 849, B 1 360 / 1 372. Même calcul sur le projet relu depuis le XML du corrigé : 2 168 / 2 222 (il porte aussi ressources et affectations, qui coûtent dans `mpp14timephased*.mpp`) ; sur ce projet sans ressources ni affectations : 2 179 / 2 222, comme la lecture `.mpp`.

## 6. Mesure sur les fichiers réels (2026-11-15)

Même commande, sur les 41 fichiers réels puis sur les 7 de la relecture.

**Fichiers** : 41 lus sur 41 (40 B, 1 A ; le `.mpt` compris), 0 refusé, aucun enregistrement incohérent (R-292) ; 7 sur 7 pour la relecture (dont `Practice-11.mpp`, lu avant correction comme 0 tâche, et `MyRecourcePool.mpp`, un réservoir de ressources sans tâche).

| Champ | Égaux / comparés | Taux | A | B |
|---|---|---|---|---|
| Tâche présente (UID) | 2 968 / 2 969 | 100,0 % | 40 / 40 | 2 928 / 2 929 |
| Nombre de tâches par fichier | 40 / 41 | 97,6 % | 1 / 1 | 39 / 40 |
| Ordre des tâches (UID par ID) | 40 / 41 | 97,6 % | 1 / 1 | 39 / 40 |
| Ligne vide | 2 968 / 2 968 | 100 % | 40 / 40 | 2 928 / 2 928 |
| Nom, niveau, durée, unité de durée, jalon, % achevé, active, priorité, type, calendrier, contrainte, échéance, notes, liens (chacun) | 2 966 / 2 966 | 100 % | 40 / 40 | 2 926 / 2 926 |
| Début enregistré | 2 965 / 2 966 | 100,0 % | 40 / 40 | 2 925 / 2 926 |
| Fin enregistrée | 2 964 / 2 966 | 99,9 % | 40 / 40 | 2 924 / 2 926 |
| Début réel, fin réelle, durée réelle (chacun) | 2 966 / 2 966 | 100 % | 40 / 40 | 2 926 / 2 926 |
| Reprise (tâche commencée) | 2 966 / 2 966 | 100 % | 40 / 40 | 2 926 / 2 926 |
| Retard de nivellement | 2 966 / 2 966 | 100 % | 40 / 40 | 2 926 / 2 926 |
| Planification manuelle | 2 966 / 2 966 | 100 % | 40 / 40 | 2 926 / 2 926 |
| Valeurs saisies d'une tâche manuelle | 2 883 / 2 966 | 97,2 % | 40 / 40 | 2 843 / 2 926 |
| Projet : sens et date de planification, titre, facteurs, heure de début par défaut, date d'état, calendrier (chacun) | 41 / 41 | 100 % | 1 / 1 | 40 / 40 |
| Calendrier de base présent | 96 / 106 | 90,6 % | 1 / 1 | 95 / 105 |
| Calendrier : nom, semaine, exceptions (chacun) | 96 / 96 | 100 % | 1 / 1 | 95 / 95 |
| Non repris : nombre de ressources annoncé | 40 / 41 | 97,6 % | 1 / 1 | 39 / 40 |
| Non repris : nombre d'affectations annoncé | 22 / 41 | 53,7 % | 0 / 1 | 22 / 40 |

Relecture (7 fichiers, 75 tâches, tous B) : tous les champs à 100 %, sauf « calendrier de base présent » 10 / 11 et le nombre d'affectations annoncé 6 / 7.

**Moteur** : **2 255 sur 2 966** tâches réelles recalculées identiques aux dates enregistrées (76,0 %) ; même calcul sur le projet du corrigé sans ressources ni affectations : 2 257 / 2 967 ; avec : 2 314 / 2 967. Relecture : 74 / 75, comme le corrigé. Par fichier :

- **Tout identique** : 21 fichiers.
- **Moteur écarté de MS Project autant sur les deux lectures** (10 fichiers, 523 tâches) : `advaith14…` 14 / 330, `AtharvaTK1997…` 1 / 62, `4jvarun…` 29 / 72, `Dimitrije-Sokolovic…` 42 / 64, `chandana7373…` 37 / 83, `Masetto97…` 57 / 69, `lautarofaccini…` 37 / 44, `svsuriyaa…` 73 / 80, `vincenzorusso3…` 88 / 96, `HamzaMalikProjects…` 55 / 56. Cause **inconnue**, hors de la lecture (le moteur ou des dates enregistrées sans recalcul) ; non examinée dans ce lot.
- **Écarts dus aux ressources et affectations** (non reprises, M2 ; 7 fichiers, lecture égale au corrigé sans ressources) : `ShidehParastariFarkoush…` 32 / 66 (corrigé avec ressources 65), `Smruti0708…` 38 / 102 (83), `salemsolomon25…` 7 / 27 (17), `tefsantana…` 114 / 120 (118), `zahmed02…` 217 / 227 (219). À l'inverse, `davidramosf…` 48 / 61 (25) et `nehathota123…` 55 / 83 (46) : les ressources du corrigé y coûtent.
- **Écarts propres à la lecture** : `Kalani0056…` 34 / 41 contre 35 et `ubhurase…` 105 / 106 contre 106 : une date enregistrée lue différemment (§ 7), cause inconnue. `AmaalAyman…` 15 / 20 contre 15 / 21 : une tâche de moins (§ 7).

Ce que les fichiers réels ont corrigé dans le lecteur (le corpus de test le cachait) : nombre d'éléments en service et taille connue des éléments (`Practice-11.mpp` lu 0 tâche avant), bourrage des propriétés (`BigProject.mpp`), semaine par défaut du projet (13 calendriers), secondes tronquées, exceptions périodiques, clé du sens de planification, durée des tâches manuelles, réels, reprise, retard de nivellement, date d'état, premier jour de la semaine, options « fractionner les tâches en cours », « la tâche met à jour la ressource » et « respecter les contraintes » (`OzBuild_Workshop_18_End_Para_20.mpp` : 17 / 18 avant, 18 / 18 après). Moteur sur les fichiers réels : 1 696 / 2 966 à la première mesure (`OzBuild…` 2 / 18), 2 255 après.

**Robustesse** : 150 versions tronquées ou altérées (graine fixe) de chacun des 164 fichiers de test, 40 `.mpp` réels et 7 de la relecture (31 650 lectures) : aucune panique.

**Temps** : 41 fichiers réels (52,9 Mo) lus et comparés en 4 s en debug, deux lectures par fichier.

## 7. Fichiers encore mal lus, et écarts où le corrigé est soupçonné

Mal lus par ce lecteur (cause connue) :

- `assignment/assignment-deletion/sf248.mpp` (corpus de test, fichier d'un rapport d'anomalie) : `FixedMeta` incohérent (6 éléments annoncés, 42 emplacements, copies contradictoires) ; 2 tâches lues sur 3, une durée et des notes fausses. Sous les seuils de R-292 (aucun diagnostic d'incohérence) : lu, pas refusé.
- `task/task-deletion/task-deletion2-project2010-mpp14.mpp` : après des suppressions, Project 2010 laisse un ID ancien dans un enregistrement : une tâche et une ligne vide permutées (`docs/formats/mpp.md` § 11).

Écarts où le corrigé est soupçonné :

- Calendriers de base « sans nom » gardés par le corrigé : 10 dans 2 fichiers réels (enregistrements marqués supprimés, bit 1, ou au-delà du nombre annoncé : vérifié sur `ShidehParastariFarkoush…`, UID 8 à 15 et 26), 17 au corpus de test (dont 9 de `ResourceIdAndUniqueId-project2013-mpp14.mpp`, supprimés ; les autres non examinés un à un, le corrigé a 19 calendriers de base sans nom au corpus).
- 2 calendriers nommés par le corrigé d'après une ressource (`mpp14assignmentfields.mpp`, `timephased-prorated-cost-resource.mpp`) : sans nom dans le fichier.
- `AmaalAyman__Project-Management__Pm_Project_1.mpp` UID 25 : enregistrement au-delà du nombre annoncé, sans nom ; gardé par le corrigé, pas par le lecteur. Le lire ferait relire aussi des liens périmés (`Deapry…`, lien 57 → 60, qui fausse 4 tâches) : le nombre annoncé est la règle (`docs/formats/mpp.md` § 4).
- Durée saisie nulle d'une tâche manuelle (83 tâches réelles, 2 843 / 2 926 en B) : le corrigé n'écrit pas `ManualDuration`, le lecteur lit 0 ; même calcul.
- Décalage nul d'un lien : unité du projet au corrigé, unité du fichier au lecteur ; sans effet.

Cause inconnue :

- `ubhurase__IPM__Big_Pharma_Case_Project_Final.mpp` UID 171 (début et fin enregistrés, 15 h d'écart) et `Kalani0056__ms-project-planning__st20357430_CIS6011.mpp` UID 189 (fin enregistrée, 41 jours d'écart) : le lecteur lit l'octet de la disposition B, le corrigé une autre valeur ; non expliqué.
- Nombres de ressources et d'affectations « non repris » : comptes approchés (enregistrements vivants), annoncés comme tels ; 19 fichiers réels sur 41 diffèrent du corrigé pour les affectations.

Refusés : aucun fichier réel ; au corpus de test, les seuls formats non pris en charge. 3 conversions du corrigé ont échoué (fichiers MPP9 protégés par mot de passe), sans effet sur la mesure MPP14.

## 8. Limites connues (M1)

- Formats : MPP12, MPP9, MPP8 refusés ; fichiers protégés par mot de passe : aucun échantillon MPP14, comportement inconnu ; `.mpt` : un seul échantillon.
- Non repris : ressources, affectations, calendriers de ressource, lignes de base, travail, coûts, champs personnalisés, affichages et styles ; comptés quand on sait les compter. Réels lus tels qu'enregistrés ; ceux des récapitulatives, calculés par MS Project, ne sont pas repris.
- « Respecter les contraintes » : deux clés toujours égales, lue à « non » seulement quand les deux valent 0 (`docs/formats/mpp.md` § 5).
- Calendriers : semaines datées (aucune dans les fichiers lus).
- Ordre des tâches après suppression dans un fichier enregistré par Project 2010 (§ 7).
- Les fichiers réels sont des projets d'étudiants et de formation (version 16 surtout, un seul Project 2010, aucun Project 2013) ; aucun planning de chantier d'entreprise n'a été lu. À mesurer sur des plannings du propriétaire (hors dépôt) avant M3.

## 9. Recommandations

- **M2** : commencer par les ressources et affectations (5 fichiers réels y perdent 94 tâches recalculées que le corrigé avec ressources retrouve), avec les champs du corrigé et `generated/resource-*`, `generated/assignment-*` ; MPP12 (178 fichiers au corpus, même conteneur, enregistrements de 264 octets) ensuite.
- **Moteur** : examiner les 10 fichiers réels où le moteur s'écarte de MS Project sur les deux lectures (523 tâches) ; c'est hors de la lecture, mais c'est le plus gros écart restant.
- **M3** : ouvrir un `.mpp` comme un `.xml` (même chemin de chargement), rapport de lecture dans le panneau des diagnostics ; sur le web, lecture locale du fichier déposé.
- Faire relire au propriétaire quelques-uns de ses propres `.mpp` (hors dépôt, comme les plannings R1 à R6 de l'import PDF) avant M3.
