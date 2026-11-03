//! R-175 — Tâche sans affectation de travail (R-113 amendée) : une feuille
//! sans affectation de travail et sans calendrier propre se place sur le
//! calendrier du projet, sa base s'il est un calendrier dérivé à base
//! utilisable ; le calendrier désigné ne place plus rien (ses ressources
//! placent les tâches qui leur sont affectées, R-171). La mesure (marges,
//! récapitulatives, décalages) reste celle de R-113. Source : S6 (« The
//! project calendar is used to calculate […] especially when no resources
//! are assigned »), S1 ; O24 (A6 UID 24). Garde : A4 reste à 42 cas
//! identiques sur 42.

use plannercraft_core::CalendarUid;
use plannercraft_tests::corpus::verdict::FileStatus;
use plannercraft_tests::corpus::{manifest, run_file};
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, tests_path};

use crate::r113_derived_project_calendar::{net, staffed};
use crate::r171_assignment_effective_calendar::{L6, with_l6};

#[test]
fn r175_unassigned_on_project_base() {
    // Calendrier du projet L6 (lun-sam), tâche sans ressource de 2 j depuis
    // ven 23/10 08:00 ⇒ ven et sam 24/10 17:00 (A6 UID 24, O24).
    let run = with_l6(Net::new("2026-10-23T08:00"))
        .project(|p| p.calendar = CalendarUid::new(L6))
        .task("A", 960)
        .run();
    assert_eq!(run.at("A").finish, moment("2026-10-24T17:00"));
    // Calendrier désigné dérivé 08:00-17:00 de base 09:00-13:00,
    // 15:00-19:00 : tâche sans ressource de 1 j ⇒ lun 09:00 – 19:00 ; la même
    // tâche affectée à une ressource du calendrier désigné est inchangée
    // (lun 08:00 – 17:00).
    let run = staffed(net().task("A", 480).task("B", 480), &[("B", 480)]).run();
    assert_eq!(run.at("A").start, moment("2026-10-19T09:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-19T19:00"));
    assert_eq!(run.at("B").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("B").finish, moment("2026-10-19T17:00"));
}

#[test]
fn r175_a4_guard() {
    // A4 (`shopmetrics.xml`) : calendrier du projet dérivé, feuilles toutes
    // affectées aux ressources de ce calendrier, jalons sans ressource ⇒ 42
    // cas identiques sur 42 (mesuré avant le commit de R-175).
    let m = manifest::load(&tests_path("corpus/manifest.toml")).unwrap();
    let entry = m.file.iter().find(|e| e.name == "shopmetrics.xml").unwrap();
    let run = run_file(entry, &tests_path("corpus/files"));
    match run.status {
        FileStatus::Compared { cases, identical, .. } => {
            assert_eq!((identical, cases), (42, 42));
        }
        FileStatus::Absent(_) => {
            eprintln!("r175_a4_guard : shopmetrics.xml absent (`cargo xtask corpus fetch`), test ignoré");
        }
        FileStatus::ReadError(e) => panic!("lecture de A4 : {e}"),
    }
}
