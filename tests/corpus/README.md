# Corpus public

Fichiers MSPDI réels enregistrés par MS Project : chacun contient les valeurs que MS Project a calculées (dates au plus tôt et au plus tard, marges, criticité, durées des récapitulatives). Le harnais (ADR-0004, ADR-0009) lit chaque fichier — les valeurs calculées vont dans `RecordedSchedule`, jamais dans le `Project`, l'effacement est donc structurel —, laisse le moteur recalculer et compare champ par champ, sans tolérance.

## Commandes

```sh
cargo xtask corpus fetch                              # télécharge les fichiers (curl, taille et sha256 vérifiés)
cargo test -p plannercraft-tests --test corpus -- --nocapture
PLANNERCRAFT_CORPUS=required cargo test -p plannercraft-tests --test corpus   # échec si tous les fichiers sont absents (job CI `corpus`)
```

Le rapport est écrit dans `target/tmp/corpus-report.md` et `target/tmp/corpus-report.json` (déterministe, **aucun nom de tâche ni texte du fichier** : UID, ID, champs et valeurs seulement). Le job CI `corpus` le publie en artefact ; les fichiers eux-mêmes ne le sont jamais.

## Règles

- Les fichiers ne sont **pas** versionnés (droits d'auteur) : ils vont dans `tests/corpus/files/`, ignoré par git. Copie privée de test : jamais commitée, publiée en artefact ni redistribuée. En CI, ils sont téléchargés à chaque exécution (1,3 Mo, adresses figées sur un commit, taille bornée par `curl --max-filesize` et vérifiée avant lecture, sha256 contrôlé) et **jamais mis en cache** : un cache d'Actions peut être restauré par une exécution venue d'un fork (qui exécute le `ci.yml` de sa PR), il n'y a donc aucune copie à extraire. Le job ne tourne pas pour une PR venue d'un fork, par confort : ce n'est pas un contrôle de sécurité.
- Seul [`manifest.toml`](manifest.toml) est versionné (format 2). Une entrée `[[file]]` par fichier, **toutes les clés obligatoires**, clé inconnue refusée :

| Clé | Contenu |
|---|---|
| `name` | nom du fichier dans `files/`, sans séparateur, unique |
| `source` | provenance lisible |
| `url` | adresse de téléchargement, `https://` |
| `mirrors` | adresses de repli `https://`, dans l'ordre (peut être vide) |
| `license` | identifiant SPDX, ou `"none"` (aucune licence : tous droits réservés) |
| `rights` | droits et usage |
| `date` | date de récupération, `AAAA-MM-JJ` |
| `size` | taille en octets (> 0) |
| `sha256` | empreinte SHA-256, 64 hexadécimaux minuscules, unique |
| `producer` | `msproject`, `msproject-probable` ou `unknown` |
| `save_version` | `SaveVersion` attendu |
| `note` | remarque (peut être vide) : contenu du fichier, divergences connues et jalon qui les traitera ; recopiée dans la section du fichier du rapport |
| `floor` | plancher : nombre minimal de cas identiques sur ce fichier (≥ 0) |

- Le manifeste est **figé** : on ajoute des lignes ; un retrait passe par un ADR et garde le nom. `plannercraft_core::parity::CORPUS_FLOOR` vaut la somme des `floor` (testé) et `FLOOR = CORPUS_FLOOR + ORACLE_FLOOR`.
- Critère « enregistré par MS Project » (signature vérifiée et affichée par le rapport) : `SaveVersion` conforme, tâche UID 0, `EarlyStart`/`LateFinish`/`TotalSlack`/`FreeSlack` sur toutes les tâches, sections d'interface (`Views` … `VBAProjects`). Exclus : composants Gantt dotés d'un moteur, visionneuses et clones (règle 1), exports d'autres outils, fichiers sans valeurs calculées.
- Un `.mpp` peut être converti en MSPDI avec MPXJ **comme outil externe uniquement** ; son code et sa documentation ne sont jamais lus (règle 1). Le noter dans `note`.

## Comparaison et verdict

- **Cas** : une tâche (UID 0 compris) présente dans le fichier et planifiée par le moteur. **Exclus**, comptés par raison : manuelle (feuille ou récapitulative), inactive, ligne vide, exclue par le moteur, aucune valeur comparable.
- **Champs**, comparés quand la valeur enregistrée existe : `start`, `finish`, `early_start`, `early_finish`, `late_start`, `late_finish` (instant exact), `total_slack`, `free_slack` (minutes du moteur × 10 contre les dixièmes de MSPDI, exact), `critical`, `duration` (récapitulatives et UID 0). **Identique** : tous les champs comparés égaux.
- Les tâches touchées par une fonction hors J2 ne sont pas exclues : elles sont **étiquetées** par causes probables (`constraint`, `deadline`, `progress`, `leveling_delay`, `assigned`, `summary_link`, `milestone`, `manual_predecessor`, `task_calendar`, `project_calendar_derived`, `elapsed`, `lag`, `slack_not_tenth_multiple`, plus `divergent_predecessor` et `divergent_child` pour une divergence propagée) et comptent si elles coïncident.
- **Verdict** : tout fichier présent doit atteindre son `floor` (sinon régression nommée) ; un fichier `producer = msproject` doit comparer, pour chaque cas, les 9 champs de base (10 pour une récapitulative ou l'UID 0), et n'avoir aucune tâche exclue faute de valeur comparable, sinon la comparaison est « incomplète » et le verdict échoue (une régression du lecteur ne tient pas le plancher à vide) ; le rapport publie, par fichier, le nombre de comparaisons par champ ; un fichier absent ne compte ni pour ni contre ; une erreur de lecture d'un fichier qui a un plancher échoue ; « plancher relevable » si identiques > `floor`.
- **Relever le plancher** : recopier dans chaque `floor` le nombre d'identiques mesuré, recalculer `CORPUS_FLOOR`, commiter les deux ensemble ; jamais à la baisse, sauf ADR. Les règles exercées par des cas identiques (table du rapport) passent à `vérifié contre MS Project` dans `docs/scheduling-rules.md`, avec la référence du cas.

## Aller-retour MSPDI (P25)

Le harnais relit aussi chaque fichier présent après une écriture par PlannerCraft (profil observé, R-121) : lecture → écriture → relecture. Le projet relu doit égaler `plannercraft_mspdi::normalize(projet lu)` (R-133) avec les valeurs relues écrites (`Computed::Recorded`) comme avec celles du moteur (`Computed::Schedule`), le planning enregistré relu doit égaler celui du fichier, et les éléments réécrits de chaque entité doivent suivre l'ordre source. Le rapport publie, par fichier, ces quatre verdicts, le nombre d'`Extras` par entité, les calculés non recalculés écartés (par nom), les enfants perdus et les diagnostics d'écriture. Un fichier `producer = msproject` qui n'est pas identique fait échouer le verdict. « Relu identique » par PlannerCraft n'est pas une vérification par MS Project.

## Ressources (J5)

`corpus_resource_floor` compare, à compteur séparé des cas de tâches (`plannercraft_core::parity::CORPUS_RESOURCE_FLOOR`, hors `FLOOR`, planchers par fichier dans `resources.rs`) :

- **cas de tâche** : tâche automatique, active, planifiée, qui porte du travail ou une affectation à une vraie ressource ; `Work` de la tâche (R-184) et `Start`/`Finish` de chaque affectation hors fictive −65 535 (R-173 pour une feuille ; R-179 pour une récapitulative, comparées depuis la correction de la durée des affectations de récapitulatives à durée fixe : A4 et A6 identiques, cas et planchers inchangés) ;
- **cas de ressource** : ressource (UID 0 exclu) dont le fichier enregistre `Work` ou `OverAllocated` (R-184, R-186) ; `OverAllocated` de A7 indicatif (O31).

Divergences listées par le test (`--nocapture`) et dans `target/tmp/corpus-resources.json` (UID seulement, aucun nom).
