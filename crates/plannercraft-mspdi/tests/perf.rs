//! Performance de la lecture (ADR-0009 § 7.8) et de l'écriture (ADR-0010),
//! en `--release`, tests ignorés
//! par défaut :
//! - informative : un MSPDI généré de 5 000 tâches et 10 000 liens, cible
//!   < 100 ms, médiane affichée, aucun seuil ;
//! - coût linéaire des liens (§ 7.5, relecture J2) : une tâche à 199 999
//!   prédécesseurs, surcoût < 1 s, vérifié par le job CI `perf` ;
//! - écriture (J3) : 5 000 tâches et 10 000 liens avec les valeurs du
//!   moteur, échec au-delà de 100 ms (cible 30 ms), job CI `perf` ;
//! - champs personnalisés (seconde passe UX, lot L8, `custom_mspdi_5000`) :
//!   lecture puis écriture du même réseau avec 10 champs personnalisés par
//!   tâche, échec au-delà de 1,5 × la lecture puis l'écriture sans champs.
//!
//! `cargo test -p plannercraft-mspdi --release --test perf -- --ignored --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test de mesure : les interdits de la règle 10 ne s'appliquent pas"
)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use plannercraft_mspdi::{ReadOptions, read};

/// Document MSPDI de `n` tâches (une récapitulative toutes les 10), chaque
/// tâche liée aux deux précédentes, à la façon d'un fichier de MS Project
/// (valeurs calculées comprises).
fn generated(n: u32) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Project xmlns=\"http://schemas.microsoft.com/project\"><SaveVersion>14</SaveVersion>\
         <StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID><Calendars><Calendar>\
         <UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar><WeekDays>\
         <WeekDay><DayType>1</DayType><DayWorking>0</DayWorking></WeekDay>",
    );
    for d in 2..=6 {
        let _ = write!(
            xml,
            "<WeekDay><DayType>{d}</DayType><DayWorking>1</DayWorking><WorkingTimes>\
             <WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime>\
             <WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime>\
             </WorkingTimes></WeekDay>"
        );
    }
    xml.push_str("<WeekDay><DayType>7</DayType><DayWorking>0</DayWorking></WeekDay></WeekDays></Calendar></Calendars><Tasks>");
    for uid in 1..=n {
        let (level, summary) = if uid % 10 == 1 { (1, 1) } else { (2, 0) };
        let _ = write!(
            xml,
            "<Task><UID>{uid}</UID><ID>{uid}</ID><Name>Tâche {uid}</Name><Active>1</Active><Manual>0</Manual>\
             <Type>1</Type><IsNull>0</IsNull><OutlineLevel>{level}</OutlineLevel><Priority>500</Priority>\
             <Start>2026-10-19T08:00:00</Start><Finish>2026-10-20T17:00:00</Finish><Duration>PT16H0M0S</Duration>\
             <DurationFormat>7</DurationFormat><Estimated>0</Estimated><Milestone>0</Milestone><Summary>{summary}</Summary>\
             <Critical>0</Critical><EarlyStart>2026-10-19T08:00:00</EarlyStart><EarlyFinish>2026-10-20T17:00:00</EarlyFinish>\
             <LateStart>2026-10-19T08:00:00</LateStart><LateFinish>2026-10-20T17:00:00</LateFinish>\
             <FreeSlack>0</FreeSlack><TotalSlack>0</TotalSlack><ConstraintType>0</ConstraintType>\
             <CalendarUID>-1</CalendarUID><PercentComplete>0</PercentComplete>"
        );
        for back in [1, 2] {
            if uid > back + 1 {
                let _ = write!(
                    xml,
                    "<PredecessorLink><PredecessorUID>{}</PredecessorUID><Type>1</Type><CrossProject>0</CrossProject>\
                     <LinkLag>0</LinkLag><LagFormat>7</LagFormat></PredecessorLink>",
                    uid - back
                );
            }
        }
        xml.push_str("</Task>");
    }
    xml.push_str("</Tasks></Project>");
    xml
}

#[test]
#[ignore = "mesure informative, à lancer en --release"]
fn read_5000_tasks() {
    let xml = generated(5_000);
    let opts = ReadOptions::new("2026-01-01T00:00".parse().unwrap());
    let doc = read(xml.as_bytes(), &opts).unwrap();
    assert_eq!(doc.project.tasks.len(), 5_000);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics.first());
    let mut times: Vec<Duration> = (0..15)
        .map(|_| {
            let t = Instant::now();
            let _ = read(xml.as_bytes(), &opts).unwrap();
            t.elapsed()
        })
        .collect();
    times.sort();
    println!(
        "lecture MSPDI, 5 000 tâches, {} Kio : médiane {:?} (cible < 100 ms en release)",
        xml.len() / 1024,
        times.get(7)
    );
}

/// Une tâche à 200 000 prédécesseurs (relecture J2 : 36 s avec la détection
/// quadratique des doublons). Mesure : le surcoût des liens par rapport au
/// même document sans liens, moins d'une seconde en release (les 200 000
/// tâches seules se lisent en 0,6 à 0,8 s sur la machine de développement).
#[test]
#[ignore = "mesure, à lancer en --release"]
fn read_200k_predecessors_under_a_second() {
    let n: u32 = 200_000;
    let document = |links: bool| {
        let mut xml = String::from("<Project><StartDate>2026-10-19T08:00:00</StartDate><Tasks>");
        for uid in 1..n {
            let _ = write!(xml, "<Task><UID>{uid}</UID><Duration>PT8H0M0S</Duration></Task>");
        }
        let _ = write!(xml, "<Task><UID>{n}</UID><Duration>PT8H0M0S</Duration>");
        for uid in (1..n).filter(|_| links) {
            let _ = write!(
                xml,
                "<PredecessorLink><PredecessorUID>{uid}</PredecessorUID></PredecessorLink>"
            );
        }
        xml.push_str("</Task></Tasks></Project>");
        xml
    };
    let opts = ReadOptions::new("2026-01-01T00:00".parse().unwrap());
    let median = |xml: &str| {
        let mut times: Vec<Duration> = (0..5)
            .map(|_| {
                let t = Instant::now();
                let doc = read(xml.as_bytes(), &opts).unwrap();
                assert_eq!(doc.project.tasks.len(), 200_000);
                t.elapsed()
            })
            .collect();
        times.sort();
        times[2]
    };
    let with = median(&document(true));
    let without = median(&document(false));
    let extra = with.saturating_sub(without);
    println!(
        "lecture MSPDI, 200 000 tâches : {without:?} sans liens, {with:?} avec 199 999 prédécesseurs \
         sur une tâche (surcoût {extra:?}, limite 1 s en release)"
    );
    assert!(extra < Duration::from_secs(1), "{extra:?}");
}

/// Écriture de 5 000 tâches et 10 000 liens (R-120, ADR-0010 § 9.2) avec les
/// valeurs du moteur : échec au-delà de 100 ms en release, cible 30 ms.
#[test]
#[ignore = "mesure, à lancer en --release"]
fn write_5000_tasks_under_100_ms() {
    use plannercraft_mspdi::write::{Computed, WriteOptions, write};
    let xml = generated(5_000);
    let opts = ReadOptions::new("2026-01-01T00:00".parse().unwrap());
    let doc = read(xml.as_bytes(), &opts).unwrap();
    let schedule = plannercraft_core::schedule(&doc.project);
    let options = WriteOptions::default();
    let mut times: Vec<Duration> = (0..15)
        .map(|_| {
            let t = Instant::now();
            let out = write(&doc.project, Computed::Schedule(&schedule), &options);
            assert_eq!(out.diagnostics, Vec::new());
            t.elapsed()
        })
        .collect();
    times.sort();
    let median = times[7];
    println!(
        "écriture MSPDI, 5 000 tâches et 10 000 liens : médiane {median:?} (limite 100 ms, cible 30 ms)"
    );
    assert!(median < Duration::from_millis(100), "{median:?}");
}

/// Texte1-3, Nombre1-3, Date1-2, Indicateur1-2 (S90) et le type de leur
/// valeur.
const CUSTOM_FIELDS: [(i64, char); 10] = [
    (188_743_731, 't'),
    (188_743_734, 't'),
    (188_743_737, 't'),
    (188_743_767, 'n'),
    (188_743_768, 'n'),
    (188_743_769, 'n'),
    (188_743_945, 'd'),
    (188_743_946, 'd'),
    (188_743_752, 'f'),
    (188_743_753, 'f'),
];

/// Éléments à durée nulle d'une tâche enregistrée par MS Project.
const ZERO_D: [&str; 12] = [
    "Work",
    "OvertimeWork",
    "ActualDuration",
    "ActualWork",
    "ActualOvertimeWork",
    "RegularWork",
    "RemainingWork",
    "RemainingOvertimeWork",
    "WorkVariance",
    "ACWP",
    "CV",
    "BCWS",
];

/// Éléments à 0 d'une tâche enregistrée par MS Project.
const ZERO: [&str; 30] = [
    "ResumeValid",
    "EffortDriven",
    "Recurring",
    "OverAllocated",
    "Estimated",
    "Milestone",
    "DisplayAsSummary",
    "Critical",
    "IsSubproject",
    "IsSubprojectReadOnly",
    "ExternalTask",
    "StartVariance",
    "FinishVariance",
    "FreeSlack",
    "TotalSlack",
    "StartSlack",
    "FinishSlack",
    "FixedCost",
    "PercentComplete",
    "PercentWorkComplete",
    "Cost",
    "OvertimeCost",
    "ActualCost",
    "ActualOvertimeCost",
    "RemainingCost",
    "RemainingOvertimeCost",
    "ConstraintType",
    "IgnoreResourceCalendar",
    "HideBar",
    "Rollup",
];

/// Document de `n` tâches écrit à la façon d'un fichier enregistré par
/// MS Project (indentation, environ 80 éléments par tâche, comme les
/// fichiers du corpus), chaque tâche liée à la précédente ; `custom` :
/// définitions des 10 champs de [`CUSTOM_FIELDS`] (deux avec alias) et 10
/// valeurs par tâche.
fn saved_like(n: u32, custom: bool) -> String {
    let mut xml = generated(0);
    let cut = xml.find("<Tasks>").unwrap();
    xml.truncate(cut);
    if custom {
        xml.push_str("\n\t<ExtendedAttributes>");
        for (i, (id, _)) in CUSTOM_FIELDS.iter().enumerate() {
            let alias = if i < 2 {
                format!("\n\t\t\t<Alias>Champ {i}</Alias>")
            } else {
                String::new()
            };
            let _ = write!(
                xml,
                "\n\t\t<ExtendedAttribute>\n\t\t\t<FieldID>{id}</FieldID>\n\t\t\t<FieldName>F{i}</FieldName>{alias}\n\t\t</ExtendedAttribute>"
            );
        }
        xml.push_str("\n\t</ExtendedAttributes>");
    }
    xml.push_str("\n\t<Tasks>");
    for uid in 1..=n {
        let _ = write!(
            xml,
            "\n\t\t<Task>\n\t\t\t<UID>{uid}</UID>\n\t\t\t<GUID>F1AD89CD-6EE0-48F2-9945-{uid:012}</GUID>\n\t\t\t<ID>{uid}</ID>\n\t\t\t<Name>Tâche {uid}</Name>\n\t\t\t<Active>1</Active>\n\t\t\t<Manual>0</Manual>\n\t\t\t<Type>0</Type>\n\t\t\t<IsNull>0</IsNull>\n\t\t\t<CreateDate>2026-10-01T08:00:00</CreateDate>\n\t\t\t<WBS>{uid}</WBS>\n\t\t\t<OutlineNumber>{uid}</OutlineNumber>\n\t\t\t<OutlineLevel>1</OutlineLevel>\n\t\t\t<Priority>500</Priority>\n\t\t\t<Start>2026-10-19T08:00:00</Start>\n\t\t\t<Finish>2026-10-20T17:00:00</Finish>\n\t\t\t<Duration>PT16H0M0S</Duration>\n\t\t\t<DurationFormat>7</DurationFormat>\n\t\t\t<Summary>0</Summary>\n\t\t\t<EarlyStart>2026-10-19T08:00:00</EarlyStart>\n\t\t\t<EarlyFinish>2026-10-20T17:00:00</EarlyFinish>\n\t\t\t<LateStart>2026-10-19T08:00:00</LateStart>\n\t\t\t<LateFinish>2026-10-20T17:00:00</LateFinish>\n\t\t\t<RemainingDuration>PT16H0M0S</RemainingDuration>\n\t\t\t<CalendarUID>-1</CalendarUID>"
        );
        for name in ZERO {
            let _ = write!(xml, "\n\t\t\t<{name}>0</{name}>");
        }
        for name in ZERO_D {
            let _ = write!(xml, "\n\t\t\t<{name}>PT0H0M0S</{name}>");
        }
        if uid > 1 {
            let _ = write!(
                xml,
                "\n\t\t\t<PredecessorLink>\n\t\t\t\t<PredecessorUID>{}</PredecessorUID>\n\t\t\t\t<Type>1</Type>\n\t\t\t\t<CrossProject>0</CrossProject>\n\t\t\t\t<LinkLag>0</LinkLag>\n\t\t\t\t<LagFormat>7</LagFormat>\n\t\t\t</PredecessorLink>",
                uid - 1
            );
        }
        if custom {
            for (id, kind) in CUSTOM_FIELDS {
                let value = match kind {
                    't' => format!("Lot {}", uid % 7),
                    'n' => format!("{}.25", uid % 1000),
                    'd' => "2026-11-02T08:00:00".to_owned(),
                    _ => "1".to_owned(),
                };
                let _ = write!(
                    xml,
                    "\n\t\t\t<ExtendedAttribute>\n\t\t\t\t<FieldID>{id}</FieldID>\n\t\t\t\t<Value>{value}</Value>\n\t\t\t</ExtendedAttribute>"
                );
            }
        }
        xml.push_str("\n\t\t</Task>");
    }
    xml.push_str("\n\t</Tasks>\n</Project>\n");
    xml
}

/// `custom_mspdi_5000` (seconde passe UX, lot L8, R-265, R-266) : lecture
/// puis écriture de 5 000 tâches écrites comme par MS Project
/// ([`saved_like`]) avec 10 champs personnalisés par tâche ; échec au-delà
/// de 1,5 × la lecture puis l'écriture du même document sans champs
/// (médianes de 9 mesures, release).
#[test]
#[ignore = "mesure, à lancer en --release"]
fn custom_mspdi_5000() {
    use plannercraft_mspdi::write::{Computed, WriteOptions, write};
    let opts = ReadOptions::new("2026-01-01T00:00".parse().unwrap());
    let median = |xml: &str| {
        let mut times: Vec<Duration> = (0..9)
            .map(|_| {
                let t = Instant::now();
                let doc = read(xml.as_bytes(), &opts).unwrap();
                let out = write(&doc.project, Computed::Omit, &WriteOptions::default());
                std::hint::black_box(out);
                t.elapsed()
            })
            .collect();
        times.sort();
        times[4]
    };
    let with = saved_like(5_000, true);
    let doc = read(with.as_bytes(), &opts).unwrap();
    assert_eq!(doc.project.custom_fields.len(), 2);
    assert!(doc.project.tasks.iter().all(|t| t.custom.len() == 10));
    let without = median(&saved_like(5_000, false));
    let custom = median(&with);
    println!(
        "MSPDI, 5 000 tâches : lecture + écriture {without:?} sans champs, {custom:?} avec 10 champs \
         personnalisés par tâche (limite 1,5 ×)"
    );
    assert!(
        custom.as_nanos() * 2 < without.as_nanos() * 3,
        "{custom:?} / {without:?}"
    );
}
