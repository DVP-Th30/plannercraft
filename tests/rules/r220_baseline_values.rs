//! R-220 — Ligne de base : valeurs copiées. Tâche planifiée : début, fin,
//! durée calculée du moteur (`TaskSchedule::duration`, unité et estimation
//! de la tâche), travail calculé (`TaskSchedule::work`) ; affectation :
//! `span`, travail ; ressource : `ResourceLoad::work` ; récapitulatives et
//! UID 0 compris. Source : S57 (« the date in the task's Start field at
//! that point in time is copied ») ; durée, travail, affectations :
//! hypothèse.
//!
//! Le module porte aussi les aides des tests des lignes de base (R-220 à
//! R-225).

use plannercraft_core::baseline::{self, BaselineEdit, BaselineSpec, Rollup, Scope};
use plannercraft_core::{
    AssignmentBaseline, AssignmentRole, AssignmentUid, BaselineNumber, Duration, DurationUnit, Minutes,
    Project, ResourceBaseline, ResourceUid, Schedule, Task, TaskBaseline, TaskUid, TimeBasis, Work,
};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r171_assignment_effective_calendar::{assign, resource};

/// Projet et planning d'un réseau de test.
pub(crate) fn plan(net: Net) -> (Project, Schedule) {
    let p = net.build();
    let s = plannercraft_core::schedule(&p);
    (p, s)
}

/// Ajoute la récapitulative du projet (UID 0) au modèle, comme un fichier
/// MSPDI.
pub(crate) fn with_root(net: Net) -> Net {
    net.project(|p| {
        let mut root = Task::new(TaskUid::PROJECT_SUMMARY, "Projet".to_owned(), &p.options);
        root.duration = Duration::zero(DurationUnit::Day);
        p.summary_task = Some(root);
    })
}

/// Numéro de ligne de base.
pub(crate) fn number(n: u8) -> BaselineNumber {
    BaselineNumber::new(n).unwrap()
}

/// Demande de capture.
pub(crate) fn spec(n: u8, scope: Scope, rollup: Rollup, overwrite: bool) -> BaselineSpec {
    BaselineSpec {
        number: number(n),
        scope,
        rollup,
        overwrite,
    }
}

/// Capture puis écrit la ligne ; rend la capture.
pub(crate) fn set(p: &mut Project, s: &Schedule, spec: &BaselineSpec) -> BaselineEdit {
    let edit = baseline::capture(p, s, spec).unwrap();
    let _ = baseline::apply(p, spec.number, edit.clone());
    edit
}

/// Durée en jours ouvrés (unité des tâches de `Net`), non estimée.
pub(crate) fn days(minutes: i64) -> Duration {
    Duration::new(
        Minutes::new(minutes),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap()
}

/// Travail en minutes.
pub(crate) fn work(minutes: i64) -> Work {
    Work::new(Minutes::new(minutes)).unwrap()
}

/// Ligne de tâche sans coût ; travail nul : absent (O42).
pub(crate) fn line(start: &str, finish: &str, duration: i64, w: i64) -> TaskBaseline {
    TaskBaseline {
        start: Some(moment(start)),
        finish: Some(moment(finish)),
        duration: Some(days(duration)),
        work: (w != 0).then(|| work(w)),
        cost: None,
        fixed_cost: None,
    }
}

#[test]
fn r220_copies_engine_values() {
    // T 5 j (2 400) lun 19/10 08:00 – ven 23/10 17:00, R1 à 100 % pour
    // 2 400 min ⇒ ligne 0 { lun 08:00, ven 17:00, 2 400 (j), 2 400 } ;
    // affectation et ressource : mêmes dates, 2 400. Travail nul : absent,
    // comme MS Project (O42).
    let net = assign(
        resource(Net::new("2026-10-19T08:00").task("T", 2_400), 1, None),
        1,
        "T",
        1,
        2_400,
    );
    let t = net.uid("T");
    let (mut p, s) = plan(net);
    let edit = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    let expected = line("2026-10-19T08:00", "2026-10-23T17:00", 2_400, 2_400);
    assert_eq!(edit.tasks.get(&t), Some(&expected));
    assert_eq!(
        edit.assignments.get(&AssignmentUid::new(1)),
        Some(&AssignmentBaseline {
            start: Some(moment("2026-10-19T08:00")),
            finish: Some(moment("2026-10-23T17:00")),
            work: Some(work(2_400)),
            cost: None,
        })
    );
    assert_eq!(
        edit.resources.get(&ResourceUid::new(1)),
        Some(&ResourceBaseline {
            work: Some(work(2_400)),
            cost: None,
        })
    );
    assert!(edit.skipped.is_empty() && !edit.cost_copied);
    // Écrite dans le modèle.
    assert_eq!(p.tasks.get(t).unwrap().baselines.get(&number(0)), Some(&expected));
    assert_eq!(
        p.resources
            .get(ResourceUid::new(1))
            .unwrap()
            .baselines
            .get(&number(0))
            .and_then(|b| b.work),
        Some(work(2_400))
    );
}

#[test]
fn r220_summary_computed_duration() {
    // S {T 5 j, U 2 j FS} : S lun 19/10 08:00 – mar 27/10 17:00, durée
    // calculée 3 360 (7 j), et non la durée du modèle (0) ; l'UID 0 de même.
    let net = with_root(
        Net::new("2026-10-19T08:00")
            .task("S", 0)
            .task("T", 2_400)
            .level("T", 2)
            .task("U", 960)
            .level("U", 2)
            .fs("T", "U"),
    );
    let s_uid = net.uid("S");
    let (mut p, s) = plan(net);
    let edit = set(&mut p, &s, &spec(0, Scope::Project, Rollup::None, false));
    let expected = line("2026-10-19T08:00", "2026-10-27T17:00", 3_360, 0);
    assert_eq!(edit.tasks.get(&s_uid), Some(&expected));
    assert_eq!(edit.tasks.get(&TaskUid::PROJECT_SUMMARY), Some(&expected));
    assert_eq!(
        p.summary_task.as_ref().unwrap().baselines.get(&number(0)),
        Some(&expected)
    );
    // Une durée estimée garde son estimation.
    let net = Net::new("2026-10-19T08:00").task("E", 480).with_task("E", |t| {
        t.duration = Duration::new(Minutes::new(480), DurationUnit::Hour, TimeBasis::Working, true).unwrap();
    });
    let e = net.uid("E");
    let (p, s) = plan(net);
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, false)).unwrap();
    let d = edit.tasks.get(&e).unwrap().duration.unwrap();
    assert_eq!(
        (d.minutes(), d.unit(), d.is_estimated()),
        (Minutes::new(480), DurationUnit::Hour, true)
    );
}

#[test]
fn r220_corpus_a7_capture_matches_recorded() {
    // A7 (fichier du corpus, absent hors du job `corpus`), O42 : la ligne 0
    // relue est égale aux valeurs courantes ; la capture du moteur la
    // redonne (début, fin, durée, travail des tâches et de l'UID 0 ; dates
    // et travail des affectations de ressource, sans travail pour les
    // fictives ; travail des ressources), coûts recopiés.
    let Ok(bytes) = std::fs::read(plannercraft_tests::tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let doc = plannercraft_mspdi::read(
        &bytes,
        &plannercraft_mspdi::ReadOptions::new(moment("2026-01-01T00:00")),
    )
    .unwrap();
    let p = doc.project;
    let s = plannercraft_core::schedule(&p);
    let edit = baseline::capture(&p, &s, &spec(0, Scope::Project, Rollup::None, true)).unwrap();
    assert!(edit.cost_copied);
    let tasks = p.summary_task.iter().chain(p.tasks.iter());
    let mut tasks_seen = 0;
    for task in tasks {
        let Some(recorded) = task.baselines.get(&number(0)) else {
            continue;
        };
        let got = edit.tasks.get(&task.uid()).unwrap();
        assert_eq!(
            (got.start, got.finish, got.work, got.cost),
            (recorded.start, recorded.finish, recorded.work, recorded.cost),
            "UID {}",
            task.uid()
        );
        assert_eq!(
            got.duration.map(|x| x.minutes()),
            recorded.duration.map(|x| x.minutes()),
            "UID {}",
            task.uid()
        );
        tasks_seen += 1;
    }
    assert_eq!(tasks_seen, 26);
    // Affectations : celles qui placent leur tâche (rôle `Work`, R-173) ;
    // les fictives et celles d'une tâche manuelle ne sont pas placées comme
    // MS Project par le moteur (R-176, R-183 : écart hors des lignes de
    // base). Fictives : travail de base absent.
    let mut placed_seen = 0;
    for a in &p.assignments {
        let Some(recorded) = a.baselines.get(&number(0)) else {
            continue;
        };
        if a.resource.is_none() {
            assert_eq!(edit.assignments.get(&a.uid()).unwrap().work, None);
        }
        if s.assignments.get(&a.uid()).unwrap().role != AssignmentRole::Work {
            continue;
        }
        placed_seen += 1;
        let got = edit.assignments.get(&a.uid()).unwrap();
        assert_eq!(
            (got.start, got.finish, got.work),
            (recorded.start, recorded.finish, recorded.work),
            "affectation {}",
            a.uid()
        );
    }
    assert!(placed_seen > 0);
    for r in &p.resources {
        if let Some(recorded) = r.baselines.get(&number(0)) {
            assert_eq!(
                edit.resources.get(&r.uid()).unwrap().work,
                recorded.work,
                "ressource {}",
                r.uid()
            );
        }
    }
}
