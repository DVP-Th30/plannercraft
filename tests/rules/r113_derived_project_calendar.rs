//! R-113 — Calendrier du projet dérivé : le calendrier du projet est un
//! calendrier de base (S41, S42). Quand `Project::calendar` désigne un
//! calendrier dérivé (calendrier de ressource), sa base mesure la durée des
//! récapitulatives (R-054), les marges (R-048, R-049) et les décalages
//! (R-041) des tâches sans calendrier propre. Amendée en J5 (R-171, R-175) :
//! le calendrier désigné ne place plus que les tâches affectées à ses
//! ressources (comme les feuilles de A4) ; une tâche sans affectation de
//! travail se place sur la base. Observé : O13 (A4, 20 cas reproduits).
//! Hypothèse pour les décalages.

use plannercraft_core::calendar::{Calendar, DayRule, DaySetting};
use plannercraft_core::{CalendarUid, LinkType, Minutes, Week, Weekday};
use plannercraft_tests::net::{Net, lag_w};
use plannercraft_tests::{hours, moment, working};

pub(crate) const BASE: u32 = 10;
pub(crate) const DERIVED: u32 = 11;

pub(crate) fn uid(v: u32) -> CalendarUid {
    CalendarUid::new(v)
}

fn weekdays(day: impl Fn() -> DaySetting) -> Week<DaySetting> {
    Week::from_fn(|d| match d {
        Weekday::Saturday | Weekday::Sunday => DaySetting::NonWorking,
        _ => day(),
    })
}

/// Base sur le modèle de A4 (UID 2, O13) : lun-ven 09:00-13:00, 15:00-19:00.
pub(crate) fn base() -> Calendar {
    Calendar::new_base(
        uid(BASE),
        "Base".to_owned(),
        weekdays(|| working(&[("09:00", "13:00"), ("15:00", "19:00")])),
    )
}

/// Dérivé (calendrier de ressource, A4 UID 1) : lun-ven 08:00-12:00,
/// 13:00-17:00.
pub(crate) fn derived(base_uid: u32) -> Calendar {
    let mut c = Calendar::new_derived(uid(DERIVED), "Ressource".to_owned(), uid(base_uid));
    for d in [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
    ] {
        c.set_day(
            d,
            DayRule::Working(hours(&[("08:00", "12:00"), ("13:00", "17:00")])),
        )
        .unwrap();
    }
    c
}

/// Projet dont le calendrier est le dérivé, début lun 19/10/2026 08:00.
pub(crate) fn net() -> Net {
    Net::new("2026-10-19T08:00").project(|p| {
        p.calendars.push(base()).unwrap();
        p.calendars.push(derived(BASE)).unwrap();
        p.calendar = uid(DERIVED);
    })
}

/// Affecte les tâches `(nom, durée)` (travail = la durée, 100 %) à la
/// ressource UID 1 du calendrier dérivé, créée ici (comme les ressources
/// UID 1 à 4 de A4).
pub(crate) fn staffed(net: Net, tasks: &[(&str, i64)]) -> Net {
    use crate::r171_assignment_effective_calendar::{assign, resource};
    let mut net = resource(net, 1, Some(DERIVED));
    for (k, (name, minutes)) in tasks.iter().enumerate() {
        net = assign(net, 1 + u32::try_from(k).unwrap(), name, 1, *minutes);
    }
    net
}

#[test]
fn r113_assigned_leaves_placed_on_the_resource_calendar() {
    // A 2 j affectée à une ressource du dérivé : placée sur le dérivé, lun
    // 08:00 – mar 17:00 ; B 2 j sans affectation : sur la base, lun 09:00 –
    // mar 19:00 (R-175).
    let run = staffed(net().task("A", 960).task("B", 960), &[("A", 960)]).run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-19T09:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-20T19:00"));
}

#[test]
fn r113_summary_duration_measured_on_the_base() {
    // S {A 2 j ; B 1 j FS} : A lun – mar, B mer 08:00 – 17:00 ; durée de S
    // sur la base = 8 h + 8 h + 6 h (mer 09:00-13:00, 15:00-17:00) = 1 320,
    // 120 de moins que sur le dérivé (O13 : A4 UID 1, 9 480 contre 9 600).
    let run = staffed(
        net()
            .task("S", 0)
            .task("A", 960)
            .level("A", 2)
            .task("B", 480)
            .level("B", 2)
            .fs("A", "B"),
        &[("A", 960), ("B", 480)],
    )
    .run();
    assert_eq!(run.at("S").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("S").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(1_320));
}

#[test]
fn r113_free_slack_measured_on_the_base() {
    // A 1 j → B 1 j (FS) ; C 3 j seule. A lun, B mar, fin mer 17:00 ;
    // FS(A) = base.between(lun 17:00, mar 08:00) = 120 (lun 17:00-19:00),
    // borné par TF(A) = 480 : 120, comme A4 UID 14, 20, 21 (FreeSlack
    // 1 200 dixièmes). Marge totale inchangée (deux fins à 17:00).
    let run = staffed(
        net().task("A", 480).task("B", 480).task("C", 1_440).fs("A", "B"),
        &[("A", 480), ("B", 480), ("C", 1_440)],
    )
    .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("A").free_slack, Minutes::new(120));
    assert_eq!(run.at("B").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").free_slack, Minutes::new(480));
}

#[test]
fn r113_lag_counted_on_the_base() {
    // A 1 j → B FS+2 h : base.add(lun 17:00, 120) = lun 19:00, placé sur le
    // dérivé → mar 08:00 – 17:00 (sur le dérivé, le décalage donnerait
    // mar 10:00). R-041 : sans calendrier propre du successeur, le décalage
    // se compte sur le calendrier du projet, qui est la base.
    let run = staffed(
        net()
            .task("A", 480)
            .task("B", 480)
            .link("A", "B", LinkType::FinishToStart, lag_w(120)),
        &[("A", 480), ("B", 480)],
    )
    .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-20T17:00"));
}

#[test]
fn r113_base_project_calendar_unchanged() {
    // Calendrier du projet de base (la base elle-même) : placement et mesure
    // sur lui seul ; S {A 2 j ; B 1 j FS} : A lun 09:00 – mar 19:00, B mer
    // 09:00 – 19:00, durée de S 1 440.
    let run = Net::new("2026-10-19T08:00")
        .project(|p| {
            p.calendars.push(base()).unwrap();
            p.calendar = uid(BASE);
        })
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T09:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T19:00"));
    assert_eq!(run.at("S").duration, Minutes::new(1_440));
}

#[test]
fn r113_task_calendar_measures_its_own_slack() {
    // Une tâche de calendrier propre (le dérivé désigné explicitement) garde
    // son calendrier pour la mesure : FS(A) = 0, comme avant R-113.
    let run = net()
        .task("A", 480)
        .calendar("A", DERIVED)
        .task("B", 480)
        .task("C", 1_440)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
}

#[test]
fn r113_unusable_base_keeps_the_designated_calendar() {
    // Base sans temps ouvré : le calendrier désigné sert à tout (repli),
    // durée de S 1 440 sur le dérivé.
    let empty = Calendar::new_base(
        uid(BASE),
        "Vide".to_owned(),
        Week::from_fn(|_| DaySetting::NonWorking),
    );
    let run = Net::new("2026-10-19T08:00")
        .project(|p| {
            p.calendars.push(empty).unwrap();
            p.calendars.push(derived(BASE)).unwrap();
            p.calendar = uid(DERIVED);
        })
        .task("S", 0)
        .task("A", 960)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("S").early.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("S").duration, Minutes::new(1_440));
}

/// Vérifie que `measure_calendar` donne l'échelle de mesure du moteur pour
/// chaque tâche planifiée (feuilles ouvrées et récapitulatives) et pour
/// l'UID 0 : la marge totale (R-048) et la durée d'une récapitulative
/// (R-054) s'y retrouvent exactement.
fn assert_matches_engine(p: &plannercraft_core::Project) {
    use plannercraft_core::calendar::{CalendarSet, TimeScale};
    use plannercraft_core::schedule::measure_calendar;
    use plannercraft_core::{TaskOutcome, TaskRole, TaskSchedule, TimeBasis};
    let sched = plannercraft_core::schedule(p);
    let (set, _) = CalendarSet::compile(p);
    let check = |task: &plannercraft_core::Task, s: &TaskSchedule| {
        let summary = s.role == TaskRole::Summary;
        if !summary && task.duration.basis() == TimeBasis::Elapsed {
            return;
        }
        let uid = measure_calendar(p, task).expect("calendrier du projet utilisable");
        let m = TimeScale::Working(set.get(uid).unwrap());
        let tf = m
            .between(s.early.start, s.late.start)
            .unwrap()
            .min(m.between(s.early.finish, s.late.finish).unwrap());
        assert_eq!(tf, s.total_slack, "marge de {:?} sur {uid:?}", task.uid());
        if summary {
            assert_eq!(m.between(s.start, s.finish).unwrap(), s.duration);
        }
    };
    let mut n = 0;
    for t in &p.tasks {
        if let Some(TaskOutcome::Scheduled(s)) = sched.get(t.uid()) {
            check(t, s);
            n += 1;
        }
    }
    assert!(n > 0);
    if let (Some(t), Some(s)) = (&p.summary_task, &sched.project_summary) {
        check(t, s);
    }
}

#[test]
fn r113_measure_calendar_public_matches_engine() {
    use plannercraft_core::schedule::measure_calendar;
    use plannercraft_core::{Task, TaskUid};
    // Projet dérivé : A 4 h, C 1 j ; S {X 2 j ; Y 1 j FS} ; D de calendrier
    // propre (le dérivé), E de calendrier propre 24 heures (UID 2), F à
    // durée écoulée.
    let build = || {
        net()
            .task("A", 240)
            .task("C", 480)
            .task("S", 0)
            .task("X", 960)
            .level("X", 2)
            .task("Y", 480)
            .level("Y", 2)
            .fs("X", "Y")
            .task("D", 480)
            .calendar("D", DERIVED)
            .task("E", 480)
            .calendar("E", 2)
            .elapsed("F", 600)
    };
    let names = build();
    let (ua, ud, ue, us) = (names.uid("A"), names.uid("D"), names.uid("E"), names.uid("S"));
    let mut proj = build().build();
    proj.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &proj.options));
    let task = |u| proj.tasks.get(u).unwrap();
    assert_eq!(measure_calendar(&proj, task(ua)), Some(uid(BASE)));
    assert_eq!(measure_calendar(&proj, task(us)), Some(uid(BASE)));
    assert_eq!(measure_calendar(&proj, task(ud)), Some(uid(DERIVED)));
    assert_eq!(measure_calendar(&proj, task(ue)), Some(uid(2)));
    assert_eq!(
        measure_calendar(&proj, proj.summary_task.as_ref().unwrap()),
        Some(uid(BASE))
    );
    assert_matches_engine(&proj);
    // Réseau qui distingue les deux calendriers : A 4 h, C 1 j, affectées
    // à une ressource du dérivé. A lun 08:00-12:00, au plus tard
    // 13:00-17:00 ; TF(A) = min(base(08:00, 13:00), base(12:00, 17:00)) =
    // min(240, 180) = 180 ; sur le dérivé, 240.
    let small = staffed(net().task("A", 240).task("C", 480), &[("A", 240), ("C", 480)]).build();
    let t = small.tasks.iter().next().unwrap();
    assert_eq!(measure_calendar(&small, t), Some(uid(BASE)));
    assert_eq!(
        plannercraft_core::schedule(&small)
            .scheduled(t.uid())
            .unwrap()
            .total_slack,
        Minutes::new(180)
    );
    assert_matches_engine(&small);
    // UID 0 doté d'un calendrier propre : non lu (R-068), la base mesure.
    let mut summary_own = proj.clone();
    if let Some(t) = summary_own.summary_task.as_mut() {
        t.calendar = Some(uid(2));
    }
    assert_eq!(
        measure_calendar(&summary_own, summary_own.summary_task.as_ref().unwrap()),
        Some(uid(BASE))
    );
    assert_matches_engine(&summary_own);
    // Calendrier propre inconnu (R-066) : repli sur le calendrier du projet,
    // donc sa base.
    let mut unknown_own = proj.clone();
    unknown_own.tasks.get_mut(ua).unwrap().calendar = Some(uid(99));
    assert_eq!(
        measure_calendar(&unknown_own, unknown_own.tasks.get(ua).unwrap()),
        Some(uid(BASE))
    );
    assert_matches_engine(&unknown_own);
    // Calendrier du projet de base : lui-même.
    let base_project = Net::new("2026-10-19T08:00")
        .project(|p| {
            p.calendars.push(base()).unwrap();
            p.calendar = uid(BASE);
        })
        .task("A", 240)
        .task("C", 480)
        .build();
    let t = base_project.tasks.iter().next().unwrap();
    assert_eq!(measure_calendar(&base_project, t), Some(uid(BASE)));
    assert_matches_engine(&base_project);
    // Base inutilisable : le calendrier désigné mesure.
    let empty = Calendar::new_base(
        uid(BASE),
        "Vide".to_owned(),
        Week::from_fn(|_| DaySetting::NonWorking),
    );
    let unusable_base = Net::new("2026-10-19T08:00")
        .project(|p| {
            p.calendars.push(empty).unwrap();
            p.calendars.push(derived(BASE)).unwrap();
            p.calendar = uid(DERIVED);
        })
        .task("A", 240)
        .task("C", 480)
        .build();
    let t = unusable_base.tasks.iter().next().unwrap();
    assert_eq!(measure_calendar(&unusable_base, t), Some(uid(DERIVED)));
    assert_matches_engine(&unusable_base);
    // Calendrier du projet inutilisable : repli sur le Standard intégré.
    let mut broken = base_project.clone();
    broken.calendar = uid(98);
    let t = broken.tasks.iter().next().unwrap();
    assert_eq!(measure_calendar(&broken, t), None);
}
