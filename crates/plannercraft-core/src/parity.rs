//! Plancher de parité avec MS Project.
//!
//! La parité se mesure en **nombre de cas** reproduits à l'identique sur un
//! manifeste figé (`tests/corpus/manifest.toml`, puis `tests/oracle/`), et non
//! en pourcentage d'un corpus mouvant : ajouter un fichier difficile ouvre une
//! nouvelle ligne sans faire baisser la parité mesurée (ADR-0004, écart au
//! brief qui parlait de pourcentage).
//!
//! Le harnais du corpus (`tests/corpus`, ADR-0009) échoue si un fichier
//! présent passe sous son `floor`, ou si la parité globale passe sous
//! [`CORPUS_FLOOR`] (planchers des fichiers absents déduits : un absent ne
//! compte ni pour ni contre). Quand la parité monte, on relève les planchers
//! du manifeste et [`CORPUS_FLOOR`] dans le même commit ; on ne les baisse
//! jamais, sauf ADR (fichier retiré, valeur enregistrée reconnue fausse).

/// Somme des `floor` du manifeste du corpus (vérifiée par le harnais).
/// Mesuré le 2026-10-20 : A1 51, A2 18, A4 42, A6 34, A7 11 (159 cas ; J2 :
/// 91, A4 22, A7 0 ; J3 : contraintes FNET de A7, R-090, R-094 ; calendrier
/// du projet dérivé de A4, R-113 ; J5 : placement par affectation sur A6,
/// puis retards de nivellement, R-178 ; J6 lot L1 : feuilles achevées et
/// agrégats de A7, R-203, R-210).
pub const CORPUS_FLOOR: u32 = 156;

/// Plancher de la comparaison des ressources du corpus (J5, lot L9) :
/// cas de tâches (`Work`, dates des affectations) et de ressources (`Work`,
/// `OverAllocated`) identiques, somme des planchers par fichier du harnais
/// (`tests/corpus/resources.rs`). Compteur séparé, hors [`FLOOR`].
/// Mesuré le 2026-10-20 : A4 44/44, A6 35/35, A7 7/7 (A1, A2 : aucun cas ;
/// A7 5 → 7 en J6 lot L1, affectations des feuilles achevées, R-209).
pub const CORPUS_RESOURCE_FLOOR: u32 = 86;

/// Cas de l'oracle MS Project (`tests/oracle`), zéro sans licence.
pub const ORACLE_FLOOR: u32 = 0;

/// Plancher total en nombre de cas identiques (ADR-0004).
pub const FLOOR: u32 = match CORPUS_FLOOR.checked_add(ORACLE_FLOOR) {
    Some(v) => v,
    None => u32::MAX,
};
