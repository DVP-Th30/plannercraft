# Tests transverses

Paquet interne `plannercraft-tests` (non publié). Les tests unitaires restent dans chaque crate ; ce dossier regroupe ce qui traverse les crates :

| Dossier | Cible | Rôle |
|---|---|---|
| `rules/` | `cargo test -p plannercraft-tests --test rules` | un test par règle de calcul `R-xxx` |
| `fixtures/` | (données) | plannings MSPDI de test et résultats attendus figés |
| `oracle/` | `cargo test -p plannercraft-tests --test oracle` | comparaison avec MS Project (coquille, ADR-0004) |
| `corpus/` | `cargo xtask corpus fetch` puis `cargo test -p plannercraft-tests --test corpus` | fichiers MSPDI publics recalculés et comparés aux valeurs de MS Project (manifeste, planchers, rapport) |
| `schema/` | `cargo xtask schema fetch` puis `cargo test -p plannercraft-tests --test schema` | conformité au schéma MSPDI `mspdi_pj15.xsd` de Microsoft (P27 : table `plannercraft_mspdi::schema` égale au XSD ; P26 : documents écrits valides) ; XSD extrait du SDK de Project 2013, jamais commité, test sauté s'il est absent sauf sous `PLANNERCRAFT_SCHEMA=required` (job CI `schema`) |

Les interdits de la règle 10 (`unwrap`, `expect`, `panic!`, indexation…) sont levés dans les tests par un `#![allow(...)]` en tête de fichier, commenté : un test qui échoue doit paniquer.
