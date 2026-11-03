//! Import des affectations (R-088, R-125) : tâche, ressource, unités,
//! travail, retard, retard de nivellement (R-198), profil, lignes de base ;
//! dates et travail enregistrés dans `RecordedSchedule` (R-198) ; le reste
//! en `Extras` (R-087),
//! sauf la répartition calculée (`TimephasedData` de type 1, travail
//! restant planifié) quand le profil n'est pas modifié à la main.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Arena, Assignment, AssignmentUid, Extras, Minutes, ResourceUid, Subject, TaskUid, Work, WorkContour,
};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::baseline;
use crate::codes;
use crate::diag::{ImportCode, Sink};
use crate::extras;
use crate::fields::Fields;
use crate::project::{code, guid};
use crate::recorded::RecordedAssignment;
use crate::resource::units;
use crate::scalar::tenths_to_minutes;
use crate::schema::Parent;
use crate::task::{Replaced, leveling_delay};
use crate::xml::Element;

/// Chemin d'une affectation.
const ASSIGNMENT: &str = "Assignments/Assignment";

/// Type de `TimephasedData` calculé par MS Project : travail restant
/// planifié de l'affectation (S37).
const TIMEPHASED_SCHEDULED_WORK: i64 = 1;

/// Format par défaut du retard de nivellement d'une affectation, si
/// `LevelingDelayFormat` manque ou ne convient pas : jours ouvrés (7),
/// format le plus fréquent du corpus (hypothèse, R-198).
pub(crate) const ASSIGNMENT_DELAY_ELAPSED: bool = false;

/// Importe les affectations ; `tasks` : UID des tâches lues (UID 0
/// compris) ; valeurs enregistrées dans `recorded` (R-198). Rend aussi le
/// nombre de répartitions calculées écartées.
pub(crate) fn import(
    elements: &[Element<'_>],
    tasks: &BTreeSet<TaskUid>,
    recorded: &mut BTreeMap<AssignmentUid, RecordedAssignment>,
    replaced: &mut Replaced,
    sink: &mut Sink<'_>,
) -> (Arena<AssignmentUid, Assignment>, u64) {
    let mut out = Arena::new();
    let mut seen: BTreeSet<AssignmentUid> = BTreeSet::new();
    let mut dropped: u64 = 0;
    for element in elements {
        let mut f = Fields::new(&element.children, ASSIGNMENT, None, element.at);
        let Some(uid) = assignment_uid(&f, sink) else {
            continue;
        };
        if !seen.insert(uid) {
            f.report(
                ImportCode::Duplicate,
                "UID",
                element.at,
                Some(&uid.get().to_string()),
                sink,
            );
            continue;
        }
        f.set_subject(Some(Subject::Assignment(uid)));
        let Some(task) = task_uid(&f, tasks, sink) else {
            continue;
        };
        let mut a = Assignment::new(uid, task, resource_uid(&f, sink));
        a.guid = guid(&f, "GUID", sink);
        a.units = units(&f, "Units", sink).unwrap_or(plannercraft_core::Units::FULL);
        let work = baseline::work(&f, "Work", sink);
        a.work = work.unwrap_or(Work::ZERO);
        a.actual_work = baseline::work(&f, "ActualWork", sink).unwrap_or(Work::ZERO);
        // R-214 : travail restant relu remplacé par la dérivation (R-211).
        let read_rest = baseline::work(&f, "RemainingWork", sink);
        a.remaining_work = plannercraft_core::progress::remaining_work(&a);
        replaced.note(read_rest, &a.remaining_work, f.at(), &f.path_of("RemainingWork"));
        a.delay = delay(&f, sink);
        a.leveling_delay = leveling_delay(&f, ASSIGNMENT_DELAY_ELAPSED, sink);
        recorded.insert(
            uid,
            RecordedAssignment {
                start: f.datetime("Start", sink),
                finish: f.datetime("Finish", sink),
                work,
                actual_start: f.datetime("ActualStart", sink),
                actual_finish: f.datetime("ActualFinish", sink),
                stop: f.datetime("Stop", sink),
                resume: f.datetime("Resume", sink),
                start_variance: f.int("StartVariance", sink).map(|(v, _)| v),
                finish_variance: f.int("FinishVariance", sink).map(|(v, _)| v),
                work_variance: crate::task::variance_decimal(&f, "WorkVariance", sink),
            },
        );
        a.contour = code(&f, "WorkContour", codes::work_contour, sink).unwrap_or(WorkContour::Flat);
        let baselines = baseline::assignment(&element.children, f.subject(), sink);
        a.baselines = baselines.map;
        let mut kept = extras::collect(&element.children, Parent::Assignment, None);
        if a.contour != WorkContour::Contoured {
            let before = kept.len();
            kept.retain(|e| {
                !(e.key == "TimephasedData" && timephased_type(&e.payload) == Some(TIMEPHASED_SCHEDULED_WORK))
            });
            dropped =
                dropped.saturating_add(u64::try_from(before.saturating_sub(kept.len())).unwrap_or(u64::MAX));
        }
        kept.extend(baselines.extras);
        a.extras = Extras(kept);
        extras::canonical_order(&mut a.extras, Parent::Assignment);
        // UID uniques (filtrés plus haut) : l'ajout ne peut pas échouer.
        let _ = out.push(a);
    }
    (out, dropped)
}

/// Compte, par tâche, les affectations lues.
pub(crate) fn count_by_task(assignments: &Arena<AssignmentUid, Assignment>) -> BTreeMap<TaskUid, u32> {
    let mut counts: BTreeMap<TaskUid, u32> = BTreeMap::new();
    for a in assignments {
        let n = counts.entry(a.task).or_insert(0);
        *n = n.saturating_add(1);
    }
    counts
}

/// UID d'une affectation ; absent, négatif ou illisible : ignorée.
fn assignment_uid(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<AssignmentUid> {
    let Some((v, at)) = f.int("UID", sink) else {
        if f.scalar("UID", sink).is_none() {
            f.missing("UID", sink);
        }
        return None;
    };
    let uid = u32::try_from(v).ok().map(AssignmentUid::new);
    if uid.is_none() {
        f.invalid("UID", at, &v.to_string(), sink);
    }
    uid
}

/// Tâche affectée ; absente ou inconnue : affectation ignorée,
/// `mspdi.dangling_reference`.
fn task_uid(f: &Fields<'_>, tasks: &BTreeSet<TaskUid>, sink: &mut Sink<'_>) -> Option<TaskUid> {
    let Some((v, at)) = f.int("TaskUID", sink) else {
        if f.scalar("TaskUID", sink).is_none() {
            f.missing("TaskUID", sink);
        }
        return None;
    };
    let uid = u32::try_from(v)
        .ok()
        .map(TaskUid::new)
        .filter(|u| tasks.contains(u));
    if uid.is_none() {
        f.report(
            ImportCode::DanglingReference,
            "TaskUID",
            at,
            Some(&v.to_string()),
            sink,
        );
    }
    uid
}

/// R-125 : ressource ; négative (−65 535 : aucune ressource) ou absente →
/// `None` ; une ressource inconnue est gardée (rapportée par
/// `Project::validate`).
fn resource_uid(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<ResourceUid> {
    let (v, at) = f.int("ResourceUID", sink)?;
    if v < 0 {
        return None;
    }
    let uid = u32::try_from(v).ok().map(ResourceUid::new);
    if uid.is_none() {
        f.invalid("ResourceUID", at, &v.to_string(), sink);
    }
    uid
}

/// R-125 : `Delay` en dixièmes de minute (comme `LinkLag`, O20) →
/// minutes, `mspdi.rounded` s'il y a un reste ; négatif → 0, signalé.
fn delay(f: &Fields<'_>, sink: &mut Sink<'_>) -> Minutes {
    let Some((tenths, at)) = f.int("Delay", sink) else {
        return Minutes::ZERO;
    };
    let (minutes, rounded) = tenths_to_minutes(tenths);
    if minutes.is_negative() {
        f.invalid("Delay", at, &tenths.to_string(), sink);
        return Minutes::ZERO;
    }
    if rounded {
        f.report(ImportCode::Rounded, "Delay", at, Some(&tenths.to_string()), sink);
    }
    minutes
}

/// `Type` (enfant direct) d'une `TimephasedData` capturée.
fn timephased_type(payload: &str) -> Option<i64> {
    let mut reader = Reader::from_str(payload);
    let mut depth: u32 = 0;
    let mut in_type = false;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                depth = depth.saturating_add(1);
                in_type = depth == 2 && e.local_name().as_ref() == "Type";
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                in_type = false;
            }
            Event::Text(t) if in_type => {
                return t.xml10_content().trim().parse().ok();
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::timephased_type;

    #[test]
    fn r087_timephased_type() {
        assert_eq!(
            timephased_type("<TimephasedData><Type>1</Type><UID>3</UID></TimephasedData>"),
            Some(1)
        );
        assert_eq!(
            timephased_type("<p:TimephasedData><p:UID>3</p:UID><p:Type> 2 </p:Type></p:TimephasedData>"),
            Some(2)
        );
        assert_eq!(
            timephased_type("<TimephasedData><X><Type>1</Type></X></TimephasedData>"),
            None
        );
        assert_eq!(timephased_type("<TimephasedData/>"), None);
        assert_eq!(timephased_type("garbage"), None);
    }
}
