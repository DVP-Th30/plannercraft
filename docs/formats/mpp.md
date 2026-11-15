# Format des fichiers `.mpp` (MPP14) : notes d'observation

Lot M1 (ADR-0036). Ce document est la seule source des faits de format qu'utilise `crates/plannercraft-mpp`. Il prouve la propreté du travail (clean-room) : **chaque fait** porte son statut et sa preuve.

- **observé** : fichier (chemin relatif à `junit/data/` du dépôt public `joniles/mpxj`, commit `4982974b2594ffc7146cdb9ebdf2ae46186737ca`, ou « réel : » suivi du nom d'un fichier réel de § 13), flux, décalage, octets lus, et la valeur du corrigé qui leur correspond ;
- **déduit** : règle généralisée, avec le nombre de fichiers ou d'enregistrements sur lesquels elle tient ;
- **inconnu** : non établi ; le lecteur ne le lit pas, ou le signale.

## 0. Méthode et sources

- **Sources publiques** : `[MS-CFB]` (conteneur, § 1), `[MS-OLEPS]` (non utilisé dans ce lot), énumération `PjField` de la documentation Microsoft (noms des identifiants de champ, § 3), schéma MSPDI (sens des codes numériques de format de durée, de type de lien, de contrainte et de tâche, déjà tabulés dans `plannercraft-mspdi::codes`).
- **Données** : 609 fichiers `.mpp` de test publics, puis 48 fichiers réels publics (§ 13), lus comme des données (liste du conteneur, vidages hexadécimaux, recherche de valeurs connues, corrélation sur l'ensemble du corpus, comparaison de fichiers qui diffèrent d'un champ). Les fichiers ne sont jamais commités ; une sélection de 57 est décrite par `tests/corpus-mpp/manifest.toml`.
- **Corrigé** : chaque fichier a été converti en MSPDI par MPXJ lancé comme outil externe (jamais son code ni sa documentation). Le XML relu donne la valeur attendue de chaque champ. Ce corrigé est la lecture d'un autre programme, pas celle de MS Project : un écart peut venir de lui (§ 12).
- **Outils** : scripts Python d'analyse hors du dépôt (`olefile` pour lister le conteneur, corrélations octet par octet contre le XML du corrigé). Aucun outil n'a été appliqué au programme MS Project ; aucune description du format venue d'un autre lecteur n'a été lue.
- **Mesure** : test ignoré `crates/plannercraft-mpp/tests/measure.rs`, chiffres dans `docs/plans/mpp-import.md`.

Abréviations : « A » = disposition des fichiers enregistrés par Project 2010 (version 14, 80 fichiers MPP14 du corpus) ; « B » = disposition des fichiers enregistrés par Project 2013 et suivants (versions 15 et 16, 84 fichiers). Entiers petit-boutistes.

## 1. Conteneur

- **observé** (tous les fichiers) : signature `D0 CF 11 E0 A1 B1 1A E1`, fichier composé `[MS-CFB]` (versions 3 et 4). Lecture faite d'après la spécification : en-tête, DIFAT, FAT, répertoire, mini FAT et mini flux (`src/cfb.rs`).
- **observé** : noms de flux de la racine comme `\x01CompObj`, `\x05SummaryInformation`, `Props14`, et dossiers dont le nom commence par trois espaces (`   114`, `   214`).

## 2. Format et version d'enregistrement

- **observé** : `generated/task-dates/task-dates-project2010-mpp14.mpp` a les dossiers `   114` et `   214` ; `…-mpp12.mpp` a `   112` et `   212` ; `…-project2003-mpp9.mpp` a `   19` et `   29` ; `…-project98-mpp8.mpp` a `   1` et `   2`.
- **déduit** (609 fichiers) : dossier `   114` = MPP14 (164 fichiers), `   112` = MPP12 (178), `   19`/`   29` = MPP9 (238), `   1`/`   2` = MPP8 (29). Aucun fichier n'a deux familles.
- **observé** : le flux `Props14` de la racine contient en UTF-16 « 14,0,7011,1000 » (`generated/task-dates/task-dates-project2010-mpp14.mpp`), « 15,0,4569,1504 » (`…-project2013-mpp14.mpp`), « 16,0,6366,2062 » (`…-project2016-mpp14.mpp`), « 16,0,10730,20102 » (`…-project2019-mpp14.mpp`).
- **déduit** (164 fichiers MPP14) : le premier numéro majeur de `Props14` décide de la disposition : 14 → A (taille des enregistrements de tâche 206 octets), 15 ou 16 → B (202 octets). La version du `SummaryInformation` ne suffit pas : `calendar-exception-precedence.mpp` y annonce 14 mais `Props14` dit 15 et ses enregistrements suivent B. Quand `Props14` porte deux versions (`mpp14timephasedsegmentsmanual.mpp` : « 16,0,17830,20138 » puis « 14,0,4751,1000 »), la première est la bonne (enregistrements B).
- **observé** (104 fichiers MPP12 de `generated/`) : les enregistrements de tâche de `   112/TBkndTask/FixedData` font 264 octets dans les 69 fichiers qui ont des tâches (206 ou 202 en MPP14) : MPP12 ne se lit pas avec les décalages de MPP14 ; il est reconnu et refusé dans ce lot.
- **observé** (un seul échantillon, réel, § 13) : un fichier `.mpt` (gabarit) a le même conteneur et le même dossier `   114` qu'un `.mpp`.
- **inconnu** : fichier MPP14 protégé par un mot de passe : aucun dans le corpus (les trois fichiers protégés du corpus sont des MPP9, refusés comme tels).

## 3. Valeurs variables (`VarMeta`, `Var2Data`)

- **observé** (`mpp14relations.mpp`, `   114/TBkndTask/VarMeta`, 744 octets) : en-tête `ba ad df fa 00 00 00 00 3c 00 00 00 00 00 00 00 00 00 00 00 44 02 00 00` : 60 (`0x3c`) entrées, taille de `Var2Data` 580 (`0x244`) ; puis entrées de 12 octets `[u32 UID][u32 décalage dans Var2Data][u32 clé]`, par exemple `01 00 00 00 84 00 00 00 0e 00 40 0b` (UID 1, décalage `0x84`, clé `0x0B40000E`).
- **observé** (même fichier, `Var2Data` à `0x84`) : `0e 00 00 00` puis « Task 1 » en UTF-16 terminé par un nul (14 octets) ; le corrigé nomme la tâche UID 1 « Task 1 ».
- **déduit** (2 224 noms de tâche, 164 fichiers, tous égaux au corrigé) : valeur = `[u32 longueur][octets]` ; la clé `0x0B40000E` porte le nom (UTF-16, nul final). Les clés de tâche coïncident avec les identifiants `PjField` (`pjTaskName` = 188743694 = `0x0B40000E`) ; d'autres clés aussi (`0x0B400505` `pjTaskStartText`), sans qu'on s'en serve.
- **déduit** (notes de `mpp14task.mpp`, `mpp14embedded.mpp`, `mpp14timephasedsegmentsmanualoffsets.mpp` : 2 223 tâches sur 2 224 égales au corrigé) : la clé `0x0B40000F` (`pjTaskNotes`) porte les notes en RTF (`{\rtf1\ansi…`) ; le texte est tiré du RTF d'après la spécification RTF publiée par Microsoft (`src/rtf.rs`).
- **observé** (réel : `AmaalAyman__Project-Management__Pm_Project_1.mpp`, `TBkndTask/VarMeta`) : l'en-tête annonce 206 entrées pour 240 présentes ; les 34 de plus désignent des enregistrements périmés. Le lecteur lit les entrées annoncées seulement (comme pour les tables fixes, § 4).
- **déduit** (calendriers, 435 noms) : dans `TBkndCal`, clé `0x0D400001` = nom (UTF-16), clé `0x0D400008` = semaine et exceptions (§ 10).
- **observé** : la clé `0x0B40011B` (`pjTaskFinish6`) porte une date : dans `mpp14task.mpp` elle égale l'échéance du corrigé (2006-08-30 17:00), mais dans `generated/task-finishes/task-finishes-project2010-mpp14.mpp` (champ Fin6 rempli, sans échéance) elle porte Fin6. Ce n'est donc pas l'échéance (§ 7).

## 4. Tables fixes (`FixedMeta`, `FixedData`, `Fixed2Meta`, `Fixed2Data`)

- **observé** (`mpp14relations.mpp`, `TBkndTask/FixedMeta`, 439 octets) : en-tête `ba ad df fa 04 00 00 00 09 00 00 00 04 05 00 00` : 9 éléments, taille de `FixedData` 1 284 (`0x504`) ; (439 − 16) / 9 = 47 octets par élément ; chaque élément a ses indicateurs (`u16`, octet 0) et le décalage de son enregistrement dans `FixedData` (`u32`, octet 4) : `00 00 0a 00 30 00 00 00 …`, `00 00 0a 00 fe 00 00 00 …` (enregistrements de 206 octets).
- **observé** (réel : `Practice-11.mpp`, `TBkndTask/FixedMeta`, 1 614 octets) : en-tête `ba ad df fa 04 00 00 00 13 00 00 00 a6 18 00 00` : 19 éléments annoncés, mais (1 614 − 16) / 47 = 34 emplacements ; les emplacements 19 à 33 désignent les mêmes enregistrements que 4 à 18 (copies). L'ancienne règle « taille = (longueur − 16) / nombre » donnait 84 octets et lisait des enregistrements faux (tâches toutes en double à l'UID 0, dates invalides) ; le corrigé y lit 15 tâches.
- **observé** (réel : `AmaalAyman__Project-Management__Pm_Project_1.mpp`) : 24 éléments annoncés, 29 emplacements ; l'emplacement 28 désigne un enregistrement en service (UID 25, ID 23) sans aucune valeur variable (pas de nom), absent des 24 premiers ; le corrigé le garde comme une 21e tâche sans nom. Réel : `Deapry__MS_Project-SamyLao__ProjetFinal_SamyLao.mpp`, `TBkndCons` : 9 éléments annoncés, 20 emplacements ; l'emplacement 14 désigne un lien 57 → 60 en service, absent du corrigé ; avec ce lien, le moteur recalcule faux 4 tâches, sans lui juste les 30.
- **déduit** (164 fichiers de test et 48 réels) : le nombre de l'en-tête est le nombre d'éléments en service ; les emplacements suivants sont des restes (copies ou enregistrements périmés) et ne sont pas lus. Taille d'un élément : constante par table (47 pour `TBkndTask/FixedMeta`, 10 pour `TBkndCons` et `TBkndCal`, 37 pour `TBkndRsc`, 34 pour `TBkndAssn` ; vérifiée sur tous les fichiers où le nombre tombe juste) ; `Fixed2Meta` a autant d'emplacements que `FixedMeta` et sa taille (92 à 96 octets) s'en déduit. Deux éléments en service qui désignent le même enregistrement : le dernier fait foi. Exception : `assignment/assignment-deletion/sf248.mpp` (6 éléments annoncés, 42 emplacements, plusieurs copies contradictoires ; § 12).
- **observé** (`task/task-deletion/task-deletion1-project2013-mpp14.mpp` puis `task-deletion2-…`, même projet après suppression de lignes) : indicateurs `0x0000` = enregistrement complet ; `0x0004` = enregistrement court de 16 octets ; `0x0002` et `0x0006` = enregistrement de tâche supprimée (complet ou court), absente du corrigé.
- **déduit** (164 fichiers) : bit 1 (`0x2`) = supprimé, ignoré ; bit 2 (`0x4`) = enregistrement court. Les trois ou quatre premiers enregistrements de chaque table sont courts et commencent par `n 00 ff ff` (UID ≥ `0xFFFF0000`) : réservés, jamais une tâche ni un calendrier.
- **observé** (`mpp14nulltasks.mpp`) : les tâches UID 2, 5 et 7, lignes vides au corrigé (`IsNull` = 1), ont un enregistrement court : `02 00 00 00 02 00 00 00 …` (UID 2, ID 2).
- **déduit** (13 lignes vides, 5 fichiers) : enregistrement court = ligne vide ; UID à l'octet 0, ID à l'octet 4 (les deux dispositions ; `task-deletion2-project2013-mpp14.mpp` : `08 00 00 00 04 00 00 00` = UID 8, ID 4 au corrigé).
- **déduit** : l'élément de rang k de `Fixed2Meta` décrit la même tâche que celui de `FixedMeta` (164 fichiers de test, 48 réels) ; ses enregistrements (`Fixed2Data`) font 64 octets (160 fichiers de test sur 164 ; les autres ont des enregistrements plus longs, lus aux mêmes décalages) (§ 8).

## 5. Propriétés du projet (`   114/Props`)

- **observé** (`mpp14relations.mpp`, `   114/Props` à `0xda`) : `04 00 00 00 02 00 40 02 04 00 00 00 c0 12 6c 20` : taille 4, clé `0x02400002`, un `u32` (ici 4), puis l'horodatage 2006-09-21 08:00 (§ 6), début du projet au corrigé.
- **observé** (réel : `BigProject.mpp`) : entrée à `0x12e` de taille 297 (impaire), clé `0x0240000B` ; l'entrée suivante commence à `0x264`, un octet plus loin que `0x12e + 12 + 297` : `14 00 00 00 0e 00 40 02 …` (clé `0x0240000E`, « Standard »). Sans ce bourrage, la lecture s'arrêtait après 10 entrées (minutes par jour et calendrier du projet « absents »).
- **déduit** (164 fichiers de test et 48 réels, lus jusqu'au dernier octet) : en-tête de 16 octets, puis entrées `[u32 taille][u32 clé][u32 ?][taille octets]`, une valeur de longueur impaire suivie d'un octet de bourrage ; clés vérifiées contre le corrigé :

| Clé | Valeur | Corrigé | Fichiers égaux |
|---|---|---|---|
| `0x02400002` | horodatage | `StartDate` | 164/164 |
| `0x02400003` | horodatage | `FinishDate` | 164/164 |
| `0x02400004` | `u16` (1 depuis le début, 0 depuis la fin) | `ScheduleFromStart` | 164/164 ; réels 40/40 |
| `0x02400019` | `u16` | `TaskUpdatesResource` | 164/164 ; réels 41/41 |
| `0x0240001A` | `u16` | `SplitsInProgressTasks` | 164/164 ; réels 41/41 (5 fichiers à 0) |
| `0x02400038` et `0x024013CD` | `u16` chacune | `HonorConstraints` | 164/164 et réels 41/41 chacune ; égales dans les 212 fichiers lus, donc indiscernables : lue à 0 seulement si les deux valent 0 (2 fichiers réels) |
| `0x02400025` | `u16`, codes MSPDI | `WeekStartDay` | 164/164 ; réels 40/40 |
| `0x02400045` | horodatage | `StatusDate` | 8 fichiers où elle existe |
| `0x02401388` | 7 jours de 60 octets (§ 10) | semaine des jours « par défaut » des calendriers | 206 + 96 calendriers |
| `0x02400008` | UTF-16 | `Title` | 164/164 |
| `0x0240000E` | UTF-16 | nom du calendrier de `CalendarUID` | 164/164 |
| `0x02400015` | `u16`, codes MSPDI | `DurationFormat` du projet | 164/164 |
| `0x0240001C` | `u16`, dixièmes de minute | `DefaultStartTime` | 164/164 |
| `0x02400021` | `u16`, dixièmes de minute | `DefaultFinishTime` | 164/164 |
| `0x0240001D` | `u32` | `MinutesPerDay` | 164/164 |
| `0x0240001E` | `u32` | `MinutesPerWeek` | 164/164 |
| `0x0240138F` | `u16` | `DaysPerMonth` | 164/164 |

- **observé** (réel : `Kalani0056__ms-project-planning__st20357430_CIS6011.mpp`) : `0x02400004` = `00 00`, `0x02400019` = `01 00` ; le corrigé : `ScheduleFromStart` 0, `TaskUpdatesResource` 1. Sur le corpus de test, les deux clés étaient indiscernables (un seul fichier à rebours, `mpp14header.mpp`, où les deux valent 0) : ce fichier réel les a départagées.
- **observé** (réel : `PacktPublishing__Planning-and-Control-Using-Microsoft-Project-365-and-2021__OzBuild_Workshop_18_End_Para_20.mpp`) : `0x02400038`, `0x024013CD`, `0x0240001A` et `0x0240003C` valent `00 00` ; le corrigé : `HonorConstraints` 0, `SplitsInProgressTasks` 0. `0x0240003C` est écartée : elle vaut 0 dans un autre fichier réel où `HonorConstraints` vaut 1. Avec l'option lue, le moteur recalcule les 18 tâches comme MS Project (17 sans).
- **observé** (réel : `Deapry__MS_Project-SamyLao__ProjetFinal_SamyLao.mpp`, `   114/Props` à `0x6cc`, clé `0x02401388`, 420 octets) : dimanche sans plage ; lundi `00 00 02 00 68 10 00 00 18 15 d0 20 …`, durées `08 07 00 00 60 09 00 00` : 09:00-12:00 et 14:00-18:00 ; le corrigé donne ces plages aux jours « par défaut » du calendrier « Standard » de ce fichier (et non 08:00-12:00, 13:00-17:00).
- **inconnu** : auteur, société et les autres propriétés (corrélation partielle, non lues).

## 6. Horodatages, durées, codes

- **observé** (`mpp14relations.mpp`, tâche UID 1, `FixedData`) : `c0 12 6c 20` = `0x206C12C0` ; 16 bits hauts `0x206C` = 8 300, 16 bits bas `0x12C0` = 4 800 ; le corrigé dit 2006-09-21 08:00 ; 2006-09-21 est le jour 8 300 si le jour 1 est 1984-01-01 ; 4 800 dixièmes de minute = 08:00. `d8 27 6c 20` : 10 200 dixièmes = 17:00, le même jour (fin au corrigé).
- **déduit** (toutes les dates comparées : 2 224 débuts, 2 224 fins, 164 débuts de projet, exceptions de calendrier) : horodatage = `u32`, jour en 16 bits hauts (1 = 1984-01-01), heure en dixièmes de minute en 16 bits bas ; `ff ff ff ff` = aucune date (NA). Les instants du moteur comptent en minutes depuis 1984-01-01 00:00 : minute = (jour − 1) × 1 440 + dixièmes / 10, les secondes **tronquées** comme une date MSPDI (R-004, R-075 ; observé, réel : `AtharvaTK1997__Software-Project-Management-Assignment-1__assign_1_20%.mpp`, dates à 30 secondes lues une minute plus tard par l'arrondi, tronquées par le corrigé).
- **observé** (`mpp14relations.mpp`, UID 1) : durée `c0 12 00 00` = 4 800 ; le corrigé dit `PT8H0M0S` = 480 minutes.
- **déduit** (2 224 durées sur 2 225 de test, 2 966 sur 2 966 réelles) : durée et décalage = `i32` en dixièmes de minute (aussi pour les durées écoulées : un exemple chacune de `em`, `eh`, `ew`, `emo` au corrigé).
- **observé** (`generated/task-links/task-links-project2016-mpp14.mpp`, UID 4) : format `35 00` = 53 ; le corrigé dit 39 (jours estimés). `mpp14relations.mpp`, UID 3 : `27 00` = 39, corrigé 39.
- **déduit** (2 224 tâches) : le format de durée suit les codes MSPDI (3 m, 4 em, 5 h, 6 eh, 7 j, 8 ej, 9 sem, 10 esem, 11 mois, 12 emois, + 32 si estimée) ; 21 et 53 = format nul, que le corrigé rend dans le format par défaut du projet (7 et 39 pour un projet en jours). Le lecteur applique l'unité par défaut du projet (clé `0x02400015`).
- **déduit** : types de lien, de contrainte et de tâche suivent aussi les codes MSPDI (égalité sur toutes les tâches et tous les liens, § 7 et § 9).

## 7. Enregistrement fixe d'une tâche (`TBkndTask/FixedData`)

Preuve A : `mpp14relations.mpp`, tâche UID 3, enregistrement à l'octet 666 de `FixedData`. Preuve B : `generated/task-links/task-links-project2016-mpp14.mpp`, tâche UID 4, enregistrement à l'octet 856. Colonne « égaux » : tâches dont la valeur lue égale le corrigé, sur toutes les tâches non vides des 80 fichiers A et des 84 fichiers B (mesure du lecteur, `docs/plans/mpp-import.md`).

| Champ | A : décalage, octets lus → valeur, corrigé | B : décalage, octets lus → valeur, corrigé | Égaux A ; B |
|---|---|---|---|
| UID | 0 : `03 00 00 00` → 3, 3 | 4 : `04 00 00 00` → 4, 4 | toutes |
| ID | 4 : `03 00 00 00` → 3, 3 | 0 : `04 00 00 00` → 4, 4 | voir § 11 |
| Début | 106 : `c0 12 70 20` → 2006-09-25 08:00, idem | 104 : `c0 12 d1 2d` → 2016-02-10 08:00, idem | 850/850 ; 1 374/1 374 |
| Fin | 8 : `d8 27 70 20` → 2006-09-25 17:00, idem | 108 : `d8 27 d1 2d` → 2016-02-10 17:00, idem | 850/850 ; 1 374/1 374 |
| Niveau (`u16`) | 40 : `01 00` → 1, 1 | 172 : `01 00` → 1, 1 | 850/850 ; 1 374/1 374 |
| Durée (`i32`) | 42 : `c0 12 00 00` → 480 min, `PT8H` | 84 : `c0 12 00 00` → 480 min, `PT8H` | 849/850 ; 1 372/1 374 |
| Format de durée (`u16`) | 46 : `27 00` → 39, 39 | 164 : `35 00` → 53, 39 (§ 6) | 850/850 ; 1 374/1 374 |
| Type de contrainte (`u16`) | 56 : `00 00` → 0, 0 | 64 : `00 00` → 0, 0 | 850/850 ; 1 374/1 374 |
| Date de contrainte | 80 : `ff ff ff ff` → NA, absente | 66 : `ff ff ff ff` → NA, absente | idem (contrainte) |
| Priorité (`u16`) | 88 : `f4 01` → 500, 500 | 78 : `f4 01` → 500, 500 | 850/850 ; 1 374/1 374 |
| % achevé (`u16`) | 90 : `00 00` → 0, 0 | 92 : `00 00` → 0, 0 | 850/850 ; 1 374/1 374 |
| Type de tâche (`u16`) | 94 : `00 00` → 0, 0 | 140 : `00 00` → 0, 0 | 850/850 ; 1 374/1 374 |
| Date de création | 98 : `4a 29 6c 20` → 2006-09-21 17:37, idem | 128 : `24 18 cf 2d` → 2016-02-08 10:18, idem | non lue au modèle |
| Calendrier (`i32`, −1 = aucun) | 118 : `ff ff ff ff` → −1, −1 | 178 : `ff ff ff ff` → −1, −1 | 850/850 ; 1 374/1 374 |
| Échéance | 122 : `ff ff ff ff` → NA, absente | 182 : `ff ff ff ff` → NA, absente | 850/850 ; 1 374/1 374 |
| Début réel | 72 | 120 | voir ci-dessous |
| Fin réelle | 76 | 124 | voir ci-dessous |
| Durée réelle (`i32`) | 48 | 80 | voir ci-dessous |
| Reprise (tâche commencée) | 20 | 132 | voir ci-dessous |
| Retard de nivellement (`i32`, dixièmes) et son format (`u16`) | 58, 62 | 70, 162 | voir ci-dessous |

Faits détaillés :

- **déduit** : les champs de la table ci-dessus, aux décalages indiqués, avec les comptes de la dernière colonne. Positifs rares, à garder en tête : contraintes datées (A : 24 tâches dont 3 où la date diffère du début ; B : 7 tâches, toutes avec date = début : la date de B est à l'octet 66 parce qu'il vaut NA sur les 1 450 tâches sans contrainte et une date sur les 7 contraintes, pas par une date distincte du début) ; calendrier de tâche (A : 10 tâches ; B : 5) ; échéance (une tâche par disposition : `mpp14task.mpp` octet 122 et `mpp14task-from2013.mpp` octet 182, 2006-08-30 17:00).
- **déduit** : % achevé en B à l'octet 92, % travail achevé à l'octet 94 : indiscernables sur le corpus de test, départagés sur les fichiers réels (13 feuilles où les deux diffèrent : l'octet 92 égale le % achevé du corrigé pour les 13). En A, octet 90 (19 tâches où ils diffèrent).
- **observé** (réel : `OlgaSlipak__-Banking-System-__BankingSystem_MSProject_ver3.mpp`, A, UID 65, tâche commencée) : octet 72 `c0 12 3e 30` → 2017-10-23 08:00 (début réel au corrigé), octet 76 `ff ff ff ff` (pas de fin réelle), octet 48 `d0 11 00 00` → 456 min (`PT7H36M0S`), octet 20 `e8 26 3e 30` → 2017-10-23 16:36 (`Resume`). Réel : `tefsantana__uala-digital-project-management-case-study__uala-project-schedule.mpp`, B, UID 71 : octet 120 `18 15 79 3c` → 2026-05-20 09:00, octet 124 NA, octet 80 `80 3e 00 00` → 1 600 min (`PT26H40M0S`), octet 132 `58 1b 7e 3c` → 2026-05-25 11:40.
- **déduit** (réels) : début et fin réels A 72 et 76 (932/940 tâches de test et 41/41 réelles égales au corrigé ou NA des deux côtés ; les autres sont des récapitulatives dont le corrigé calcule les réels), B 120 et 124 (2 931 et 2 916 sur 3 053 tâches réelles, mêmes écarts des récapitulatives ; les octets 104 et 108, début et fin, ne sont jamais NA, ce qui les écarte) ; durée réelle A 48 (939/940), B 80 (2 952/3 053). Reprise : B 132 (37 feuilles commencées sur 37) ; A : octets 20 et 84 égaux sur les 51 feuilles commencées, **20 retenu par analogie** (en B, la reprise précède l'arrêt d'un mot ; en A, l'arrêt est à l'octet 16, seul candidat sur 38 feuilles), **à confirmer**. Retard de nivellement : A octet 58 et format octet 62 (5 retards non nuls, `mpp14barstyle.mpp`), B octet 70 et format octet 162 (12 retards non nuls dans des fichiers réels, dont `ahmerakamran964-hue__Ms-Project-Solar-Panel-Installation-__Solar_panel_installation.mpp`).
- **observé** : la date de création (A 98, B 128) égale le corrigé ; non reprise au modèle dans ce lot.
- **inconnu** : durée restante enregistrée (dérivée par le lecteur, R-200), travail, coûts, champs personnalisés, lignes de base.

## 8. Indicateurs d'une tâche (éléments de méta) et valeurs saisies d'une tâche manuelle

- **observé** (`taskFlags-mpp14Project2013.mpp`, fichier où chaque indicateur est posé sur des tâches nommées « Milestone: Yes », « Milestone: No », etc.) : élément de `FixedMeta`, octets 8 à 10 : UID 32 « Milestone: Yes » `e3 ff ff`, UID 33 « Milestone: No » `e3 ff fd` ; UID 69 « Mode: Manual », octet 8 de l'élément de `Fixed2Meta` : `11111111` ; UID 68 « Mode: Auto » : `01111111` ; UID 64 « Active: Off » : `00111111` ; UID 63 « Active: On » : `01111111`.
- **observé** (`taskFlags-mpp14Project2010.mpp`, mêmes tâches) : octet 8 de l'élément de `FixedMeta` : UID 32 `63`, UID 33 `43` ; octet 8 de l'élément de `Fixed2Meta` : UID 69 `11001111`, UID 68 `11000111`, UID 64 `11000011`, UID 63 `11000111`.
- **déduit**, sur toutes les tâches des 164 fichiers (aucune exception) :

| Indicateur | A | B | Positifs au corpus |
|---|---|---|---|
| Jalon | `FixedMeta`, octet 8, bit 5 (`0x20`) | `FixedMeta`, octet 10, bit 1 (`0x02`) | 17 (A) ; 6 (B) |
| Planification manuelle | `Fixed2Meta`, octet 8, bit 3 (`0x08`) | `Fixed2Meta`, octet 8, bit 7 (`0x80`) | 356 (A) ; 1 292 (B) |
| Active | `Fixed2Meta`, octet 8, bit 2 (`0x04`) | `Fixed2Meta`, octet 8, bit 6 (`0x40`) | 2 inactives (A) ; 2 (B), un seul fichier chacune |

- **observé** (`generated/task-links/task-links-project2016-mpp14.mpp`, UID 4, tâche manuelle) : `Fixed2Data`, octets 50 à 62 : `c0 12 d1 2d d8 27 d1 2d c0 12 00 00` → 2016-02-10 08:00, 2016-02-10 17:00, 480 minutes ; le corrigé : `ManualStart`, `ManualFinish`, `ManualDuration` mêmes valeurs. Tâche automatique (`mpp14relations.mpp`, UID 3) : `ff` partout.
- **déduit** (356 tâches manuelles A, 1 292 B) : `Fixed2Data` (64 octets, même rang) : début saisi à l'octet 50, fin saisie à 54, durée saisie (`i32`, dixièmes) à 58, dans les deux dispositions. Les dates saisies peuvent rester dans une tâche repassée en automatique (71 tâches A, 4 B) : l'indicateur, pas la présence de dates, décide du mode.
- **observé** (réel : `AmaalAyman__Project-Management__Pm_Project_1.mpp`, UID 4, manuelle) : durée de l'enregistrement fixe (octet 84) 2 760 min, durée saisie (`Fixed2Data` octet 58) 2 640 min ; le corrigé : `Duration` et `ManualDuration` 2 640 min (`PT44H`). Sur le corpus de test, les deux étaient égales.
- **déduit** (réels, 2 966 durées égales) : la durée d'une tâche manuelle est la durée saisie.
- **inconnu** : récapitulative (déduite de la hiérarchie, R-052, comme le lecteur MSPDI), critique, estimée (bit 5 du code de format, § 6), autres indicateurs.

## 9. Liens (`TBkndCons/FixedData`)

- **observé** (`mpp14relations.mpp`, A) : enregistrements de 20 octets ; le deuxième : `02 00 00 00 02 00 00 00 03 00 00 00 03 00 07 00 c0 12 00 00` ; le corrigé : tâche 3, prédécesseur 2, type 3 (DD), décalage 4 800 dixièmes, format 7.
- **observé** (`generated/task-links/task-links-project2016-mpp14.mpp`, B) : `02 00 00 00 03 00 00 00 04 00 00 00 01 00 c0 12 00 00 07 00` ; le corrigé : tâche 4, prédécesseur 3, type 1 (FD), décalage 4 800, format 7.
- **déduit** (62 liens, 164 fichiers, tous égaux au corrigé) : `[u32 ?][u32 prédécesseur][u32 successeur][u16 type]`, puis en A `[u16 format][i32 décalage]`, en B `[i32 décalage][u16 format]`. Types et formats : codes MSPDI.
- **inconnu** : décalage en pourcentage (formats 19, 20 : aucun échantillon ; le lecteur garde le lien avec un décalage nul et le signale) ; liens supprimés (aucun échantillon : le lecteur ignore un élément dont le bit 1 est posé, par analogie avec les tâches).

## 10. Calendriers (`TBkndCal`)

- **observé** (`generated/calendar-calendars/calendar-calendars-project2016-mpp14.mpp`, B) : enregistrements de 12 octets `00 00 00 00 ff ff ff ff 01 00 00 00` (calendrier 1, « Standard », de base au corrigé), `01 00 00 00 00 00 00 00 02 00 00 00` (calendrier 2, dérivé du 1 au corrigé), `01 00 00 00 01 00 00 00 05 00 00 00` (calendrier 5, dérivé du 1). Même fichier enregistré par Project 2010 (A) : `01 00 00 00 ff ff ff ff ff ff ff ff` (1, de base), `02 00 00 00 01 00 00 00 00 00 00 00` (2, base 1).
- **déduit** (164 fichiers, 540 calendriers en B, 719 en A, tous de nature et de base égales au corrigé, hors calendriers supprimés et absents du corrigé) : A `[u32 UID][i32 base][i32 ?]` ; B `[i32 base][i32 ?][u32 UID]` ; base nulle ou négative = calendrier de base (B : 0 ; A : −1, ou une valeur négative aberrante sur 2 calendriers), sinon UID du calendrier de base. Un calendrier dérivé est un calendrier de ressource (repris depuis le lot M2, R-296).
- **observé** (lot M2 ; 41 fichiers réels, champ `?` des calendriers dérivés comparé au calendrier des ressources du corrigé) : le champ `i32 ?` d'un calendrier dérivé est l'UID de sa ressource (631 sur 633) ; `ffffffff` sur un calendrier de base.
- **déduit** (lot M2 ; 632 calendriers dérivés réels et 1 068 de test, tous de base, semaine et nombre d'exceptions égaux au corrigé) : un calendrier dérivé a les mêmes valeurs variables qu'un calendrier de base ; jour au code 1 = hérité de la base (`Inherit`), code 0 = réglé ici.
- **observé** (`mpp14calendar.mpp`, calendrier 7 « Night Shift », clé `0x0D400008`, 428 octets) : 7 jours de 60 octets puis 8 octets ; jour 1 (lundi) `00 00 01 00 58 02 00 00 e8 35 …` puis à l'octet 20 `58 02 00 00` : une plage, 600 dixièmes au total, début 13 800 dixièmes (23:00), durée 600 (1 h) ; le corrigé : lundi 23:00-00:00. Jour 2 : `00 00 03 00 c0 12 00 00 00 00 60 09 e8 35 …`, durées à l'octet 20 `08 07 00 00 60 09 00 00 58 02 00 00` : plages 00:00-03:00, 04:00-08:00, 23:00-24:00 ; le corrigé : idem. Jour 0 (dimanche) : `01 00 00 00 …`, chômé au corrigé.
- **déduit** (207 calendriers de base de test et 96 réels, semaine entière égale au corrigé pour tous) : jour = `[u16 code][u16 nombre de plages][u32 total][5 × u16 débuts][u16][5 × u32 durées][5 × u32 cumuls]`, dixièmes de minute, dimanche d'abord ; code 0 = jour défini ici (aucune plage = chômé) ; code 1 = jour « par défaut » : celui de la semaine par défaut du projet (propriété `0x02401388`, § 5 ; Standard 08:00-12:00, 13:00-17:00 sur tout le corpus de test, autre sur 13 calendriers réels) ; calendrier de base sans clé `0x0D400008` = semaine par défaut. Sur un calendrier dérivé, code 1 = jour hérité de la base (5 jours vérifiés).
- **observé** (`mpp14exceptions.mpp`, calendrier 1, 704 octets) : à l'octet 420 `03 00 00 00` (3 exceptions) ; première à 424 : `7f 22 7f 22` (jour 8 831 = 2008-03-05, du 5 au 5), puis un jour de 60 octets à l'octet 12 sans plage ; le corrigé : exception chômée du 2008-03-05. Troisième : `8a 22 8a 22` (2008-03-16) et cinq plages de 1 h (08:00, 11:00, 14:00, 16:00, 18:00) : idem au corrigé.
- **observé** (`calendar/multi-day-exceptions/multi-day-exceptions-project2010-mpp14.mpp`) : exception `55 33 57 33` (2019-12-23 au 2019-12-25), à l'octet 88 de l'exception `14 00 00 00` puis « Christmas » en UTF-16 (20 octets), exception suivante juste après ; le corrigé : « Christmas », 2019-12-23 au 2019-12-25. `calendar-exception-precedence.mpp`, calendrier 18 : nom « Labor Day  » de 22 octets, exception suivante 2 octets plus loin.
- **observé** (`generated/calendar-recurring-exceptions/calendar-recurring-exceptions-project2013-mpp14.mpp`, 27 exceptions de chaque type) : « Weekly 2 Tuesday » `43 18 6e 18 04 00 00 00 01 00 00 00 …` puis à l'octet 72 `06 00 00 00 04 00 02 00` ; le corrigé : du 2001-01-02 au 2001-02-13, `Type` 6 (hebdomadaire), `DaysOfWeek` 4 (mardi), `Period` 2, `Occurrences` 4. « Daily 2 » : `07 00 00 00 03 00 …` ; corrigé `Type` 7, `Period` 3. `calendar-exception-precedence.mpp` : « 4th of July » `02 00 00 00 06 04 …` (corrigé `Type` 2, `Month` 6, `MonthDay` 4) ; « Thanksgiving Day » `03 00 00 00 0a 03 07 00` (corrigé `Type` 3, `Month` 10, `MonthPosition` 3, `MonthItem` 7).
- **déduit** (207 calendriers de test, 96 réels : exceptions égales au corrigé par leurs jours couverts, récurrence dépliée par le moteur) : exception = `[u16 premier jour][u16 dernier jour][u32 occurrences][u32 saisie par occurrences][jour de 60 octets][u32 type][4 octets de paramètres][8 octets][u32 longueur du nom][nom UTF-16]`, l'exception suivante alignée sur 4 octets depuis le début du bloc. Type : 1 période ; 7 tous les N jours (N = `u16` à 76) ; 6 hebdomadaire (masque des jours, codes MSPDI, `u8` à 76 ; pas `u16` à 78) ; 5 mensuelle par rang (rang `u8` à 76, jour visé `u8` à 77, pas `u16` à 78) ; 4 mensuelle par jour (jour `u8` à 76, pas `u16` à 78) ; 3 annuelle par rang (mois `u8` à 76, rang `u8` à 77, jour visé `u16` à 78) ; 2 annuelle par jour (mois `u8` à 76, jour `u8` à 77). Mois de 0 (janvier) à 11, rangs et jours visés : codes MSPDI. Nombre d'occurrences pris quand l'octet 8 vaut 1 (type autre que 1). Le premier jour est celui saisi, le corrigé écrit la première occurrence : mêmes jours couverts. Le code 7 correspond à « tous les N jours » d'après les occurrences du corrigé (et non au code 7 non documenté de MSPDI, ADR-0036). Type inconnu : exception ignorée (`mpp.calendar_exception_unknown_type`).
- L'ancienne note de ce paragraphe (« exception périodique : `u32` non nul à l'octet 8, longueur différente, lecture arrêtée ») était fausse : l'arrêt venait d'un nom de longueur non multiple de 4 mal aligné, corrigé avant ; l'octet 8 vaut 1 aussi pour des exceptions simples.
- **inconnu** : semaines de travail datées (aucune dans le corpus MPP14), calendrier de base d'un calendrier de base (jamais observé).

## 11. Ordre des tâches et ID

- **observé** (`task/task-deletion/task-deletion2-project2013-mpp14.mpp`, B) : après suppression de lignes, les enregistrements gardent leur place et leur ID est réécrit : `01 00 00 00 02 00 00 00` (ID 1, UID 2 : « T1 », ID 1 au corrigé).
- **observé** (`task/task-deletion/task-deletion2-project2010-mpp14.mpp`, A) : même projet ; l'enregistrement complet de « T4 » garde `06 00 00 00 06 00 00 00` (UID 6, ID 6) alors que le corrigé lui donne l'ID 3 ; la ligne vide UID 8 a l'ID 4, à jour.
- **déduit** (162 fichiers sur 164) : l'ordre des tâches suit l'ID enregistré (à égalité, l'ordre des enregistrements) ; l'UID 0 est la récapitulative du projet. Un ID resté ancien (A, après suppression) peut déplacer une tâche (`task-deletion2-project2010-mpp14.mpp` : T4 et la ligne vide permutées) ; l'ordre par la position des enregistrements, essayé, est faux sur 49 tâches d'autres fichiers. **Inconnu** : où MS Project range l'ordre réel dans ce cas.

## 12. Écarts où le corrigé est soupçonné, et fichiers à part

- Calendriers supprimés (indicateur 2) gardés par le corrigé comme calendriers de base sans nom (`ResourceIdAndUniqueId-project2013-mpp14.mpp`, 9 calendriers) : le lecteur les ignore.
- Calendrier de base sans nom au fichier, nommé d'après une ressource par le corrigé (`mpp14assignmentfields.mpp` calendrier 4, `timephased-prorated-cost-resource.mpp` calendrier 3).
- Exception simple rendue « quotidienne, tous les 1 jour » par le corrigé : même effet que la période du lecteur.
- `assignment/assignment-deletion/sf248.mpp` : `FixedMeta` incohérent (6 éléments annoncés, 42 emplacements, copies contradictoires) ; le lecteur y lit 2 tâches, le corrigé 3. Fichier issu d'un rapport d'anomalie, sans doute endommagé ; laissé en écart mesuré.
- Enregistrement en service au-delà du nombre annoncé et sans nom (réel : `AmaalAyman__…`, UID 25) : gardé par le corrigé, ignoré par le lecteur (§ 4).
- Décalage nul d'un lien : le corrigé écrit l'unité par défaut du projet (semaines dans `CaseStudy-1.mpp`), le fichier dit jours (`07 00`) ; sans effet.
- Durée saisie nulle d'une tâche manuelle (jalon manuel) : le corrigé n'écrit pas `ManualDuration`, le lecteur lit 0 (83 tâches réelles).
- Début ou fin enregistrés de 2 tâches réelles (`ubhurase__IPM__Big_Pharma_Case_Project_Final.mpp` UID 171, `Kalani0056__…` UID 189) : écart non expliqué.

## 13. Fichiers réels

- **Source** : 41 fichiers publics de dépôts GitHub (projets d'étudiants et de cours de gestion de projet, un livre de formation ; un fichier `.mpt`) trouvés par la liste des arbres de dépôts publics, sans ouvrir de page ni de dépôt de lecteur de `.mpp` ; 7 fichiers d'un dépôt de formation fournis à la relecture. Source, commit et empreinte de chacun : `SOURCES.tsv` hors dépôt ; une sélection de 12 est au manifeste du corpus.
- **observé** (réel : `PacktPublishing__Planning-and-Control-Using-Microsoft-Project-365-and-2021__Eastwood_Harris_MSP2021_Template_Ver_01.mpt`) : un gabarit `.mpt` a le même conteneur (dossier `   114`) et se lit comme un `.mpp` (12 tâches, toutes égales au corrigé). Un seul échantillon.
- Ce que les fichiers réels ont appris, et que le corpus de test cachait : emplacements en surnombre (§ 4), bourrage des propriétés et semaine par défaut (§ 5), secondes tronquées (§ 6), réels et reprise, retard de nivellement, % achevé de B, durée des tâches manuelles (§ 7, § 8), exceptions périodiques (§ 10), clé du sens de planification (§ 5).

## 14. Ressources (`TBkndRsc`, lot M2)

- **observé** (`generated/resource-type/resource-type-project2013-mpp14.mpp`, B, 15 ressources : 5 coût, 5 matérielles, 5 travail au corrigé) : enregistrements fixes de 172 octets ; `u16` à l'octet 170 : `01 00` pour les UID 1 à 10 (coût et matérielles), `00 00` pour les UID 11 à 15 (travail) ; octet 8 de l'élément de `Fixed2Meta` de même rang : `1b` pour les UID 1 à 5 (coût), `0b` pour les autres. `f64` à l'octet 8 : `00 00 00 00 00 88 c3 40` = 10 000 (unités maximales 1 au corrigé).
- **observé** (réel : `abhuiyan00__Computer-Project-Management__export-import-system.mpp`, B) : l'enregistrement de l'emplacement 13 commence par `0a 00 00 00 0b 00 00 00` ; son `Fixed2Data` porte le GUID `9DAFFB8F-0465-F111-A9D1-F447DB86B7CA`, celui de la ressource UID 11 (« Server hosting ») au corrigé : l'octet 0 est l'ID, l'octet 4 l'UID (comme pour les tâches en B).
- **déduit** (lot M2 ; B : 638 ressources réelles et 453 de test ; A : 617 de test) :

| Champ | A (Project 2010) | B (Project 2013 et suivants) | Égaux au corrigé |
|---|---|---|---|
| UID (`u32`) | 0 | 4 | A 617/617 ; B 638/638 et 453/453 |
| Unités maximales (`f64`, 1/10 000) | 44 | 8 | A 617/617 ; B 634/638 et 453/453 |
| Calendrier (`i32`, UID d'un calendrier dérivé) | 16 | 154 | A 615/617 ; B 625/631 et 453/453 |
| Nature (`u16`, bit 0 posé = matérielle ou coût) | 14 | 170 | A 540/540 ; B 598/598 réelles, 368/369 de test |
| Nom (valeur variable `0x0C400001`, UTF-16) | id. | id. | 598/598 |

- **déduit** (A 15/15 ; B 39/39 réelles et 25/26 de test) : ressource coût = bit `0x10` de l'octet 8 de l'élément de `TBkndRsc/Fixed2Meta` de même rang. Le contre-exemple de test (une ressource « travail » au corrigé, bit coût et nature non travail posés dans le fichier) est soupçonné côté corrigé.
- **inconnu** : étiquette d'unité d'une ressource matérielle (17 ressources réelles), taux et coûts, disponibilités datées, ressource générique ou inactive.

## 15. Affectations (`TBkndAssn`, lot M2)

- **observé** (réel : `advaith14__Project-Management-for-World-Hotels-__IST645F18A2BKamathA.mpp`, B, affectation UID 80) : `50 00 00 00` (UID 80) `50 00 00 00` (tâche 80) `09 00 00 00` (ressource 9) ; octet 12 : `00 00 00 00 00 70 b7 40` = 6 000 (unités 0,6 au corrigé) ; octet 20 : `00 00 00 00 c0 7a f0 40` = 67 500 (travail `PT1H7M30S`, 67,5 min) ; octet 52 : `28 16 8e 31` (début 2018-09-24 09:27:12 au corrigé).
- **observé** (réel : `tefsantana__uala-digital-project-management-case-study__uala-project-schedule.mpp`, UID 638) : `i32` à l'octet 64 = 96 000 dixièmes (9 600 min) ; le corrigé écrit `Delay` 30 464 = 96 000 − 65 536. Même écart de 65 536 sur 5 autres affectations réelles (`AmaalAyman__…` UID 68, 69, 91, 92 ; `AlizaKhan3__…` UID 219) : le corrigé lit 16 bits ; les 21 autres retards non nuls sont égaux.
- **observé** (`mpp14timephasedsegments.mpp`, A, une affectation par profil) : valeur variable `0x0F400031` (répartition), octet 28 : `02` pour le profil 2 au corrigé (UID 20 et 36), `01` (UID 22), `03` (24), `04` (26), `05` (28), `06` (30), `07` (32) ; `00` pour l'affectation UID 34, profil 8 au corrigé, et pour les profils uniformes.
- **déduit** (B : 4 091 affectations réelles et 1 396 de test ; A : 823 de test) :

| Champ | A | B | Égaux au corrigé |
|---|---|---|---|
| UID, tâche, ressource (`u32`, `u32`, `i32` ; −65 535 = aucune) | 0, 4, 8 | 0, 4, 8 | A 823/823 ; B 4 091/4 091 et 1 396/1 396 |
| Unités (`f64`, 1/10 000) | 46 | 12 | A 823/823 ; B 4 051/4 091 et 1 396/1 396 |
| Travail (`f64`, millièmes de minute) | 54 | 20 | A 804/804 ; B 3 565/3 736 et 1 376/1 376 |
| Travail réel (`f64`, millièmes de minute) | 62 | 28 | A 64/65 ; B 9/9 de test |
| Début, fin enregistrés (horodatages) | 12, 16 | 52, 56 | A 823/823 ; B 4 055/4 091 et 1 396/1 396 |
| Retard d'affectation (`i32`, dixièmes) | inconnu | 64 | B 21/29, les 8 autres par troncature du corrigé (ci-dessus) |
| Retard de nivellement (`i32`, dixièmes) | inconnu | 100 | B 17/17 |
| Profil modifié à la main (bit de l'élément de `FixedMeta`) | octet 8, `0x10` | octet 10, `0x04` | A 25/25 ; B 42/42 réelles (0 sur 4 049 autres) et 28/28 de test |

- **déduit** : profil 1 à 7 = octet 28 de la répartition (21 affectations de test, toutes égales) ; profil modifié à la main (8) : bit ci-dessus, la répartition saisie (la suite de la valeur `0x0F400031`) n'est pas lue.
- Mesure après lecture (§ 10 du plan) : 4 096 affectations réelles présentes sur 4 225 au corrigé ; les 129 absentes sont des affectations de récapitulatives (ressource −65 535) que le fichier ne contient pas (aucun emplacement de `FixedMeta` ne les désigne : réel `AlejoTechEngineer__…` UID 45, 46, 47, sur les tâches 0, 1, 5) et que le corrigé fabrique. Unités 4 096/4 096, tâche et ressource 4 096/4 096, profil 4 096/4 096, retard 4 088/4 096 (troncature du corrigé), travail 4 072/4 096 (travail sous la minute : le lecteur arrondit, le corrigé tronque).
- **inconnu** : retards d'affectation et de nivellement en A (aucun échantillon non nul lu), répartition saisie d'un profil modifié à la main, heures supplémentaires, coûts.
