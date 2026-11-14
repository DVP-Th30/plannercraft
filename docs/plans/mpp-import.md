# Import des fichiers `.mpp` de MS Project (lots M1 à M4)

Décision : ADR-0036. Règles : R-281 à R-291 (`docs/scheduling-rules.md`). Faits de format, chacun avec sa preuve : `docs/formats/mpp.md`. Code : `crates/plannercraft-mpp` (lecture seule, sans interface, compile en `wasm32-unknown-unknown`), CLI `plannercraft import-mpp`.

## 1. Besoin

Les plannings échangés entre entreprises du bâtiment circulent souvent en `.mpp`, le format binaire de MS Project, et non en XML. Le brief mettait `.mpp` hors périmètre ; le propriétaire a demandé la compatibilité le 2026-11-14 et autorisé l'analyse du format (ADR-0036 D1). On ne lit que ; on n'écrira jamais de `.mpp`.

## 2. Lots

| Lot | Contenu | État |
|---|---|---|
| **M1** | Lecteur MPP14 (Project 2010 à 2021, Microsoft 365) : tâches, liens, calendriers de base, propriétés du projet ; refus clair des autres formats ; CLI `import-mpp` ; corpus public et mesure contre un corrigé | livré (ce document) |
| **M2** | Ressources, affectations, calendriers de ressource ; exceptions périodiques des calendriers ; réels (début, fin, durée) ; retard de nivellement ; MPP12 (Project 2007), puis MPP9 et MPP8 si la demande existe | à faire |
| **M3** | Ouverture d'un `.mpp` depuis l'application bureau et la version web (fichier déposé dans le navigateur, lu localement) : même chemin que l'ouverture d'un XML, rapport dans les diagnostics | à faire |
| **M4** | Gabarits et présentation : affichages, tables, styles de barres (`   214`), champs personnalisés ; fichiers `.mpt` | à faire |

## 3. Chaîne de lecture (M1)

Entrée : des octets (aucun système de fichiers). Sortie : `MppDocument` (projet du cœur avec les **entrées** seules, dates **enregistrées** à part, provenance, diagnostics), puis comparaison au calcul du moteur.

1. **Conteneur** (`cfb.rs`) : fichier composé lu depuis `[MS-CFB]` (en-tête, DIFAT, FAT, répertoire, mini FAT, mini flux), chaînes bornées, boucles détectées (R-281).
2. **Format** (`detect.rs`) : dossier `   114`, `   112`, `   19`/`   29`, `   1`/`   2` ; version d'enregistrement dans `Props14`, qui choisit la disposition Project 2010 ou Project 2013 (R-282).
3. **Tables** (`tables.rs`) : `FixedMeta`/`FixedData`, `Fixed2Meta`/`Fixed2Data`, `VarMeta`/`Var2Data`, `Props`.
4. **Projet** (`project.rs`) : propriétés (R-288), calendriers de base (`calendar.rs`, R-287), tâches (`task.rs`, R-283 à R-285, R-289, R-290), liens (R-286). Décalages par disposition dans `layout.rs`.
5. **Comparaison** (`compare.rs`, R-291) : le moteur calcule, chaque tâche est comparée à la minute aux dates enregistrées.
6. **Rapport** (`report.rs`) : texte et JSON (comptes, non repris, diagnostics par code, comparaison).

CLI : `plannercraft import-mpp projet.mpp -o projet.xml [--json] [--dry-run]` ; MSPDI écrit par la session (dates du moteur), écriture atomique ; refus : code de sortie 4 ; fichier illisible : 3.

## 4. Corpus et corrigé

- **Corpus d'analyse** (hors dépôt) : 609 fichiers `.mpp` de test publics du dépôt `joniles/mpxj` (commit `4982974b2594ffc7146cdb9ebdf2ae46186737ca`), extraits seuls (aucun code) : 164 MPP14 (80 enregistrés par Project 2010, 84 par Project 2013 et suivants), 178 MPP12, 238 MPP9, 29 MPP8. Le sous-dossier `generated/` fait varier un champ par dossier, un fichier par version de MS Project.
- **Corrigé** : chaque fichier converti en MSPDI par MPXJ **lancé comme outil externe** (son code et sa documentation ne sont pas lus, ADR-0036 D1), XML relu par `plannercraft-mspdi`. **Limite** : c'est la lecture d'un autre programme, pas celle de MS Project ; un écart peut venir de lui (exemples au § 6). Rien n'est vérifié contre MS Project lui-même (aucune licence).
- **Corpus versionné** : `tests/corpus-mpp/manifest.toml`, 45 fichiers choisis (40 MPP14 des deux dispositions : dates, liens, durées, indicateurs, textes, calendriers, exceptions simples et périodiques, propriétés, lignes vides, suppressions, notes, échéance, tâches manuelles ; 5 refus MPP12, MPP9, MPP8), URL brute figée sur le commit, taille, SHA-256 ; `cargo xtask corpus fetch-mpp` ; test `crates/plannercraft-mpp/tests/corpus.rs` (comptes et début du projet lus, plancher des tâches identiques au calcul) ; job CI `corpus-mpp`. Valeurs attendues lues par ce lecteur après vérification contre le corrigé, jamais copiées du XML du corrigé.

## 5. Mesure (2026-11-14)

Commande : `MPP_MEASURE_DIR=… MPP_ORACLE_DIR=… cargo test -p plannercraft-mpp --test measure -- --ignored --nocapture` (test ignoré, hors CI), sur les 609 fichiers.

**Fichiers** : 164 MPP14 lus sur 164 (0 refusé) ; 445 refusés avec la version nommée (`mpp.unsupported_version` : 178 MPP12, 238 MPP9, 29 MPP8). MPP12 n'est pas lu : ses enregistrements de tâche font 264 octets (`docs/formats/mpp.md` § 2), il demande sa propre analyse (lot M2).

**Champs, contre le corrigé** (A = enregistrés par Project 2010, 80 fichiers ; B = par Project 2013 et suivants, 84 fichiers) :

| Champ | Égaux / comparés | Taux | A | B |
|---|---|---|---|---|
| Tâche présente (UID) | 2 237 / 2 239 | 99,9 % | 858 / 860 | 1 379 / 1 379 |
| Nombre de tâches par fichier | 163 / 164 | 99,4 % | 79 / 80 | 84 / 84 |
| Ordre des tâches (UID par ID) | 162 / 164 | 98,8 % | 78 / 80 | 84 / 84 |
| Ligne vide | 2 237 / 2 237 | 100 % | 858 / 858 | 1 379 / 1 379 |
| Nom | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Niveau | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Durée (minutes) | 2 221 / 2 224 | 99,9 % | 849 / 850 | 1 372 / 1 374 |
| Unité de durée (unité, base, estimée) | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Début enregistré | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Fin enregistrée | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Jalon | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| % achevé | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Planification manuelle | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Valeurs saisies d'une tâche manuelle | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Active | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Contrainte (type et date) | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Échéance | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Notes | 2 223 / 2 224 | 100 % | 849 / 850 | 1 374 / 1 374 |
| Calendrier de la tâche | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Priorité | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Type de tâche | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Liens (prédécesseurs d'une tâche, type, décalage) | 2 224 / 2 224 | 100 % | 850 / 850 | 1 374 / 1 374 |
| Projet : sens et date de planification | 164 / 164 | 100 % | 80 / 80 | 84 / 84 |
| Projet : titre | 164 / 164 | 100 % | 80 / 80 | 84 / 84 |
| Projet : minutes par jour, par semaine, jours par mois | 164 / 164 | 100 % | 80 / 80 | 84 / 84 |
| Projet : heure de début par défaut | 164 / 164 | 100 % | 80 / 80 | 84 / 84 |
| Projet : calendrier (nom) | 163 / 164 | 99,4 % | 79 / 80 | 84 / 84 |
| Calendrier de base présent | 206 / 224 | 92,0 % | 107 / 112 | 99 / 112 |
| Calendrier : nom | 204 / 206 | 99,0 % | 105 / 107 | 99 / 99 |
| Calendrier : semaine | 206 / 206 | 100 % | 107 / 107 | 99 / 99 |
| Calendrier : exceptions | 197 / 206 | 95,6 % | 106 / 107 | 91 / 99 |
| Non repris : nombre de ressources annoncé | 160 / 164 | 97,6 % | 77 / 80 | 83 / 84 |
| Non repris : nombre d'affectations annoncé | 141 / 164 | 86,0 % | 63 / 80 | 78 / 84 |

Peu d'exemples positifs pour certains champs (le taux de 100 % ne dit pas tout) : 23 jalons, 4 tâches inactives (2 fichiers), 2 échéances, 31 contraintes datées (dont 3 dont la date diffère du début), 15 calendriers de tâche, 62 liens, 13 lignes vides ; aucune tâche B dont le % achevé diffère du % travail achevé (`docs/formats/mpp.md` § 7).

**Moteur** : tâches dont le début et la fin recalculés égalent, à la minute, ceux enregistrés par MS Project : **2 157 sur 2 220** (97,2 %) ; A 797 / 848, B 1 360 / 1 372. Même calcul sur le projet relu depuis le XML du corrigé (qui porte aussi ressources, affectations et retards de nivellement) : 2 168 / 2 222. Les tâches perdues par rapport au corrigé sont dans des fichiers à ressources et affectations (non reprises en M1 : `mpp14barstyle.mpp` 1/7 contre 7/7, où cinq tâches sont décalées par un retard de nivellement non lu ; `mpp14timephased.mpp` 15/40 contre 25/40 ; `timephased-budget*.mpp`) ; d'autres en gagnent (`mpp14timephasedsegments.mpp` 19/20 contre 11/20, `timephased-multi-rate.mpp` 10/12 contre 7/12).

**Robustesse** : 164 fichiers MPP14 × 150 versions tronquées ou altérées (graine fixe) = 24 600 lectures, aucune panique (build debug, débordements contrôlés), 31 s ; 1 200 versions altérées de fichiers synthétiques ; conteneurs hostiles (FAT bouclée, nombre de secteurs démesuré, en-tête seul). La crate `cfb` (écartée, ADR-0036 D2) a été passée sur 6 000 mutations de 60 fichiers : aucune panique non plus.

**Temps** : 609 fichiers lus et comparés en moins de 5 s en debug (mesure, 2 fois par fichier : lecteur et corrigé relu).

## 6. Écarts où le corrigé est soupçonné

- 9 calendriers de base « sans nom » d'un fichier (`ResourceIdAndUniqueId-project2013-mpp14.mpp`) : enregistrements marqués supprimés, gardés par le corrigé.
- 2 calendriers de base nommés par le corrigé d'après une ressource (`mpp14assignmentfields.mpp`, `timephased-prorated-cost-resource.mpp`) : sans nom dans le fichier.
- Exceptions simples écrites « quotidiennes, tous les 1 jour » par le corrigé, « période » par le lecteur : même effet (la mesure les tient pour égales).

Écarts dus au lecteur ou au fichier :

- `assignment/assignment-deletion/sf248.mpp` (fichier d'un rapport d'anomalie) : table des tâches incohérente ; 1 tâche lue sur 3, durée et notes d'une tâche fausses, calendrier du projet non trouvé. Le seul fichier où le lecteur se trompe sur la structure.
- `task/task-deletion/task-deletion2-project2010-mpp14.mpp` : après des suppressions, Project 2010 laisse un ID ancien dans un enregistrement : une tâche et une ligne vide permutées (`docs/formats/mpp.md` § 11).
- Exceptions périodiques non lues (9 calendriers) : signalées (`mpp.calendar_recurring_skipped`) ; les exceptions simples qui les précèdent sont gardées.
- Durée de deux tâches à affectations découpées (`mpp14timephasedsegmentsmanualoffsets.mpp`) : cause inconnue.
- Nombres de ressources et d'affectations « non repris » : comptes approchés (enregistrements vivants), annoncés comme tels.

## 7. Limites connues (M1)

- Formats : MPP12, MPP9, MPP8 refusés ; `.mpt` et fichiers protégés par mot de passe : aucun échantillon, comportement inconnu (un gabarit sans dossier `   114` serait refusé comme « pas un projet MS Project »).
- Non repris : ressources, affectations, calendriers de ressource, lignes de base, réels, travail, coûts, champs personnalisés, retard de nivellement, affichages et styles ; comptés quand on sait les compter.
- Calendriers : exceptions périodiques, semaines datées (aucune dans le corpus).
- Ordre des tâches après suppression dans un fichier enregistré par Project 2010 (§ 6).
- Le corpus est un corpus de test (petits projets) : aucun planning de chantier réel n'a été lu ; à mesurer sur des fichiers réels avant M3.

## 8. Recommandations

- **M2** : commencer par les ressources et affectations (les écarts du moteur viennent surtout de là), avec les champs du corrigé et `generated/resource-*`, `generated/assignment-*` ; puis les exceptions périodiques (`generated/calendar-recurring-exceptions` donne 27 exceptions de chaque type) ; MPP12 (178 fichiers au corpus, même conteneur, enregistrements de 264 octets) ensuite.
- **M3** : ouvrir un `.mpp` comme un `.xml` (même chemin de chargement), rapport de lecture dans le panneau des diagnostics ; sur le web, lecture locale du fichier déposé.
- Faire relire au propriétaire quelques-uns de ses propres `.mpp` (hors dépôt, comme les plannings R1 à R6 de l'import PDF) avant M3.
