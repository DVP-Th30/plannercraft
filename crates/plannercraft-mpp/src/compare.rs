//! Dates enregistrées par MS Project contre dates recalculées par le moteur
//! (R-291). Rien n'est corrigé : l'écart est une information.

use plannercraft_core::{Project, Schedule, TaskOutcome};
use serde::Serialize;

use crate::Recorded;

/// Écarts listés au plus.
pub const MAX_GAPS: usize = 200;

/// Date dont le calcul diffère de l'enregistrement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DateGap {
    /// UID de la tâche.
    pub uid: u32,
    /// ID (position).
    pub id: u32,
    /// `start` ou `finish`.
    pub field: &'static str,
    /// Date enregistrée (`AAAA-MM-JJTHH:MM`).
    pub recorded: String,
    /// Date calculée par le moteur.
    pub computed: String,
    /// Écart en minutes (calculé − enregistré).
    pub minutes: i64,
}

/// Bilan de la comparaison.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Comparison {
    /// Tâches comparées (non vides, planifiées, avec début et fin
    /// enregistrés).
    pub tasks_compared: u32,
    /// Tâches dont le début et la fin calculés égalent, à la minute, ceux
    /// enregistrés.
    pub tasks_identical: u32,
    /// Débuts égaux.
    pub starts_equal: u32,
    /// Fins égales.
    pub finishes_equal: u32,
    /// Premiers écarts, dans l'ordre des tâches.
    pub gaps: Vec<DateGap>,
}

impl Comparison {
    /// Tâches dont une date diffère.
    #[must_use]
    pub const fn tasks_different(&self) -> u32 {
        self.tasks_compared.saturating_sub(self.tasks_identical)
    }
}

/// Compare les dates enregistrées au planning calculé par le moteur.
#[must_use]
pub fn compare(project: &Project, schedule: &Schedule, recorded: &Recorded) -> Comparison {
    let mut out = Comparison::default();
    for (pos, task) in project.tasks.iter().enumerate() {
        if task.is_null {
            continue;
        }
        let uid = task.uid();
        let (Some(rec), Some(TaskOutcome::Scheduled(s))) =
            (recorded.tasks.get(&uid), schedule.tasks.get(&uid))
        else {
            continue;
        };
        let (Some(rs), Some(rf)) = (rec.start, rec.finish) else {
            continue;
        };
        out.tasks_compared = out.tasks_compared.saturating_add(1);
        let start_ok = s.start == rs;
        let finish_ok = s.finish == rf;
        if start_ok {
            out.starts_equal = out.starts_equal.saturating_add(1);
        }
        if finish_ok {
            out.finishes_equal = out.finishes_equal.saturating_add(1);
        }
        if start_ok && finish_ok {
            out.tasks_identical = out.tasks_identical.saturating_add(1);
        }
        let id = u32::try_from(pos).unwrap_or(u32::MAX).saturating_add(1);
        for (field, r, c, ok) in [
            ("start", rs, s.start, start_ok),
            ("finish", rf, s.finish, finish_ok),
        ] {
            if !ok && out.gaps.len() < MAX_GAPS {
                out.gaps.push(DateGap {
                    uid: uid.get(),
                    id,
                    field,
                    recorded: r.to_string(),
                    computed: c.to_string(),
                    minutes: c.minutes().saturating_sub(r.minutes()),
                });
            }
        }
    }
    out
}
