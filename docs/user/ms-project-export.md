# Échanger avec MS Project : XML et CSV

*English version below.* Sources : pages Microsoft « Export or import data to another file format » et « Saving and Opening Projects in XML Format » ; détail des règles dans [`docs/scheduling-rules.md`](../scheduling-rules.md) (R-087 à R-089, R-120 à R-133, R-140 à R-146).

## 1. Pourquoi passer par le format XML

PlannerCraft ne lit pas les fichiers `.mpp` : c'est un format binaire non documenté, hors du périmètre de la version 1. MS Project sait en revanche enregistrer **tout le projet** dans son format XML (« Format XML », schéma MSPDI) et le relire lui-même : c'est le format natif de PlannerCraft et le chemin d'échange recommandé.

Project pour le web (devenu Planner Premium) et Project Online sans le client de bureau ne produisent pas ce fichier : à notre connaissance (fils Microsoft Q&A, pas de page officielle), ils n'exportent que vers Excel. Il faut MS Project Desktop (2010 ou plus récent), ou passer par le CSV (section 5).

## 2. Les cinq étapes (MS Project Desktop 2010 et suivants)

Les libellés des menus sont ceux des pages Microsoft en anglais, traduits : ils peuvent varier légèrement selon la version.

1. Dans MS Project : **Fichier › Enregistrer sous › Parcourir**.
2. Dans **Type**, choisir **Format XML (\*.xml)**. Tout le projet est enregistré, sans assistant ni table de correspondance.
3. Ouvrir le `.xml` dans PlannerCraft et lire les diagnostics : chaque valeur illisible, corrigée ou ignorée y est nommée, avec l'élément en cause. La ligne de commande (`plannercraft`) et l'application arrivent avec les jalons J4 et J7 ; en J3, la lecture et l'écriture existent dans la bibliothèque `plannercraft-mspdi`.
4. Enregistrer depuis PlannerCraft : le fichier écrit est du XML MS Project. S'il existe un fichier compagnon `<nom>.plannercraft.json` (état des vues, historique), le garder à côté du `.xml` ; il n'est jamais nécessaire pour rouvrir le projet.
5. Rouvrir dans MS Project : **Fichier › Ouvrir**, type **Format XML**, puis dans l'assistant d'importation choisir **Comme nouveau projet**. « Ajouter au projet actif » ajoute les tâches au projet ouvert (nouveaux numéros) ; « Fusionner avec le projet actif » écrase les tâches de même numéro unique.

Si MS Project refuse d'ouvrir le fichier, vérifier **Fichier › Options › Centre de gestion de la confidentialité › Paramètres › Formats hérités** (source communautaire, Microsoft Q&A : l'ouverture de certains formats peut y être bloquée). Selon Microsoft, MS Project n'importe qu'un XML valide contre son schéma d'échange ; PlannerCraft écrit dans l'ordre des fichiers que MS Project enregistre lui-même, mais **la relecture par MS Project n'a pas encore été vérifiée** (aucune licence pour l'instant).

## 3. Ce qui est conservé

| Donnée | Conservation |
|---|---|
| Tâches, hiérarchie, ordre, lignes vides | lues et réécrites |
| Durées et leur format (unité, écoulé, estimé « ? ») | lues et réécrites |
| Liens (FS, SS, FF, SF) et décalages (durée, écoulé, %) | lus et réécrits |
| Contraintes, échéances, option « respecter les contraintes » | lues et réécrites |
| Planification à rebours (depuis la date de fin) | lue et réécrite |
| Calendriers : semaine type, exceptions (récurrentes comprises), semaines de travail datées | lus et réécrits |
| Ressources, affectations (unités, travail, profil, retard) | lues et réécrites |
| Lignes de base 0 à 10 | lues et réécrites |
| Avancement saisi (% achevé, dates et durées réelles, date de reprise d'une tâche en cours), options de la date d'état, notes | lus et réécrits ; durée et travail restants dérivés (durée − réel, travail − travail réel), une valeur stockée en désaccord est remplacée et signalée une fois (`mspdi.progress_inconsistent`) |
| Tâches manuelles et inactives | lues et réécrites, telles quelles |
| Champs personnalisés, codes hiérarchiques, vues, tables, filtres, groupes, tables de taux, disponibilités, répartitions détaillées (sauf le travail planifié d'une affectation, voir section 4) | **recopiés tels quels**, sans être interprétés |

## 4. Ce qui est recalculé ou perdu

**Recalculé par MS Project à l'ouverture** (PlannerCraft ne les écrit pas) : coûts, valeur acquise, pic d'unités des ressources, répartition du travail planifié d'une affectation (`TimephasedData` de type 1, écartée à la lecture sauf si le profil de travail a été modifié à la main ; gardée alors seulement tant que l'affectation n'a pas bougé). Début et fin des affectations, travail et surutilisation des ressources, avancement (date d'arrêt `Stop`, réels et restes des récapitulatives et du projet, parties réelles des affectations) et écarts (début, fin et travail, par rapport à la ligne de base 0, en minutes × 1000) sont écrits par PlannerCraft depuis son moteur ; les écarts de début et de fin d'une affectation sont omis (leur unité n'est pas documentée). La répartition du travail réel d'une affectation (`TimephasedData` de types 2 et 3) est gardée tant que sa somme vaut toujours son travail réel. **Date de reprise (`Resume`) d'une tâche en cours** : PlannerCraft n'écrit que la reprise **saisie** (celle de « replanifier le reste », ou relue d'un fichier), jamais la reprise qu'il calcule depuis les liens ; MS Project la recalcule à l'ouverture. Raison : un `Resume` relu est pris pour une reprise saisie, un plancher ; écrire la reprise calculée la figerait. Conséquence à l'ouverture d'un fichier enregistré par MS Project : la reprise qu'il a calculée pour une tâche hors séquence devient un plancher dans PlannerCraft, et le reste ne peut plus avancer si son prédécesseur raccourcit (`explain_date` cite alors « reprise saisie »).

**Dates calculées.** PlannerCraft écrit ses propres dates (au plus tôt, au plus tard, marges, critique), avancement compris : une tâche achevée garde ses dates réelles, une tâche en cours est coupée une fois à sa date d'arrêt et reprend à sa date de reprise. Il écrit aussi le travail restant des affectations (travail − travail réel), le travail des récapitulatives et la durée des tâches placées par leurs ressources. Les options de la date d'état autres que les défauts de MS Project (« déplacer la fin des parties achevées… », les quatre options `Move*` ; « la mise à jour de l'état des tâches met à jour l'état des ressources » désactivée) sont gardées dans le fichier mais non appliquées, et signalées à la lecture (`progress.option_unsupported`).

**Perdu à l'aller-retour** :
- la forme ancienne des exceptions de calendrier (`WeekDay` de `DayType 0`, que MS Project écrit en plus de la forme `Exceptions`) : PlannerCraft n'écrit que la forme `Exceptions`, que MS Project 2007 et suivants lisent ; qu'il régénère l'ancienne à l'enregistrement est une hypothèse, non vérifiée faute de licence ;
- le nombre d'occurrences d'une exception qui n'a pas été saisie par occurrences ;
- les éléments inconnus à l'intérieur d'un jour de calendrier, d'une exception, d'une semaine de travail, d'une plage horaire ou d'un lien ;
- un élément recopié tel quel qui contient une entité XML non définie (`&nbsp;`…) : il serait illisible par MS Project, PlannerCraft ne le réécrit pas et le signale ;
- l'unité propre de la durée saisie d'une tâche manuelle (`ManualDuration`) : elle est relue avec l'unité, la base et l'estimation de la durée de la tâche ;
- un retard d'affectation négatif (ramené à 0) ;
- les valeurs illisibles (remplacées par leur valeur par défaut, signalées à la lecture) et les liens invalides ;
- les textes au-delà de la longueur maximale du schéma (tronqués, signalés) ;
- les pourcentages non entiers (arrondis au pour-cent) et la précision des décalages en % (arrondis au pas de 0,1 %) ;
- une exception « périodique » réécrite en exception quotidienne (mêmes jours).

## 5. CSV

Le CSV convient pour partir d'un tableur, ou depuis MS Project quand le XML n'est pas disponible.

- **Depuis MS Project** : **Fichier › Enregistrer sous › Parcourir**, type **CSV**, puis l'assistant d'exportation : nouvelle table de correspondance, données « Tâches », colonnes recommandées ci-dessous. Le séparateur, pas l'extension, décide du type de fichier.
- **Colonnes reconnues** (casse, accents et ponctuation ignorés ; en français ou en anglais) : N° / ID, N° unique / Unique ID, Nom / Name, Durée / Duration, Début / Start, Fin / Finish, Prédécesseurs / Predecessors, N° unique des prédécesseurs / Unique ID Predecessors, Niveau hiérarchique / Outline Level, Jalon / Milestone, Notes, WBS, % achevé / % Complete. « Noms des ressources » et « Travail » sont signalés et ignorés pour l'instant ; une colonne inconnue est signalée et ignorée. Il faut au moins Nom ou Durée.
- **Durées** : « 5j », « 1,5 j », « 3h », « 2s », « 1mo », « 5ej » (jours écoulés), « 2j? » (estimée) ; en anglais « 5d », « 2w ». Sans unité : l'unité par défaut du projet (le jour).
- **Prédécesseurs** : « 3FD+2j;5DD » en français, « 3FS+2d,5SS » (ou « 3FS+2d;5SS ») en anglais ; un numéro seul = fin à début ; décalage en durée ou en % (« 3FS+50% »). La colonne « N° unique des prédécesseurs » n'est lue qu'avec la colonne « N° unique » (sans elle, les numéros uniques sont réattribués : la colonne est ignorée et signalée).
- **Séparateurs et décimales** : français `;` et virgule, dates JJ/MM/AAAA ; anglais `,` et point, dates MM/JJ/AAAA. Séparateur, langue et encodage (UTF-8, UTF-16, Windows-1252) sont détectés ; une première ligne Excel `sep=;` fixe le séparateur. En anglais, « 1,000 » est refusé (séparateur de milliers, pas une décimale).
- **Attention aux dates** : l'ordre jour/mois vient de la langue des en-têtes, pas des valeurs. « 03/04/2026 » est le 3 avril sous des en-têtes français et le 4 mars sous des en-têtes anglais. Quand aucune valeur d'une colonne de dates ne lève l'ambiguïté (aucun jour au-delà de 12), l'import le signale par un avertissement `csv.date_ambiguous`, une fois par colonne ; il n'y en a pas quand la langue est imposée. Un fichier aux en-têtes anglais avec des dates JJ/MM (Excel en français, par exemple) doit être importé en imposant la langue.
- **Dates** : ignorées par défaut (PlannerCraft recalcule et signale les écarts) ; sur option, Début devient « ne pas commencer avant le » ou, sans Début, Fin devient « ne pas finir avant le » (une seule contrainte par tâche), comme une date saisie à la main dans MS Project.
- **Hiérarchie** : la colonne Niveau hiérarchique fait les récapitulatives ; leur durée est calculée (celle du fichier est ignorée).
- **Erreurs** : une valeur illisible est signalée avec son numéro de ligne et remplacée par sa valeur par défaut (au plus 10 000 signalements, 1 000 par type, puis leur nombre). Le fichier n'est refusé que s'il est vide, trop grand (plus de 64 Mio), sans ligne d'en-tête, sans colonne Nom ni Durée, s'il a plus de 200 000 lignes ou plus de 256 colonnes, ou si ses octets ne sont pas dans l'encodage imposé.
- **Export** : UTF-8 avec BOM, fins de ligne CRLF ; un texte qui commence par `=`, `+`, `-` ou `@` est précédé d'une apostrophe pour qu'un tableur ne l'exécute pas comme une formule (retirée à la relecture).

---

# Exchanging with MS Project: XML and CSV

## 1. Why the XML format

PlannerCraft does not read `.mpp` files: the binary format is undocumented and out of scope for version 1. MS Project can save **the whole project** in its XML format ("XML Format", MSPDI schema) and read it back itself: it is PlannerCraft's native format and the recommended exchange path.

Project for the web (now Planner Premium) and Project Online without the desktop client do not produce this file: to our knowledge (Microsoft Q&A threads, no official page), they only export to Excel. Use MS Project Desktop (2010 or later), or go through CSV (section 5).

## 2. Five steps (MS Project Desktop 2010 and later)

1. In MS Project: **File › Save As › Browse**.
2. In **Save as type**, choose **XML Format (\*.xml)**. The whole project is saved, without a wizard or map.
3. Open the `.xml` in PlannerCraft and read the diagnostics: every unreadable, corrected or ignored value is named, with its element. The command line (`plannercraft`) and the application come with milestones J4 and J7; in J3, reading and writing are in the `plannercraft-mspdi` library.
4. Save from plannercraft: the file is MS Project XML. If a companion file `<name>.plannercraft.json` exists (view state, history), keep it next to the `.xml`; it is never required to reopen the project.
5. Reopen in MS Project: **File › Open**, type **XML Format**, then in the Import Wizard choose **As a new project**. "Append the data to the active project" adds the tasks to the open project (new numbers); "Merge the data into the active project" overwrites tasks with the same unique ID.

If MS Project refuses the file, check **File › Options › Trust Center › Trust Center Settings › Legacy Formats** (community source, Microsoft Q&A: opening some formats can be blocked there). According to Microsoft, MS Project imports only XML that validates against its data interchange schema; PlannerCraft writes in the order of the files MS Project saves itself, but **reading back by MS Project has not been verified yet** (no licence so far).

## 3. What is kept

| Data | Kept |
|---|---|
| Tasks, outline, order, blank rows | read and written back |
| Durations and their format (unit, elapsed, estimated "?") | read and written back |
| Links (FS, SS, FF, SF) and lags (duration, elapsed, %) | read and written back |
| Constraints, deadlines, "honor constraint dates" option | read and written back |
| Scheduling from the project finish date | read and written back |
| Calendars: work week, exceptions (recurring included), dated work weeks | read and written back |
| Resources, assignments (units, work, contour, delay) | read and written back |
| Baselines 0 to 10 | read and written back |
| Entered progress (% complete, actual dates and durations, resume date of a task in progress), status-date options, notes | read and written back; remaining duration and work are derived (duration − actual, work − actual work), a stored value that disagrees is replaced and reported once (`mspdi.progress_inconsistent`) |
| Manual and inactive tasks | read and written back, as they are |
| Custom fields, outline codes, views, tables, filters, groups, cost rate tables, availability, detailed timephased data (except an assignment's scheduled work, see section 4) | **copied verbatim**, not interpreted |

## 4. What is recalculated or lost

**Recalculated by MS Project on opening** (PlannerCraft does not write them): costs, earned value, an assignment's scheduled work distribution (`TimephasedData` type 1, dropped on reading unless the work contour was edited by hand; written back only while PlannerCraft places the assignment the same way). Assignment start and finish, resource work and overallocation, progress (stop and resume dates, actual parts of assignments) and variances (start, finish and work, against baseline 0, in minutes × 1000) are written by PlannerCraft from its engine; an assignment's start and finish variances are left out (their unit is not documented). An assignment's actual work distribution (`TimephasedData` types 2-3) is kept while it still adds up to its actual work.

**Calculated dates.** PlannerCraft writes its own dates (early, late, slack, critical), progress included: completed tasks keep their actual dates, a task in progress is split once at its stop date and resumes on its resume date. Status-date options other than MS Project's defaults ("move end of completed parts…", the four `Move*` options; "updating task status updates resource status" off) are kept in the file but not applied, and reported on reading (`progress.option_unsupported`).

**Lost in the round trip**:
- the legacy form of calendar exceptions (`WeekDay` with `DayType 0`, which MS Project writes alongside the `Exceptions` form): PlannerCraft writes only the `Exceptions` form, read by MS Project 2007 and later; that MS Project regenerates the legacy form on save is an assumption, not verified (no licence);
- the occurrence count of an exception not entered by occurrences;
- unknown elements inside a calendar day, an exception, a work week, a working time or a link;
- a verbatim element that contains an undefined XML entity (`&nbsp;`…): MS Project could not read it, PlannerCraft does not write it back and reports it;
- the own unit of a manual task's entered duration (`ManualDuration`): it is read back with the unit, basis and estimate of the task's duration;
- a negative assignment delay (set to 0);
- unreadable values (replaced by their default, reported on reading) and invalid links;
- text beyond the schema's maximum length (truncated, reported);
- non-integer percentages (rounded to the percent) and percentage-lag precision (rounded to steps of 0.1 %);
- a "periodic" exception written back as a daily exception (same days).

## 5. CSV

CSV suits a start from a spreadsheet, or from MS Project when XML is not available.

- **From MS Project**: **File › Save As › Browse**, type **CSV**, then the Export Wizard: new map, "Tasks" data, the columns below. The delimiter, not the file extension, decides the file type.
- **Recognized columns** (case, accents and punctuation ignored; French or English): ID / N°, Unique ID / N° unique, Name / Nom, Duration / Durée, Start / Début, Finish / Fin, Predecessors / Prédécesseurs, Unique ID Predecessors, Outline Level / Niveau hiérarchique, Milestone / Jalon, Notes, WBS, % Complete / % achevé. "Resource Names" and "Work" are reported and ignored for now; an unknown column is reported and ignored. Name or Duration is required.
- **Durations**: "5d", "1.5 d", "3h", "2w", "1mo", "5ed" (elapsed days), "2d?" (estimated); in French "5j", "2s". Without a unit: the project's default unit (day).
- **Predecessors**: "3FS+2d,5SS" (or "3FS+2d;5SS") in English, "3FD+2j;5DD" in French; a number alone means finish-to-start; lag as a duration or a percentage ("3FS+50%"). The Unique ID Predecessors column is read only with the Unique ID column (without it, unique IDs are reassigned: the column is ignored and reported).
- **Separators and decimals**: English `,` and dot, dates MM/DD/YYYY; French `;` and comma, dates DD/MM/YYYY. Separator, language and encoding (UTF-8, UTF-16, Windows-1252) are detected; an Excel first line `sep=;` sets the separator. In English, "1,000" is refused (thousands separator, not a decimal).
- **Beware of dates**: the day/month order comes from the language of the headers, not from the values. "03/04/2026" is 3 April under French headers and 4 March under English headers. When no value of a date column settles the order (no day above 12), the import reports it with a `csv.date_ambiguous` warning, once per column; there is none when the language is forced. A file with English headers and DD/MM dates (a French Excel, for instance) must be imported with the language forced.
- **Dates**: ignored by default (PlannerCraft recomputes and reports differences); as an option, Start becomes "start no earlier than" or, without a Start, Finish becomes "finish no earlier than" (one constraint per task), like a date typed by hand in MS Project.
- **Outline**: the Outline Level column makes summary tasks; their duration is calculated (the file's is ignored).
- **Errors**: an unreadable value is reported with its line number and replaced by its default (at most 10,000 reports, 1,000 per kind, then their count). The file is refused only if it is empty, too large (over 64 MiB), has no header line, has neither a Name nor a Duration column, has more than 200,000 rows or more than 256 columns, or if its bytes are not in the forced encoding.
- **Export**: UTF-8 with BOM, CRLF line endings; a text starting with `=`, `+`, `-` or `@` gets a leading apostrophe so that a spreadsheet does not run it as a formula (removed on reading back).
