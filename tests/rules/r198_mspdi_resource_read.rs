//! R-198 — MSPDI lecture (J5) : retards de nivellement d'une tâche et d'une
//! affectation lus dans le modèle, dates et travail d'affectation, travail
//! et surutilisation de ressource relus dans `RecordedSchedule`,
//! disponibilités gardées en `Extras`.

use plannercraft_core::{AssignmentUid, DurationUnit, Minutes, ResourceUid, TaskUid};
use plannercraft_mspdi::write::{Computed, WriteOptions, write};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::mspdi::{compact, reread, text};
use plannercraft_tests::{moment, tests_path};

/// Document minimal : une tâche (UID 1) et ses éléments `task`, une
/// ressource (UID 1) et ses éléments `resource`, une affectation (UID 7)
/// et ses éléments `assignment`.
fn doc(task: &str, resource: &str, assignment: &str) -> MspdiDocument {
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
         <Tasks><Task><UID>1</UID><ID>1</ID><Name>T</Name><Duration>PT16H0M0S</Duration>{task}</Task></Tasks>\
         <Resources><Resource><UID>1</UID><ID>1</ID><Name>R</Name>{resource}</Resource></Resources>\
         <Assignments><Assignment><UID>7</UID><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID>{assignment}</Assignment></Assignments>\
         </Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn has(doc: &MspdiDocument, code: &ImportCode, element: &str) -> bool {
    doc.diagnostics
        .iter()
        .any(|d| d.code == *code && d.element == element)
}

#[test]
fn r198_task_leveling_delay_elapsed() {
    // O27 (A6 UID 20) : 43 200 dixièmes, format 8 (j écoulés) → 4 320 min
    // écoulées ; gardé dans le modèle, plus en `Extras`.
    let d = doc(
        "<LevelingDelay>43200</LevelingDelay><LevelingDelayFormat>8</LevelingDelayFormat>",
        "",
        "",
    );
    let t = d.project.tasks.get(TaskUid::new(1)).unwrap();
    let delay = t.leveling_delay.unwrap();
    assert_eq!(delay.minutes, Minutes::new(4_320));
    assert_eq!(delay.unit, DurationUnit::Day);
    assert!(delay.elapsed);
    assert!(t.extras.0.iter().all(|e| !e.key.starts_with("LevelingDelay")));
    // Retard nul : aucun retard.
    let d = doc(
        "<LevelingDelay>0</LevelingDelay><LevelingDelayFormat>8</LevelingDelayFormat>",
        "",
        "",
    );
    assert_eq!(d.project.tasks.get(TaskUid::new(1)).unwrap().leveling_delay, None);
    assert!(d.diagnostics.iter().all(|x| !x.element.contains("Leveling")));
}

#[test]
fn r198_assignment_leveling_delay_working() {
    // O26 (A6 affectation 47) : 4 800 dixièmes, format 7 (j ouvrés) →
    // 480 min ouvrées.
    let d = doc(
        "",
        "",
        "<LevelingDelay>4800</LevelingDelay><LevelingDelayFormat>7</LevelingDelayFormat>",
    );
    let a = d.project.assignments.get(AssignmentUid::new(7)).unwrap();
    let delay = a.leveling_delay.unwrap();
    assert_eq!(delay.minutes, Minutes::new(480));
    assert_eq!(delay.unit, DurationUnit::Day);
    assert!(!delay.elapsed);
    assert_eq!(a.extras, plannercraft_core::Extras::default());
}

#[test]
fn r198_leveling_delay_format_fallbacks_and_rounding() {
    // Format absent : jours écoulés pour une tâche, ouvrés pour une
    // affectation (hypothèse, formats du corpus) ; 4 805 dixièmes → 481 min
    // (R-001), `mspdi.rounded`.
    let d = doc(
        "<LevelingDelay>4805</LevelingDelay>",
        "",
        "<LevelingDelay>600</LevelingDelay>",
    );
    let t = d
        .project
        .tasks
        .get(TaskUid::new(1))
        .unwrap()
        .leveling_delay
        .unwrap();
    assert_eq!(
        (t.minutes, t.unit, t.elapsed),
        (Minutes::new(481), DurationUnit::Day, true)
    );
    assert!(has(&d, &ImportCode::Rounded, "Tasks/Task/LevelingDelay"));
    let a = d
        .project
        .assignments
        .get(AssignmentUid::new(7))
        .unwrap()
        .leveling_delay
        .unwrap();
    assert_eq!(
        (a.minutes, a.unit, a.elapsed),
        (Minutes::new(60), DurationUnit::Day, false)
    );
    // Pourcentage (19) : illisible ; code inconnu (99) : signalé ; défaut.
    let d = doc(
        "<LevelingDelay>4800</LevelingDelay><LevelingDelayFormat>19</LevelingDelayFormat>",
        "",
        "<LevelingDelay>4800</LevelingDelay><LevelingDelayFormat>99</LevelingDelayFormat>",
    );
    assert!(has(
        &d,
        &ImportCode::InvalidValue,
        "Tasks/Task/LevelingDelayFormat"
    ));
    assert!(has(
        &d,
        &ImportCode::UnknownCode { code: 99 },
        "Assignments/Assignment/LevelingDelayFormat"
    ));
    assert!(
        d.project
            .tasks
            .get(TaskUid::new(1))
            .unwrap()
            .leveling_delay
            .unwrap()
            .elapsed
    );
    // Format estimé (39 = j?) : jours ouvrés, l'estimation n'est pas gardée.
    let d = doc(
        "<LevelingDelay>9600</LevelingDelay><LevelingDelayFormat>39</LevelingDelayFormat>",
        "",
        "",
    );
    let t = d
        .project
        .tasks
        .get(TaskUid::new(1))
        .unwrap()
        .leveling_delay
        .unwrap();
    assert_eq!(
        (t.minutes, t.unit, t.elapsed),
        (Minutes::new(960), DurationUnit::Day, false)
    );
    // Négatif ou hors du domaine des durées : refusé, signalé.
    let d = doc(
        "<LevelingDelay>-4800</LevelingDelay>",
        "",
        "<LevelingDelay>9000000000</LevelingDelay>",
    );
    assert_eq!(d.project.tasks.get(TaskUid::new(1)).unwrap().leveling_delay, None);
    assert_eq!(
        d.project
            .assignments
            .get(AssignmentUid::new(7))
            .unwrap()
            .leveling_delay,
        None
    );
    assert!(has(&d, &ImportCode::InvalidValue, "Tasks/Task/LevelingDelay"));
    assert!(has(
        &d,
        &ImportCode::InvalidValue,
        "Assignments/Assignment/LevelingDelay"
    ));
}

#[test]
fn r198_leveling_delay_written_back() {
    // Écrit depuis le modèle : minutes × 10 et format ; relu identique.
    let d = doc(
        "<LevelingDelay>43200</LevelingDelay><LevelingDelayFormat>8</LevelingDelayFormat>",
        "",
        "<LevelingDelay>4800</LevelingDelay><LevelingDelayFormat>7</LevelingDelayFormat>",
    );
    let xml = compact(&text(&write(
        &d.project,
        Computed::Omit,
        &WriteOptions::default(),
    )));
    assert!(xml.contains("<LevelingDelay>43200</LevelingDelay><LevelingDelayFormat>8</LevelingDelayFormat>"));
    assert!(xml.contains("<LevelingDelay>4800</LevelingDelay><LevelingDelayFormat>7</LevelingDelayFormat>"));
    let back = reread(xml.as_bytes());
    assert_eq!(
        back.project,
        plannercraft_mspdi::normalize(&d.project, &WriteOptions::default())
    );
    // Aucun retard : aucun élément écrit (MS Project lit l'absence comme 0).
    let d = doc("<LevelingDelay>0</LevelingDelay>", "", "");
    let xml = text(&write(&d.project, Computed::Omit, &WriteOptions::default()));
    assert!(!xml.contains("LevelingDelay"), "{xml}");
}

#[test]
fn r198_legacy_extras_rewritten_until_modeled() {
    // Projet enregistré avant J5 : `LevelingDelay` gardé en `Extras`.
    // Réécrit tel quel tant que le modèle n'a pas de retard, relu dans le
    // modèle ; jeté quand le modèle en a un (pas d'élément en double).
    let mut d = doc("", "", "");
    let t = d.project.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.extras.0.push(plannercraft_core::ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "LevelingDelay".to_owned(),
        payload: "<LevelingDelay>4800</LevelingDelay>".to_owned(),
    });
    let xml = text(&write(&d.project, Computed::Omit, &WriteOptions::default()));
    assert_eq!(xml.matches("<LevelingDelay>4800</LevelingDelay>").count(), 1);
    let back = reread(xml.as_bytes());
    let delay = back
        .project
        .tasks
        .get(TaskUid::new(1))
        .unwrap()
        .leveling_delay
        .unwrap();
    assert_eq!(delay.minutes, Minutes::new(480));
    let t = d.project.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.leveling_delay = plannercraft_core::LevelingDelay::new(Minutes::new(960), DurationUnit::Day, false);
    let xml = text(&write(&d.project, Computed::Omit, &WriteOptions::default()));
    assert_eq!(xml.matches("<LevelingDelay>").count(), 1, "{xml}");
    assert!(xml.contains("<LevelingDelay>9600</LevelingDelay>"));
}

#[test]
fn r198_recorded_assignment_dates() {
    // `Start`, `Finish`, `Work` d'une affectation : relus (classe C), pas
    // écartés ; réécrits avec les valeurs relues, omis sans valeurs.
    let d = doc(
        "",
        "",
        "<Start>2026-10-22T08:00:00</Start><Finish>2026-10-23T17:00:00</Finish><Work>PT16H0M0S</Work>",
    );
    let r = d.recorded.assignments[&AssignmentUid::new(7)];
    assert_eq!(r.start, Some(moment("2026-10-22T08:00")));
    assert_eq!(r.finish, Some(moment("2026-10-23T17:00")));
    assert_eq!(r.work.unwrap().minutes(), Minutes::new(960));
    assert!(!d.source.dropped_computed.contains_key("Start"));
    assert!(!d.source.dropped_computed.contains_key("Finish"));
    let recorded = write(
        &d.project,
        Computed::Recorded(&d.recorded),
        &WriteOptions::default(),
    );
    assert_eq!(reread(&recorded.xml).recorded.assignments, d.recorded.assignments);
    let omitted = text(&write(&d.project, Computed::Omit, &WriteOptions::default()));
    assert!(!omitted.contains("2026-10-22T08:00:00"), "{omitted}");
}

#[test]
fn r198_recorded_resource_overallocated() {
    // O31 : `OverAllocated` et `Work` de ressource relus (classe C) ; les
    // pics d'unités restent écartés (X).
    let d = doc(
        "",
        "<OverAllocated>1</OverAllocated><PeakUnits>1.00</PeakUnits><Work>PT80H0M0S</Work>",
        "",
    );
    let r = d.recorded.resources[&ResourceUid::new(1)];
    assert_eq!(r.overallocated, Some(true));
    assert_eq!(r.work.unwrap().minutes(), Minutes::new(4_800));
    assert_eq!(d.source.dropped_computed.get("PeakUnits"), Some(&1));
    assert!(!d.source.dropped_computed.contains_key("OverAllocated"));
    let recorded = write(
        &d.project,
        Computed::Recorded(&d.recorded),
        &WriteOptions::default(),
    );
    assert_eq!(reread(&recorded.xml).recorded.resources, d.recorded.resources);
    let omitted = text(&write(&d.project, Computed::Omit, &WriteOptions::default()));
    assert!(!omitted.contains("OverAllocated"), "{omitted}");
    // Absents : rien d'enregistré.
    let d = doc("", "", "");
    assert_eq!(
        d.recorded.resources[&ResourceUid::new(1)],
        plannercraft_mspdi::RecordedResource::default()
    );
}

#[test]
fn r198_availability_periods_kept_in_extras() {
    // R-187 : disponibilités ignorées par le moteur, gardées verbatim.
    let periods = "<AvailabilityPeriods><AvailabilityPeriod><AvailableFrom>2026-11-02T08:00:00</AvailableFrom>\
                   <AvailableTo>2026-11-30T17:00:00</AvailableTo><AvailableUnits>0.5</AvailableUnits>\
                   </AvailabilityPeriod></AvailabilityPeriods>";
    let d = doc("", periods, "");
    let r = d.project.resources.get(ResourceUid::new(1)).unwrap();
    assert!(
        r.extras
            .0
            .iter()
            .any(|e| e.key == "AvailabilityPeriods" && e.payload == periods)
    );
}

#[test]
fn r198_corpus_a6_delays_in_model() {
    // A6 (fichier du corpus, absent hors du job `corpus`) : UID 20 et 36,
    // affectation 47 ; aucun `LevelingDelay` dans les `Extras`.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/takeout-system-zh.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let task = |uid: u32| d.project.tasks.get(TaskUid::new(uid)).unwrap().leveling_delay;
    let t20 = task(20).unwrap();
    assert_eq!(
        (t20.minutes, t20.unit, t20.elapsed),
        (Minutes::new(4_320), DurationUnit::Day, true)
    );
    let t36 = task(36).unwrap();
    assert_eq!((t36.minutes, t36.elapsed), (Minutes::new(11_520), true));
    let a47 = d
        .project
        .assignments
        .get(AssignmentUid::new(47))
        .unwrap()
        .leveling_delay
        .unwrap();
    assert_eq!(
        (a47.minutes, a47.unit, a47.elapsed),
        (Minutes::new(480), DurationUnit::Day, false)
    );
    let delayed = d
        .project
        .tasks
        .iter()
        .filter(|t| t.leveling_delay.is_some())
        .count()
        + d.project
            .assignments
            .iter()
            .filter(|a| a.leveling_delay.is_some())
            .count();
    assert_eq!(delayed, 3);
    let leaks = d
        .project
        .tasks
        .iter()
        .flat_map(|t| t.extras.0.iter())
        .chain(d.project.assignments.iter().flat_map(|a| a.extras.0.iter()))
        .filter(|e| e.key.contains("LevelingDelay"))
        .count();
    assert_eq!(leaks, 0);
}
