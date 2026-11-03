//! R-171 — Calendrier effectif d'une affectation : calendrier propre de la
//! tâche ∩ calendrier de la ressource ; la tâche ignore les calendriers de
//! ressource : le sien seul ; sans calendrier propre : celui de la
//! ressource ; ressource sans calendrier : celui de la tâche, sinon du
//! projet ; intersection sans temps ouvré : calendrier de la tâche,
//! `assignment.calendar_empty`. Source : S44, S48, S8 ; O23 (A6 UID 2).
//!
//! Le module porte aussi les aides des tests J5 du moteur (R-170 à R-177) :
//! ressources, affectations, calendrier « L6 » (lun-sam).

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{
    Assignment, AssignmentSchedule, AssignmentUid, CalendarUid, EffectiveCalendar, Minutes, Resource,
    ResourceKind, ResourceUid, Units, Week, Weekday, Work,
};
use plannercraft_tests::net::{Net, Run};
use plannercraft_tests::{moment, working};

/// UID du calendrier « L6 » : lun-sam 08:00-12:00, 13:00-17:00 (comme A6
/// UID 9).
pub(crate) const L6: u32 = 20;
/// UID du calendrier Standard intégré.
pub(crate) const STANDARD: u32 = 1;

/// Calendrier de base d'UID `uid`, ouvré 08:00-12:00, 13:00-17:00 les jours
/// pour lesquels `open` est vrai.
pub(crate) fn base_calendar(uid: u32, open: impl Fn(Weekday) -> bool) -> Calendar {
    Calendar::new_base(
        CalendarUid::new(uid),
        format!("C{uid}"),
        Week::from_fn(|d| {
            if open(d) {
                working(&[("08:00", "12:00"), ("13:00", "17:00")])
            } else {
                DaySetting::NonWorking
            }
        }),
    )
}

/// Calendrier « L6 » (lun-sam).
pub(crate) fn l6() -> Calendar {
    base_calendar(L6, |d| d != Weekday::Sunday)
}

/// Ajoute le calendrier L6 au projet.
pub(crate) fn with_l6(net: Net) -> Net {
    net.project(|p| p.calendars.push(l6()).unwrap())
}

/// Ajoute une ressource de travail d'UID `uid`, de calendrier `calendar`.
pub(crate) fn resource(net: Net, uid: u32, calendar: Option<u32>) -> Net {
    resource_kind(net, uid, calendar, ResourceKind::Work)
}

/// Ajoute une ressource d'UID `uid`, de type `kind`.
pub(crate) fn resource_kind(net: Net, uid: u32, calendar: Option<u32>, kind: ResourceKind) -> Net {
    net.project(|p| {
        let mut r = Resource::new(ResourceUid::new(uid), format!("R{uid}"));
        r.calendar = calendar.map(CalendarUid::new);
        r.kind = kind;
        p.resources.push(r).unwrap();
    })
}

/// Affecte la ressource `resource` (`None` : fictive −65 535) à la tâche
/// `task` : travail en minutes, unités en 1/10 000, délai en minutes
/// ouvrées.
pub(crate) fn assign_full(
    net: Net,
    uid: u32,
    task: &str,
    resource: Option<u32>,
    (work, units, delay): (i64, i64, i64),
) -> Net {
    let t = net.uid(task);
    net.project(|p| {
        let mut a = Assignment::new(AssignmentUid::new(uid), t, resource.map(ResourceUid::new));
        a.work = Work::new(Minutes::new(work)).unwrap();
        a.units = Units::new(units).unwrap();
        a.delay = Minutes::new(delay);
        p.assignments.push(a).unwrap();
    })
}

/// Affecte la ressource `resource` à 100 % pour `work` minutes, sans délai.
pub(crate) fn assign(net: Net, uid: u32, task: &str, resource: u32, work: i64) -> Net {
    assign_full(net, uid, task, Some(resource), (work, 10_000, 0))
}

/// Affectation placée d'UID `uid`.
pub(crate) fn placed(run: &Run, uid: u32) -> AssignmentSchedule {
    match run.schedule.assignments.get(&AssignmentUid::new(uid)) {
        Some(a) => *a,
        None => panic!("affectation {uid} non placée : {:?}", run.schedule.assignments),
    }
}

#[test]
fn r171_intersection_skips_saturday() {
    // Tâche L6 de 3 j depuis jeu 22/10/2026 08:00, ressource sur le
    // Standard : L6 ∩ S = lun-ven ⇒ jeu, ven, lun 26/10 17:00 (A6 UID 2,
    // O23 : jamais placée le samedi).
    let net = with_l6(Net::new("2026-10-22T08:00"))
        .task("A", 1_440)
        .calendar("A", L6);
    let run = assign(resource(net, 1, Some(STANDARD)), 1, "A", 1, 1_440).run();
    assert_eq!(run.at("A").start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-26T17:00"));
    let a = placed(&run, 1);
    assert_eq!(
        a.calendar,
        EffectiveCalendar::Intersection {
            task: CalendarUid::new(L6),
            resource: CalendarUid::new(STANDARD),
        }
    );
    assert_eq!(a.span.finish, moment("2026-10-26T17:00"));
}

#[test]
fn r171_ignore_resource_calendar() {
    // S48 : « only the task calendar is considered » ⇒ samedi ouvré, fin
    // sam 24/10 17:00.
    let net = with_l6(Net::new("2026-10-22T08:00"))
        .task("A", 1_440)
        .calendar("A", L6)
        .with_task("A", |t| t.ignore_resource_calendar = true);
    let run = assign(resource(net, 1, Some(STANDARD)), 1, "A", 1, 1_440).run();
    assert_eq!(run.at("A").finish, moment("2026-10-24T17:00"));
    assert_eq!(
        placed(&run, 1).calendar,
        EffectiveCalendar::Task(CalendarUid::new(L6))
    );
}

#[test]
fn r171_resource_calendar_only() {
    // Projet sur L6, tâche sans calendrier propre : la ressource (Standard)
    // place la tâche (S8), jamais le samedi ; ressource sans calendrier :
    // calendrier du projet (samedi ouvré).
    let net = with_l6(Net::new("2026-10-22T08:00"))
        .project(|p| p.calendar = CalendarUid::new(L6))
        .task("A", 1_440)
        .task("B", 1_440);
    let net = resource(resource(net, 1, Some(STANDARD)), 2, None);
    let run = assign(assign(net, 1, "A", 1, 1_440), 2, "B", 2, 1_440).run();
    assert_eq!(run.at("A").finish, moment("2026-10-26T17:00"));
    assert_eq!(
        placed(&run, 1).calendar,
        EffectiveCalendar::Resource(CalendarUid::new(STANDARD))
    );
    assert_eq!(run.at("B").finish, moment("2026-10-24T17:00"));
    assert_eq!(
        placed(&run, 2).calendar,
        EffectiveCalendar::Project(CalendarUid::new(L6))
    );
}

#[test]
fn r171_empty_intersection_fallback() {
    // Tâche sur le Standard (lun-ven), ressource ouvrée le week-end
    // seulement : intersection vide ⇒ calendrier de la tâche,
    // `assignment.calendar_empty`.
    let weekend = base_calendar(30, |d| matches!(d, Weekday::Saturday | Weekday::Sunday));
    let net = Net::new("2026-10-19T08:00")
        .project(|p| p.calendars.push(weekend).unwrap())
        .task("A", 960)
        .calendar("A", STANDARD);
    let run = assign(resource(net, 1, Some(30)), 1, "A", 1, 960).run();
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(
        placed(&run, 1).calendar,
        EffectiveCalendar::Task(CalendarUid::new(STANDARD))
    );
    assert!(
        run.codes().contains(&"assignment.calendar_empty"),
        "{:?}",
        run.codes()
    );
}

#[test]
fn r171_unusable_resource_calendar_falls_back() {
    // Calendrier de ressource inconnu : comme sans calendrier,
    // `task.calendar_fallback`.
    let net = Net::new("2026-10-19T08:00").task("A", 960);
    let run = assign(resource(net, 1, Some(99)), 1, "A", 1, 960).run();
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(
        placed(&run, 1).calendar,
        EffectiveCalendar::Project(CalendarUid::new(STANDARD))
    );
    assert!(
        run.codes().contains(&"task.calendar_fallback"),
        "{:?}",
        run.codes()
    );
}

#[test]
fn r171_combined_segment_budget_falls_back_to_task_calendar() {
    // Budget des calendriers combinés épuisé (`Limits::combined_segments`
    // 0, R-172, règle 10) : l'intersection L6 ∩ S n'est pas compilée,
    // `calendar.too_many_segments`, l'affectation retombe sur le
    // calendrier de la tâche (comme une intersection vide) : samedi ouvré,
    // fin sam 24/10 17:00 au lieu du lun 26/10.
    let build = || {
        let net = with_l6(Net::new("2026-10-22T08:00"))
            .task("A", 1_440)
            .calendar("A", L6);
        assign(resource(net, 1, Some(STANDARD)), 1, "A", 1, 1_440)
    };
    let run = build().run_with(plannercraft_core::Limits {
        combined_segments: 0,
        ..plannercraft_core::Limits::default()
    });
    assert_eq!(run.at("A").finish, moment("2026-10-24T17:00"));
    assert_eq!(
        placed(&run, 1).calendar,
        EffectiveCalendar::Task(CalendarUid::new(L6))
    );
    assert!(
        run.codes().contains(&"calendar.too_many_segments"),
        "{:?}",
        run.codes()
    );
    // Budget par défaut : intersection compilée.
    let run = build().run();
    assert_eq!(run.at("A").finish, moment("2026-10-26T17:00"));
}
