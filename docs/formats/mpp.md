# Format des fichiers `.mpp` (MPP14) : notes d'observation

Lot M1 (ADR-0036). Ce document est la seule source des faits de format qu'utilise `crates/plannercraft-mpp`. Il prouve la propreté du travail (clean-room) : **chaque fait** porte son statut et sa preuve.

- **observé** : fichier du corpus (chemin relatif à `junit/data/` du dépôt public `joniles/mpxj`, commit `4982974b2594ffc7146cdb9ebdf2ae46186737ca`), flux, décalage, octets lus, et la valeur du corrigé qui leur correspond ;
- **déduit** : règle généralisée, avec le nombre de fichiers ou d'enregistrements sur lesquels elle tient ;
- **inconnu** : non établi ; le lecteur ne le lit pas, ou le signale.

## 0. Méthode et sources

- **Sources publiques** : `[MS-CFB]` (conteneur, § 1), `[MS-OLEPS]` (non utilisé dans ce lot), énumération `PjField` de la documentation Microsoft (noms des identifiants de champ, § 3), schéma MSPDI (sens des codes numériques de format de durée, de type de lien, de contrainte et de tâche, déjà tabulés dans `plannercraft-mspdi::codes`).
- **Données** : 609 fichiers `.mpp` de test publics, lus comme des données (liste du conteneur, vidages hexadécimaux, recherche de valeurs connues, corrélation sur l'ensemble du corpus, comparaison de fichiers qui diffèrent d'un champ). Les fichiers ne sont jamais commités ; une sélection de 45 est décrite par `tests/corpus-mpp/manifest.toml`.
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
- **inconnu** : fichier `.mpt` (gabarit) : aucun dans le corpus. Fichier MPP14 protégé par un mot de passe : aucun dans le corpus (les trois fichiers protégés du corpus sont des MPP9, refusés comme tels).

## 3. Valeurs variables (`VarMeta`, `Var2Data`)

- **observé** (`mpp14relations.mpp`, `   114/TBkndTask/VarMeta`, 744 octets) : en-tête `ba ad df fa 00 00 00 00 3c 00 00 00 00 00 00 00 00 00 00 00 44 02 00 00` : 60 (`0x3c`) entrées, taille de `Var2Data` 580 (`0x244`) ; puis entrées de 12 octets `[u32 UID][u32 décalage dans Var2Data][u32 clé]`, par exemple `01 00 00 00 84 00 00 00 0e 00 40 0b` (UID 1, décalage `0x84`, clé `0x0B40000E`).
- **observé** (même fichier, `Var2Data` à `0x84`) : `0e 00 00 00` puis « Task 1 » en UTF-16 terminé par un nul (14 octets) ; le corrigé nomme la tâche UID 1 « Task 1 ».
- **déduit** (2 224 noms de tâche, 164 fichiers, tous égaux au corrigé) : valeur = `[u32 longueur][octets]` ; la clé `0x0B40000E` porte le nom (UTF-16, nul final). Les clés de tâche coïncident avec les identifiants `PjField` (`pjTaskName` = 188743694 = `0x0B40000E`) ; d'autres clés aussi (`0x0B400505` `pjTaskStartText`), sans qu'on s'en serve.
- **déduit** (notes de `mpp14task.mpp`, `mpp14embedded.mpp`, `mpp14timephasedsegmentsmanualoffsets.mpp` : 2 223 tâches sur 2 224 égales au corrigé) : la clé `0x0B40000F` (`pjTaskNotes`) porte les notes en RTF (`{\rtf1\ansi…`) ; le texte est tiré du RTF d'après la spécification RTF publiée par Microsoft (`src/rtf.rs`).
- **déduit** (calendriers, 435 noms) : dans `TBkndCal`, clé `0x0D400001` = nom (UTF-16), clé `0x0D400008` = semaine et exceptions (§ 10).
- **observé** : la clé `0x0B40011B` (`pjTaskFinish6`) porte une date : dans `mpp14task.mpp` elle égale l'échéance du corrigé (2006-08-30 17:00), mais dans `generated/task-finishes/task-finishes-project2010-mpp14.mpp` (champ Fin6 rempli, sans échéance) elle porte Fin6. Ce n'est donc pas l'échéance (§ 7).

## 4. Tables fixes (`FixedMeta`, `FixedData`, `Fixed2Meta`, `Fixed2Data`)

- **observé** (`mpp14relations.mpp`, `TBkndTask/FixedMeta`, 439 octets) : en-tête `ba ad df fa 04 00 00 00 09 00 00 00 04 05 00 00` : 9 éléments, taille de `FixedData` 1 284 (`0x504`) ; (439 − 16) / 9 = 47 octets par élément ; chaque élément a ses indicateurs (`u16`, octet 0) et le décalage de son enregistrement dans `FixedData` (`u32`, octet 4) : `00 00 0a 00 30 00 00 00 …`, `00 00 0a 00 fe 00 00 00 …` (enregistrements de 206 octets).
- **déduit** (163 fichiers sur 164) : taille d'un élément = (longueur − 16) / nombre d'éléments (47 pour `TBkndTask/FixedMeta`, 10 pour `TBkndCons` et `TBkndCal`, 92 à 96 pour `TBkndTask/Fixed2Meta`). Exception : `assignment/assignment-deletion/sf248.mpp` annonce 6 éléments pour 42 présents (fichier sans doute endommagé, § 12).
- **observé** (`task/task-deletion/task-deletion1-project2013-mpp14.mpp` puis `task-deletion2-…`, même projet après suppression de lignes) : indicateurs `0x0000` = enregistrement complet ; `0x0004` = enregistrement court de 16 octets ; `0x0002` et `0x0006` = enregistrement de tâche supprimée (complet ou court), absente du corrigé.
- **déduit** (164 fichiers) : bit 1 (`0x2`) = supprimé, ignoré ; bit 2 (`0x4`) = enregistrement court. Les trois ou quatre premiers enregistrements de chaque table sont courts et commencent par `n 00 ff ff` (UID ≥ `0xFFFF0000`) : réservés, jamais une tâche ni un calendrier.
- **observé** (`mpp14nulltasks.mpp`) : les tâches UID 2, 5 et 7, lignes vides au corrigé (`IsNull` = 1), ont un enregistrement court : `02 00 00 00 02 00 00 00 …` (UID 2, ID 2).
- **déduit** (13 lignes vides, 5 fichiers) : enregistrement court = ligne vide ; UID à l'octet 0, ID à l'octet 4 (les deux dispositions ; `task-deletion2-project2013-mpp14.mpp` : `08 00 00 00 04 00 00 00` = UID 8, ID 4 au corrigé).
- **déduit** : `Fixed2Meta` a autant d'éléments que `FixedMeta` (164 fichiers sur 164), au même rang ; ses enregistrements (`Fixed2Data`) font 64 octets (160 fichiers sur 164 ; les 4 autres ont des enregistrements plus longs, lus aux mêmes décalages) (§ 8).

## 5. Propriétés du projet (`   114/Props`)

- **observé** (`mpp14relations.mpp`, `   114/Props` à `0xda`) : `04 00 00 00 02 00 40 02 04 00 00 00 c0 12 6c 20` : taille 4, clé `0x02400002`, un `u32` (ici 4), puis l'horodatage 2006-09-21 08:00 (§ 6), début du projet au corrigé.
- **déduit** (164 fichiers) : en-tête de 16 octets, puis entrées `[u32 taille][u32 clé][u32 ?][taille octets]` ; clés vérifiées contre le corrigé sur les 164 fichiers MPP14 :

| Clé | Valeur | Corrigé | Fichiers égaux |
|---|---|---|---|
| `0x02400002` | horodatage | `StartDate` | 164/164 |
| `0x02400003` | horodatage | `FinishDate` | 164/164 |
| `0x02400004` et `0x02400019` | `u16` | `ScheduleFromStart` | 164/164 chacune |
| `0x02400008` | UTF-16 | `Title` | 164/164 |
| `0x0240000E` | UTF-16 | nom du calendrier de `CalendarUID` | 164/164 |
| `0x02400015` | `u16`, codes MSPDI | `DurationFormat` du projet | 164/164 |
| `0x0240001C` | `u16`, dixièmes de minute | `DefaultStartTime` | 164/164 |
| `0x02400021` | `u16`, dixièmes de minute | `DefaultFinishTime` | 164/164 |
| `0x0240001D` | `u32` | `MinutesPerDay` | 164/164 |
| `0x0240001E` | `u32` | `MinutesPerWeek` | 164/164 |
| `0x0240138F` | `u16` | `DaysPerMonth` | 164/164 |

- **déduit, avec réserve** : un seul fichier est planifié à rebours (`mpp14header.mpp`), où `0x02400004` et `0x02400019` valent toutes deux 0 ; ailleurs, toutes deux valent 1, comme `TaskUpdatesResource` au corrigé. Laquelle porte le sens de planification est **inconnu** ; le lecteur planifie à rebours seulement si les deux valent 0.
- **inconnu** : auteur, société et les autres propriétés (corrélation partielle, non lues).

## 6. Horodatages, durées, codes

- **observé** (`mpp14relations.mpp`, tâche UID 1, `FixedData`) : `c0 12 6c 20` = `0x206C12C0` ; 16 bits hauts `0x206C` = 8 300, 16 bits bas `0x12C0` = 4 800 ; le corrigé dit 2006-09-21 08:00 ; 2006-09-21 est le jour 8 300 si le jour 1 est 1984-01-01 ; 4 800 dixièmes de minute = 08:00. `d8 27 6c 20` : 10 200 dixièmes = 17:00, le même jour (fin au corrigé).
- **déduit** (toutes les dates comparées : 2 224 débuts, 2 224 fins, 164 débuts de projet, exceptions de calendrier) : horodatage = `u32`, jour en 16 bits hauts (1 = 1984-01-01), heure en dixièmes de minute en 16 bits bas ; `ff ff ff ff` = aucune date (NA). Les instants du moteur comptent en minutes depuis 1984-01-01 00:00 : minute = (jour − 1) × 1 440 + dixièmes / 10, arrondi par R-001.
- **observé** (`mpp14relations.mpp`, UID 1) : durée `c0 12 00 00` = 4 800 ; le corrigé dit `PT8H0M0S` = 480 minutes.
- **déduit** (2 221 durées sur 2 224) : durée et décalage = `i32` en dixièmes de minute (aussi pour les durées écoulées : un exemple chacune de `em`, `eh`, `ew`, `emo` au corrigé).
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

Faits détaillés :

- **déduit** : les champs de la table ci-dessus, aux décalages indiqués, avec les comptes de la dernière colonne. Positifs rares, à garder en tête : contraintes datées (A : 24 tâches dont 3 où la date diffère du début ; B : 7 tâches, toutes avec date = début : la date de B est à l'octet 66 parce qu'il vaut NA sur les 1 450 tâches sans contrainte et une date sur les 7 contraintes, pas par une date distincte du début) ; calendrier de tâche (A : 10 tâches ; B : 5) ; échéance (une tâche par disposition : `mpp14task.mpp` octet 122 et `mpp14task-from2013.mpp` octet 182, 2006-08-30 17:00).
- **déduit, avec réserve** : % achevé en B à l'octet 92. Les octets 92 et 94 égalent tous deux le % achevé et le % travail achevé sur les 1 374 tâches B : aucune tâche B du corpus n'a deux valeurs différentes. En A, l'octet 90 est le % achevé et l'octet 92 le % travail achevé (19 tâches où ils diffèrent). L'octet 92 est retenu en B par analogie d'ordre ; **à confirmer** sur un fichier qui les distingue.
- **observé** : la date de création (A 98, B 128) égale le corrigé ; non reprise au modèle dans ce lot.
- **déduit** (2 durées en écart, `mpp14timephasedsegmentsmanualoffsets.mpp` UID 27 et 29) : la durée lue à l'octet 84 diffère du corrigé pour deux tâches à affectations découpées ; cause **inconnue** (§ 12).
- **inconnu** : début et fin réels, durée réelle et restante (candidats ambigus : plusieurs décalages égaux sur le peu d'échantillons), travail, coûts, retard de nivellement (A : octet 58 en dixièmes de minute, 5 valeurs non nulles dans un seul fichier, `mpp14barstyle.mpp` ; B : aucun échantillon), champs personnalisés, lignes de base.

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
- **inconnu** : récapitulative (déduite de la hiérarchie, R-052, comme le lecteur MSPDI), critique, estimée (bit 5 du code de format, § 6), autres indicateurs.

## 9. Liens (`TBkndCons/FixedData`)

- **observé** (`mpp14relations.mpp`, A) : enregistrements de 20 octets ; le deuxième : `02 00 00 00 02 00 00 00 03 00 00 00 03 00 07 00 c0 12 00 00` ; le corrigé : tâche 3, prédécesseur 2, type 3 (DD), décalage 4 800 dixièmes, format 7.
- **observé** (`generated/task-links/task-links-project2016-mpp14.mpp`, B) : `02 00 00 00 03 00 00 00 04 00 00 00 01 00 c0 12 00 00 07 00` ; le corrigé : tâche 4, prédécesseur 3, type 1 (FD), décalage 4 800, format 7.
- **déduit** (62 liens, 164 fichiers, tous égaux au corrigé) : `[u32 ?][u32 prédécesseur][u32 successeur][u16 type]`, puis en A `[u16 format][i32 décalage]`, en B `[i32 décalage][u16 format]`. Types et formats : codes MSPDI.
- **inconnu** : décalage en pourcentage (formats 19, 20 : aucun échantillon ; le lecteur garde le lien avec un décalage nul et le signale) ; liens supprimés (aucun échantillon : le lecteur ignore un élément dont le bit 1 est posé, par analogie avec les tâches).

## 10. Calendriers (`TBkndCal`)

- **observé** (`generated/calendar-calendars/calendar-calendars-project2016-mpp14.mpp`, B) : enregistrements de 12 octets `00 00 00 00 ff ff ff ff 01 00 00 00` (calendrier 1, « Standard », de base au corrigé), `01 00 00 00 00 00 00 00 02 00 00 00` (calendrier 2, dérivé du 1 au corrigé), `01 00 00 00 01 00 00 00 05 00 00 00` (calendrier 5, dérivé du 1). Même fichier enregistré par Project 2010 (A) : `01 00 00 00 ff ff ff ff ff ff ff ff` (1, de base), `02 00 00 00 01 00 00 00 00 00 00 00` (2, base 1).
- **déduit** (164 fichiers, 540 calendriers en B, 719 en A, tous de nature et de base égales au corrigé, hors calendriers supprimés et absents du corrigé) : A `[u32 UID][i32 base][i32 ?]` ; B `[i32 base][i32 ?][u32 UID]` ; base nulle ou négative = calendrier de base (B : 0 ; A : −1, ou une valeur négative aberrante sur 2 calendriers), sinon UID du calendrier de base. Un calendrier dérivé est un calendrier de ressource (non repris dans ce lot).
- **observé** (`mpp14calendar.mpp`, calendrier 7 « Night Shift », clé `0x0D400008`, 428 octets) : 7 jours de 60 octets puis 8 octets ; jour 1 (lundi) `00 00 01 00 58 02 00 00 e8 35 …` puis à l'octet 20 `58 02 00 00` : une plage, 600 dixièmes au total, début 13 800 dixièmes (23:00), durée 600 (1 h) ; le corrigé : lundi 23:00-00:00. Jour 2 : `00 00 03 00 c0 12 00 00 00 00 60 09 e8 35 …`, durées à l'octet 20 `08 07 00 00 60 09 00 00 58 02 00 00` : plages 00:00-03:00, 04:00-08:00, 23:00-24:00 ; le corrigé : idem. Jour 0 (dimanche) : `01 00 00 00 …`, chômé au corrigé.
- **déduit** (206 calendriers de base, semaine entière égale au corrigé pour les 206) : jour = `[u16 code][u16 nombre de plages][u32 total][5 × u16 débuts][u16][5 × u32 durées][5 × u32 cumuls]`, dixièmes de minute, dimanche d'abord ; code 0 = jour défini ici (aucune plage = chômé) ; code 1 = jour « par défaut », qui vaut au corrigé le jour du calendrier Standard (samedi et dimanche chômés, lundi à vendredi 08:00-12:00 et 13:00-17:00) ; calendrier de base sans clé `0x0D400008` = semaine par défaut. Sur un calendrier dérivé, code 1 = jour hérité de la base (5 jours vérifiés).
- **observé** (`mpp14exceptions.mpp`, calendrier 1, 704 octets) : à l'octet 420 `03 00 00 00` (3 exceptions) ; première à 424 : `7f 22 7f 22` (jour 8 831 = 2008-03-05, du 5 au 5), puis un jour de 60 octets à l'octet 12 sans plage ; le corrigé : exception chômée du 2008-03-05. Troisième : `8a 22 8a 22` (2008-03-16) et cinq plages de 1 h (08:00, 11:00, 14:00, 16:00, 18:00) : idem au corrigé.
- **observé** (`calendar/multi-day-exceptions/multi-day-exceptions-project2010-mpp14.mpp`) : exception `55 33 57 33` (2019-12-23 au 2019-12-25), à l'octet 88 de l'exception `14 00 00 00` puis « Christmas » en UTF-16 (20 octets), exception suivante juste après ; le corrigé : « Christmas », 2019-12-23 au 2019-12-25. `calendar-exception-precedence.mpp`, calendrier 18 : nom « Labor Day  » de 22 octets, exception suivante 2 octets plus loin.
- **déduit** (59 calendriers avec exceptions simples, toutes égales au corrigé) : exception = `[u16 premier jour][u16 dernier jour][u32][u32 0][jour de 60 octets][16 octets][u32 longueur du nom][nom UTF-16]`, l'exception suivante alignée sur 4 octets depuis le début du bloc.
- **déduit** (9 calendriers, `generated/calendar-recurring-exceptions/*`, `calendar-exception-precedence*.mpp`) : une exception périodique (au corrigé : quotidienne, hebdomadaire, mensuelle ou annuelle) a un `u32` non nul à l'octet 8 et une longueur différente ; son format est **inconnu** : le lecteur garde les exceptions qui la précèdent, s'arrête, et le signale (`mpp.calendar_recurring_skipped`).
- **inconnu** : semaines de travail datées (aucune dans le corpus MPP14), calendrier de base d'un calendrier de base (jamais observé).

## 11. Ordre des tâches et ID

- **observé** (`task/task-deletion/task-deletion2-project2013-mpp14.mpp`, B) : après suppression de lignes, les enregistrements gardent leur place et leur ID est réécrit : `01 00 00 00 02 00 00 00` (ID 1, UID 2 : « T1 », ID 1 au corrigé).
- **observé** (`task/task-deletion/task-deletion2-project2010-mpp14.mpp`, A) : même projet ; l'enregistrement complet de « T4 » garde `06 00 00 00 06 00 00 00` (UID 6, ID 6) alors que le corrigé lui donne l'ID 3 ; la ligne vide UID 8 a l'ID 4, à jour.
- **déduit** (162 fichiers sur 164) : l'ordre des tâches suit l'ID enregistré (à égalité, l'ordre des enregistrements) ; l'UID 0 est la récapitulative du projet. Un ID resté ancien (A, après suppression) peut déplacer une tâche (`task-deletion2-project2010-mpp14.mpp` : T4 et la ligne vide permutées) ; l'ordre par la position des enregistrements, essayé, est faux sur 49 tâches d'autres fichiers. **Inconnu** : où MS Project range l'ordre réel dans ce cas.

## 12. Écarts où le corrigé est soupçonné, et fichiers à part

- Calendriers supprimés (indicateur 2) gardés par le corrigé comme calendriers de base sans nom (`ResourceIdAndUniqueId-project2013-mpp14.mpp`, 9 calendriers) : le lecteur les ignore.
- Calendrier de base sans nom au fichier, nommé d'après une ressource par le corrigé (`mpp14assignmentfields.mpp` calendrier 4, `timephased-prorated-cost-resource.mpp` calendrier 3).
- Exception simple rendue « quotidienne, tous les 1 jour » par le corrigé : même effet que la période du lecteur.
- `assignment/assignment-deletion/sf248.mpp` : `FixedMeta` incohérent (6 éléments annoncés, 42 présents, plusieurs éléments vers le même enregistrement) ; le lecteur y lit 1 tâche, le corrigé 3. Fichier issu d'un rapport d'anomalie, sans doute endommagé ; laissé en écart mesuré.
- Durées de deux tâches à affectations découpées (`mpp14timephasedsegmentsmanualoffsets.mpp` UID 27, 29) : écart non expliqué.
