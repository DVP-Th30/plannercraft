//! Affectations à l'écriture (R-125, R-199).

use plannercraft_core::{Assignment, Subject, Work};

use super::computed::Values;
use super::emit::Emitter;
use super::resource::units_text;
use super::{OrderProfile, WriteCode, baseline, entity, leveling, plan, timephased, unscoped};
use crate::codes;
use crate::scalar::format_iso_duration;
use crate::schema::Parent;

/// `ResourceUID` d'une affectation sans ressource (R-125, R-079).
const NO_RESOURCE: &str = "-65535";

/// Travail restant écrit d'une affectation (R-199, R-211, R-214) : la
/// dérivation W − travail réel, jamais négative (ADR-0014 D4) ; sans
/// travail réel, le travail, comme MS Project l'écrit toujours.
pub(crate) fn remaining_work(a: &Assignment) -> Work {
    plannercraft_core::progress::remaining_work(a)
}

/// Écrit une affectation ; `values` : `Start`/`Finish` à écrire (R-198,
/// R-199, R-122), absents sinon ; `TimephasedData` propres jetées si le
/// placement du moteur a changé (R-199, `mspdi.write.timephased_dropped`).
pub(crate) fn write(em: &mut Emitter, a: &Assignment, values: Option<&Values>, profile: OrderProfile) {
    let computed = values.and_then(|v| v.assignments.get(&a.uid()));
    em.subject = Some(Subject::Assignment(a.uid()));
    let numbers: Vec<u8> = a.baselines.keys().map(|n| n.get()).collect();
    let planned = plan(em, "Assignment", Parent::Assignment, &a.extras, &numbers, profile);
    let mut top = unscoped(&planned);
    leveling::drop_legacy(&mut top, a.leveling_delay);
    let drop_scheduled = computed.is_some_and(|v| v.drop_timephased);
    let drop_actuals = computed.is_some_and(|v| v.drop_actuals);
    if drop_scheduled || drop_actuals {
        let before = top.len();
        top.retain(|p| {
            !((drop_scheduled && timephased::is_scheduled_work_key(&p.key, p.payload))
                || (drop_actuals && timephased::is_actual_key(&p.key, p.payload)))
        });
        let count = u32::try_from(before.saturating_sub(top.len())).unwrap_or(u32::MAX);
        if count > 0 {
            em.report(
                WriteCode::TimephasedDropped { count },
                "Assignment/TimephasedData",
                None,
            );
        }
    }
    entity(
        em,
        "Assignment",
        Parent::Assignment,
        profile,
        &top,
        |em, name| match name {
            "UID" => em.leaf(name, &a.uid().get().to_string()),
            "GUID" => em.opt_leaf(name, a.guid.map(|g| g.to_string())),
            "TaskUID" => em.leaf(name, &a.task.get().to_string()),
            "ResourceUID" => em.leaf(
                name,
                &a.resource
                    .map_or_else(|| NO_RESOURCE.to_owned(), |r| r.get().to_string()),
            ),
            "ActualWork" => em.leaf(name, &format_iso_duration(a.actual_work.minutes().get())),
            // R-125 : retard en dixièmes de minute (O20).
            "Delay" => em.leaf(name, &a.delay.get().saturating_mul(10).to_string()),
            "RemainingWork" => em.leaf(name, &format_iso_duration(remaining_work(a).minutes().get())),
            "Units" => em.leaf(name, &units_text(a.units)),
            "Work" => em.leaf(name, &format_iso_duration(a.work.minutes().get())),
            "WorkContour" => em.leaf(name, &codes::work_contour_code(a.contour).to_string()),
            "Baseline" => baseline::assignment(em, &a.baselines, &planned, profile),
            "Start" => em.opt_date(name, computed.and_then(|v| v.start)),
            // R-215 : réels et coupure de l'affectation, calculés.
            "ActualStart" => em.opt_date(name, computed.and_then(|v| v.actual_start)),
            "ActualFinish" => em.opt_date(name, computed.and_then(|v| v.actual_finish)),
            "Stop" => em.opt_date(name, computed.and_then(|v| v.stop)),
            "Resume" => em.opt_date(name, computed.and_then(|v| v.resume)),
            // R-226 : écarts ; dates relues seulement (unité non documentée).
            "StartVariance" => em.opt_int(name, computed.and_then(|v| v.start_variance)),
            "FinishVariance" => em.opt_int(name, computed.and_then(|v| v.finish_variance)),
            "WorkVariance" => em.opt_leaf(
                name,
                computed
                    .and_then(|v| v.work_variance)
                    .map(super::computed::variance_text),
            ),
            "Finish" => em.opt_date(name, computed.and_then(|v| v.finish)),
            "LevelingDelay" | "LevelingDelayFormat" => leveling::write(em, name, a.leveling_delay),
            _ => {}
        },
    );
    em.subject = None;
}
