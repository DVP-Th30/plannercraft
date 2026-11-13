//! En-tête du tableau et colonnes (étape 3) : repérage de la ligne des
//! intitulés (français et anglais), bornes des colonnes par le
//! quadrillage vertical s'il existe, sinon par les vides de la projection
//! des mots ; bord droit du tableau (début de la zone graphique).

use serde::Serialize;

use crate::extract::{Cpt, PageContent, Word};
use crate::lines::{TextLine, fold, join_words, mid};

/// Sorte de colonne reconnue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    /// N° / ID.
    Id,
    /// N° unique / Unique ID.
    UniqueId,
    /// Numéro hiérarchique / WBS / EDT.
    Wbs,
    /// Mode de la tâche (icônes).
    TaskMode,
    /// Indicateurs (icônes).
    Indicators,
    /// Nom de la tâche.
    Name,
    /// Durée.
    Duration,
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Prédécesseurs.
    Predecessors,
    /// Successeurs (lus, non utilisés : les liens viennent des
    /// prédécesseurs).
    Successors,
    /// Noms ressources.
    ResourceNames,
    /// % achevé.
    PercentComplete,
    /// Colonne non reconnue (intitulé gardé).
    Other,
}

impl ColumnKind {
    /// Nom stable (`snake_case`) de la sorte.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::UniqueId => "unique_id",
            Self::Wbs => "wbs",
            Self::TaskMode => "task_mode",
            Self::Indicators => "indicators",
            Self::Name => "name",
            Self::Duration => "duration",
            Self::Start => "start",
            Self::Finish => "finish",
            Self::Predecessors => "predecessors",
            Self::Successors => "successors",
            Self::ResourceNames => "resource_names",
            Self::PercentComplete => "percent_complete",
            Self::Other => "other",
        }
    }
}

/// Langue des intitulés.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderLanguage {
    /// Français.
    Fr,
    /// Anglais.
    En,
}

/// Intitulés reconnus (repliés par [`fold`]) : sorte et langue (`None` :
/// commun aux deux langues).
const DICTIONARY: &[(&str, ColumnKind, Option<HeaderLanguage>)] = {
    use ColumnKind as K;
    use HeaderLanguage::{En, Fr};
    &[
        ("id", K::Id, None),
        ("n°", K::Id, Some(Fr)),
        ("nº", K::Id, Some(Fr)),
        ("no", K::Id, None),
        ("no.", K::Id, None),
        ("n o", K::Id, Some(Fr)),
        ("#", K::Id, None),
        ("num", K::Id, None),
        ("numero", K::Id, Some(Fr)),
        ("unique id", K::UniqueId, Some(En)),
        ("n° unique", K::UniqueId, Some(Fr)),
        ("id unique", K::UniqueId, Some(Fr)),
        ("wbs", K::Wbs, None),
        ("edt", K::Wbs, Some(Fr)),
        ("outline number", K::Wbs, Some(En)),
        ("numero hierarchique", K::Wbs, Some(Fr)),
        ("n° hierarchique", K::Wbs, Some(Fr)),
        ("task mode", K::TaskMode, Some(En)),
        ("mode task", K::TaskMode, Some(En)),
        ("mode tache", K::TaskMode, Some(Fr)),
        ("tache mode", K::TaskMode, Some(Fr)),
        ("mode", K::TaskMode, None),
        ("mode de la tache", K::TaskMode, Some(Fr)),
        ("indicators", K::Indicators, Some(En)),
        ("indicateurs", K::Indicators, Some(Fr)),
        ("i", K::Indicators, None),
        ("task name", K::Name, Some(En)),
        ("name", K::Name, Some(En)),
        ("task", K::Name, Some(En)),
        ("tasks", K::Name, Some(En)),
        ("task description", K::Name, Some(En)),
        ("description", K::Name, None),
        ("nom de la tache", K::Name, Some(Fr)),
        ("nom des taches", K::Name, Some(Fr)),
        ("nom de tache", K::Name, Some(Fr)),
        ("nom", K::Name, Some(Fr)),
        ("tache", K::Name, Some(Fr)),
        ("taches", K::Name, Some(Fr)),
        ("libelle", K::Name, Some(Fr)),
        ("designation", K::Name, Some(Fr)),
        ("duration", K::Duration, Some(En)),
        ("dur", K::Duration, Some(En)),
        ("dur.", K::Duration, None),
        ("duree", K::Duration, Some(Fr)),
        ("start", K::Start, Some(En)),
        ("start date", K::Start, Some(En)),
        ("begin", K::Start, Some(En)),
        ("debut", K::Start, Some(Fr)),
        ("date de debut", K::Start, Some(Fr)),
        ("date debut", K::Start, Some(Fr)),
        ("finish", K::Finish, Some(En)),
        ("finish date", K::Finish, Some(En)),
        ("end", K::Finish, Some(En)),
        ("end date", K::Finish, Some(En)),
        ("fin", K::Finish, Some(Fr)),
        ("date de fin", K::Finish, Some(Fr)),
        ("date fin", K::Finish, Some(Fr)),
        ("predecessors", K::Predecessors, Some(En)),
        ("predecessor", K::Predecessors, Some(En)),
        ("pred", K::Predecessors, None),
        ("pred.", K::Predecessors, None),
        ("predecesseurs", K::Predecessors, Some(Fr)),
        ("predecesseur", K::Predecessors, Some(Fr)),
        ("successors", K::Successors, Some(En)),
        ("successeurs", K::Successors, Some(Fr)),
        ("resource names", K::ResourceNames, Some(En)),
        ("resources", K::ResourceNames, Some(En)),
        ("resource", K::ResourceNames, Some(En)),
        ("noms ressources", K::ResourceNames, Some(Fr)),
        ("noms des ressources", K::ResourceNames, Some(Fr)),
        ("nom des ressources", K::ResourceNames, Some(Fr)),
        ("ressources", K::ResourceNames, Some(Fr)),
        ("% complete", K::PercentComplete, Some(En)),
        ("%complete", K::PercentComplete, Some(En)),
        ("% acheve", K::PercentComplete, Some(Fr)),
        ("%acheve", K::PercentComplete, Some(Fr)),
        ("% achevement", K::PercentComplete, Some(Fr)),
    ]
};

/// Sorte et langue d'un intitulé replié.
#[must_use]
pub fn classify(folded: &str) -> Option<(ColumnKind, Option<HeaderLanguage>)> {
    DICTIONARY
        .iter()
        .find(|(text, _, _)| *text == folded)
        .map(|(_, kind, lang)| (*kind, *lang))
}

/// Colonne du tableau d'une page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Column {
    /// Sorte.
    pub kind: ColumnKind,
    /// Intitulé tel que lu.
    pub header: String,
    /// Bord gauche (centièmes de point).
    pub x0: Cpt,
    /// Bord droit.
    pub x1: Cpt,
    /// Bornes tirées du quadrillage (sinon : des vides entre les mots).
    pub ruled: bool,
}

/// Tableau repéré sur une page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableLayout {
    /// Colonnes, de gauche à droite.
    pub columns: Vec<Column>,
    /// Rang de la dernière ligne de texte de l'en-tête.
    pub header_last_line: usize,
    /// Bas de l'en-tête (ligne de base la plus basse de l'en-tête).
    pub header_bottom: Cpt,
    /// Bas du tableau (fin du quadrillage vertical), s'il est connu.
    pub table_bottom: Option<Cpt>,
    /// Langue des intitulés (`None` : seulement des intitulés communs).
    pub language: Option<HeaderLanguage>,
}

impl TableLayout {
    /// Colonne d'une sorte (la première).
    #[must_use]
    pub fn column(&self, kind: ColumnKind) -> Option<&Column> {
        self.columns.iter().find(|c| c.kind == kind)
    }

    /// Bord droit du tableau.
    #[must_use]
    pub fn right(&self) -> Cpt {
        self.columns.iter().map(|c| c.x1).max().unwrap_or(0)
    }

    /// Bord gauche du tableau.
    #[must_use]
    pub fn left(&self) -> Cpt {
        self.columns.iter().map(|c| c.x0).min().unwrap_or(0)
    }
}

/// Intitulé reconnu : sorte, langue, mots.
type Found = (ColumnKind, Option<HeaderLanguage>, Phrase);

/// Groupe d'intitulé : mots voisins de la bande d'en-tête.
#[derive(Clone, Debug)]
struct Phrase {
    words: Vec<Word>,
    x0: Cpt,
    x1: Cpt,
}

impl Phrase {
    fn text(&self) -> String {
        let mut words: Vec<&Word> = self.words.iter().collect();
        words.sort_by_key(|w| (w.baseline, w.x0));
        join_words(words)
    }
}

/// Groupe les mots d'une bande en intitulés : deux mots se suivent s'ils
/// se chevauchent horizontalement (deux lignes d'un même intitulé) ou si
/// l'écart est inférieur à 0,6 corps.
fn phrases(words: &[&Word]) -> Vec<Phrase> {
    let mut sorted: Vec<&Word> = words.to_vec();
    sorted.sort_by_key(|w| (w.x0, w.baseline));
    let mut out: Vec<Phrase> = Vec::new();
    for w in sorted {
        let gap_max = w.size.saturating_mul(6) / 10;
        match out.last_mut() {
            Some(p) if w.x0.saturating_sub(p.x1) < gap_max => {
                p.x1 = p.x1.max(w.x1);
                p.words.push(w.clone());
            }
            _ => out.push(Phrase {
                words: vec![w.clone()],
                x0: w.x0,
                x1: w.x1,
            }),
        }
    }
    out
}

/// Intitulés reconnus d'une bande de lignes : (sorte, langue, phrase).
fn recognize(band: &[&TextLine]) -> Vec<Found> {
    let words: Vec<&Word> = band.iter().flat_map(|l| l.words.iter()).collect();
    phrases(&words)
        .into_iter()
        .filter_map(|p| classify(&fold(&p.text())).map(|(k, l)| (k, l, p)))
        .collect()
}

/// Score d'une bande : vrai si elle contient un nom de tâche et au moins
/// une colonne de durée ou de date, chacune une seule fois.
fn is_header(found: &[Found]) -> bool {
    let has = |k: ColumnKind| found.iter().filter(|(kind, _, _)| *kind == k).count() == 1;
    has(ColumnKind::Name) && (has(ColumnKind::Duration) || has(ColumnKind::Start) || has(ColumnKind::Finish))
}

/// Repère le tableau d'une page : la première bande (une ligne, ou deux
/// lignes consécutives pour les intitulés repliés) du haut de la page qui
/// porte un nom de tâche et une durée ou une date.
#[must_use]
pub fn find_table(page: &PageContent, lines: &[TextLine]) -> Option<TableLayout> {
    for (i, line) in lines.iter().enumerate() {
        let next = lines.get(i.saturating_add(1)).filter(|n| {
            n.baseline.saturating_sub(line.bottom) < line.size.max(n.size).saturating_mul(16) / 10
        });
        let mut candidates: Vec<(usize, Vec<&TextLine>)> = vec![(i, vec![line])];
        if let Some(n) = next {
            candidates.push((i.saturating_add(1), vec![line, n]));
        }
        // Une bande sur deux lignes l'emporte si elle reconnaît plus
        // d'intitulés (« Task » / « Name » sur deux lignes).
        let mut best: Option<(usize, Vec<Found>)> = None;
        for (last, band) in candidates {
            let found = recognize(&band);
            if is_header(&found) && best.as_ref().is_none_or(|(_, b)| found.len() > b.len()) {
                best = Some((last, found));
            }
        }
        if let Some((last, found)) = best {
            return Some(layout(page, lines, last, found));
        }
    }
    None
}

/// Bornes des colonnes à partir des intitulés reconnus.
#[allow(
    clippy::too_many_lines,
    reason = "bornes, colonnes sans intitulé et bas du tableau, dans l'ordre"
)]
fn layout(page: &PageContent, lines: &[TextLine], header_last_line: usize, found: Vec<Found>) -> TableLayout {
    let header_bottom = lines
        .get(header_last_line)
        .map_or(0, |l| l.bottom.saturating_add(l.size.saturating_mul(3) / 10));
    let header_top = found
        .iter()
        .flat_map(|(_, _, p)| p.words.iter().map(Word::top))
        .min()
        .unwrap_or(header_bottom);
    let mut fr = 0_u32;
    let mut en = 0_u32;
    for (_, lang, _) in &found {
        match lang {
            Some(HeaderLanguage::Fr) => fr = fr.saturating_add(1),
            Some(HeaderLanguage::En) => en = en.saturating_add(1),
            None => {}
        }
    }
    let language = match (fr, en) {
        (0, 0) => None,
        (f, e) if f >= e => Some(HeaderLanguage::Fr),
        _ => Some(HeaderLanguage::En),
    };
    let mut found = found;
    found.sort_by_key(|(_, _, p)| p.x0);
    // Quadrillage : traits verticaux qui commencent dans l'en-tête ou
    // juste dessous (les cellules d'en-tête sont parfois séparées par des
    // aplats, le trait ne commence qu'à la première ligne) et descendent
    // sous lui d'au moins trois corps.
    let size = lines.get(header_last_line).map_or(800, |l| l.size.max(1));
    let mut rules: Vec<(Cpt, Cpt)> = page
        .segments
        .iter()
        .filter(|s| s.is_vertical())
        .filter(|s| {
            s.y0 <= header_bottom.saturating_add(size.saturating_mul(15) / 10)
                && s.y1 >= header_bottom.saturating_add(size.saturating_mul(3))
        })
        .filter(|s| s.y0 >= header_top.saturating_sub(size.saturating_mul(4)))
        .map(|s| (mid(s.x0, s.x1), s.y1))
        .collect();
    // Par abscisse, le plus long d'abord : le doublon gardé est le plus bas.
    rules.sort_unstable_by_key(|r| (r.0, std::cmp::Reverse(r.1)));
    rules.dedup_by(|a, b| a.0.saturating_sub(b.0) < 60);
    let rule_xs: Vec<Cpt> = rules.iter().map(|r| r.0).collect();
    let has_rules = found.len() >= 2 && rules_separate(&rule_xs, &found);
    let mut columns: Vec<Column> = Vec::with_capacity(found.len());
    if has_rules {
        let fallback = projected(lines, header_last_line, &found);
        for (i, (kind, _, p)) in found.iter().enumerate() {
            let center = mid(p.x0, p.x1);
            let size = p.words.first().map_or(800, |w| w.size);
            let x0 = rule_xs
                .iter()
                .copied()
                .filter(|x| *x <= center)
                .max()
                .unwrap_or_else(|| p.x0.saturating_sub(size));
            let x1 = rule_xs
                .iter()
                .copied()
                .filter(|x| *x > center)
                .min()
                .or_else(|| fallback.get(i).map(|c| c.x1))
                .unwrap_or(p.x1);
            columns.push(Column {
                kind: *kind,
                header: p.text(),
                x0,
                x1,
                ruled: true,
            });
        }
    } else {
        columns = projected(lines, header_last_line, &found);
    }
    // Colonnes sans intitulé reconnu entre deux traits : gardées comme
    // « autre » pour que leurs mots ne débordent pas sur une voisine.
    if has_rules {
        let left = columns.iter().map(|c| c.x0).min().unwrap_or(0);
        let right = columns.iter().map(|c| c.x1).max().unwrap_or(0);
        for pair in rule_xs.windows(2) {
            let (Some(a), Some(b)) = (pair.first().copied(), pair.get(1).copied()) else {
                continue;
            };
            if a >= left && b <= right && !columns.iter().any(|c| c.x0 < b && c.x1 > a) {
                let header = lines
                    .iter()
                    .take(header_last_line.saturating_add(1))
                    .filter(|l| l.baseline >= header_top)
                    .flat_map(|l| l.words_in(a, b))
                    .map(|w| w.text.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                columns.push(Column {
                    kind: ColumnKind::Other,
                    header,
                    x0: a,
                    x1: b,
                    ruled: true,
                });
            }
        }
        columns.sort_by_key(|c| c.x0);
    }
    let table_bottom = if has_rules {
        let right = columns.iter().map(|c| c.x1).max().unwrap_or(0);
        let left = columns.iter().map(|c| c.x0).min().unwrap_or(0);
        rules
            .iter()
            .filter(|(x, _)| *x >= left && *x <= right)
            .map(|(_, y1)| *y1)
            .max()
    } else {
        None
    };
    TableLayout {
        columns,
        header_last_line,
        header_bottom,
        table_bottom,
        language,
    }
}

/// Vrai si deux intitulés voisins sont toujours séparés par au moins un
/// trait (le bord gauche de la première colonne et le bord droit de la
/// dernière peuvent manquer : cadre de page, graphique).
fn rules_separate(xs: &[Cpt], found: &[Found]) -> bool {
    found.windows(2).all(|w| match (w.first(), w.get(1)) {
        (Some((_, _, a)), Some((_, _, b))) => {
            let (ca, cb) = (mid(a.x0, a.x1), mid(b.x0, b.x1));
            xs.iter().any(|x| *x > ca && *x <= cb)
        }
        _ => false,
    })
}

/// Bornes sans quadrillage : entre deux intitulés voisins A et B, la
/// borne est au milieu du plus large vide de la projection horizontale des
/// mots des lignes du tableau (sous l'en-tête) entre la fin de l'intitulé
/// de A et le milieu de celui de B (une durée alignée à droite déborde à
/// gauche de son intitulé) ; la dernière colonne s'arrête au premier vide de plus de
/// trois corps après son intitulé.
fn projected(lines: &[TextLine], header_last_line: usize, found: &[Found]) -> Vec<Column> {
    let body: Vec<&TextLine> = lines.iter().skip(header_last_line.saturating_add(1)).collect();
    let mut intervals: Vec<(Cpt, Cpt)> = body
        .iter()
        .flat_map(|l| l.words.iter().map(|w| (w.x0, w.x1)))
        .collect();
    intervals.sort_unstable();
    // Union des intervalles occupés.
    let mut occupied: Vec<(Cpt, Cpt)> = Vec::new();
    for (a, b) in intervals {
        match occupied.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => occupied.push((a, b)),
        }
    }
    let widest_gap = |from: Cpt, to: Cpt| -> Option<Cpt> {
        let mut best: Option<(Cpt, Cpt)> = None;
        for pair in occupied.windows(2) {
            let (Some(l), Some(r)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            let (g0, g1) = (l.1.max(from), r.0.min(to));
            if g1 > g0 && best.is_none_or(|(b0, b1)| g1.saturating_sub(g0) > b1.saturating_sub(b0)) {
                best = Some((g0, g1));
            }
        }
        best.map(|(a, b)| mid(a, b))
    };
    let mut columns = Vec::with_capacity(found.len());
    let mut left = found.first().map_or(0, |(_, _, p)| {
        p.x0.saturating_sub(p.words.first().map_or(0, |w| w.size))
    });
    for (i, (kind, _, p)) in found.iter().enumerate() {
        let right = if let Some((_, _, next)) = found.get(i.saturating_add(1)) {
            widest_gap(p.x1, mid(next.x0, next.x1)).unwrap_or_else(|| mid(p.x1, next.x0))
        } else {
            {
                let size = p.words.first().map_or(800, |w| w.size);
                let mut end = p.x1;
                for (a, b) in &occupied {
                    if *b <= end {
                        continue;
                    }
                    if a.saturating_sub(end) > size.saturating_mul(3) {
                        break;
                    }
                    end = end.max(*b);
                }
                end.saturating_add(size / 2)
            }
        };
        columns.push(Column {
            kind: *kind,
            header: p.text(),
            x0: left,
            x1: right,
            ruled: false,
        });
        left = right;
    }
    columns
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;
    use crate::lines::group_lines;

    fn w(text: &str, x0: Cpt, baseline: Cpt) -> Word {
        Word {
            text: text.to_owned(),
            x0,
            x1: x0.saturating_add(Cpt::try_from(text.chars().count()).unwrap().saturating_mul(450)),
            baseline,
            size: 800,
            bold: false,
            glyphs: Vec::new(),
        }
    }

    #[test]
    fn dictionary_covers_fr_and_en_headers() {
        for (text, kind) in [
            ("N°", ColumnKind::Id),
            ("ID", ColumnKind::Id),
            ("Nom de la tâche", ColumnKind::Name),
            ("Task Name", ColumnKind::Name),
            ("Durée", ColumnKind::Duration),
            ("Duration", ColumnKind::Duration),
            ("Début", ColumnKind::Start),
            ("Start", ColumnKind::Start),
            ("Fin", ColumnKind::Finish),
            ("Finish", ColumnKind::Finish),
            ("Prédécesseurs", ColumnKind::Predecessors),
            ("Predecessors", ColumnKind::Predecessors),
            ("Noms ressources", ColumnKind::ResourceNames),
            ("Resource Names", ColumnKind::ResourceNames),
            ("% achevé", ColumnKind::PercentComplete),
            ("% Complete", ColumnKind::PercentComplete),
        ] {
            assert_eq!(classify(&fold(text)).map(|c| c.0), Some(kind), "{text}");
        }
    }

    #[test]
    fn header_without_rules_uses_projection_gaps() {
        let words = vec![
            w("N°", 400, 1000),
            w("Nom", 2000, 1000),
            w("de", 3500, 1000),
            w("la", 4600, 1000),
            w("tâche", 5600, 1000),
            w("Durée", 20000, 1000),
            w("Début", 25000, 1000),
            w("1", 600, 2200),
            w("Terrassement", 2000, 2200),
            w("général", 7500, 2200),
            w("5", 19000, 2200),
            w("jours", 19600, 2200),
            w("Lun", 25000, 2200),
            w("02/01/23", 26800, 2200),
        ];
        let page = PageContent::default();
        let lines = group_lines(&words);
        let t = find_table(&page, &lines).unwrap();
        let kinds: Vec<ColumnKind> = t.columns.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [
                ColumnKind::Id,
                ColumnKind::Name,
                ColumnKind::Duration,
                ColumnKind::Start
            ]
        );
        assert_eq!(t.language, Some(HeaderLanguage::Fr));
        let name = t.column(ColumnKind::Name).unwrap();
        let duration = t.column(ColumnKind::Duration).unwrap();
        // « 5 » (19 000) appartient à la durée, malgré son intitulé à 20 000.
        assert!(name.x1 <= 19000 && duration.x0 <= 19000, "{name:?} {duration:?}");
        assert!(name.x1 >= 7500 + 7 * 450);
    }
}
