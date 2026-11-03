//! R-142 — Prédécesseurs en texte : liste séparée par le séparateur de
//! liste (`;` en français, `,` en anglais) ; élément `id [type] [décalage]` ;
//! types FS/FD, SS/DD, FF, SF/DF ; écriture « 3 » pour un FS sans décalage,
//! sinon le code de la langue puis le décalage signé. Options 480 / 2 400 / 20.

use plannercraft_automation::text::{
    Language, PredecessorRef, TextContext, TextError, format_predecessor, format_predecessors,
    parse_predecessors,
};
use plannercraft_core::{
    BuiltinCalendarNames, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Project, ProjectInit,
    ProjectOptions, ScheduleAnchor, SignedPercent, Task, TaskUid, TimeBasis,
};
use plannercraft_tests::moment;

fn ctx(language: Language) -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), language)
}

fn days(minutes: i64) -> Lag {
    Lag::new(
        TimeBasis::Working,
        LagValue::Time {
            minutes: Minutes::new(minutes),
            unit: DurationUnit::Day,
        },
    )
    .unwrap()
}

fn r(id: u32, kind: LinkType, lag: Lag) -> PredecessorRef {
    PredecessorRef { id, kind, lag }
}

#[test]
fn r142_french_and_english_lists() {
    // « 3FD+2j;5DD » : 3 fin à début + 2 j (960 min), 5 début à début.
    let expected = vec![
        r(3, LinkType::FinishToStart, days(960)),
        r(5, LinkType::StartToStart, Lag::ZERO),
    ];
    assert_eq!(
        parse_predecessors("3FD+2j;5DD", &ctx(Language::Fr)),
        (expected.clone(), vec![])
    );
    // Même liste en anglais, séparateur `,`.
    assert_eq!(
        parse_predecessors("3FS+2d,5SS", &ctx(Language::En)),
        (expected, vec![])
    );
}

#[test]
fn r142_types_and_lags() {
    let fr = ctx(Language::Fr);
    let one = |s: &str| {
        let (refs, errors) = parse_predecessors(s, &fr);
        assert_eq!(errors, vec![], "{s}");
        assert_eq!(refs.len(), 1, "{s}");
        refs[0]
    };
    // « 4FF-1j » : fin à fin, avance d'un jour (−480).
    assert_eq!(one("4FF-1j"), r(4, LinkType::FinishToFinish, days(-480)));
    // « 2DF » : début à fin ; codes en minuscules admis.
    assert_eq!(one("2DF"), r(2, LinkType::StartToFinish, Lag::ZERO));
    assert_eq!(one("2sf"), r(2, LinkType::StartToFinish, Lag::ZERO));
    // ID seul : fin à début sans décalage (S38).
    assert_eq!(one(" 12 "), r(12, LinkType::FinishToStart, Lag::ZERO));
    // « 3 FS + 50% » : espaces permises, +50 % = 5 000 (1/10 000).
    let half = Lag::new(
        TimeBasis::Working,
        LagValue::Percent {
            percent: SignedPercent::new(5_000).unwrap(),
        },
    )
    .unwrap();
    assert_eq!(one("3 FS + 50%"), r(3, LinkType::FinishToStart, half));
    // Décalage sans type : fin à début.
    assert_eq!(one("3+2j"), r(3, LinkType::FinishToStart, days(960)));
}

#[test]
fn r142_refusals_keep_the_other_elements() {
    let fr = ctx(Language::Fr);
    assert_eq!(
        parse_predecessors("C:\\a.mpp\\3FF", &fr),
        (vec![], vec![TextError::ExternalPredecessor])
    );
    assert_eq!(
        parse_predecessors("3XY", &fr),
        (
            vec![],
            vec![TextError::BadLinkType {
                code: "XY".to_owned()
            }]
        )
    );
    // Un élément illisible n'empêche pas les autres ; un élément vide est ignoré.
    let (refs, errors) = parse_predecessors("2;;x;4FF+1z;5", &fr);
    assert_eq!(
        refs,
        vec![
            r(2, LinkType::FinishToStart, Lag::ZERO),
            r(5, LinkType::FinishToStart, Lag::ZERO)
        ]
    );
    assert_eq!(
        errors,
        vec![
            TextError::BadNumber { at: 0 },
            TextError::UnknownUnit { unit: "z".to_owned() }
        ]
    );
    assert_eq!(parse_predecessors("", &fr), (vec![], vec![]));
}

#[test]
fn r142_writing() {
    let fr = ctx(Language::Fr);
    let en = ctx(Language::En);
    // FS sans décalage → « 3 » ; SS +1 j vers l'ID 7 → « 7DD+1j » (« 7SS+1d »).
    assert_eq!(
        format_predecessor(&r(3, LinkType::FinishToStart, Lag::ZERO), &fr),
        "3"
    );
    let ss = r(7, LinkType::StartToStart, days(480));
    assert_eq!(format_predecessor(&ss, &fr), "7DD+1j");
    assert_eq!(format_predecessor(&ss, &en), "7SS+1d");
    assert_eq!(
        format_predecessor(&r(4, LinkType::FinishToFinish, Lag::ZERO), &fr),
        "4FF"
    );
    assert_eq!(
        format_predecessor(&r(2, LinkType::StartToFinish, days(-480)), &fr),
        "2DF-1j"
    );
    assert_eq!(
        format_predecessor(&r(3, LinkType::FinishToStart, days(960)), &en),
        "3FS+2d"
    );
}

#[test]
fn r142_task_predecessors_by_position() {
    // Tâches d'UID 10, 20, 30 aux ID 1, 2, 3 ; la 3 a pour prédécesseurs
    // 10 (FS), 20 (SS +1 j) et l'UID 99, absent du projet (omis, rendu).
    let mut p = Project::new(ProjectInit {
        name: "P".to_owned(),
        anchor: ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        current_date: moment("2026-10-17T08:00"),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let o = p.options;
    for uid in [10, 20, 30] {
        p.tasks
            .push(Task::new(TaskUid::new(uid), format!("T{uid}"), &o))
            .unwrap();
    }
    let t = p.tasks.get_mut(TaskUid::new(30)).unwrap();
    t.predecessors = vec![
        Link::finish_to_start(TaskUid::new(10)),
        Link {
            predecessor: TaskUid::new(20),
            kind: LinkType::StartToStart,
            lag: days(480),
        },
        Link::finish_to_start(TaskUid::new(99)),
    ];
    let task = p.tasks.get(TaskUid::new(30)).unwrap();
    assert_eq!(
        format_predecessors(task, &p, &ctx(Language::Fr)),
        ("1;2DD+1j".to_owned(), vec![TaskUid::new(99)])
    );
    assert_eq!(
        format_predecessors(task, &p, &ctx(Language::En)),
        ("1,2SS+1d".to_owned(), vec![TaskUid::new(99)])
    );
}

#[test]
fn r142_english_accepts_semicolons() {
    // En anglais, `;` sépare aussi les éléments (pas d'ambiguïté avec le
    // point décimal) : « 3;5 » → deux liens, comme « 3,5 ». Le français
    // garde `;` seul (la virgule y est décimale : « 3FD+1,5j » est un seul
    // élément).
    let fs = |id| r(id, LinkType::FinishToStart, Lag::ZERO);
    let en = ctx(Language::En);
    assert_eq!(parse_predecessors("3;5", &en), (vec![fs(3), fs(5)], vec![]));
    assert_eq!(parse_predecessors("3,5", &en), (vec![fs(3), fs(5)], vec![]));
    assert_eq!(
        parse_predecessors("3FS+2d; 5SS,7", &en),
        (
            vec![
                r(3, LinkType::FinishToStart, days(960)),
                r(5, LinkType::StartToStart, Lag::ZERO),
                fs(7)
            ],
            vec![]
        )
    );
    let fr = ctx(Language::Fr);
    assert_eq!(parse_predecessors("3;5", &fr), (vec![fs(3), fs(5)], vec![]));
    assert_eq!(
        parse_predecessors("3FD+1,5j", &fr),
        (vec![r(3, LinkType::FinishToStart, days(720))], vec![])
    );
}
