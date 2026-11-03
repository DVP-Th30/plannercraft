//! R-199 — MSPDI écriture : valeurs calculées des affectations et des
//! ressources, travail des récapitulatives, durée des feuilles affectées,
//! `TimephasedData` gardée ou jetée. R-125 amendée (`Start`/`Finish`
//! d'affectation écrits).

use plannercraft_core::{
    Assignment, AssignmentUid, Constraint, Duration, DurationUnit, ExtraEntry, Extras, Link, Minutes,
    OutlineLevel, Project, Resource, ResourceUid, ScheduleAnchor, Subject, Task, TaskType, TaskUid,
    TimeBasis, Work, WorkContour, schedule,
};
use plannercraft_mspdi::write::{Computed, WriteCode, WriteOptions, choose_computed, schedule_image, write};
use plannercraft_mspdi::{ReadOptions, normalize, read};
use plannercraft_tests::mspdi::{compact, new_project, reread, text};
use plannercraft_tests::session::{commit, session, set_days};
use plannercraft_tests::{moment, tests_path};

fn duration(minutes: i64) -> Duration {
    Duration::new(
        Minutes::new(minutes),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap()
}

fn work(minutes: i64) -> Work {
    Work::new(Minutes::new(minutes)).unwrap()
}

/// Tâche `uid` de `minutes` ouvrées, au niveau `level`, travail `w`.
fn task(p: &Project, uid: u32, minutes: i64, level: u16, w: i64) -> Task {
    let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
    t.duration = duration(minutes);
    t.outline_level = OutlineLevel::new(level).unwrap();
    t.work = work(w);
    t
}

fn resource(p: &mut Project, uid: u32) {
    p.resources
        .push(Resource::new(ResourceUid::new(uid), format!("R{uid}")))
        .unwrap();
}

/// Affectation `uid` de la ressource `res` à la tâche `on`, travail
/// `minutes`, 100 %.
fn assign(p: &mut Project, uid: u32, on: u32, res: u32, minutes: i64) -> AssignmentUid {
    let mut assignment = Assignment::new(
        AssignmentUid::new(uid),
        TaskUid::new(on),
        Some(ResourceUid::new(res)),
    );
    assignment.work = work(minutes);
    p.assignments.push(assignment).unwrap();
    AssignmentUid::new(uid)
}

/// Fragment compact de l'élément `element` d'UID `uid`.
fn entity_xml(xml: &str, element: &str, uid: u32) -> String {
    let marker = format!("<{element}><UID>{uid}</UID>");
    let start = xml.find(&marker).unwrap_or_else(|| panic!("{marker} absent"));
    let rest = xml.get(start..).unwrap();
    rest.get(..rest.find(&format!("</{element}>")).unwrap())
        .unwrap()
        .to_owned()
}

/// Écrit avec les valeurs du moteur, vérifie que la relecture redonne
/// `normalize(schedule_image(p))` et rend le texte compact et le document.
fn written_by_engine(p: &Project) -> (String, plannercraft_mspdi::write::Written) {
    let sched = schedule(p);
    let (computed, diags) = choose_computed(p, &sched, None);
    assert!(matches!(computed, Computed::Schedule(_)), "{diags:?}");
    let options = WriteOptions::default();
    let out = write(p, computed, &options);
    let back = reread(&out.xml);
    assert_eq!(back.project, normalize(&schedule_image(p, &sched), &options));
    (compact(&text(&out)), out)
}

#[test]
fn r199_assignment_dates_written() {
    // T1 : R1 1 j ; R2 1 j avec un délai de 2 j (S, lun 19/10/2026 08:00).
    // R1 lun ; R2 mer ; tâche lun 08:00 – mer 17:00, durée calculée 960
    // (union, R-173) écrite à la place des 2 j du modèle, reste compris.
    let mut p = new_project("2026-10-19T08:00");
    let t = task(&p, 1, 480, 1, 960);
    p.tasks.push(t).unwrap();
    resource(&mut p, 1);
    resource(&mut p, 2);
    assign(&mut p, 1, 1, 1, 480);
    let a2 = assign(&mut p, 2, 1, 2, 480);
    p.assignments.get_mut(a2).unwrap().delay = Minutes::new(960);
    let (xml, out) = written_by_engine(&p);
    let a1 = entity_xml(&xml, "Assignment", 1);
    assert!(a1.contains("<Start>2026-10-19T08:00:00</Start>"), "{a1}");
    assert!(a1.contains("<Finish>2026-10-19T17:00:00</Finish>"), "{a1}");
    let a2 = entity_xml(&xml, "Assignment", 2);
    assert!(a2.contains("<Start>2026-10-21T08:00:00</Start>"), "{a2}");
    assert!(a2.contains("<Finish>2026-10-21T17:00:00</Finish>"), "{a2}");
    let t1 = entity_xml(&xml, "Task", 1);
    assert!(t1.contains("<Duration>PT16H0M0S</Duration>"), "{t1}");
    assert!(
        t1.contains("<RemainingDuration>PT16H0M0S</RemainingDuration>"),
        "{t1}"
    );
    let back = reread(&out.xml);
    let rec = &back.recorded.assignments[&AssignmentUid::new(2)];
    assert_eq!(rec.start, Some(moment("2026-10-21T08:00")));
    assert_eq!(rec.finish, Some(moment("2026-10-21T17:00")));
    // Sans valeurs calculées : rien d'écrit (R-125 avant J5).
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    assert!(!entity_xml(&omitted, "Assignment", 2).contains("<Start>"));
}

#[test]
fn r199_from_finish_assignment_dates_written() {
    // À rebours depuis ven 23/10/2026 17:00 : T1 1 j ALAP avec R1 8 h, T2
    // 5 j sans ressource. T1 est planifiée ven 08:00 – 17:00 (dates au plus
    // tard, R-112) ; l'affectation est écrite sur ces mêmes dates, et non
    // sur le lundi de ses dates au plus tôt.
    let mut p = new_project("2026-10-19T08:00");
    p.anchor = ScheduleAnchor::FromFinish(moment("2026-10-23T17:00"));
    let mut t1 = task(&p, 1, 480, 1, 480);
    t1.constraint = Constraint::AsLateAsPossible;
    p.tasks.push(t1).unwrap();
    let mut t2 = task(&p, 2, 2_400, 1, 0);
    t2.constraint = Constraint::AsLateAsPossible;
    p.tasks.push(t2).unwrap();
    resource(&mut p, 1);
    assign(&mut p, 1, 1, 1, 480);
    let (xml, _) = written_by_engine(&p);
    let t1 = entity_xml(&xml, "Task", 1);
    assert!(t1.contains("<Start>2026-10-23T08:00:00</Start>"), "{t1}");
    let a1 = entity_xml(&xml, "Assignment", 1);
    assert!(a1.contains("<Start>2026-10-23T08:00:00</Start>"), "{a1}");
    assert!(a1.contains("<Finish>2026-10-23T17:00:00</Finish>"), "{a1}");
}

#[test]
fn r199_fixed_duration_keeps_entered_duration() {
    // Durée fixe : la durée est l'entrée tenue (S41) ; 2 j écrits même si
    // l'union des intervalles (R1 lun-mar, R2 mer-jeu) en fait 4.
    let mut p = new_project("2026-10-19T08:00");
    let mut t = task(&p, 1, 960, 1, 1_920);
    t.task_type = TaskType::FixedDuration;
    p.tasks.push(t).unwrap();
    resource(&mut p, 1);
    resource(&mut p, 2);
    assign(&mut p, 1, 1, 1, 960);
    let a2 = assign(&mut p, 2, 1, 2, 960);
    p.assignments.get_mut(a2).unwrap().delay = Minutes::new(960);
    let (xml, _) = written_by_engine(&p);
    let t1 = entity_xml(&xml, "Task", 1);
    assert!(t1.contains("<Duration>PT16H0M0S</Duration>"), "{t1}");
    let a2 = entity_xml(&xml, "Assignment", 2);
    assert!(a2.contains("<Finish>2026-10-22T17:00:00</Finish>"), "{a2}");
}

#[test]
fn r199_zero_units_keeps_model_duration() {
    // Unités fixes 2 j : R1 100 % 16 h ; R2 0 % 3 h décalée d'un jour
    // (`d_a` = durée de la tâche, R-181) ⇒ union lun – mer (3 j). La durée
    // écrite reste celle du modèle, 2 j : reporter l'union l'allongerait à
    // chaque recalcul (R-173).
    let mut p = new_project("2026-10-19T08:00");
    let t = task(&p, 1, 960, 1, 1_140);
    p.tasks.push(t).unwrap();
    resource(&mut p, 1);
    resource(&mut p, 2);
    assign(&mut p, 1, 1, 1, 960);
    let a2 = assign(&mut p, 2, 1, 2, 180);
    let a = p.assignments.get_mut(a2).unwrap();
    a.units = plannercraft_core::Units::ZERO;
    a.delay = Minutes::new(480);
    let (xml, _) = written_by_engine(&p);
    let t1 = entity_xml(&xml, "Task", 1);
    assert!(t1.contains("<Duration>PT16H0M0S</Duration>"), "{t1}");
    let a2 = entity_xml(&xml, "Assignment", 2);
    assert!(a2.contains("<Finish>2026-10-21T17:00:00</Finish>"), "{a2}");
}

#[test]
fn r199_assignment_remaining_work_follows_work() {
    // RemainingWork d'une affectation sans travail réel = Work (MS Project
    // l'écrit toujours ; aucune commande ne tient le reste du modèle) :
    // affectation créée (reste 0 dans le modèle) ⇒ PT8H ; reste relu de
    // 64 h périmé après un rééquilibrage à 32 h ⇒ PT32H. Amendement J6
    // (R-214, ADR-0014 D4) : avec un travail réel saisi (8 h), le reste est
    // dérivé, W − réel = 24 h ; le reste saisi (30 h) n'est plus écrit.
    let mut p = new_project("2026-10-19T08:00");
    for uid in 1..=3 {
        let t = task(&p, uid, 480, 1, 0);
        p.tasks.push(t).unwrap();
    }
    resource(&mut p, 1);
    assign(&mut p, 1, 1, 1, 480);
    let a2 = assign(&mut p, 2, 2, 1, 1_920);
    p.assignments.get_mut(a2).unwrap().remaining_work = work(3_840);
    let a3 = assign(&mut p, 3, 3, 1, 1_920);
    let a = p.assignments.get_mut(a3).unwrap();
    a.actual_work = work(480);
    a.remaining_work = work(1_800);
    let (xml, _) = written_by_engine(&p);
    let remaining = |uid| {
        let x = entity_xml(&xml, "Assignment", uid);
        let start = x.find("<RemainingWork>").unwrap();
        x.get(start..).unwrap().split('<').nth(1).unwrap().to_owned()
    };
    assert_eq!(remaining(1), "RemainingWork>PT8H0M0S");
    assert_eq!(remaining(2), "RemainingWork>PT32H0M0S");
    assert_eq!(remaining(3), "RemainingWork>PT24H0M0S");
}

#[test]
fn r199_resource_overallocated_written() {
    // T1 et T2 en parallèle, 1 j chacune ; R1 à 100 % sur les deux
    // (surutilisée, 16 h), R2 sur T1 seulement (8 h). `PeakUnits` jamais
    // écrit (O31).
    let mut p = new_project("2026-10-19T08:00");
    let t1 = task(&p, 1, 480, 1, 960);
    let t2 = task(&p, 2, 480, 1, 480);
    p.tasks.push(t1).unwrap();
    p.tasks.push(t2).unwrap();
    resource(&mut p, 1);
    resource(&mut p, 2);
    assign(&mut p, 1, 1, 1, 480);
    assign(&mut p, 2, 2, 1, 480);
    assign(&mut p, 3, 1, 2, 480);
    let (xml, out) = written_by_engine(&p);
    let r1 = entity_xml(&xml, "Resource", 1);
    assert!(r1.contains("<OverAllocated>1</OverAllocated>"), "{r1}");
    assert!(r1.contains("<Work>PT16H0M0S</Work>"), "{r1}");
    let r2 = entity_xml(&xml, "Resource", 2);
    assert!(r2.contains("<OverAllocated>0</OverAllocated>"), "{r2}");
    assert!(r2.contains("<Work>PT8H0M0S</Work>"), "{r2}");
    assert!(!xml.contains("<PeakUnits>"));
    let back = reread(&out.xml);
    let rec = &back.recorded.resources[&ResourceUid::new(1)];
    assert_eq!((rec.work, rec.overallocated), (Some(work(960)), Some(true)));
}

#[test]
fn r199_summary_and_project_work_written() {
    // Récapitulative S (UID 1) : enfants de 8 h et 8 h, sa propre
    // affectation de 4 h (R-179) ; travail du modèle faux (0) : 20 h
    // écrites (R-184), UID 0 aussi.
    let mut p = new_project("2026-10-19T08:00");
    p.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &p.options));
    let s = task(&p, 1, 0, 1, 0);
    let c1 = task(&p, 2, 480, 2, 480);
    let c2 = task(&p, 3, 480, 2, 480);
    p.tasks.push(s).unwrap();
    p.tasks.push(c1).unwrap();
    p.tasks.push(c2).unwrap();
    resource(&mut p, 1);
    resource(&mut p, 2);
    assign(&mut p, 1, 2, 1, 480);
    assign(&mut p, 2, 3, 2, 480);
    assign(&mut p, 3, 1, 1, 240);
    let (xml, _) = written_by_engine(&p);
    let s = entity_xml(&xml, "Task", 1);
    assert!(s.contains("<Work>PT20H0M0S</Work>"), "{s}");
    let p0 = entity_xml(&xml, "Task", 0);
    assert!(p0.contains("<Work>PT20H0M0S</Work>"), "{p0}");
    // Valeurs relues ou omises : travail du modèle.
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    assert!(entity_xml(&omitted, "Task", 1).contains("<Work>PT0H0M0S</Work>"));
}

/// Ligne `TimephasedData` de type `kind`.
fn row(kind: u8, start: &str, finish: &str, value: &str) -> ExtraEntry {
    ExtraEntry {
        origin: "mspdi".to_owned(),
        key: "TimephasedData".to_owned(),
        payload: format!(
            "<TimephasedData><Type>{kind}</Type><UID>1</UID><Start>{start}</Start><Finish>{finish}</Finish><Unit>2</Unit><Value>{value}</Value></TimephasedData>"
        ),
    }
}

/// T1 1 j, lun 19/10, R1 8 h à profil modifié à la main, avec ses lignes de
/// type 1 (lun 08:00-12:00, 13:00-17:00) et une ligne de type 2 (réel,
/// 1 h = son travail réel : gardée, R-215).
fn contoured() -> Project {
    let mut p = new_project("2026-10-19T08:00");
    let t = task(&p, 1, 480, 1, 480);
    p.tasks.push(t).unwrap();
    resource(&mut p, 1);
    let a = assign(&mut p, 1, 1, 1, 480);
    let a = p.assignments.get_mut(a).unwrap();
    a.contour = WorkContour::Contoured;
    // Réel 1 h (la ligne de type 2) ; reste dérivé W − réel (R-214).
    a.actual_work = work(60);
    a.remaining_work = work(420);
    a.extras = Extras(vec![
        row(1, "2026-10-19T08:00:00", "2026-10-19T12:00:00", "PT4H0M0S"),
        row(1, "2026-10-19T13:00:00", "2026-10-19T17:00:00", "PT4H0M0S"),
        row(2, "2026-10-19T08:00:00", "2026-10-19T09:00:00", "PT1H0M0S"),
    ]);
    p
}

#[test]
fn r199_timephased_kept_when_unchanged() {
    let p = contoured();
    let (xml, out) = written_by_engine(&p);
    let a = entity_xml(&xml, "Assignment", 1);
    assert_eq!(a.matches("<TimephasedData>").count(), 3, "{a}");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.code.code() == "mspdi.write.timephased_dropped"),
        "{:?}",
        out.diagnostics
    );
    // Valeurs relues : réécrites telles quelles.
    let back = reread(&out.xml);
    assert_eq!(back.project.assignments, p.assignments);
}

#[test]
fn r199_timephased_dropped_when_moved() {
    // Travail porté à 16 h : l'affectation finit mar 17:00 ; ses deux
    // lignes de type 1 (lun) sont jetées, le réel (type 2) gardé.
    let mut p = contoured();
    p.assignments.get_mut(AssignmentUid::new(1)).unwrap().work = work(960);
    p.tasks.get_mut(TaskUid::new(1)).unwrap().work = work(960);
    let (xml, out) = written_by_engine(&p);
    let a = entity_xml(&xml, "Assignment", 1);
    assert_eq!(a.matches("<TimephasedData>").count(), 1, "{a}");
    assert!(a.contains("<Type>2</Type>"), "{a}");
    let dropped: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code.code() == "mspdi.write.timephased_dropped")
        .collect();
    assert_eq!(dropped.len(), 1, "{:?}", out.diagnostics);
    assert_eq!(dropped[0].code, WriteCode::TimephasedDropped { count: 2 });
    assert_eq!(
        dropped[0].subject,
        Some(Subject::Assignment(AssignmentUid::new(1)))
    );
    // Avec les valeurs relues (projet non modifié), rien n'est jeté.
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    assert_eq!(
        entity_xml(&omitted, "Assignment", 1)
            .matches("<TimephasedData>")
            .count(),
        3
    );
}

#[test]
fn r199_successor_shift_drops_timephased() {
    // A 1 j → B (FS) ; B porte R1 8 h, profil modifié, lignes de type 1 du
    // mar 20/10. Dans une session, A passe à 2 j : B glisse au mer, aucune
    // commande ne touche son affectation (R-161 ne la voit pas), l'écriture
    // jette ses lignes (dette R-161 soldée).
    let mut p = new_project("2026-10-19T08:00");
    let first = task(&p, 1, 480, 1, 0);
    let mut second = task(&p, 2, 480, 1, 480);
    second.predecessors = vec![Link::finish_to_start(TaskUid::new(1))];
    p.tasks.push(first).unwrap();
    p.tasks.push(second).unwrap();
    resource(&mut p, 1);
    let uid = assign(&mut p, 1, 2, 1, 480);
    let x = p.assignments.get_mut(uid).unwrap();
    x.contour = WorkContour::Contoured;
    x.extras = Extras(vec![
        row(1, "2026-10-20T08:00:00", "2026-10-20T12:00:00", "PT4H0M0S"),
        row(1, "2026-10-20T13:00:00", "2026-10-20T17:00:00", "PT4H0M0S"),
    ]);
    let mut s = session(p);
    let before = s.save_bytes(&WriteOptions::default());
    assert_eq!(text(&before).matches("<TimephasedData>").count(), 2);
    let _ = commit(&mut s, "a", &[set_days(1, 2)]);
    let after = s.save_bytes(&WriteOptions::default());
    let xml = compact(&text(&after));
    assert!(!xml.contains("<TimephasedData>"), "{xml}");
    assert!(
        entity_xml(&xml, "Assignment", 1).contains("<Start>2026-10-21T08:00:00</Start>"),
        "{xml}"
    );
    assert!(
        after
            .diagnostics
            .iter()
            .any(|d| d.code == WriteCode::TimephasedDropped { count: 2 }),
        "{:?}",
        after.diagnostics
    );
}

#[test]
fn r199_corpus_a6_written_by_engine() {
    // A6 (fichier du corpus, absent hors du job `corpus`) : plus aucun
    // manque (R-122 amendée), écrit avec les valeurs du moteur ;
    // affectation 47 (retard de nivellement de 1 j) le ven 15/11/2024.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/takeout-system-zh.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let (xml, _) = written_by_engine(&d.project);
    let a47 = entity_xml(&xml, "Assignment", 47);
    assert!(a47.contains("<Start>2024-11-15T08:00:00</Start>"), "{a47}");
}
