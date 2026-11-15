# Import des fichiers `.mpp` de MS Project (lots M1 à M4)

Décision : ADR-0036 (M1), ADR-0037 (M2), ADR-0038 (M3). Règles : R-281 à R-299 (`docs/scheduling-rules.md`). Faits de format, chacun avec sa preuve : `docs/formats/mpp.md`. Code : `crates/plannercraft-mpp` (lecture seule, sans interface, compile en `wasm32-unknown-unknown`), CLI `plannercraft import-mpp`.

## 1. Besoin

Les plannings échangés entre entreprises du bâtiment circulent souvent en `.mpp`, le format binaire de MS Project, et non en XML. Le brief mettait `.mpp` hors périmètre ; le propriétaire a demandé la compatibilité le 2026-11-14 et autorisé l'analyse du format (ADR-0036 D1). On ne lit que ; on n'écrira jamais de `.mpp`.

## 2. Lots

| Lot | Contenu | État |
|---|---|---|
| **M1** | Lecteur MPP14 (Project 2010 à 2021, Microsoft 365) et gabarits `.mpt` : tâches (avec réels, reprise, retard de nivellement), liens, calendriers de base (exceptions périodiques comprises), propriétés et options de calcul du projet ; refus clair des autres formats et des tables incohérentes ; CLI `import-mpp` ; corpus public, fichiers réels et mesure contre un corrigé | livré (ce document) |
| **M2** | Parité des dates sur les fichiers réels : diagnostic des écarts par tâche racine, récapitulatives manuelles (R-293), ressources, affectations et calendriers de ressource (R-294 à R-296), durée des profils non uniformes (R-297), classes d'écart et causes non recalculables au rapport (R-298) ; analyse de l'arrondi (ADR-0037) | livré (§ 10) |
| **M3** | Ouverture d'un `.mpp` ou d'un `.mpt` depuis l'application bureau et la version web (lu dans le navigateur) : même « Ouvrir » qu'un XML, type reconnu au contenu, nouveau planning enregistré en XML, bandeau de compte rendu, fenêtre « Différences avec MS Project » et « Garder les dates de MS Project » (R-299) | livré (§ 11) |
| **M5** | Formats 2007 et 2003 : MPP12 (Project 2007), puis MPP9 (2000 à 2003) et MPP8 (98) si la demande existe | à faire |
| **M4** | Présentation : gabarits avec leurs colonnes et leurs styles (affichages, tables, styles de barres, `   214`), champs personnalisés | à faire |

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
- **M3** (fait, § 11) : ouvrir un `.mpp` comme un `.xml` (même chemin de chargement) ; le rapport est un bandeau et une fenêtre des différences plutôt que le panneau des diagnostics (ADR-0038).
- Faire relire au propriétaire quelques-uns de ses propres `.mpp` (hors dépôt, comme les plannings R1 à R6 de l'import PDF) avant M3 : non fait au lot M3, toujours à faire.

## 10. Lot M2 : parité des dates sur les fichiers réels (2026-11-15)

Décision : ADR-0037. Question : sur les 41 fichiers réels, le moteur ne retrouvait les dates enregistrées par MS Project que pour 2 255 tâches sur 2 966, presque autant en partant du XML du corrigé : l'écart venait du calcul ou de données non reprises.

### 10.1 Méthode

- Outil `diagnose_roots` (`crates/plannercraft-mpp/tests/measure.rs`, ignoré, hors CI) : une ligne par tâche (écarts de début et de fin, indices de la lecture et du corrigé). Une tâche est **racine** si elle est en écart réel (plus de 2 minutes) alors que ses prédécesseurs, ceux de ses récapitulatives (ses successeurs à rebours) et, pour une récapitulative, ses sous-tâches sont justes à 2 minutes près ; sinon elle est **héritée**. Chaque héritée est attribuée à la cause des racines qui la pilotent.
- Les causes sont établies sur les champs du XML du corrigé et des fichiers (scripts d'analyse hors dépôt), puis classées : (a) donnée non lue dans le `.mpp` ; (b) donnée lue mais ignorée par l'import ; (c) règle du moteur absente ou différente ; (d) non reproductible par un calcul (valeur saisie, nivellement, état laissé par l'utilisateur).
- Le rapport d'import liste désormais tous les écarts (plus de plafond de 200) avec leur classe (R-298).

### 10.2 Causes, avant corrections

Tâches en écart réel sur les 41 fichiers réels à la fin du lot M1 : 568 (71 racines, 497 héritées), plus 143 tâches à deux minutes près.

| Cause | Classe | Racines | Héritées | Fichiers | Suite donnée |
|---|---|---|---|---|---|
| Fractions de minute (MS Project calcule en dixièmes de minute, le moteur en minutes ; un écart d'une minute franchit une fin de journée et devient un jour) | c | 8 | 191 | 2 | analysée, non corrigée (ADR-0037) |
| Récapitulative manuelle : ses sous-tâches partent de son début saisi, ses liens entrants ne passent pas | c | 26 | 142 | 7 | corrigée (R-293) |
| Ressources, affectations, calendriers de ressource non lus | a | 9 | 103 | 5 | corrigée (R-294 à R-296) |
| Profil d'affectation modifié à la main | d | 5 | 33 | 4 | signalée (R-298) |
| Retard ou fractionnement de nivellement | d | 3 | 14 | 3 | non reproductible |
| Jalon contraint (FNLT, SNET, ALAP) | c | 3 | 3 | 3 | non corrigée, cause exacte inconnue |
| Affectations décalées sans retard enregistré (nivellement par affectation) | d | 4 | 0 | 1 | signalée (R-298) |
| Début enregistré sans lien ni contrainte qui l'explique | d | 2 | 1 | 2 | signalée (R-298) |
| Travail fixe et unités fractionnaires (durée enregistrée ≠ travail ÷ unités) | c | 2 | 1 | 2 | non corrigée, cause inconnue |
| Tâche commencée et fractionnée (reprise) | c | 1 | 1 | 1 | non corrigée |
| Inconnue | ? | 8 | 8 | 2 | — |

Sur le corpus de test MPP14, une cause de plus : les profils non uniformes (`WorkContour` 1 à 8), où MS Project enregistre une durée de tâche qui n'est pas travail ÷ unités ; corrigée par R-297 une fois les affectations lues.

### 10.3 Corrections et mesure

| Étape | Fichiers réels (2 966 tâches) | Corpus de test MPP14 (2 221) | Corpus MSPDI (159 cas) | Relecture (75) |
|---|---|---|---|---|
| Fin du lot M1 | 2 255 | 2 179 | 156 | 74 |
| R-293, plancher des sous-tâches d'une récapitulative manuelle | 2 335 | 2 179 | 156 | 74 |
| R-293, liens de la récapitulative manuelle non hérités | 2 358 | 2 179 | 156 | 74 |
| R-294 à R-296, ressources, affectations, calendriers de ressource | 2 423 | 2 167 | 156 | 74 |
| R-297, durée d'une affectation à profil non uniforme | 2 423 | 2 188 | 156 | 74 |

Comparaison : même calcul sur le projet relu depuis le XML du corrigé, avec ses ressources : 2 417 sur les fichiers réels, 2 189 sur le corpus de test. Classes après corrections (R-298) :

| | Identiques à la minute | À deux minutes près | Écart réel | Dont cause non recalculable constatée |
|---|---|---|---|---|
| Fichiers réels | 2 423 | 165 | 378 | fractions de minute 238, profil modifié à la main 6, affectations décalées 5, début inexpliqué 1 |
| Corpus de test MPP14 | 2 188 | 0 | 33 | profil modifié à la main 10, affectations décalées 1, début inexpliqué 1 |
| Relecture | 74 | 0 | 1 | — |

Lecture des ressources et affectations contre le corrigé (fichiers réels) : ressources 598/598 présentes, nature 581/598 (17 étiquettes d'unité matérielle non lues), unités maximales 596/598, calendrier 587/598 ; affectations 4 096 présentes sur 4 225 (les 129 absentes sont des affectations de récapitulatives que le corrigé fabrique, § 15 de `docs/formats/mpp.md`), unités et profil 4 096/4 096, travail 4 072/4 096, retard 4 088/4 096 ; calendriers de ressource 632/632 (base, semaine, exceptions). Corpus de test : ressources 909/909, affectations 2 197/2 241, calendriers de ressource 1 068/1 070.

### 10.4 Par fichier (fichiers réels en écart avant ou après)

| Fichier | Fin M1 | Après M2 | À 2 min près | Écart réel |
|---|---|---|---|---|
| `4jvarun__Total-Quality-Project-Management__Project_V2v_Communications.mpp` | 29 / 72 | 72 / 72 | 0 | 0 |
| `advaith14__Project-Management-for-World-Hotels-__IST645F18A2BKamathA.mpp` | 14 / 330 | 14 / 330 | 143 | 173 |
| `AmaalAyman__Project-Management__Pm_Project_1.mpp` | 15 / 20 | 14 / 20 | 0 | 6 |
| `AtharvaTK1997__Software-Project-Management-Assignment-1__assign_1_20%.mpp` | 1 / 62 | 1 / 62 | 22 | 39 |
| `chandana7373__branding-and-website-strategy-project-capstone__Capstone_Project_Integrated_Plan_Latinas_Outreach.mpp` | 37 / 83 | 81 / 83 | 0 | 2 |
| `davidramosf__Planificacion-de-un-proyecto-utilizando-MS-Project__Paso_1.mpp` | 48 / 61 | 25 / 61 | 0 | 36 |
| `Dimitrije-Sokolovic__Guitar-Shop-Management-Project__GuitarShop.mpp` | 42 / 64 | 42 / 64 | 0 | 22 |
| `HamzaMalikProjects__CPS714-Software-Project-Management__CPS714_Lab6.mpp` | 55 / 56 | 55 / 56 | 0 | 1 |
| `Kalani0056__ms-project-planning__st20357430_CIS6011.mpp` | 34 / 41 | 34 / 41 | 0 | 7 |
| `lautarofaccini__Proyecto-Planificacion__Proyect_POS_Corralon_M_-_G3_-_SEGUIMIENTO.mpp` | 37 / 44 | 37 / 44 | 0 | 7 |
| `MaralSmi__residential-construction-project-planning__Maral_Esmikhani_MSP_projrect.mpp` | 176 / 176 | 172 / 176 | 0 | 4 |
| `Masetto97__PLANIFICACION_DE_PROYECTOS__Plan_de_Proyecto_E11.mpp` | 57 / 69 | 57 / 69 | 0 | 12 |
| `nehathota123__projectmanagement__ISM4314_–_MS_Project_Plan_for_Gress_J_&_Thota_N_(1).mpp` | 55 / 83 | 46 / 83 | 0 | 37 |
| `salemsolomon25__project-server-2019-deployment-plan__project_server_2019_deployment_plan.mpp` | 7 / 27 | 17 / 27 | 0 | 10 |
| `ShidehParastariFarkoush__Project-Control-Project__PC-MSP-Phase2-Code.mpp` | 32 / 66 | 66 / 66 | 0 | 0 |
| `Smruti0708__Residential-Construction-Simulation__Residential_Construction_Project_Simulation.mpp` | 38 / 102 | 83 / 102 | 0 | 19 |
| `svsuriyaa__Project-Management--Mitigating-Systematic-Biases-in-Project-US-Air-force-Firefly__Gantt_Chart_Team7.mpp` | 73 / 80 | 80 / 80 | 0 | 0 |
| `tefsantana__uala-digital-project-management-case-study__uala-project-schedule.mpp` | 114 / 120 | 118 / 120 | 0 | 2 |
| `ubhurase__IPM__Big_Pharma_Case_Project_Final.mpp` | 105 / 106 | 105 / 106 | 0 | 1 |
| `vincenzorusso3__uniseats__C06_SA_Vers.1.1.mpp` | 88 / 96 | 96 / 96 | 0 | 0 |
| `zahmed02__CS4044-Software-Project-Management__CS4044_FSPM_Project_Schedule.mpp` | 217 / 227 | 227 / 227 | 0 | 0 |

25 fichiers sur 41 tout identiques (21 à la fin du lot M1). Quatre fichiers perdent des tâches en lisant les affectations, comme le même calcul sur le XML du corrigé avec ses ressources : `davidramosf…` (−23 : affectations à travail fixe décalées les unes après les autres, sans retard enregistré, laissées par un nivellement par affectation), `nehathota123…` (−9 : travail fixe à unités fractionnaires, MS Project enregistre 9 h là où travail ÷ unités donne 10 h, cause inconnue), `MaralSmi…` (−4 : une affectation de 48 h à 100 % sur une tâche de 8 h, enregistrée sur 8 h : valeurs incohérentes laissées dans le fichier), `AmaalAyman…` (−1). Le plancher du manifeste de `MaralSmi…` passe de 176 à 172 ; les autres planchers du manifeste montent (`zahmed02…` 217 → 227, `tefsantana…` 114 → 118).

### 10.5 Ce qui reste en écart réel (fichiers réels, 378 tâches)

| Cause | Classe | Racines | Héritées | Fichiers |
|---|---|---|---|---|
| Fractions de minute (avec 165 tâches à deux minutes près) | c | 12 | 226 | 3 |
| Retard ou fractionnement de nivellement (dont `davidramosf…`) | d | 6 | 40 | 4 |
| Profil d'affectation modifié à la main | d | 5 | 33 | 4 |
| Inconnue | ? | 5 | 13 | 4 |
| Travail fixe et unités fractionnaires (`nehathota123…`) | c | 8 | 3 | 1 |
| Début enregistré inexpliqué | d | 3 | 7 | 2 |
| Tâche commencée et fractionnée (`Dimitrije-Sokolovic…`, `tefsantana…`) | c | 2 | 4 | 2 |
| Rebours : début enregistré à la fin de la période précédente (`Kalani0056…` UID 24, 42, 168, 185 : 17:00 la veille au lieu de 08:00 ; les 20 autres feuilles du fichier sont justes à 08:00, et les 4 ont une affectation de travail à 0 unité) | c | 4 | 0 | 1 |
| Jalon contraint (`chandana7373…` UID 272, 275 : FNLT, placés au début du projet par MS Project ; `AmaalAyman…` UID 8 ; `ubhurase…` UID 171) | c | 4 | 0 | 3 |
| Récapitulative manuelle dans un projet planifié à rebours (`Kalani0056…` UID 3, au début saisi de sa récapitulative ; R-293 ne joue que depuis le début) | c | 1 | 2 | 1 |

### 10.6 Arrondi

MS Project calcule en dixièmes de minute et les fichiers le montrent : `advaith14…` UID 80 a une durée de 1 125 dixièmes (`PT1H52M30S`), un début à 09:27:12 et une fin à 11:19:42, exactement 112,5 minutes plus tard ; son affectation a 67,5 minutes de travail à 60 %. Les dates affichées sont tronquées à la minute (R-283), les durées gardées en dixièmes. Le moteur compte en minutes entières (règle 5) : chaque tâche arrondit sa durée (R-001), l'écart s'accumule le long des chaînes (1 à 2 minutes), puis franchit une fin de journée et devient un écart d'une nuit (902 minutes chez `advaith14…`). 4 fichiers réels sur 41 sont concernés (`advaith14…`, `AtharvaTK1997…`, `nehathota123…`, `lautarofaccini…`), environ 400 tâches. Analyse, coût et options : ADR-0037 ; rien n'est changé dans le moteur en attendant l'arbitrage.

### 10.7 Limites et suite

- Le seuil d'arrondi de 2 minutes est une convention de rapport (R-298), pas une tolérance de calcul.
- Causes non recalculables : constatées sur la tâche elle-même ; une tâche héritée n'en reçoit pas (elle suit sa racine).
- Non lus : répartition saisie d'un profil modifié à la main, retards d'affectation en disposition Project 2010, étiquette des ressources matérielles, coûts.
- Suite proposée : l'arbitrage de l'ADR-0037 (dixièmes de minute) ; puis M3 (interface), avec le rapport d'écarts affiché par cause.

## 11. Lot M3 : ouvrir un `.mpp` dans l'application (2026-11-15)

Décision : ADR-0038. Interface : `docs/features/ui.md` § 1.56 (H-UI-230 à H-UI-236). Guide : `docs/user/ms-project-export.md` § 1. Règle : R-299.

### 11.1 Parcours

« Ouvrir » (menu, raccourci, accueil, glisser-déposer ; bureau et web) accepte `.mpp` et `.mpt` ; le type se reconnaît au contenu (signature `[MS-CFB]`). Le planning s'affiche tout de suite, comme un nouveau planning importé (nom du fichier sans extension, `.mpt` : « Projet1 », aucun fichier lié, « Enregistrer » en XML). Un bandeau dit ce qui a été ouvert et, s'il y a lieu, combien de tâches diffèrent de MS Project (écart réel, R-298) ; « Voir » ouvre la fenêtre « Différences avec MS Project » (aussi au menu Fichier tant que le planning n'est pas modifié), dont l'action unique « Garder les dates de MS Project » passe les feuilles en écart en planification manuelle avec les dates enregistrées. Refus clairs pour les formats 2007 et plus anciens, les fichiers abîmés ou protégés et les autres documents Office.

### 11.2 Mesure de « Garder les dates » (R-299)

`MPP_MEASURE_DIR=… cargo test --release -p plannercraft-mpp --test measure keep_measure -- --ignored --nocapture` : tâches en écart réel avant, tâches gardées, écart réel après (projet relu avec les tâches gardées en manuel).

| | Écart réel avant | Tâches gardées | Écart réel après | Temps (release) |
|---|---|---|---|---|
| 41 fichiers réels (16 avec écart) | 378 | 277 | 2 | 5 ms au plus par fichier (`advaith14…`, 330 tâches) |
| Corpus de test MPP14 (164 fichiers) | 33 | 33 | 0 | 0,8 s pour tout le corpus |

Les deux écarts restants sont des récapitulatives, qui ne se gardent pas : `Kalani0056…` UID 189 (fin enregistrée lue autrement par le corrigé, § 7) et `ubhurase…` UID 121 (fin enregistrée à 08:00 le lendemain de la fin de ses sous-tâches). Les écarts d'arrondi ne sont pas gardés : 65 tâches à deux minutes près après l'action sur les fichiers réels, contre 165 avant (déduit : des tâches à l'arrondi retombent juste quand leurs prédécesseurs gardent les dates enregistrées).

Par l'application (`tests/ui/mpp_open.rs`, `h_ui_230_corpus_files_open`, chemin réel `App::open_path`, puis le lot de l'action) : 41 fichiers réels ouverts sur 41, 16 avec « Garder les dates », aucun écart réel restant hors récapitulatives, aucun lot refusé ; corpus versionné : 52 ouverts, 5 refusés avec message (formats anciens), 8 avec l'action ; 76 fichiers de test pris au hasard : 31 ouverts, 45 refusés avec message, 3 avec l'action.

### 11.3 Taille de la version web

`.wasm` de `cargo build --profile wasm-release -p plannercraft-web --target wasm32-unknown-unknown`, sections personnalisées retirées, avant wasm-bindgen et `wasm-opt` (même méthode qu'ADR-0033 D3), gzip -9 : `main` (`9934298`) 5 652 738 octets, lot M3 5 699 766 octets (+47 028, +0,8 %), sous la porte C-REL-05 de 6 Mio. Le lecteur `.mpp` était déjà dans le lien de la CLI, pas dans le web : c'est son poids. La mesure de référence est celle du job web de la CI (trunk, `wasm-opt`).

### 11.4 Limites et suite

- Rien n'est vérifié dans un navigateur réel par ce lot (chemins web compilés en `wasm32-unknown-unknown`, logique testée en natif) ni contre MS Project.
- Diagnostics de lecture `mpp.*` hors du panneau des diagnostics ; compte rendu perdu au rechargement de la page.
- Suite : M5 (formats 2007 et 2003, si des fichiers du propriétaire l'exigent), M4 (gabarits avec leurs colonnes et leurs styles, champs personnalisés) ; mesure sur des plannings de chantier du propriétaire, toujours à faire.
