//! Mesure contre le corrigé externe (ADR-0036 D5), hors CI.
//!
//! `MPP_MEASURE_DIR=<dossier des .mpp> MPP_ORACLE_DIR=<dossier des XML>
//! cargo test --release -p plannercraft-mpp --test measure -- --ignored --nocapture`
//!
//! Chaque `.mpp` du dossier (récursif) est lu par ce lecteur ; le XML du
//! corrigé de même chemin relatif (extension `.xml`) est relu par
//! `plannercraft-mspdi` ; les deux projets sont comparés champ par champ.
//! Le corrigé est la lecture d'un autre programme (MPXJ, lancé comme
//! outil), pas celle de MS Project : un écart peut venir de lui. Rien de ce
//! fichier ne lit le corpus en CI ; les chiffres vont dans
//! `docs/plans/mpp-import.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::explicit_iter_loop,
    reason = "outil de mesure hors CI : les interdits de la règle 10 ne s'appliquent pas"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use plannercraft_core::calendar::model::CalendarKind;
use plannercraft_core::{Moment, Project, TaskUid, schedule};
use plannercraft_mpp::{FileFormat, MppError, ReadOptions, compare, read};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else if p
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mpp") || e.eq_ignore_ascii_case("mpt"))
        {
            out.push(p);
        }
    }
}

#[derive(Default)]
struct Tally {
    by_field: BTreeMap<&'static str, (u64, u64)>,
    bad_examples: BTreeMap<&'static str, Vec<String>>,
}

impl Tally {
    fn check<T: PartialEq + std::fmt::Debug>(
        &mut self,
        field: &'static str,
        file: &str,
        uid: u32,
        mine: &T,
        oracle: &T,
    ) {
        let e = self.by_field.entry(field).or_insert((0, 0));
        e.1 += 1;
        if mine == oracle {
            e.0 += 1;
        } else {
            let ex = self.bad_examples.entry(field).or_default();
            if ex.len() < 6 {
                ex.push(format!("{file} UID {uid}: lu {mine:?} / corrigé {oracle:?}"));
            }
        }
    }
}

fn links(p: &Project, uid: TaskUid) -> BTreeSet<String> {
    p.tasks
        .get(uid)
        .map(|t| {
            t.predecessors
                .iter()
                .map(|l| {
                    // Décalage nul : l'unité d'affichage n'a pas d'effet (le
                    // corrigé écrit l'unité par défaut du projet).
                    let lag = match l.lag.value {
                        plannercraft_core::LagValue::Time { minutes, .. } if minutes.get() == 0 => {
                            "0".to_owned()
                        }
                        _ => format!("{:?}", l.lag),
                    };
                    format!("{}-{:?}-{lag}", l.predecessor.get(), l.kind)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "mesure locale contre le corrigé, hors CI"]
fn measure_against_oracle() {
    let dir = PathBuf::from(std::env::var("MPP_MEASURE_DIR").expect("MPP_MEASURE_DIR"));
    let oracle_dir = PathBuf::from(std::env::var("MPP_ORACLE_DIR").expect("MPP_ORACLE_DIR"));
    let verbose = std::env::var("MPP_MEASURE_VERBOSE").is_ok();
    let now: Moment = "2026-11-14T08:00".parse().unwrap();
    let mut files = Vec::new();
    walk(&dir, &mut files);
    let mut refused: BTreeMap<String, u32> = BTreeMap::new();
    let mut read_ok: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut tally = Tally::default();
    let mut engine = (0_u64, 0_u64);
    let mut engine_oracle = (0_u64, 0_u64);
    let mut engine_bare = (0_u64, 0_u64);
    let mut no_oracle = 0;
    for path in &files {
        let rel = path.strip_prefix(&dir).unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(path).unwrap();
        let doc = match read(&bytes, &ReadOptions::new("m".to_owned(), now)) {
            Ok(d) => d,
            Err(e) => {
                let key = match &e {
                    MppError::Unsupported(f) => format!("{} ({})", e.code(), f.label()),
                    _ => e.code().to_owned(),
                };
                *refused.entry(key).or_default() += 1;
                if verbose
                    && !matches!(
                        e,
                        MppError::Unsupported(FileFormat::Mpp12 | FileFormat::Mpp9 | FileFormat::Mpp8)
                    )
                {
                    println!("REFUS {rel}: {}", e.code());
                }
                continue;
            }
        };
        if let Ok(want) = std::env::var("MPP_MEASURE_LAYOUT")
            && !want.is_empty()
            && want != format!("{:?}", doc.source.layout)
        {
            continue;
        }
        *read_ok.entry(doc.source.format.label()).or_default() += 1;
        if doc.source.inconsistent_records > 0 {
            println!(
                "R-292 {rel} : {} enregistrements incohérents sur {}",
                doc.source.inconsistent_records, doc.source.task_records
            );
        }
        let xml = oracle_dir.join(&rel).with_extension("xml");
        let Ok(xml) = std::fs::read(&xml) else {
            no_oracle += 1;
            continue;
        };
        let o = plannercraft_mspdi::read(&xml, &plannercraft_mspdi::ReadOptions::new(now)).unwrap();
        let (p, op) = (&doc.project, &o.project);
        let file = rel.as_str();
        // Projet.
        tally.check("projet : ancre (sens et date)", file, 0, &p.anchor, &op.anchor);
        tally.check("projet : titre", file, 0, &p.info.title, &op.info.title);
        tally.check("projet : date d'état", file, 0, &p.status_date, &op.status_date);
        tally.check(
            "projet : facteurs",
            file,
            0,
            &p.options.factors,
            &op.options.factors,
        );
        let cal_name = |pr: &Project| pr.calendars.get(pr.calendar).map(|c| c.name.clone());
        tally.check("projet : calendrier (nom)", file, 0, &cal_name(p), &cal_name(op));
        tally.check(
            "projet : heure de début par défaut",
            file,
            0,
            &p.options.default_start_time,
            &op.options.default_start_time,
        );
        // Non repris : comptes annoncés par le rapport.
        let ni = doc.source.not_imported;
        tally.check(
            "non repris : ressources",
            file,
            0,
            &(ni.resources as usize),
            &op.resources.len(),
        );
        tally.check(
            "non repris : affectations",
            file,
            0,
            &(ni.assignments as usize),
            &op.assignments.len(),
        );
        // Tâches : ordre et nombre.
        let mine_uids: Vec<u32> = p.tasks.iter().map(|t| t.uid().get()).collect();
        let oracle_uids: Vec<u32> = op.tasks.iter().map(|t| t.uid().get()).collect();
        tally.check("tâches : nombre", file, 0, &mine_uids.len(), &oracle_uids.len());
        tally.check("tâches : ordre (UID par ID)", file, 0, &mine_uids, &oracle_uids);
        for ot in op.tasks.iter() {
            let uid = ot.uid();
            let Some(t) = p.tasks.get(uid) else {
                tally.check("tâche présente", file, uid.get(), &false, &true);
                continue;
            };
            tally.check("tâche présente", file, uid.get(), &true, &true);
            let u = uid.get();
            tally.check("tâche : ligne vide", file, u, &t.is_null, &ot.is_null);
            if ot.is_null {
                continue;
            }
            tally.check("tâche : nom", file, u, &t.name, &ot.name);
            tally.check("tâche : niveau", file, u, &t.outline_level, &ot.outline_level);
            tally.check(
                "tâche : durée (minutes)",
                file,
                u,
                &t.duration.minutes(),
                &ot.duration.minutes(),
            );
            tally.check(
                "tâche : unité de durée (unité, base, estimée)",
                file,
                u,
                &(t.duration.unit(), t.duration.basis(), t.duration.is_estimated()),
                &(
                    ot.duration.unit(),
                    ot.duration.basis(),
                    ot.duration.is_estimated(),
                ),
            );
            tally.check("tâche : jalon", file, u, &t.milestone, &ot.milestone);
            tally.check(
                "tâche : % achevé",
                file,
                u,
                &t.progress.percent_complete,
                &ot.progress.percent_complete,
            );
            tally.check(
                "tâche : début réel",
                file,
                u,
                &t.progress.actual_start,
                &ot.progress.actual_start,
            );
            tally.check(
                "tâche : fin réelle",
                file,
                u,
                &t.progress.actual_finish,
                &ot.progress.actual_finish,
            );
            tally.check(
                "tâche : durée réelle",
                file,
                u,
                &t.progress.actual_duration,
                &ot.progress.actual_duration,
            );
            tally.check(
                "tâche : retard de nivellement",
                file,
                u,
                &t.leveling_delay,
                &ot.leveling_delay,
            );
            tally.check(
                "tâche : reprise (tâche commencée)",
                file,
                u,
                &t.progress.resume,
                &ot.progress.resume,
            );
            tally.check("tâche : contrainte", file, u, &t.constraint, &ot.constraint);
            tally.check("tâche : échéance", file, u, &t.deadline, &ot.deadline);
            tally.check(
                "tâche : notes",
                file,
                u,
                &t.notes.trim().to_owned(),
                &ot.notes.trim().to_owned(),
            );
            tally.check("tâche : calendrier", file, u, &t.calendar, &ot.calendar);
            tally.check("tâche : priorité", file, u, &t.priority, &ot.priority);
            tally.check("tâche : type", file, u, &t.task_type, &ot.task_type);
            let manual =
                |m: &plannercraft_core::SchedulingMode| !matches!(m, plannercraft_core::SchedulingMode::Auto);
            tally.check(
                "tâche : planification manuelle",
                file,
                u,
                &manual(&t.mode),
                &manual(&ot.mode),
            );
            tally.check(
                "tâche : valeurs saisies (mode manuel)",
                file,
                u,
                &t.mode,
                &ot.mode,
            );
            tally.check("tâche : active", file, u, &t.active, &ot.active);
            let r = doc.recorded.tasks.get(&uid).copied().unwrap_or_default();
            let orr = o.recorded.tasks.get(&uid).copied().unwrap_or_default();
            tally.check("tâche : début enregistré", file, u, &r.start, &orr.start);
            tally.check("tâche : fin enregistrée", file, u, &r.finish, &orr.finish);
            tally.check(
                "tâche : liens (prédécesseurs)",
                file,
                u,
                &links(p, uid),
                &links(op, uid),
            );
        }
        // Calendriers de base du corrigé : semaine et exceptions.
        for oc in op.calendars.iter() {
            let CalendarKind::Base { week: ow } = oc.kind() else {
                continue;
            };
            let u = oc.uid().get();
            let Some(c) = p.calendars.get(oc.uid()) else {
                tally.check("calendrier de base présent", file, u, &false, &true);
                continue;
            };
            tally.check("calendrier de base présent", file, u, &true, &true);
            tally.check("calendrier : nom", file, u, &c.name, &oc.name);
            if let CalendarKind::Base { week } = c.kind() {
                tally.check("calendrier : semaine", file, u, week, ow);
            }
            // Exceptions comparées par leur effet : jours couverts (récurrence
            // dépliée par le moteur), réglage du jour, nom. Le corrigé écrit une
            // récurrence « tous les N jours » avec le code 7, que le lecteur
            // MSPDI garde sans la déplier : ramenée ici à « Daily ».
            let ex = |cal: &plannercraft_core::calendar::model::Calendar| {
                let mut v: Vec<String> = cal
                    .exceptions()
                    .iter()
                    .map(|e| {
                        let mut e = e.clone();
                        if let plannercraft_core::calendar::model::Recurrence::ByDayCount { every } =
                            e.recurrence
                        {
                            e.recurrence = plannercraft_core::calendar::model::Recurrence::Daily { every };
                        }
                        let days = plannercraft_core::calendar::expand_exception(&e)
                            .map(|d| format!("{d:?}"))
                            .unwrap_or_else(|_| "?".to_owned());
                        format!("{days} {:?} {}", e.setting, e.name.trim())
                    })
                    .collect();
                v.sort();
                v
            };
            tally.check("calendrier : exceptions", file, u, &ex(c), &ex(oc));
        }
        // Moteur : dates recalculées contre dates enregistrées.
        let s = schedule(p);
        let cmp = compare(p, &s, &doc.recorded);
        engine.0 += u64::from(cmp.tasks_identical);
        engine.1 += u64::from(cmp.tasks_compared);
        let so = schedule(op);
        let mut orec = plannercraft_mpp::Recorded::default();
        for (uid, r) in &o.recorded.tasks {
            orec.tasks.insert(
                *uid,
                plannercraft_mpp::RecordedTask {
                    start: r.start,
                    finish: r.finish,
                },
            );
        }
        let cmpo = compare(op, &so, &orec);
        engine_oracle.0 += u64::from(cmpo.tasks_identical);
        engine_oracle.1 += u64::from(cmpo.tasks_compared);
        // Même projet du corrigé sans ressources ni affectations : ce que
        // ce lot reprend (référence équitable pour le lecteur).
        let mut bare = op.clone();
        bare.resources = plannercraft_core::Arena::new();
        bare.assignments = plannercraft_core::Arena::new();
        let sb = schedule(&bare);
        let cmpb = compare(&bare, &sb, &orec);
        engine_bare.0 += u64::from(cmpb.tasks_identical);
        engine_bare.1 += u64::from(cmpb.tasks_compared);
        if verbose {
            println!(
                "{rel}: {} tâches, moteur {}/{} (corrigé relu par MSPDI : {}/{} ; sans ressources : {}/{})",
                p.tasks.len(),
                cmp.tasks_identical,
                cmp.tasks_compared,
                cmpo.tasks_identical,
                cmpo.tasks_compared,
                cmpb.tasks_identical,
                cmpb.tasks_compared
            );
        }
    }
    println!(
        "\nFichiers : {} ; lus : {read_ok:?} ; refusés : {refused:?} ; sans corrigé : {no_oracle}",
        files.len()
    );
    println!("\n| Champ | Égaux | Comparés | Taux |\n|---|---|---|---|");
    for (field, (ok, n)) in &tally.by_field {
        let rate = if *n == 0 {
            0.0
        } else {
            *ok as f64 * 100.0 / *n as f64
        };
        println!("| {field} | {ok} | {n} | {rate:.1} % |");
    }
    println!(
        "\nMoteur : tâches aux dates recalculées identiques aux dates enregistrées : {}/{} (lecture .mpp) ; {}/{} (même calcul sur le projet relu depuis le XML du corrigé) ; {}/{} (corrigé sans ressources ni affectations)",
        engine.0, engine.1, engine_oracle.0, engine_oracle.1, engine_bare.0, engine_bare.1
    );
    for (field, ex) in &tally.bad_examples {
        println!("\nÉcarts « {field} » (exemples) :");
        for e in ex {
            println!("  {e}");
        }
    }
}

/// Différences de champs de tâche et d'options entre la lecture et le corrigé
/// pour un fichier (`MPP_DIFF_FILE`, `MPP_DIFF_XML`) : outil de mise au point.
#[test]
#[ignore = "outil de mise au point"]
fn diff_one_file() {
    let now: Moment = "2026-11-14T08:00".parse().unwrap();
    let bytes = std::fs::read(std::env::var("MPP_DIFF_FILE").unwrap()).unwrap();
    let xml = std::fs::read(std::env::var("MPP_DIFF_XML").unwrap()).unwrap();
    let doc = read(&bytes, &ReadOptions::new("m".to_owned(), now)).unwrap();
    let o = plannercraft_mspdi::read(&xml, &plannercraft_mspdi::ReadOptions::new(now)).unwrap();
    let (p, op) = (&doc.project, &o.project);
    println!("options lu {:?}\noptions corrigé {:?}", p.options, op.options);
    println!(
        "statut lu {:?} / {:?} ; corrigé {:?} / {:?}",
        p.status_date, p.current_date, op.status_date, op.current_date
    );
    let mut shown = 0;
    for ot in op.tasks.iter() {
        let Some(t) = p.tasks.get(ot.uid()) else { continue };
        // Champs que ce lot ne lit pas : alignés sur la lecture.
        let mut ot = ot.clone();
        ot.guid = t.guid;
        ot.wbs.clone_from(&t.wbs);
        ot.work = t.work;
        ot.baselines.clone_from(&t.baselines);
        ot.extras.clone_from(&t.extras);
        ot.custom.clone_from(&t.custom);
        ot.effort_driven = t.effort_driven;
        ot.progress.percent_work_complete = t.progress.percent_work_complete;
        ot.progress.physical_percent_complete = t.progress.physical_percent_complete;
        ot.progress.actual_work = t.progress.actual_work;
        ot.progress.remaining_work = t.progress.remaining_work;
        ot.created = t.created;
        let ot = &ot;
        let a = format!("{:?}", t.fields());
        let b = format!("{:?}", ot.fields());
        if a != b && shown < 4 {
            shown += 1;
            let fa: Vec<&str> = a.split(", ").collect();
            let fb: Vec<&str> = b.split(", ").collect();
            for (x, y) in fa.iter().zip(fb.iter()) {
                if x != y {
                    println!("UID {} : lu {x} / corrigé {y}", ot.uid().get());
                }
            }
        }
    }
}
