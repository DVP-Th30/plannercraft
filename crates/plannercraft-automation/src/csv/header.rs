//! En-têtes reconnus (R-144).

use crate::text::lex::fold;

use super::{CsvColumn, CsvLocale};

/// Champ d'une colonne d'en-tête.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    /// Colonne lue.
    Column(CsvColumn),
    /// Colonne connue, ignorée en J3 (ressources, travail).
    Unsupported,
}

/// Langue d'un en-tête : propre au français, à l'anglais, ou commune.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderLanguage {
    Fr,
    En,
    Both,
}

/// En-tête replié : casse et accents ignorés, `°` lu `o`, `%` lu `pct`,
/// tout caractère non alphanumérique retiré (« N° unique » → `nounique`).
pub(crate) fn fold_header(header: &str) -> String {
    let mut out = String::new();
    for c in fold(header.chars()).chars() {
        match c {
            '°' | 'º' => out.push('o'),
            '%' => out.push_str("pct"),
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

/// R-144 : champ et langue d'un en-tête replié (casse, accents, espaces et
/// ponctuation ignorés ; `°` lu `o`, `%` lu `pct`).
pub(crate) fn recognize(header: &str) -> Option<(Field, HeaderLanguage)> {
    use CsvColumn as C;
    use Field::{Column, Unsupported};
    use HeaderLanguage::{Both, En, Fr};
    Some(match fold_header(header).as_str() {
        "no" | "numero" => (Column(C::Id), Fr),
        "id" => (Column(C::Id), Both),
        "nounique" | "numerounique" => (Column(C::UniqueId), Fr),
        "uniqueid" => (Column(C::UniqueId), En),
        "nom" | "nomdelatache" => (Column(C::Name), Fr),
        "name" | "taskname" => (Column(C::Name), En),
        "duree" => (Column(C::Duration), Fr),
        "duration" => (Column(C::Duration), En),
        "debut" => (Column(C::Start), Fr),
        "start" => (Column(C::Start), En),
        "fin" => (Column(C::Finish), Fr),
        "finish" => (Column(C::Finish), En),
        "predecesseurs" => (Column(C::Predecessors), Fr),
        "predecessors" => (Column(C::Predecessors), En),
        "nouniquedespredecesseurs" => (Column(C::UniquePredecessors), Fr),
        "uniqueidpredecessors" => (Column(C::UniquePredecessors), En),
        "niveauhierarchique" => (Column(C::OutlineLevel), Fr),
        "outlinelevel" => (Column(C::OutlineLevel), En),
        "jalon" => (Column(C::Milestone), Fr),
        "milestone" => (Column(C::Milestone), En),
        "notes" => (Column(C::Notes), Both),
        "wbs" => (Column(C::Wbs), Both),
        "codewbs" => (Column(C::Wbs), Fr),
        "pctacheve" => (Column(C::PercentComplete), Fr),
        "pctcomplete" => (Column(C::PercentComplete), En),
        "nomsdesressources" | "travail" => (Unsupported, Fr),
        "resourcenames" | "work" => (Unsupported, En),
        _ => return None,
    })
}

/// R-144 : intitulé d'une colonne à l'export.
pub(crate) const fn label(column: CsvColumn, locale: CsvLocale) -> &'static str {
    use CsvColumn as C;
    match (column, locale) {
        (C::Id, CsvLocale::Fr) => "N°",
        (C::Id, CsvLocale::En) => "ID",
        (C::UniqueId, CsvLocale::Fr) => "N° unique",
        (C::UniqueId, CsvLocale::En) => "Unique ID",
        (C::Name, CsvLocale::Fr) => "Nom",
        (C::Name, CsvLocale::En) => "Name",
        (C::Duration, CsvLocale::Fr) => "Durée",
        (C::Duration, CsvLocale::En) => "Duration",
        (C::Start, CsvLocale::Fr) => "Début",
        (C::Start, CsvLocale::En) => "Start",
        (C::Finish, CsvLocale::Fr) => "Fin",
        (C::Finish, CsvLocale::En) => "Finish",
        (C::Predecessors, CsvLocale::Fr) => "Prédécesseurs",
        (C::Predecessors, CsvLocale::En) => "Predecessors",
        (C::UniquePredecessors, CsvLocale::Fr) => "N° unique des prédécesseurs",
        (C::UniquePredecessors, CsvLocale::En) => "Unique ID Predecessors",
        (C::OutlineLevel, CsvLocale::Fr) => "Niveau hiérarchique",
        (C::OutlineLevel, CsvLocale::En) => "Outline Level",
        (C::Milestone, CsvLocale::Fr) => "Jalon",
        (C::Milestone, CsvLocale::En) => "Milestone",
        (C::Notes, _) => "Notes",
        (C::Wbs, _) => "WBS",
        (C::PercentComplete, CsvLocale::Fr) => "% achevé",
        (C::PercentComplete, CsvLocale::En) => "% Complete",
    }
}
