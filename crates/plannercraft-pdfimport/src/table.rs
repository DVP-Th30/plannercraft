//! Lignes du tableau (étape 4) : une ligne par tâche, cellules par
//! colonne, noms repliés sur plusieurs lignes rattachés, retrait du nom.
//!
//! Une tâche commence à une ligne de texte qui porte un N° (s'il y a une
//! colonne N°), sinon à une ligne qui porte un nom et une durée ou une
//! date. Les lignes suivantes sans N°, proches (écart ≤ 1,7 corps), la
//! complètent : MS Project aligne les cellules d'une ligne haute en haut
//! (observé sur le corpus), la suite d'un nom replié est donc en dessous.

use std::collections::BTreeMap;

use crate::extract::{Cpt, Word};
use crate::header::{ColumnKind, TableLayout};
use crate::lines::{TextLine, fold, join_words};

/// Ligne brute du tableau.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawRow {
    /// Page (0 = première).
    pub page: usize,
    /// Ligne de base de la première ligne de texte.
    pub baseline: Cpt,
    /// Texte des cellules, par colonne reconnue.
    pub cells: BTreeMap<ColumnKind, String>,
    /// Retrait du nom : bord gauche du premier mot du nom moins le bord
    /// gauche de la colonne.
    pub name_indent: Option<Cpt>,
    /// Corps du nom.
    pub name_size: Cpt,
    /// Nom en gras.
    pub bold: bool,
    /// Symbole de plan (triangle, case « + » / « − ») devant le nom.
    pub outline_symbol: bool,
    /// Lignes de texte de la tâche.
    pub text_lines: u32,
}

impl RawRow {
    /// Cellule d'une colonne, `None` si absente ou vide.
    #[must_use]
    pub fn cell(&self, kind: ColumnKind) -> Option<&str> {
        self.cells
            .get(&kind)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
    }
}

/// Lignes d'une page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageRows {
    /// Lignes de tâches.
    pub rows: Vec<RawRow>,
    /// Lignes de texte de la zone du tableau rattachées à aucune tâche.
    pub stray_lines: u32,
}

/// Caractère de symbole de plan (formes géométriques, flèches, signes
/// encadrés) : écarté du nom, compté comme indice de récapitulative.
fn is_outline_symbol(word: &Word) -> bool {
    let mut chars = word.text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return false;
    };
    matches!(u32::from(c), 0x25A0..=0x25FF | 0x2190..=0x21FF | 0x2295..=0x22A1 | 0xF000..=0xF0FF)
}

/// Premier mot d'une ligne de légende ou de pied de page MS Project :
/// fin du tableau de la page.
fn is_legend(line: &TextLine) -> bool {
    let Some(first) = line.words.first() else {
        return false;
    };
    let f = fold(&first.text);
    matches!(
        f.as_str(),
        "project:" | "projet:" | "projet" | "project" | "date:"
    )
}

/// Vrai si `text` est un N° de tâche (1 à 7 chiffres).
#[must_use]
pub fn parse_id(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.is_empty() || t.len() > 7 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

/// Façon de joindre les morceaux d'une cellule repliée.
fn joiner(kind: ColumnKind) -> &'static str {
    match kind {
        // « 12;13; » puis « 14 » : rien entre les morceaux.
        ColumnKind::Predecessors | ColumnKind::Successors => "",
        _ => " ",
    }
}

/// Mots d'une ligne coupés aux bords des colonnes : deux cellules
/// voisines dessinées sans espace (« 12/19/19Wed ») forment un seul mot à
/// l'extraction ; il est coupé au premier glyphe de la colonne suivante.
fn split_words(line: &TextLine, layout: &TableLayout) -> TextLine {
    let edges: Vec<Cpt> = layout.columns.iter().map(|c| c.x1).collect();
    let mut words = Vec::with_capacity(line.words.len());
    for w in &line.words {
        let mut rest = w.clone();
        for edge in &edges {
            if rest.x0.saturating_add(50) < *edge
                && rest.x1.saturating_sub(50) > *edge
                && let Some((left, right)) = rest.split_at_x(edge.saturating_sub(30))
            {
                words.push(left);
                rest = right;
            }
        }
        words.push(rest);
    }
    TextLine {
        words,
        ..line.clone()
    }
}

/// Cellules d'une ligne de texte, par colonne.
fn cells_of(line: &TextLine, layout: &TableLayout) -> BTreeMap<ColumnKind, (String, Vec<Word>)> {
    let mut out: BTreeMap<ColumnKind, (String, Vec<Word>)> = BTreeMap::new();
    let line = &split_words(line, layout);
    for column in &layout.columns {
        let words: Vec<Word> = line.words_in(column.x0, column.x1).cloned().collect();
        if words.is_empty() || column.kind == ColumnKind::Other && out.contains_key(&ColumnKind::Other) {
            continue;
        }
        let text = join_words(
            words
                .iter()
                .filter(|w| !(column.kind == ColumnKind::Name && is_outline_symbol(w))),
        );
        out.insert(column.kind, (text, words));
    }
    out
}

/// Lit les lignes du tableau d'une page.
#[must_use]
pub fn read_rows(page: usize, lines: &[TextLine], layout: &TableLayout) -> PageRows {
    let mut out = PageRows::default();
    let has_id = layout.column(ColumnKind::Id).is_some();
    let name_x0 = layout.column(ColumnKind::Name).map(|c| c.x0);
    let left = layout.left();
    let right = layout.right();
    let mut previous: Option<&TextLine> = None;
    for line in lines.iter().skip(layout.header_last_line.saturating_add(1)) {
        if line.baseline <= layout.header_bottom {
            continue;
        }
        if layout.table_bottom.is_some_and(|b| line.top() >= b) {
            break;
        }
        if line.words_in(left, right).next().is_none() {
            // Ligne du seul graphique (texte près des barres).
            continue;
        }
        if is_legend(line) {
            break;
        }
        let cells = cells_of(line, layout);
        let id_text = cells.get(&ColumnKind::Id).map(|(t, _)| t.as_str());
        let starts = if has_id {
            id_text.and_then(parse_id).is_some()
        } else {
            cells.contains_key(&ColumnKind::Name)
                && [ColumnKind::Duration, ColumnKind::Start, ColumnKind::Finish]
                    .iter()
                    .any(|k| cells.contains_key(k))
        };
        if starts {
            let name_words = cells
                .get(&ColumnKind::Name)
                .map(|(_, w)| w.as_slice())
                .unwrap_or_default();
            let outline_symbol = name_words.first().is_some_and(is_outline_symbol);
            let first_text = name_words.iter().find(|w| !is_outline_symbol(w));
            out.rows.push(RawRow {
                page,
                baseline: line.baseline,
                cells: cells.iter().map(|(k, (t, _))| (*k, t.clone())).collect(),
                name_indent: first_text.zip(name_x0).map(|(w, x0)| w.x0.saturating_sub(x0)),
                name_size: first_text.map_or(line.size, |w| w.size),
                bold: first_text.is_some_and(|w| w.bold),
                outline_symbol,
                text_lines: 1,
            });
            previous = Some(line);
            continue;
        }
        let close = previous.is_some_and(|p| {
            line.baseline.saturating_sub(p.bottom) <= p.size.max(line.size).saturating_mul(17) / 10
        });
        // Une ligne qui porte un texte dans la colonne N° sans être un N°
        // n'est pas une suite de nom.
        let foreign_id = has_id && id_text.is_some();
        match out.rows.last_mut() {
            Some(row) if close && !foreign_id => {
                for (kind, (text, _)) in cells {
                    let cell = row.cells.entry(kind).or_default();
                    if !cell.is_empty() && !text.is_empty() {
                        cell.push_str(joiner(kind));
                    }
                    cell.push_str(&text);
                }
                row.text_lines = row.text_lines.saturating_add(1);
                previous = Some(line);
            }
            _ => {
                out.stray_lines = out.stray_lines.saturating_add(1);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::*;
    use crate::extract::PageContent;
    use crate::header::find_table;
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
    fn wrapped_name_and_predecessors_join_the_row_above() {
        let words = vec![
            w("ID", 400, 1000),
            w("Task", 2000, 1000),
            w("Name", 4000, 1000),
            w("Duration", 20000, 1000),
            w("Predecessors", 26000, 1000),
            w("1", 600, 2200),
            w("Preliminary", 2000, 2200),
            w("Design", 7200, 2200),
            w("5", 20500, 2200),
            w("days", 21200, 2200),
            w("12;13;", 26000, 2200),
            w("Program", 2000, 3200),
            w("14", 26000, 3200),
            w("2", 600, 4400),
            w("Next", 2900, 4400),
            w("1", 20500, 4400),
            w("day", 21200, 4400),
            // Pied de page : ignoré.
            w("Project:", 400, 9000),
            w("Plan", 4000, 9000),
        ];
        let lines = group_lines(&words);
        let layout = find_table(&PageContent::default(), &lines).unwrap();
        let rows = read_rows(0, &lines, &layout);
        assert_eq!(rows.rows.len(), 2);
        let first = &rows.rows[0];
        assert_eq!(first.cell(ColumnKind::Name), Some("Preliminary Design Program"));
        assert_eq!(first.cell(ColumnKind::Predecessors), Some("12;13;14"));
        assert_eq!(first.text_lines, 2);
        let second = &rows.rows[1];
        assert_eq!(second.cell(ColumnKind::Id), Some("2"));
        assert_eq!(second.name_indent.unwrap() - first.name_indent.unwrap(), 900);
        assert_eq!(rows.stray_lines, 0);
    }
}
