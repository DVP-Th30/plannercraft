//! `Project::validate` (ADR-0008, § 7.3) : contrôles du modèle, en un seul
//! passage déterministe, dans cet ordre :
//! 1. UID 0 dans `tasks` ;
//! 2. niveaux hiérarchiques ;
//! 3. lignes vides porteuses de données ;
//! 4. avancement ;
//! 5. liens (auto-lien, doublon, tâche inconnue) ;
//! 6. affectations (tâche, ressource) ;
//! 7. références de calendrier (projet, tâches, ressources) ;
//! 8. calendrier du projet qui n'est pas un calendrier de base ;
//! 9. pour chaque calendrier : héritage, semaines datées qui se chevauchent,
//!    exceptions qui se chevauchent (jugées sur les jours dépliés des
//!    récurrences), récurrences non prises en charge ;
//! 10. champs personnalisés (R-263) : valeur du type du champ et dans ses
//!     bornes.
//!
//! `CalendarSet::compile` ajoute ensuite `CalendarNoWorkingTime`.

use std::collections::BTreeSet;

use crate::calendar::model::Calendar;
use crate::calendar::recurrence::{DayClaims, expand_exception};
use crate::calendar::resolve::{chain, exceptions_in_order, work_weeks_in_order};
use crate::diagnostic::{Diagnostic, Diagnostics, Subject};
use crate::ids::{CalendarUid, TaskUid};
use crate::model::Project;

impl Project {
    /// Contrôle le modèle et rend ses diagnostics (non triés : appeler
    /// [`Diagnostics::finish`]). Ne modifie rien, n'échoue jamais.
    #[must_use]
    pub fn validate(&self) -> Diagnostics {
        let mut out = Diagnostics::new();
        self.check_reserved_uid(&mut out);
        self.check_outline(&mut out);
        self.check_null_rows(&mut out);
        self.check_progress(&mut out);
        self.check_links(&mut out);
        self.check_assignments(&mut out);
        self.check_calendar_references(&mut out);
        self.check_project_calendar(&mut out);
        for calendar in &self.calendars {
            self.check_calendar(calendar, &mut out);
        }
        self.check_custom(&mut out);
        out
    }

    /// Contrôle 10 (R-263) : chaque valeur personnalisée (récapitulative du
    /// projet comprise) a le type de son champ et tient dans ses bornes ;
    /// sinon `task.custom_invalid` (erreur), une fois par valeur.
    fn check_custom(&self, out: &mut Diagnostics) {
        for task in self.summary_task.iter().chain(self.tasks.iter()) {
            for (field, value) in &task.custom {
                if value.kind() != field.kind() || value.check().is_err() {
                    out.push(Diagnostic::TaskCustomInvalid {
                        task: task.uid(),
                        field: *field,
                    });
                }
            }
        }
    }

    /// Contrôle 1 : uID 0 dans `tasks` (impossible après lecture JSON, possible par l'API).
    fn check_reserved_uid(&self, out: &mut Diagnostics) {
        if self.tasks.contains(TaskUid::PROJECT_SUMMARY) {
            out.push(Diagnostic::TaskReservedUid);
        }
    }

    /// Contrôle 2 : premier niveau égal à 1, puis jamais plus de 1 au-dessus du
    /// précédent. Les lignes vides sont ignorées (hypothèse).
    fn check_outline(&self, out: &mut Diagnostics) {
        let mut previous: Option<u16> = None;
        for task in self.tasks.iter().filter(|t| !t.is_null) {
            let level = task.outline_level.get();
            let allowed = previous.map_or(1, |p| p.saturating_add(1));
            if level > allowed {
                out.push(Diagnostic::TaskOutlineJump { task: task.uid() });
            }
            previous = Some(level);
        }
    }

    /// Contrôle 3 : ligne vide avec des liens (dans un sens ou dans l'autre) ou des
    /// affectations.
    fn check_null_rows(&self, out: &mut Diagnostics) {
        let referenced: BTreeSet<TaskUid> = self
            .tasks
            .iter()
            .flat_map(|t| t.predecessors.iter().map(|l| l.predecessor))
            .chain(self.assignments.iter().map(|a| a.task))
            .collect();
        for task in self.tasks.iter().filter(|t| t.is_null) {
            if !task.predecessors.is_empty() || referenced.contains(&task.uid()) {
                out.push(Diagnostic::TaskNullWithData { task: task.uid() });
            }
        }
    }

    /// Contrôle 4 : durée réelle ou restante négative.
    fn check_progress(&self, out: &mut Diagnostics) {
        for task in &self.tasks {
            if task.progress.actual_duration.is_negative() || task.progress.remaining_duration.is_negative() {
                out.push(Diagnostic::TaskNegativeProgress { task: task.uid() });
            }
        }
    }

    /// Contrôle 5 : liens : auto-lien, doublon de paire, prédécesseur inconnu.
    fn check_links(&self, out: &mut Diagnostics) {
        for task in &self.tasks {
            let successor = task.uid();
            let mut seen = BTreeSet::new();
            for link in &task.predecessors {
                let predecessor = link.predecessor;
                if !seen.insert(predecessor) {
                    out.push(Diagnostic::LinkDuplicate {
                        predecessor,
                        successor,
                    });
                }
                if predecessor == successor {
                    out.push(Diagnostic::LinkSelf { task: successor });
                } else if !self.tasks.contains(predecessor) {
                    out.push(Diagnostic::LinkUnknownTask {
                        predecessor,
                        successor,
                    });
                }
            }
        }
    }

    /// Contrôle 6 : affectations : tâche (la récapitulative du projet compte si elle
    /// existe) et ressource.
    fn check_assignments(&self, out: &mut Diagnostics) {
        for a in &self.assignments {
            let summary = a.task == TaskUid::PROJECT_SUMMARY && self.summary_task.is_some();
            if !summary && !self.tasks.contains(a.task) {
                out.push(Diagnostic::AssignmentUnknownTask {
                    assignment: a.uid(),
                    task: a.task,
                });
            }
            if let Some(resource) = a.resource
                && !self.resources.contains(resource)
            {
                out.push(Diagnostic::AssignmentUnknownResource {
                    assignment: a.uid(),
                    resource,
                });
            }
        }
    }

    /// Contrôle 7 : calendriers référencés par le projet, les tâches, les ressources.
    fn check_calendar_references(&self, out: &mut Diagnostics) {
        let mut check = |calendar: CalendarUid, referenced_by: Subject| {
            if !self.calendars.contains(calendar) {
                out.push(Diagnostic::CalendarUnknown {
                    calendar,
                    referenced_by,
                });
            }
        };
        check(self.calendar, Subject::Project);
        for task in self.summary_task.iter().chain(&self.tasks) {
            if let Some(c) = task.calendar {
                check(c, Subject::Task(task.uid()));
            }
        }
        for resource in &self.resources {
            if let Some(c) = resource.calendar {
                check(c, Subject::Resource(resource.uid()));
            }
        }
    }

    /// Contrôle 8 : calendrier du projet dérivé.
    fn check_project_calendar(&self, out: &mut Diagnostics) {
        if let Some(c) = self.calendars.get(self.calendar)
            && !c.is_base()
        {
            out.push(Diagnostic::ProjectCalendarNotBase {
                calendar: self.calendar,
            });
        }
    }

    /// Contrôle 9 : un calendrier : héritage, semaines datées et exceptions
    /// qui se chevauchent, récurrences non prises en charge.
    fn check_calendar(&self, calendar: &Calendar, out: &mut Diagnostics) {
        let uid = calendar.uid();
        // `chain` ne rend, pour un calendrier présent, que des erreurs
        // d'héritage.
        if let Err(error) = chain(&self.calendars, uid)
            && let Some(d) = Diagnostic::from_calendar_error(&error)
        {
            out.push(d);
        }

        // Semaines datées (non récurrentes : les jours sont ceux de la
        // période). Posées dans l'ordre (début, indice), chacune ne prend que
        // les jours libres, comme à la compilation (R-019) : `kept` est la
        // semaine qui s'applique réellement sur les jours communs.
        let mut week_claims: DayClaims<usize> = DayClaims::new();
        for (ignored, w) in work_weeks_in_order(calendar) {
            for kept in week_claims.claim(w.period.first().index(), w.period.last().index(), ignored) {
                out.push(Diagnostic::CalendarWorkWeeksOverlap {
                    calendar: uid,
                    kept,
                    ignored,
                });
            }
        }

        // Exceptions : chevauchement jugé sur les jours dépliés (R-017,
        // R-018). Elles sont posées dans l'ordre (début, indice) ; chacune ne
        // prend que les jours libres. `kept` est l'exception qui s'applique
        // sur les jours communs.
        let mut claims: DayClaims<usize> = DayClaims::new();
        for (ignored, e) in exceptions_in_order(calendar) {
            // Une récurrence invalide ne peut pas être stockée dans un
            // calendrier (lecture et commandes la refusent).
            let runs = expand_exception(e).unwrap_or_default();
            let mut kept_by: Vec<usize> = Vec::new();
            for run in runs {
                for kept in claims.claim(run.first().index(), run.last().index(), ignored) {
                    if !kept_by.contains(&kept) {
                        kept_by.push(kept);
                    }
                }
            }
            for kept in kept_by {
                out.push(Diagnostic::CalendarExceptionsOverlap {
                    calendar: uid,
                    kept,
                    ignored,
                });
            }
        }

        for (exception, e) in calendar.exceptions().iter().enumerate() {
            if e.recurrence.is_unsupported() {
                out.push(Diagnostic::CalendarRecurrenceUnsupported {
                    calendar: uid,
                    exception,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use crate::arena::Arena;
    use crate::calendar::builtin::BuiltinCalendarNames;
    use crate::calendar::model::{Calendar, CalendarException, DayRule, DaySetting, Recurrence, WorkWeek};
    use crate::diagnostic::{Diagnostic, Subject};
    use crate::duration::Minutes;
    use crate::ids::{AssignmentUid, CalendarUid, ResourceUid, TaskUid};
    use crate::model::{
        Assignment, Link, OutlineLevel, Project, ProjectInit, Resource, ScheduleAnchor, Task,
    };
    use crate::time::{Day, DayRange, Moment, Week};

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn project() -> Project {
        Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(m("2026-10-19T08:00")),
            current_date: m("2026-10-17T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        })
    }

    fn task(p: &Project, uid: u32) -> Task {
        Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options)
    }

    fn diagnostics(p: &Project) -> Vec<Diagnostic> {
        p.validate().finish()
    }

    /// Calendriers bruts (la lecture accepte des données que les commandes
    /// refusent).
    fn with_calendars(p: &mut Project, extra: Vec<serde_json::Value>) {
        let mut all: Vec<serde_json::Value> = p
            .calendars
            .iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
        all.extend(extra);
        p.calendars =
            serde_json::from_value::<Arena<CalendarUid, Calendar>>(serde_json::Value::Array(all)).unwrap();
    }

    #[test]
    fn new_project_is_clean() {
        let mut p = project();
        for uid in 1..=3 {
            let t = task(&p, uid);
            p.tasks.push(t).unwrap();
        }
        p.tasks
            .get_mut(TaskUid::new(2))
            .unwrap()
            .predecessors
            .push(Link::finish_to_start(TaskUid::new(1)));
        assert_eq!(diagnostics(&p), []);
    }

    #[test]
    fn reserved_uid() {
        let mut p = project();
        let t = task(&p, 0);
        p.tasks.push(t).unwrap();
        assert_eq!(diagnostics(&p), [Diagnostic::TaskReservedUid]);
    }

    #[test]
    fn outline_jumps() {
        let mut p = project();
        for (uid, level) in [(1, 2), (2, 2), (3, 4), (4, 1), (5, 2), (6, 3)] {
            let mut t = task(&p, uid);
            t.outline_level = OutlineLevel::new(level).unwrap();
            p.tasks.push(t).unwrap();
        }
        // Ligne vide de niveau 5 : ignorée.
        let mut blank = task(&p, 7);
        blank.is_null = true;
        blank.outline_level = OutlineLevel::new(5).unwrap();
        p.tasks.insert(3, blank).unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::TaskOutlineJump {
                    task: TaskUid::new(1)
                },
                Diagnostic::TaskOutlineJump {
                    task: TaskUid::new(3)
                }
            ]
        );
    }

    #[test]
    fn null_rows_with_data() {
        let mut p = project();
        for uid in 1..=4 {
            let mut t = task(&p, uid);
            t.is_null = uid > 1;
            p.tasks.push(t).unwrap();
        }
        // 2 a un prédécesseur ; 3 est prédécesseur de 1 ; 4 a une affectation.
        p.tasks
            .get_mut(TaskUid::new(2))
            .unwrap()
            .predecessors
            .push(Link::finish_to_start(TaskUid::new(1)));
        p.tasks
            .get_mut(TaskUid::new(1))
            .unwrap()
            .predecessors
            .push(Link::finish_to_start(TaskUid::new(3)));
        p.assignments
            .push(Assignment::new(AssignmentUid::new(1), TaskUid::new(4), None))
            .unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::TaskNullWithData {
                    task: TaskUid::new(2)
                },
                Diagnostic::TaskNullWithData {
                    task: TaskUid::new(3)
                },
                Diagnostic::TaskNullWithData {
                    task: TaskUid::new(4)
                },
            ]
        );
    }

    #[test]
    fn negative_progress() {
        let mut p = project();
        let mut a = task(&p, 1);
        a.progress.actual_duration = Minutes::new(-1);
        let mut b = task(&p, 2);
        b.progress.remaining_duration = Minutes::new(-60);
        p.tasks.push(a).unwrap();
        p.tasks.push(b).unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::TaskNegativeProgress {
                    task: TaskUid::new(1)
                },
                Diagnostic::TaskNegativeProgress {
                    task: TaskUid::new(2)
                },
            ]
        );
    }

    #[test]
    fn links() {
        let mut p = project();
        let mut t1 = task(&p, 1);
        t1.predecessors.push(Link::finish_to_start(TaskUid::new(1)));
        let mut t2 = task(&p, 2);
        t2.predecessors.extend([
            Link::finish_to_start(TaskUid::new(1)),
            Link::finish_to_start(TaskUid::new(1)),
            Link::finish_to_start(TaskUid::new(42)),
        ]);
        p.tasks.push(t1).unwrap();
        p.tasks.push(t2).unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::LinkSelf {
                    task: TaskUid::new(1)
                },
                Diagnostic::LinkDuplicate {
                    predecessor: TaskUid::new(1),
                    successor: TaskUid::new(2)
                },
                Diagnostic::LinkUnknownTask {
                    predecessor: TaskUid::new(42),
                    successor: TaskUid::new(2)
                },
            ]
        );
    }

    #[test]
    fn assignments() {
        let mut p = project();
        let t = task(&p, 1);
        p.tasks.push(t).unwrap();
        p.resources
            .push(Resource::new(ResourceUid::new(1), "R".to_owned()))
            .unwrap();
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(1),
                TaskUid::new(1),
                Some(ResourceUid::new(1)),
            ))
            .unwrap();
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(2),
                TaskUid::new(9),
                Some(ResourceUid::new(1)),
            ))
            .unwrap();
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(3),
                TaskUid::new(1),
                Some(ResourceUid::new(8)),
            ))
            .unwrap();
        p.assignments
            .push(Assignment::new(AssignmentUid::new(4), TaskUid::new(1), None))
            .unwrap();
        // Affectation sur la récapitulative du projet : connue seulement si elle existe.
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(5),
                TaskUid::PROJECT_SUMMARY,
                None,
            ))
            .unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::AssignmentUnknownTask {
                    assignment: AssignmentUid::new(2),
                    task: TaskUid::new(9)
                },
                Diagnostic::AssignmentUnknownTask {
                    assignment: AssignmentUid::new(5),
                    task: TaskUid::PROJECT_SUMMARY
                },
                Diagnostic::AssignmentUnknownResource {
                    assignment: AssignmentUid::new(3),
                    resource: ResourceUid::new(8)
                },
            ]
        );
        let summary = task(&p, 0);
        p.summary_task = Some(summary);
        assert_eq!(diagnostics(&p).len(), 2);
    }

    #[test]
    fn calendar_references() {
        let mut p = project();
        p.calendar = CalendarUid::new(50);
        let mut t = task(&p, 1);
        t.calendar = Some(CalendarUid::new(51));
        p.tasks.push(t).unwrap();
        let mut r = Resource::new(ResourceUid::new(1), "R".to_owned());
        r.calendar = Some(CalendarUid::new(52));
        p.resources.push(r).unwrap();
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::CalendarUnknown {
                    calendar: CalendarUid::new(50),
                    referenced_by: Subject::Project
                },
                Diagnostic::CalendarUnknown {
                    calendar: CalendarUid::new(51),
                    referenced_by: Subject::Task(TaskUid::new(1))
                },
                Diagnostic::CalendarUnknown {
                    calendar: CalendarUid::new(52),
                    referenced_by: Subject::Resource(ResourceUid::new(1)),
                },
            ]
        );
    }

    #[test]
    fn project_calendar_not_base() {
        let mut p = project();
        p.calendars
            .push(Calendar::new_derived(
                CalendarUid::new(4),
                "Paul".to_owned(),
                CalendarUid::new(1),
            ))
            .unwrap();
        p.calendar = CalendarUid::new(4);
        assert_eq!(
            diagnostics(&p),
            [Diagnostic::ProjectCalendarNotBase {
                calendar: CalendarUid::new(4)
            }]
        );
    }

    #[test]
    fn calendar_inheritance() {
        let mut p = project();
        let derived = |uid: u32, base: u32| {
            Calendar::new_derived(CalendarUid::new(uid), format!("D{uid}"), CalendarUid::new(base))
        };
        for c in [
            derived(4, 1),
            derived(5, 6),
            derived(6, 5),
            derived(7, 4),
            derived(8, 99),
        ] {
            p.calendars.push(c).unwrap();
        }
        let uid = CalendarUid::new;
        assert_eq!(
            diagnostics(&p),
            [
                Diagnostic::CalendarBaseMissing {
                    calendar: uid(8),
                    base: uid(99)
                },
                Diagnostic::CalendarBaseNotBase {
                    calendar: uid(7),
                    base: uid(4)
                },
                Diagnostic::CalendarInheritanceCycle {
                    chain: vec![uid(5), uid(6), uid(5)]
                },
                Diagnostic::CalendarInheritanceCycle {
                    chain: vec![uid(6), uid(5), uid(6)]
                },
            ]
        );
    }

    #[test]
    fn calendar_work_weeks_and_recurrences() {
        let mut p = project();
        let day = |s: &str| s.parse::<Day>().unwrap();
        let ww = |a: &str, b: &str| WorkWeek {
            name: String::new(),
            period: DayRange::new(day(a), day(b)).unwrap(),
            days: Week::from_fn(|_| DayRule::Inherit),
        };
        let ex = |recurrence| CalendarException {
            name: String::new(),
            period: DayRange::single(day("2026-11-11")),
            setting: DaySetting::NonWorking,
            recurrence,
            occurrences: None,
        };
        let mut c = serde_json::to_value(Calendar::new_derived(
            CalendarUid::new(4),
            "Paul".to_owned(),
            CalendarUid::new(1),
        ))
        .unwrap();
        // Stockage : n° 0 en août, n° 1 en juillet-août, n° 2 en septembre.
        c["work_weeks"] = serde_json::to_value(vec![
            ww("2027-08-01", "2027-08-15"),
            ww("2027-07-01", "2027-08-31"),
            ww("2027-09-01", "2027-09-30"),
        ])
        .unwrap();
        c["exceptions"] = serde_json::to_value(vec![
            ex(Recurrence::Period),
            ex(Recurrence::ByDayCount { every: 1 }),
            ex(Recurrence::ByWeekdayCount { every: 2 }),
        ])
        .unwrap();
        with_calendars(&mut p, vec![c]);
        let uid = CalendarUid::new(4);
        assert_eq!(
            diagnostics(&p),
            [
                // Trois exceptions sur le 11/11 : la n° 0 l'emporte ; les
                // n° 1 et 2 (récurrences ignorées) ne couvrent aucun jour.
                // La n° 1 des semaines commence la première : elle l'emporte
                // sur la n° 0.
                Diagnostic::CalendarWorkWeeksOverlap {
                    calendar: uid,
                    kept: 1,
                    ignored: 0
                },
                Diagnostic::CalendarRecurrenceUnsupported {
                    calendar: uid,
                    exception: 1
                },
                Diagnostic::CalendarRecurrenceUnsupported {
                    calendar: uid,
                    exception: 2
                },
            ]
        );
    }

    #[test]
    fn calendar_nested_work_weeks_report_the_applied_week() {
        // n° 0 : janvier 2027 ; n° 1 : du 05 au 20/01 ; n° 2 : du 10 au 12/01.
        // La n° 0 possède tout janvier : les n° 1 et 2 butent sur elle seule
        // (et non la n° 2 sur la n° 1, qui ne s'applique jamais).
        let mut p = project();
        let day = |s: &str| s.parse::<Day>().unwrap();
        let ww = |a: &str, b: &str| WorkWeek {
            name: String::new(),
            period: DayRange::new(day(a), day(b)).unwrap(),
            days: Week::from_fn(|_| DayRule::NonWorking),
        };
        let mut c = serde_json::to_value(Calendar::new_derived(
            CalendarUid::new(4),
            "Paul".to_owned(),
            CalendarUid::new(1),
        ))
        .unwrap();
        c["work_weeks"] = serde_json::to_value(vec![
            ww("2027-01-01", "2027-01-31"),
            ww("2027-01-05", "2027-01-20"),
            ww("2027-01-10", "2027-01-12"),
        ])
        .unwrap();
        with_calendars(&mut p, vec![c]);
        let uid = CalendarUid::new(4);
        let overlap = |kept, ignored| Diagnostic::CalendarWorkWeeksOverlap {
            calendar: uid,
            kept,
            ignored,
        };
        assert_eq!(diagnostics(&p), [overlap(0, 1), overlap(0, 2)]);
        // La semaine appliquée le 11/01 est bien la n° 0.
        assert_eq!(
            crate::calendar::resolve::resolve_day(&p.calendars, uid, day("2027-01-11"))
                .unwrap()
                .source,
            crate::calendar::resolve::DaySource::WorkWeek {
                calendar: uid,
                index: 0
            }
        );
    }

    #[test]
    fn calendar_exceptions_overlap() {
        let mut p = project();
        let day = |s: &str| s.parse::<Day>().unwrap();
        let ex = |from: &str, to: &str, recurrence| CalendarException {
            name: String::new(),
            period: DayRange::new(day(from), day(to)).unwrap(),
            setting: DaySetting::NonWorking,
            recurrence,
            occurrences: None,
        };
        let mondays = Recurrence::Weekly {
            every: 1,
            days: Week::from_fn(|d| d == crate::time::Weekday::Monday),
        };
        let fridays = Recurrence::Weekly {
            every: 1,
            days: Week::from_fn(|d| d == crate::time::Weekday::Friday),
        };
        let mut c = serde_json::to_value(Calendar::new_derived(
            CalendarUid::new(4),
            "Paul".to_owned(),
            CalendarUid::new(1),
        ))
        .unwrap();
        // n° 0 : 11/11 ; n° 1 : lundis de novembre ; n° 2 : vendredis de
        // novembre (aucun jour commun avec les lundis) ; n° 3 : du 10 au 16/11 ;
        // n° 4 : le 11/11 encore. Ordre de pose (début, indice) : 1, 2 (01/11),
        // 3 (10/11), 0, 4 (11/11). La n° 3 bute sur le lundi 16 (n° 1) et le
        // vendredi 13 (n° 2) et prend le 11 ; les n° 0 et 4 butent sur elle.
        c["exceptions"] = serde_json::to_value(vec![
            ex("2026-11-11", "2026-11-11", Recurrence::Period),
            ex("2026-11-01", "2026-11-30", mondays),
            ex("2026-11-01", "2026-11-30", fridays),
            ex("2026-11-10", "2026-11-16", Recurrence::Period),
            ex("2026-11-11", "2026-11-11", Recurrence::Period),
        ])
        .unwrap();
        with_calendars(&mut p, vec![c]);
        let uid = CalendarUid::new(4);
        let overlap = |kept, ignored| Diagnostic::CalendarExceptionsOverlap {
            calendar: uid,
            kept,
            ignored,
        };
        assert_eq!(
            diagnostics(&p),
            [overlap(1, 3), overlap(2, 3), overlap(3, 0), overlap(3, 4)]
        );
    }
}
