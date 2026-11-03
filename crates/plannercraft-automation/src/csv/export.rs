//! Export d'un CSV des tâches (R-146).

use plannercraft_core::{
    Decimal, Duration, Exclusion, Project, Schedule, Task, TaskOutcome, TaskRole, TimeBasis,
};
use plannercraft_mspdi::write::Newline;

use super::header::label;
use super::{CsvColumn, CsvLocale};
use crate::text::lex::format_decimal;
use crate::text::{
    Language, PredecessorRef, TextContext, format_date, format_duration, format_predecessor,
    format_predecessors,
};

/// Options d'export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvExportOptions {
    /// Paramètres régionaux : en-têtes, valeurs, séparateur (`;` en
    /// français, `,` en anglais).
    pub locale: CsvLocale,
    /// Colonnes, dans l'ordre.
    pub columns: Vec<CsvColumn>,
    /// BOM UTF-8 en tête (vrai par défaut : un tableur reconnaît l'UTF-8).
    pub bom: bool,
    /// Fin de ligne (CRLF par défaut, RFC 4180).
    pub newline: Newline,
}

impl CsvExportOptions {
    /// Options par défaut pour une locale : colonnes [`default_columns`],
    /// BOM, CRLF.
    #[must_use]
    pub fn new(locale: CsvLocale) -> Self {
        Self {
            locale,
            columns: default_columns(),
            bom: true,
            newline: Newline::Crlf,
        }
    }
}

/// Colonnes par défaut : N°, Nom, Durée, Début, Fin, Prédécesseurs, Niveau
/// hiérarchique, Jalon, Notes.
#[must_use]
pub fn default_columns() -> Vec<CsvColumn> {
    vec![
        CsvColumn::Id,
        CsvColumn::Name,
        CsvColumn::Duration,
        CsvColumn::Start,
        CsvColumn::Finish,
        CsvColumn::Predecessors,
        CsvColumn::OutlineLevel,
        CsvColumn::Milestone,
        CsvColumn::Notes,
    ]
}

/// R-146 : garde de formule : `'` ajouté devant un texte qui commence par
/// `=`, `+`, `-`, `@`, tabulation ou retour chariot, ou par `'` suivi de l'un
/// d'eux ou de `'` (réciproque exacte de l'import, R-145).
pub(crate) fn guard(s: &str) -> String {
    let risky = |c: Option<char>| matches!(c, Some('=' | '+' | '-' | '@' | '\t' | '\r'));
    let mut chars = s.chars();
    let first = chars.next();
    let needs = risky(first)
        || (first == Some('\'') && {
            let second = chars.next();
            risky(second) || second == Some('\'')
        });
    if needs { format!("'{s}") } else { s.to_owned() }
}

/// Champ entre guillemets s'il contient le séparateur, un guillemet, un
/// retour chariot ou un saut de ligne (RFC 4180), guillemets doublés.
fn quote(field: &str, separator: char) -> String {
    if field.contains([separator, '"', '\r', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// Durée affichée : celle du modèle pour une feuille ; pour une
/// récapitulative planifiée, celle du planning (R-054) dans l'unité de la
/// tâche.
fn duration_of(task: &Task, outcome: Option<&TaskOutcome>) -> Duration {
    if let Some(TaskOutcome::Scheduled(s)) = outcome
        && s.role == TaskRole::Summary
    {
        return Duration::new(s.duration, task.duration.unit(), TimeBasis::Working, false)
            .unwrap_or(task.duration);
    }
    task.duration
}

/// R-146 : exporte les tâches (lignes vides comprises, réduites à leur N°)
/// dans l'ordre des ID. UTF-8 (BOM facultatif), séparateur de la locale,
/// fin de ligne choisie. Durées par R-141 (récapitulative : durée du
/// planning) ; dates du planning par R-143 (tâche exclue : vide, sauf une
/// inactive : son placement d'affichage) ; prédécesseurs par R-142 (ID) ;
/// textes avec la garde de formule.
#[must_use]
pub fn export(project: &Project, schedule: &Schedule, options: &CsvExportOptions) -> Vec<u8> {
    let (language, separator) = match options.locale {
        CsvLocale::Fr => (Language::Fr, ';'),
        CsvLocale::En => (Language::En, ','),
    };
    let ctx = TextContext::for_project(project, language);
    let newline = match options.newline {
        Newline::Lf => "\n",
        Newline::Crlf => "\r\n",
    };
    let mut out = String::new();
    if options.bom {
        out.push('\u{feff}');
    }
    let header: Vec<String> = options
        .columns
        .iter()
        .map(|c| quote(label(*c, options.locale), separator))
        .collect();
    out.push_str(&header.join(&separator.to_string()));
    out.push_str(newline);
    for (position, task) in project.tasks.iter().enumerate() {
        let id = position.saturating_add(1);
        let outcome = schedule.get(task.uid());
        let fields: Vec<String> = options
            .columns
            .iter()
            .map(|c| {
                if task.is_null && *c != CsvColumn::Id {
                    return String::new();
                }
                quote(&cell(*c, id, task, outcome, project, &ctx), separator)
            })
            .collect();
        out.push_str(&fields.join(&separator.to_string()));
        out.push_str(newline);
    }
    out.into_bytes()
}

/// Valeur d'une cellule.
fn cell(
    column: CsvColumn,
    id: usize,
    task: &Task,
    outcome: Option<&TaskOutcome>,
    project: &Project,
    ctx: &TextContext,
) -> String {
    let span = || match outcome {
        Some(TaskOutcome::Scheduled(s)) => Some((s.start, s.finish)),
        Some(TaskOutcome::Excluded {
            reason: Exclusion::Inactive,
            display: Some(d),
        }) => Some((d.start, d.finish)),
        _ => None,
    };
    let yes_no = |b: bool| match (b, ctx.language) {
        (true, Language::Fr) => "Oui",
        (false, Language::Fr) => "Non",
        (true, Language::En) => "Yes",
        (false, Language::En) => "No",
    };
    match column {
        CsvColumn::Id => id.to_string(),
        CsvColumn::UniqueId => task.uid().get().to_string(),
        CsvColumn::Name => guard(&task.name),
        CsvColumn::Duration => format_duration(&duration_of(task, outcome), ctx),
        CsvColumn::Start => span().map(|(s, _)| format_date(s, ctx)).unwrap_or_default(),
        CsvColumn::Finish => span().map(|(_, f)| format_date(f, ctx)).unwrap_or_default(),
        CsvColumn::Predecessors => format_predecessors(task, project, ctx).0,
        CsvColumn::UniquePredecessors => task
            .predecessors
            .iter()
            .map(|l| {
                format_predecessor(
                    &PredecessorRef {
                        id: l.predecessor.get(),
                        kind: l.kind,
                        lag: l.lag,
                    },
                    ctx,
                )
            })
            .collect::<Vec<_>>()
            .join(&ctx.list_separator.to_string()),
        CsvColumn::OutlineLevel => task.outline_level.get().to_string(),
        CsvColumn::Milestone => yes_no(task.milestone).to_owned(),
        CsvColumn::Notes => guard(&task.notes),
        CsvColumn::Wbs => task.wbs.as_deref().map(guard).unwrap_or_default(),
        CsvColumn::PercentComplete => {
            let p = task.progress.percent_complete.get();
            let mut s = format_decimal(Decimal::new(p, 2).unwrap_or(Decimal::integer(0)), ctx.decimal);
            s.push('%');
            s
        }
    }
}
