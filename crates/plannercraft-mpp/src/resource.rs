//! Ressources et affectations (`TBkndRsc`, `TBkndAssn`, R-294, R-295).
//!
//! Observé (`docs/formats/mpp.md` § 14, § 15) : un enregistrement fixe par
//! ressource (UID, unités maximales, calendrier, nature) et par affectation
//! (tâche, ressource, unités, travail, travail réel, retard, retard de
//! nivellement) ; la ressource coût porte un bit de `Fixed2Meta`, le profil
//! modifié à la main un bit de `FixedMeta`, le code des autres profils un
//! octet de la répartition (valeur variable). Les dates enregistrées des
//! affectations ne vont pas dans le projet (règle 3).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Assignment, AssignmentUid, CalendarUid, Minutes, Resource, ResourceKind, ResourceUid, TaskUid, Units,
    Work, WorkContour,
};
use plannercraft_mspdi::codes;

use crate::RecordedAssignment;
use crate::bytes::{le_f64_round, le_i32, le_u8, le_u32, utf16_z};
use crate::diag::{MppCode, Sink};
use crate::layout::{self, RESOURCE_COST, assignment_var, resource_var};
use crate::tables::{FixedItem, VarValues};
use crate::time;

/// UID d'affectation sans ressource (ressource « non affectée »).
const NO_RESOURCE: i32 = -65_535;

/// Ressources lues, dans l'ordre des enregistrements.
#[must_use]
pub fn read_resources(
    items: &[FixedItem<'_>],
    items2: &[FixedItem<'_>],
    var: &VarValues<'_>,
    lay: crate::Layout,
    calendars: &BTreeSet<u32>,
    sink: &mut Sink,
) -> Vec<Resource> {
    let rl = layout::resource(lay);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (pos, item) in items.iter().enumerate() {
        if item.deleted() || item.short() {
            continue;
        }
        let Some(rec) = item.data.get(..rl.len) else {
            continue;
        };
        let Some(uid) = le_u32(rec, rl.uid) else {
            continue;
        };
        // Ressource 0 : réservée par MS Project (ressource vide du projet).
        if uid == 0 || uid >= 0xFFFF_0000 {
            continue;
        }
        if !seen.insert(uid) {
            sink.push(MppCode::DuplicateUid, Some(uid), Some("ressource".to_owned()));
            continue;
        }
        let name = var
            .get(&uid)
            .and_then(|f| f.get(&resource_var::NAME))
            .map(|b| utf16_z(b, 255))
            .unwrap_or_default();
        let mut r = Resource::new(ResourceUid::new(uid), name);
        let cost = items2
            .get(pos)
            .and_then(|i2| le_u8(i2.meta, RESOURCE_COST.0))
            .is_some_and(|b| b & RESOURCE_COST.1 != 0);
        r.kind = match (crate::bytes::le_u16(rec, rl.kind), cost) {
            (_, true) => ResourceKind::Cost,
            (Some(k), false) if k & 1 == 0 => ResourceKind::Work,
            (Some(_) | None, false) => ResourceKind::Material { label: String::new() },
        };
        match le_f64_round(rec, rl.max_units, 1).map(Units::new) {
            Some(Ok(u)) => r.max_units = u,
            _ => sink.push(
                MppCode::ResourceValueInvalid,
                Some(uid),
                Some("unités maximales".to_owned()),
            ),
        }
        match le_i32(rec, rl.calendar) {
            Some(c) if c > 0 && calendars.contains(&c.unsigned_abs()) => {
                r.calendar = Some(CalendarUid::new(c.unsigned_abs()));
            }
            Some(c) if c > 0 => sink.push(MppCode::ResourceCalendarUnknown, Some(uid), Some(c.to_string())),
            _ => {}
        }
        out.push(r);
    }
    out
}

/// Affectations lues, avec leurs dates enregistrées (jamais dans le
/// projet, règle 3) ; `tasks` : UID des tâches importées ; `resources` :
/// UID des ressources importées.
#[must_use]
pub fn read_assignments(
    items: &[FixedItem<'_>],
    var: &VarValues<'_>,
    lay: crate::Layout,
    tasks: &BTreeSet<TaskUid>,
    resources: &BTreeMap<ResourceUid, ResourceKind>,
    sink: &mut Sink,
) -> (Vec<Assignment>, BTreeMap<AssignmentUid, RecordedAssignment>) {
    let al = layout::assignment(lay);
    let mut out = Vec::new();
    let mut recorded = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for item in items {
        if item.deleted() || item.short() {
            continue;
        }
        let Some(rec) = item.data.get(..al.len) else {
            continue;
        };
        let (Some(uid), Some(task), Some(res)) = (
            le_u32(rec, al.uid),
            le_u32(rec, al.task),
            le_i32(rec, al.resource),
        ) else {
            continue;
        };
        if uid >= 0xFFFF_0000 {
            continue;
        }
        if !seen.insert(uid) {
            sink.push(MppCode::DuplicateUid, Some(uid), Some("affectation".to_owned()));
            continue;
        }
        let task = TaskUid::new(task);
        if !tasks.contains(&task) {
            sink.push(
                MppCode::AssignmentIgnored,
                Some(uid),
                Some(format!("tâche {}", task.get())),
            );
            continue;
        }
        let resource = match res {
            NO_RESOURCE => None,
            r if r > 0 && resources.contains_key(&ResourceUid::new(r.unsigned_abs())) => {
                Some(ResourceUid::new(r.unsigned_abs()))
            }
            r => {
                sink.push(
                    MppCode::AssignmentIgnored,
                    Some(uid),
                    Some(format!("ressource {r}")),
                );
                None
            }
        };
        let mut a = Assignment::new(AssignmentUid::new(uid), task, resource);
        values(&mut a, rec, item.meta, var, &al, sink);
        let date = |off: usize| match le_u32(rec, off).map(time::stamp) {
            Some(time::Stamp::At(m)) => Some(m),
            _ => None,
        };
        recorded.insert(
            a.uid(),
            RecordedAssignment {
                start: date(al.start),
                finish: date(al.finish),
            },
        );
        out.push(a);
    }
    if al.delay.is_none() && !out.is_empty() {
        sink.push(MppCode::AssignmentDelayNotRead, None, None);
    }
    (out, recorded)
}

/// Unités, travail, travail réel, retards et profil d'une affectation.
fn values(
    a: &mut Assignment,
    rec: &[u8],
    meta: &[u8],
    var: &VarValues<'_>,
    al: &layout::AssignmentLayout,
    sink: &mut Sink,
) {
    match le_f64_round(rec, al.units, 1).map(Units::new) {
        Some(Ok(u)) => a.units = u,
        _ => sink.push(
            MppCode::ResourceValueInvalid,
            Some(a.uid().get()),
            Some("unités".to_owned()),
        ),
    }
    // Travail en millièmes de minute, l'échelle du fichier (ADR-0039).
    let work = |off: usize| le_f64_round(rec, off, 1).and_then(|m| Work::from_milli(m).ok());
    match work(al.work) {
        Some(w) => a.work = w,
        None => sink.push(
            MppCode::ResourceValueInvalid,
            Some(a.uid().get()),
            Some("travail".to_owned()),
        ),
    }
    a.actual_work = work(al.actual_work).unwrap_or(Work::ZERO);
    a.remaining_work = plannercraft_core::progress::remaining_work(a);
    if let Some(off) = al.delay
        && let Some(t) = le_i32(rec, off)
    {
        a.delay = time::minutes_from_tenths(i64::from(t))
            .filter(|m| !m.is_negative())
            .unwrap_or(Minutes::ZERO);
    }
    if let Some(off) = al.leveling_delay
        && let Some(t) = le_i32(rec, off).filter(|t| *t > 0)
    {
        a.leveling_delay = time::minutes_from_tenths(i64::from(t))
            .filter(|m| *m > Minutes::ZERO)
            .and_then(|m| {
                plannercraft_core::LevelingDelay::new(m, plannercraft_core::DurationUnit::Day, false)
            });
    }
    let hand = le_u8(meta, al.contoured.0).is_some_and(|b| b & al.contoured.1 != 0);
    a.contour = if hand {
        sink.push(
            MppCode::AssignmentContoured,
            Some(a.uid().get()),
            Some(format!("tâche {}", a.task.get())),
        );
        WorkContour::Contoured
    } else {
        var.get(&a.uid().get())
            .and_then(|f| f.get(&assignment_var::DISTRIBUTION))
            .and_then(|d| le_u8(d, assignment_var::CONTOUR_AT))
            .and_then(|c| codes::work_contour(i64::from(c)))
            .unwrap_or(WorkContour::Flat)
    };
}
