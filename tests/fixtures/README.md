# Fixtures

Plannings MSPDI de test écrits pour PlannerCraft (donc versionnés, sous la licence du dépôt) et leurs résultats attendus figés (« golden files »).

## Convention

- Un planning par fichier : `<sujet>.xml` (MSPDI), accompagné de ses résultats attendus (dates, durées, marges) relus et validés à la main. Le format exact des résultats attendus est fixé avec la première fixture.
- Toute variation d'un résultat attendu fait échouer le test ; la modifier exige une justification dans le message de commit (quelle règle a changé, pourquoi).
- Pas de fichier dont la provenance n'est pas claire : les fichiers publics tiers vont dans `tests/corpus/` (non versionnés), pas ici.

## `mspdi/` — lecteur MSPDI (J2)

Petits fichiers MSPDI **écrits à la main** pour PlannerCraft (aucun fichier tiers, aucune donnée copiée d'un fichier du corpus), chacun commenté en tête. Ils suivent le schéma `mspdi_pj15.xsd` (espace de noms `http://schemas.microsoft.com/project`) et n'ont que les éléments utiles au cas. Les résultats attendus sont écrits dans les tests des règles (`tests/rules/r070_*` à `r088_*`, `r124_*` à `r126_*`), avec leur calcul commenté.

| Fichier | Contenu | Règles |
|---|---|---|
| `project_defaults.xml` | projet minimal, booléens et options absents | R-076 |
| `project_options.xml` | options du projet toutes renseignées (facteurs, heures, formats, seuil de criticité, plusieurs chemins critiques) | R-076, R-086 |
| `calendars.xml` | huit calendriers : exceptions de chaque type, plages à fusionner ou invalides, jours manquants, dérivés, base négative, ancienne forme, semaines datées | R-077 à R-082 |
| `tasks.xml` | tâches, formats de durée, liens des quatre types, décalages ouvrés, écoulés et en %, liens ignorés, valeurs enregistrées, affectations | R-070 à R-076, R-083 |
| `manual.xml` | tâches manuelles (avec et sans `Manual*`, début seul), récapitulative manuelle | R-085 |
| `null_rows.xml` | 11 lignes dont 5 vides, ID et drapeau `Summary` incohérents | R-084 |
| `resources.xml` | ressources (travail, matérielle, coût ; taux par heure et unité d'affichage), affectations (unités, retard en dixièmes, profils, répartitions), lignes de base de tâche, de ressource et d'affectation, travail des tâches ; seules valeurs numériques reprises d'une observation : le taux et le coût de O18 | R-083, R-087, R-088, R-124 à R-126 |

Les mêmes fichiers servent aux tests de robustesse du lecteur (`crates/plannercraft-mspdi/tests/robustness.rs` : chaque préfixe, mutations P18) et à la propriété « pas de fuite » P19 (`crates/plannercraft-mspdi/tests/no_leak.rs`).
