//! Rapport d'import : texte lisible et vue JSON stable. Les dates
//! enregistrées par MS Project et les dates calculées par le moteur sont
//! citées sous ces noms (règle 3).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::{Comparison, MppDocument, NotImported};

/// Diagnostics listés au plus dans le texte.
pub const MAX_DIAGNOSTICS_SHOWN: usize = 20;
/// Écarts listés au plus dans le texte.
pub const MAX_GAPS_SHOWN: usize = 20;

/// Vue JSON du rapport.
#[derive(Clone, Debug, Serialize)]
pub struct View<'a> {
    /// Format (`mpp14`).
    pub format: &'static str,
    /// Disposition (`project2010`, `project2013`).
    pub layout: crate::Layout,
    /// Version de l'application qui a enregistré le fichier.
    pub saved_by: &'a str,
    /// Titre du projet.
    pub title: &'a str,
    /// Début du projet enregistré (`AAAA-MM-JJTHH:MM`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_start: Option<String>,
    /// Fin du projet enregistrée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_finish: Option<String>,
    /// Tâches (lignes vides comprises).
    pub tasks: usize,
    /// Lignes vides.
    pub null_tasks: usize,
    /// Liens.
    pub links: usize,
    /// Calendriers repris.
    pub calendars: usize,
    /// Exceptions de calendrier reprises.
    pub calendar_exceptions: usize,
    /// Ressources reprises (R-294).
    pub resources: usize,
    /// Affectations reprises (R-295).
    pub assignments: usize,
    /// Non repris.
    pub not_imported: NotImported,
    /// Diagnostics par code.
    pub diagnostics: BTreeMap<&'static str, u32>,
    /// Liste des diagnostics.
    pub diagnostic_list: &'a [crate::MppDiagnostic],
    /// Comparaison des dates enregistrées et calculées.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<&'a Comparison>,
}

/// Vue JSON.
#[must_use]
pub fn view<'a>(doc: &'a MppDocument, comparison: Option<&'a Comparison>) -> View<'a> {
    let p = &doc.project;
    let mut diagnostics: BTreeMap<&'static str, u32> = BTreeMap::new();
    for d in &doc.diagnostics {
        let code = d.core.unwrap_or_else(|| d.code.code());
        let n = diagnostics.entry(code).or_insert(0);
        *n = n.saturating_add(1);
    }
    View {
        format: doc.source.format.label(),
        layout: doc.source.layout,
        saved_by: &doc.source.saved_by,
        title: &p.info.title,
        project_start: doc.recorded.project_start.map(|m| m.to_string()),
        project_finish: doc.recorded.project_finish.map(|m| m.to_string()),
        tasks: p.tasks.len(),
        null_tasks: p.tasks.iter().filter(|t| t.is_null).count(),
        links: p.tasks.iter().map(|t| t.predecessors.len()).sum(),
        calendars: p.calendars.len(),
        calendar_exceptions: p.calendars.iter().map(|c| c.exceptions().len()).sum(),
        resources: p.resources.len(),
        assignments: p.assignments.len(),
        not_imported: doc.source.not_imported,
        diagnostics,
        diagnostic_list: &doc.diagnostics,
        comparison,
    }
}

/// Rapport lisible (`fr` : français, sinon anglais).
#[must_use]
pub fn text(doc: &MppDocument, comparison: Option<&Comparison>, fr: bool) -> String {
    let v = view(doc, comparison);
    let t = |a: &'static str, b: &'static str| if fr { a } else { b };
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} {} ({} {})",
        t("Fichier", "File"),
        v.format,
        t("enregistré par MS Project", "saved by MS Project"),
        v.saved_by
    );
    if !v.title.is_empty() {
        let _ = writeln!(s, "{} : {}", t("Titre", "Title"), v.title);
    }
    let _ = writeln!(
        s,
        "{} : {} ({} {}), {} : {}, {} : {} ({} {})",
        t("Tâches", "Tasks"),
        v.tasks,
        v.null_tasks,
        t("lignes vides", "blank rows"),
        t("liens", "links"),
        v.links,
        t("calendriers", "calendars"),
        v.calendars,
        v.calendar_exceptions,
        t("exceptions", "exceptions"),
    );
    let _ = writeln!(
        s,
        "{} : {}, {} : {}",
        t("Ressources", "Resources"),
        v.resources,
        t("affectations", "assignments"),
        v.assignments,
    );
    let ni = v.not_imported;
    if ni.resources > 0 || ni.assignments > 0 || ni.resource_calendars > 0 {
        let _ = writeln!(
            s,
            "{} : {} {}, {} {}, {} {}",
            t("Non repris", "Not imported"),
            ni.resources,
            t("ressources", "resources"),
            ni.assignments,
            t("affectations", "assignments"),
            ni.resource_calendars,
            t("calendriers de ressource", "resource calendars"),
        );
    }
    if let Some(c) = comparison {
        comparison_text(&mut s, c, fr);
    }
    if !v.diagnostics.is_empty() {
        let _ = writeln!(s, "{} :", t("Diagnostics", "Diagnostics"));
        for (code, n) in &v.diagnostics {
            let _ = writeln!(s, "  {code} × {n}");
        }
        for d in doc
            .diagnostics
            .iter()
            .filter(|d| d.core.is_none())
            .take(MAX_DIAGNOSTICS_SHOWN)
        {
            let _ = writeln!(
                s,
                "  {}{}{}",
                d.code.code(),
                d.uid.map(|u| format!(" UID {u}")).unwrap_or_default(),
                d.value.as_ref().map(|v| format!(" : {v}")).unwrap_or_default(),
            );
        }
    }
    s
}

/// Comparaison des dates enregistrées et calculées (R-291, R-298).
fn comparison_text(s: &mut String, c: &Comparison, fr: bool) {
    let t = |a: &'static str, b: &'static str| if fr { a } else { b };
    let _ = writeln!(
        s,
        "{} : {} {} {}, {} {}, {} {} ({} {}, {} {})",
        t(
            "Dates enregistrées par MS Project comparées aux dates calculées",
            "Dates saved by MS Project compared with computed dates"
        ),
        c.tasks_identical,
        t(
            "tâches identiques à la minute sur",
            "tasks identical to the minute out of"
        ),
        c.tasks_compared,
        c.tasks_rounding,
        t("à une minute près", "within one minute"),
        c.tasks_off,
        t("en écart réel", "really different"),
        c.starts_equal,
        t("débuts égaux", "equal starts"),
        c.finishes_equal,
        t("fins égales", "equal finishes"),
    );
    for (code, n) in &c.causes {
        let label = [
            crate::compare::GapCause::ContouredAssignment,
            crate::compare::GapCause::StaggeredAssignments,
            crate::compare::GapCause::UnexplainedStart,
        ]
        .into_iter()
        .find(|g| g.code() == *code)
        .map_or("", |g| g.label(fr));
        let _ = writeln!(
            s,
            "  {} : {code} × {n} ({label})",
            t("non recalculable", "not recomputable"),
        );
    }
    for g in c
        .gaps
        .iter()
        .filter(|g| g.class == crate::compare::GapClass::Real)
        .take(MAX_GAPS_SHOWN)
    {
        let _ = writeln!(
            s,
            "  {} {} {} : {} {}, {} {} ({}{} min)",
            t("ID", "ID"),
            g.id,
            g.field,
            t("enregistrée", "saved"),
            g.recorded,
            t("calculée", "computed"),
            g.computed,
            if g.minutes.is_negative() { "" } else { "+" },
            plannercraft_core::number::to_text(g.minutes.tenths(), 1)
        );
    }
}
