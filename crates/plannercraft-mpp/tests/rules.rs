//! Règles de l'import `.mpp` (R-281 à R-291, `docs/scheduling-rules.md`),
//! sur des fichiers synthétiques construits d'après `docs/formats/mpp.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::many_single_char_names,
    reason = "tests : un échec doit paniquer"
)]

mod common;

use common::{C, Mpp, NA, T, day, exception, exception_rec, stamp, utf16, week};
use plannercraft_core::calendar::model::{CalendarKind, DaySetting, Recurrence};
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
    // Secondes tronquées (R-004) : 4 805 et 4 809 dixièmes ⇒ 08:00.
    for raw in [0x206C_12C5, 0x206C_12C9] {
        assert_eq!(decode(raw), Stamp::At("2006-09-21T08:00".parse().unwrap()));
    }
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
                exception("2026-12-25", "2026-12-25", &[], "Noël"),
                exception("2026-08-03", "2026-08-07", &[(540, 600)], "Été"),
                // 1er mai chaque année, 5 occurrences (type 2 : mois 4, jour 1).
                exception_rec(
                    "2027-05-01",
                    "2031-05-01",
                    &[],
                    "Fête du travail",
                    2,
                    [4, 1, 0, 0],
                    Some(5),
                ),
                // Type inconnu : ignorée, la suivante est lue.
                exception_rec("2027-01-01", "2027-01-01", &[], "Inconnue", 9, [0; 4], None),
                // Un vendredi sur deux, 3 occurrences (type 6 : masque 32, tous les 2).
                exception_rec(
                    "2027-06-04",
                    "2027-07-02",
                    &[],
                    "Vendredis",
                    6,
                    [32, 0, 2, 0],
                    Some(3),
                ),
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
        assert_eq!(ex.len(), 4);
        assert_eq!(
            (ex[0].name.as_str(), ex[0].setting.total()),
            ("Noël", Minutes::ZERO)
        );
        assert_eq!(
            (ex[1].name.as_str(), ex[1].setting.total()),
            ("Été", Minutes::new(60))
        );
        // Exceptions périodiques lues avec leur récurrence et leurs occurrences.
        assert_eq!(
            ex[2].recurrence,
            Recurrence::YearlyByDay {
                month: plannercraft_core::Month::May,
                day: 1
            }
        );
        assert_eq!(ex[2].occurrences, Some(5));
        assert_eq!(
            plannercraft_core::calendar::expand_exception(&ex[2])
                .unwrap()
                .len(),
            5
        );
        assert!(matches!(ex[3].recurrence, Recurrence::Weekly { every: 2, .. }));
        // Type inconnu : ignorée, signalée.
        assert!(has(&doc, MppCode::CalendarExceptionUnknownType));
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
fn r287_default_days_follow_the_project_default_week() {
    for layout in LAYOUTS {
        // Semaine par défaut du projet (0x02401388) : les jours « par
        // défaut » la suivent, pas le calendrier Standard.
        let mut m = Mpp::new(layout);
        let short = [(480, 720)];
        let split = [(540, 720), (840, 1_080)];
        let default_days = [
            day(Some(&[])),
            day(Some(&split)),
            day(Some(&split)),
            day(Some(&split)),
            day(Some(&split)),
            day(Some(&split)),
            day(Some(&[])),
        ]
        .concat();
        m.props.push((0x0240_1388, default_days));
        m.calendars[0].data = Some(week(
            [
                day(Some(&[])),
                day(Some(&short)),
                day(None),
                day(None),
                day(None),
                day(None),
                day(None),
            ],
            &[],
        ));
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let cal = doc
            .project
            .calendars
            .get(plannercraft_core::CalendarUid::new(1))
            .unwrap();
        let CalendarKind::Base { week } = cal.kind() else {
            panic!()
        };
        assert_eq!(week.get(Weekday::Monday).total(), Minutes::new(240));
        assert_eq!(week.get(Weekday::Tuesday).total(), Minutes::new(420));
    }
}

#[test]
fn r288_project_properties() {
    let mut m = Mpp::new(Layout::Project2010);
    // Valeur de longueur impaire en tête : l'octet de bourrage est sauté et
    // les propriétés suivantes restent lues.
    m.props.insert(0, (0x0240_000B, vec![1, 2, 3]));
    m.props
        .push((0x0240_0045, stamp("2026-01-16T17:00").to_le_bytes().to_vec()));
    m.props.push((0x0240_0025, 1_u16.to_le_bytes().to_vec()));
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromStart("2026-01-05T08:00".parse().unwrap())
    );
    assert_eq!(doc.project.info.title, "Projet synthétique");
    assert_eq!(doc.project.options.factors.minutes_per_day(), 480);
    assert_eq!(doc.project.status_date, Some("2026-01-16T17:00".parse().unwrap()));
    assert_eq!(doc.project.options.week_start_day, Weekday::Monday);
    assert!(!has(&doc, MppCode::PropertyMissing));
    // Planifier à rebours : la clé 0x02400004 vaut 0.
    let mut m = Mpp::new(Layout::Project2013);
    for (k, v) in &mut m.props {
        if *k == 0x0240_0004 {
            *v = 0_u16.to_le_bytes().to_vec();
        }
    }
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert_eq!(
        doc.project.anchor,
        ScheduleAnchor::FromFinish("2026-01-30T17:00".parse().unwrap())
    );
    // 0x02400019 à 0 (« la tâche met à jour la ressource ») : sans effet.
    let mut m = Mpp::new(Layout::Project2013);
    for (k, v) in &mut m.props {
        if *k == 0x0240_0019 {
            *v = 0_u16.to_le_bytes().to_vec();
        }
    }
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert!(matches!(doc.project.anchor, ScheduleAnchor::FromStart(_)));
    assert!(!doc.project.options.progress.task_updates_resource);
    assert!(doc.project.options.honor_constraints);
    // Options de calcul : fractionner les tâches en cours (0x0240001A) ;
    // respecter les contraintes : à 0 seulement si les deux clés valent 0.
    let mut m = Mpp::new(Layout::Project2013);
    m.props.push((0x0240_001A, 0_u16.to_le_bytes().to_vec()));
    m.props.push((0x0240_0038, 0_u16.to_le_bytes().to_vec()));
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert!(!doc.project.options.progress.split_in_progress);
    assert!(doc.project.options.honor_constraints);
    m.props.push((0x0240_13CD, 0_u16.to_le_bytes().to_vec()));
    let doc = read(&m.bytes(), &common::options()).unwrap();
    assert!(!doc.project.options.honor_constraints);
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

        // Réels, reprise et retard de nivellement enregistrés.
        let mut m = Mpp::new(layout);
        let mut c = T::new(1, 1, "Commencée");
        c.actual = (stamp("2026-01-05T08:00"), NA, 2_400);
        c.resume = stamp("2026-01-05T12:00");
        c.percent = 50;
        c.leveling = (4_800, 7);
        let mut d = T::new(2, 2, "Achevée");
        d.actual = (stamp("2026-01-05T08:00"), stamp("2026-01-05T17:00"), 4_800);
        d.percent = 100;
        d.resume = stamp("2026-01-05T13:00");
        m.tasks.extend([c, d]);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        let c = doc.project.tasks.get(TaskUid::new(1)).unwrap();
        assert_eq!(c.progress.actual_start, Some("2026-01-05T08:00".parse().unwrap()));
        assert_eq!(c.progress.actual_finish, None);
        assert_eq!(c.progress.actual_duration, Minutes::new(240));
        assert_eq!(c.progress.resume, Some("2026-01-05T12:00".parse().unwrap()));
        let delay = c.leveling_delay.unwrap();
        assert_eq!((delay.minutes, delay.elapsed), (Minutes::new(480), false));
        let d = doc.project.tasks.get(TaskUid::new(2)).unwrap();
        assert_eq!(
            d.progress.actual_finish,
            Some("2026-01-05T17:00".parse().unwrap())
        );
        // Reprise d'une tâche achevée : sans objet, non reprise.
        assert_eq!(d.progress.resume, None);
        assert!(d.leveling_delay.is_none());
    }
}

#[test]
fn r290_manual_mode_and_active_flag() {
    for layout in LAYOUTS {
        let mut m = Mpp::new(layout);
        let mut a = T::new(1, 1, "Manuelle");
        a.manual = Some((stamp("2026-01-07T08:00"), stamp("2026-01-09T17:00"), 14_400));
        // Durée de l'enregistrement fixe différente : la durée saisie fait foi.
        a.fixed_duration = Some(27_600);
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
        assert_eq!(a.duration.minutes(), Minutes::new(1_440));
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

#[test]
fn r292_inconsistent_task_table_is_refused() {
    use plannercraft_mpp::project::refuse_inconsistent;
    // Seuils : aucune tâche lue sur des enregistrements en service ; ou au
    // moins deux incohérents et au moins un quart.
    assert!(refuse_inconsistent(5, 0, false));
    assert!(!refuse_inconsistent(0, 0, false));
    assert!(refuse_inconsistent(8, 2, true));
    assert!(!refuse_inconsistent(9, 2, true));
    assert!(!refuse_inconsistent(4, 1, true));
    for layout in LAYOUTS {
        // Dates invalides en série (jour 0) : disposition non reconnue.
        let mut m = Mpp::new(layout);
        for u in 1..=4 {
            let mut t = T::new(u, u, "t");
            t.start = 0x0000_12C0;
            t.finish = 0x0000_27D8;
            m.tasks.push(t);
        }
        let e = read(&m.bytes(), &common::options()).unwrap_err();
        assert_eq!(e.code(), "mpp.layout_unrecognized");
        assert!(e.message(true).contains(match layout {
            Layout::Project2010 => "14,0,7011,1000",
            Layout::Project2013 => "16,0,10730,20102",
        }));
        // Une seule tâche incohérente sur dix : lue, signalée.
        let mut m = Mpp::new(layout);
        for u in 1..=9 {
            m.tasks.push(T::new(u, u, "t"));
        }
        let mut bad = T::new(10, 10, "t");
        bad.start = 0x0000_12C0;
        m.tasks.push(bad);
        let doc = read(&m.bytes(), &common::options()).unwrap();
        assert_eq!(doc.source.inconsistent_records, 1);
        assert!(has(&doc, MppCode::DateInvalid));
    }
}

#[test]
fn r284_meta_count_limits_slots_and_known_item_size() {
    use plannercraft_mpp::tables::{ItemLen, fixed_items};
    // Quatre emplacements de 47 octets, deux en service (en-tête) : le
    // nombre ne divise pas le corps, la taille connue s'applique.
    let mut meta = vec![0_u8; 16];
    meta[8] = 2;
    let data = vec![0_u8; 64];
    for k in 0..4_u32 {
        let mut item = vec![0_u8; 47];
        item[4..8].copy_from_slice(&(k * 16).to_le_bytes());
        meta.extend_from_slice(&item);
    }
    meta[8] = 3;
    let items = fixed_items(&meta, &data, 100, ItemLen::Known(47));
    assert_eq!(items.len(), 3);
    // Deux emplacements vers le même enregistrement : le dernier fait foi.
    let mut meta2 = meta.clone();
    meta2[16 + 47 + 4..16 + 47 + 8].copy_from_slice(&0_u32.to_le_bytes());
    let items = fixed_items(&meta2, &data, 100, ItemLen::Known(47));
    assert!(items[0].deleted() && !items[1].deleted());
}
