//! Règles de l'import `.mpp` (R-281 à R-291, `docs/scheduling-rules.md`),
//! sur des fichiers synthétiques construits d'après `docs/formats/mpp.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit paniquer"
)]

mod common;

use common::{C, Mpp, NA, T, day, exception, stamp, utf16, week};
use plannercraft_core::calendar::model::{CalendarKind, DaySetting};
use plannercraft_core::{
    ConstraintKind, DurationUnit, LagValue, LinkType, Minutes, ScheduleAnchor, SchedulingMode, TaskUid,
    TimeBasis, Weekday, schedule,
};
use plannercraft_mpp::{FileFormat, Layout, MppCode, MppError, compare, read};

const LAYOUTS: [Layout; 2] = [Layout::Project2010, Layout::Project2013];

fn has(doc: &plannercraft_mpp::MppDocument, code: MppCode) -> bool {
    doc.diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn r281_container_and_format_detection() {
    // Pas un fichier composé.
    assert_eq!(
        read(b"<Project/>", &common::options()).unwrap_err(),
        MppError::NotCompound
    );
    assert_eq!(read(&[], &common::options()).unwrap_err(), MppError::NotCompound);
    // Fichier composé sans dossier de projet.
    let other = common::container(&[("/WordDocument".to_owned(), vec![1, 2, 3])]);
    assert_eq!(
        read(&other, &common::options()).unwrap_err(),
        MppError::NotProject
    );
    // Anciens formats reconnus et refusés, avec leur version.
    for (dir, format) in [
        ("   112", FileFormat::Mpp12),
        ("   19", FileFormat::Mpp9),
        ("   1", FileFormat::Mpp8),
    ] {
        let mut m = Mpp::new(Layout::Project2013);
        m.dir = dir;
        let e = read(&m.bytes(), &common::options()).unwrap_err();
        assert_eq!(e, MppError::Unsupported(format));
        assert_eq!(e.code(), "mpp.unsupported_version");
        assert!(e.message(true).contains(format.versions()));
    }
    // MPP14 lu.
    let doc = read(&Mpp::new(Layout::Project2013).bytes(), &common::options()).unwrap();
    assert_eq!(doc.source.format, FileFormat::Mpp14);
}

#[test]
fn r282_layout_follows_the_saving_version() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let mut t = T::new(7, 1, "Tâche");
        t.duration = 9_600;
        m.tasks.push(t);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        assert_eq!(doc.source.layout, layout);
        let task = doc.project.tasks.get(TaskUid::new(7)).unwrap();
        assert_eq!(task.name, "Tâche");
        assert_eq!(task.duration.minutes(), Minutes::new(960));
    }
    // Version d'enregistrement absente : refus.
    let m = Mpp::new(Layout::Project2013);
    let mut streams = vec![("/   114/Props".to_owned(), common::props(&m.props))];
    streams.push(("/Props14".to_owned(), utf16("pas de version")));
    let e = read(&common::container(&streams), &common::options()).unwrap_err();
    assert_eq!(e, MppError::UnknownLayout);
}

#[test]
fn r283_timestamps() {
    use plannercraft_mpp::time::{Stamp, stamp as decode};
    // 0x206C12C0 : jour 8 300 = 2006-09-21, 4 800 dixièmes = 08:00.
    assert_eq!(
        decode(0x206C_12C0),
        Stamp::At("2006-09-21T08:00".parse().unwrap())
    );
    assert_eq!(
        decode(0x0001_0000),
        Stamp::At("1984-01-01T00:00".parse().unwrap())
    );
    assert_eq!(decode(NA), Stamp::Na);
    assert_eq!(decode(0x0000_12C0), Stamp::Invalid);
    assert_eq!(decode(0x0001_3841), Stamp::Invalid);
    assert_eq!(stamp("2026-01-05T17:00") >> 16, 15_346);
}

#[test]
fn r284_rows_order_null_deleted_and_summaries() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let mut a = T::new(10, 1, "Phase");
        a.duration = 0;
        let mut b = T::new(11, 2, "Lot");
        b.level = 2;
        let mut null = T::new(12, 3, "");
        null.null = true;
        let mut gone = T::new(13, 3, "Supprimée");
        gone.deleted = true;
        let mut c = T::new(14, 4, "Fin");
        c.level = 1;
        // Enregistrements dans le désordre : l'ordre suit les ID.
        m.tasks.extend([c, gone, null, b, a]);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let uids: Vec<u32> = doc.project.tasks.iter().map(|t| t.uid().get()).collect();
        assert_eq!(uids, [10, 11, 12, 14]);
        assert!(doc.project.tasks.get(TaskUid::new(12)).unwrap().is_null);
        assert_eq!(doc.project.summary_task.as_ref().unwrap().name, "Projet");
        // Récapitulative déduite de la hiérarchie : durée calculée (nulle).
        let phase = doc.project.tasks.get(TaskUid::new(10)).unwrap();
        assert_eq!(phase.duration.minutes(), Minutes::ZERO);
        assert_eq!(
            doc.project
                .tasks
                .get(TaskUid::new(11))
                .unwrap()
                .outline_level
                .get(),
            2
        );
        assert!(!has(&doc, MppCode::IdRenumbered));
    }
    // ID non contigus : renumérotés dans l'ordre, signalé.
    let mut m = Mpp::new(Layout::Project2013);
    m.tasks.extend([T::new(5, 9, "B"), T::new(4, 3, "A")]);
    let doc = read(&m.bytes(), &common::options()).unwrap();
    let uids: Vec<u32> = doc.project.tasks.iter().map(|t| t.uid().get()).collect();
    assert_eq!(uids, [4, 5]);
    assert!(has(&doc, MppCode::IdRenumbered));
}

#[test]
fn r285_durations_and_formats() {
    let mut m = Mpp::new(Layout::Project2013);
    let cases: [(u32, i32, u16); 5] = [
        (1, 4_800, 7),
        (2, 4_805, 5),
        (3, 14_400, 8),
        (4, 9_600, 53),
        (5, 2_400, 39),
    ];
    for (uid, tenths, format) in cases {
        let mut t = T::new(uid, uid, "t");
        t.duration = tenths;
        t.format = format;
        m.tasks.push(t);
    }
    let doc = read(&m.bytes(), &common::options()).unwrap();
    let d = |u: u32| doc.project.tasks.get(TaskUid::new(u)).unwrap().duration;
    assert_eq!(
        (d(1).minutes(), d(1).unit(), d(1).basis()),
        (Minutes::new(480), DurationUnit::Day, TimeBasis::Working)
    );
    // 480,5 min → 481 (R-001).
    assert_eq!(
        (d(2).minutes(), d(2).unit()),
        (Minutes::new(481), DurationUnit::Hour)
    );
    assert_eq!(d(3).basis(), TimeBasis::Elapsed);
    // Format nul estimé (53) : unité par défaut du projet, estimée.
    assert_eq!((d(4).unit(), d(4).is_estimated()), (DurationUnit::Day, true));
    assert!(d(5).is_estimated());
}

#[test]
fn r286_links() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        for u in 1..=4 {
            m.tasks.push(T::new(u, u, "t"));
        }
        m.links = vec![
            (1, 2, 1, 0, 7),
            (2, 3, 3, 4_800, 7),
            (3, 4, 0, -2_400, 5),
            (1, 4, 2, 0, 19),
            (1, 4, 1, 0, 7),
            (9, 4, 1, 0, 7),
            (2, 2, 1, 0, 7),
        ];
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let p = |u: u32| {
            doc.project
                .tasks
                .get(TaskUid::new(u))
                .unwrap()
                .predecessors
                .clone()
        };
        assert_eq!(p(2)[0].kind, LinkType::FinishToStart);
        assert_eq!(p(3)[0].kind, LinkType::StartToStart);
        assert_eq!(
            p(3)[0].lag.value,
            LagValue::Time {
                minutes: Minutes::new(480),
                unit: DurationUnit::Day
            }
        );
        let into4 = p(4);
        assert_eq!(into4.len(), 2, "{into4:?}");
        assert_eq!(into4[0].kind, LinkType::FinishToFinish);
        assert_eq!(
            into4[0].lag.value,
            LagValue::Time {
                minutes: Minutes::new(-240),
                unit: DurationUnit::Hour
            }
        );
        // Décalage en pourcentage : lien gardé, décalage nul, signalé.
        assert_eq!(into4[1].kind, LinkType::StartToFinish);
        assert!(has(&doc, MppCode::LagPercentUnread));
        // Doublon, tâche absente, lien vers soi : ignorés.
        assert!(has(&doc, MppCode::LinkDuplicate));
        assert!(has(&doc, MppCode::LinkIgnored));
    }
}

#[test]
fn r287_calendars_week_exceptions_and_recurring() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let short = [(480, 720)];
        let blob = week(
            [
                day(Some(&[])),
                day(Some(&short)),
                day(None),
                day(None),
                day(None),
                day(None),
                day(None),
            ],
            &[
                exception("2026-12-25", "2026-12-25", &[], "Noël", false),
                exception("2026-08-03", "2026-08-07", &[(540, 600)], "Été", false),
                exception("2027-01-01", "2027-01-01", &[], "Périodique", true),
                exception("2027-05-01", "2027-05-01", &[], "Jamais lue", false),
            ],
        );
        m.calendars.push(C {
            uid: 3,
            base: 0,
            name: "Chantier",
            data: Some(blob),
        });
        m.calendars.push(C {
            uid: 4,
            base: 1,
            name: "Ressource",
            data: None,
        });
        m.props.retain(|(k, _)| *k != 0x0240_000E);
        m.props.push((0x0240_000E, utf16("Chantier")));
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let cal = doc
            .project
            .calendars
            .get(plannercraft_core::CalendarUid::new(3))
            .unwrap();
        assert_eq!(doc.project.calendar, cal.uid());
        let CalendarKind::Base { week } = cal.kind() else {
            panic!()
        };
        assert_eq!(*week.get(Weekday::Sunday), DaySetting::NonWorking);
        assert_eq!(week.get(Weekday::Monday).total(), Minutes::new(240));
        // Jour « par défaut » : celui du calendrier Standard.
        assert_eq!(week.get(Weekday::Tuesday).total(), Minutes::new(480));
        assert_eq!(*week.get(Weekday::Saturday), DaySetting::NonWorking);
        let ex = cal.exceptions();
        assert_eq!(ex.len(), 2);
        assert_eq!(
            (ex[0].name.as_str(), ex[0].setting.total()),
            ("Noël", Minutes::ZERO)
        );
        assert_eq!(
            (ex[1].name.as_str(), ex[1].setting.total()),
            ("Été", Minutes::new(60))
        );
        // Exception périodique : elle et les suivantes non lues, signalé.
        assert!(has(&doc, MppCode::CalendarRecurringSkipped));
        // Calendrier dérivé (de ressource) : compté, pas repris.
        assert!(
            doc.project
                .calendars
                .get(plannercraft_core::CalendarUid::new(4))
                .is_none()
        );
        assert_eq!(doc.source.not_imported.resource_calendars, 1);
    }
}

#[test]
fn r288_project_properties() {
    let m = Mpp::new(Layout::Project2010);
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromStart("2026-01-05T08:00".parse().unwrap())
    );
    assert_eq!(doc.project.info.title, "Projet synthétique");
    assert_eq!(doc.project.options.factors.minutes_per_day(), 480);
    // Planifier à rebours : les deux clés valent 0.
    let mut m = Mpp::new(Layout::Project2013);
    for (k, v) in &mut m.props {
        if *k == 0x0240_0004 || *k == 0x0240_0019 {
            *v = 0_u16.to_le_bytes().to_vec();
        }
    }
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromFinish("2026-01-30T17:00".parse().unwrap())
    );
    // Une seule des deux à 0 : depuis le début.
    let mut m = Mpp::new(Layout::Project2013);
    for (k, v) in &mut m.props {
        if *k == 0x0240_0004 {
            *v = 0_u16.to_le_bytes().to_vec();
        }
    }
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert!(matches!(doc.project.anchor, ScheduleAnchor::FromStart(_)));
    // Calendrier du projet introuvable : premier calendrier, signalé.
    let mut m = Mpp::new(Layout::Project2013);
    m.props.retain(|(k, _)| *k != 0x0240_000E);
    m.props.push((0x0240_000E, utf16("Inconnu")));
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert!(has(&doc, MppCode::ProjectCalendarFallback));
}

#[test]
fn r289_constraints_deadline_notes_milestone() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let mut a = T::new(1, 1, "SNET");
        a.constraint = (4, stamp("2026-01-12T08:00"));
        a.deadline = stamp("2026-02-27T17:00");
        a.notes = Some(br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\pard Bonjour\par}");
        let mut b = T::new(2, 2, "Jalon");
        b.duration = 0;
        b.milestone = true;
        b.constraint = (2, NA);
        m.tasks.extend([a, b]);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let a = doc.project.tasks.get(TaskUid::new(1)).unwrap();
        assert_eq!(a.constraint.kind(), ConstraintKind::StartNoEarlierThan);
        assert_eq!(a.constraint.date(), Some("2026-01-12T08:00".parse().unwrap()));
        assert_eq!(a.deadline, Some("2026-02-27T17:00".parse().unwrap()));
        assert_eq!(a.notes, "Bonjour");
        let b = doc.project.tasks.get(TaskUid::new(2)).unwrap();
        assert!(b.milestone);
        // Contrainte datée sans date : « Dès que possible », signalé.
        assert_eq!(b.constraint.kind(), ConstraintKind::AsSoonAsPossible);
        assert!(has(&doc, MppCode::ConstraintWithoutDate));
    }
}

#[test]
fn r290_manual_mode_and_active_flag() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let mut a = T::new(1, 1, "Manuelle");
        a.manual = Some((stamp("2026-01-07T08:00"), stamp("2026-01-09T17:00"), 14_400));
        let mut b = T::new(2, 2, "Inactive");
        b.active = false;
        m.tasks.extend([a, b]);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let a = doc.project.tasks.get(TaskUid::new(1)).unwrap();
        let SchedulingMode::Manual {
            start,
            finish,
            duration,
        } = &a.mode
        else {
            panic!("{:?}", a.mode)
        };
        assert_eq!(*start, Some("2026-01-07T08:00".parse().unwrap()));
        assert_eq!(*finish, Some("2026-01-09T17:00".parse().unwrap()));
        assert_eq!(duration.unwrap().minutes(), Minutes::new(1_440));
        let b = doc.project.tasks.get(TaskUid::new(2)).unwrap();
        assert!(!b.active);
        assert_eq!(b.mode, SchedulingMode::Auto);
    }
}

#[test]
fn r291_saved_dates_compared_with_the_engine() {
    let mut m = Mpp::new(Layout::Project2013);
    let mut a = T::new(1, 1, "A");
    a.start = stamp("2026-01-05T08:00");
    a.finish = stamp("2026-01-05T17:00");
    let mut b = T::new(2, 2, "B");
    // Enregistrée un jour trop tard : écart listé, rien n'est corrigé.
    b.start = stamp("2026-01-07T08:00");
    b.finish = stamp("2026-01-07T17:00");
    m.tasks.extend([a, b]);
    m.links = vec![(1, 2, 1, 0, 7)];
    let doc = read(&m.bytes(), &common::options()).unwrap();
    let s = schedule(&doc.project);
    let c = compare(&doc.project, &s, &doc.recorded);
    assert_eq!(
        (c.tasks_compared, c.tasks_identical, c.tasks_different()),
        (2, 1, 1)
    );
    assert_eq!(c.gaps.len(), 2);
    assert_eq!(c.gaps[0].field, "start");
    assert_eq!(c.gaps[0].minutes, -1_440);
    // Les dates enregistrées ne vont jamais dans le projet (règle 3).
    let b = doc.project.tasks.get(TaskUid::new(2)).unwrap();
    assert_eq!(b.constraint.kind(), ConstraintKind::AsSoonAsPossible);
}
