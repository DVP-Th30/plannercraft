//! Delta de diagnostics d'un lot : refus sur nouvelle erreur (R-160, lot
//! L2).
//!
//! Un lot est refusé s'il fait apparaître une erreur absente avant ; les
//! erreurs héritées (fichier importé, état antérieur) ne bloquent pas. Les
//! avertissements et informations nouveaux sont rendus sans bloquer.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{CalendarUid, Diagnostic, Project, Schedule, Severity, Subject, TaskUid};

/// Diagnostics d'un état : `validate` ∪ diagnostics du planning, triés,
/// dédoublonnés.
pub(crate) fn state_diagnostics(p: &Project, s: &Schedule) -> Vec<Diagnostic> {
    let mut all = p.validate().finish();
    all.extend(s.diagnostics.iter().cloned());
    all.sort();
    all.dedup();
    all
}

/// Nouveautés d'un lot (R-160).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DiagDelta {
    /// Erreurs nouvelles (triées) : le lot est refusé si non vide.
    pub errors: Vec<Diagnostic>,
    /// Avertissements et informations absents avant (triés, dédoublonnés).
    pub warnings: Vec<Diagnostic>,
}

/// R-160 : compare les diagnostics avant et après (listes triées).
///
/// Clé d'une erreur : `(code, sujet)` ; pour `link.cycle`, l'ensemble trié
/// des tâches, nouveau seulement s'il n'est inclus dans aucun cycle d'avant
/// (réparer un cycle par étapes). Un avertissement ou une information est
/// nouveau s'il n'est pas présent à l'identique avant.
pub(crate) fn diagnostics_delta(before: &[Diagnostic], after: &[Diagnostic]) -> DiagDelta {
    let mut keys: BTreeSet<(&'static str, Subject)> = BTreeSet::new();
    let mut cycles: Vec<BTreeSet<TaskUid>> = Vec::new();
    for d in before.iter().filter(|d| d.severity() == Severity::Error) {
        match d {
            Diagnostic::LinkCycle { tasks } => cycles.push(tasks.iter().copied().collect()),
            _ => {
                keys.insert((d.code(), d.subject()));
            }
        }
    }
    let mut out = DiagDelta::default();
    for d in after {
        if d.severity() == Severity::Error {
            let new = match d {
                Diagnostic::LinkCycle { tasks } => {
                    !cycles.iter().any(|c| tasks.iter().all(|t| c.contains(t)))
                }
                _ => !keys.contains(&(d.code(), d.subject())),
            };
            if new {
                out.errors.push(d.clone());
            }
        } else if before.binary_search(d).is_err() {
            out.warnings.push(d.clone());
        }
    }
    tolerate_no_working_time(after, &mut out);
    out.errors.sort();
    out.errors.dedup();
    out.warnings.sort();
    out.warnings.dedup();
    out
}

/// R-160 amendée (J7, R-247, ADR-0015 D8) : un calendrier sans minute
/// ouvrée est accepté avec un avertissement. Ses erreurs nouvelles
/// (`calendar.no_working_time`, et `project.calendar_fallback` quand le
/// calendrier du projet est celui-là : repli sur le Standard intégré,
/// R-066) passent des erreurs aux nouveautés non bloquantes ; un fichier
/// relu dans le même état n'est pas refusé non plus.
fn tolerate_no_working_time(after: &[Diagnostic], out: &mut DiagDelta) {
    let empty: BTreeSet<CalendarUid> = after
        .iter()
        .filter_map(|d| match d {
            Diagnostic::CalendarNoWorkingTime { calendar } => Some(*calendar),
            _ => None,
        })
        .collect();
    if empty.is_empty() {
        return;
    }
    let (tolerated, kept): (Vec<Diagnostic>, Vec<Diagnostic>) = std::mem::take(&mut out.errors)
        .into_iter()
        .partition(|d| match d {
            Diagnostic::CalendarNoWorkingTime { .. } => true,
            Diagnostic::ProjectCalendarFallback { calendar } => empty.contains(calendar),
            _ => false,
        });
    out.errors = kept;
    out.warnings.extend(tolerated);
}

/// Index d'opération à imputer à une erreur nouvelle (R-160) : la dernière
/// opération qui a touché son sujet (pour `link.cycle` : une tâche du cycle
/// ou un lien entre deux d'entre elles), sinon `None`.
pub(crate) fn blame(d: &Diagnostic, touched: &BTreeMap<Subject, usize>) -> Option<usize> {
    match d {
        Diagnostic::LinkCycle { tasks } => {
            let members: BTreeSet<TaskUid> = tasks.iter().copied().collect();
            touched
                .iter()
                .filter(|(s, _)| match s {
                    Subject::Task(t) => members.contains(t),
                    Subject::Link {
                        predecessor,
                        successor,
                    } => members.contains(predecessor) && members.contains(successor),
                    _ => false,
                })
                .map(|(_, i)| *i)
                .max()
        }
        _ => touched.get(&d.subject()).copied(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use plannercraft_core::{CalendarUid, Diagnostic, Subject, TaskUid};

    use super::{blame, diagnostics_delta};

    fn cycle(uids: &[u32]) -> Diagnostic {
        Diagnostic::LinkCycle {
            tasks: uids.iter().map(|u| TaskUid::new(*u)).collect(),
        }
    }

    #[test]
    fn r160_no_working_time_tolerated() {
        // R-160 amendée (J7, R-247) : calendrier sans minute ouvrée et repli
        // du calendrier du projet sur lui : non bloquants, rendus en
        // nouveautés ; un repli sur un autre calendrier reste bloquant.
        let empty = Diagnostic::CalendarNoWorkingTime {
            calendar: CalendarUid::new(1),
        };
        let fallback = Diagnostic::ProjectCalendarFallback {
            calendar: CalendarUid::new(1),
        };
        let other = Diagnostic::ProjectCalendarFallback {
            calendar: CalendarUid::new(2),
        };
        let mut after = vec![empty.clone(), fallback.clone(), other.clone()];
        after.sort();
        let d = diagnostics_delta(&[], &after);
        assert_eq!(d.errors, vec![other]);
        let mut expected = vec![empty, fallback];
        expected.sort();
        assert_eq!(d.warnings, expected);
    }

    #[test]
    fn r160_cycle_subset_not_new() {
        let d = diagnostics_delta(&[cycle(&[5, 7, 9])], &[cycle(&[7, 9])]);
        assert_eq!(d.errors, Vec::new());
        let d = diagnostics_delta(&[cycle(&[5, 7])], &[cycle(&[5, 7, 9])]);
        assert_eq!(d.errors, vec![cycle(&[5, 7, 9])]);
    }

    #[test]
    fn r160_blame_cycle_by_link() {
        let mut touched = BTreeMap::new();
        touched.insert(Subject::Task(TaskUid::new(2)), 0);
        touched.insert(
            Subject::Link {
                predecessor: TaskUid::new(3),
                successor: TaskUid::new(1),
            },
            4,
        );
        assert_eq!(blame(&cycle(&[1, 3]), &touched), Some(4));
        assert_eq!(blame(&cycle(&[5, 6]), &touched), None);
    }
}
