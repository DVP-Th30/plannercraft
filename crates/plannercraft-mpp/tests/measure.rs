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
    clippy::similar_names,
    clippy::map_unwrap_or,
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
    let mut classes = (0_u64, 0_u64, 0_u64);
    let mut causes: BTreeMap<&'static str, u64> = BTreeMap::new();
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
        // Ressources et affectations (R-294, R-295) : la ressource 0 du
        // corrigé est la ressource vide du projet, non reprise.
        tally.check(
            "ressources : nombre",
            file,
            0,
            &p.resources.len(),
            &op.resources.iter().filter(|r| r.uid().get() != 0).count(),
        );
        tally.check(
            "affectations : nombre",
            file,
            0,
            &p.assignments.len(),
            &op.assignments.len(),
        );
        for oa in op.assignments.iter() {
            let Some(a) = p.assignments.get(oa.uid()) else {
                tally.check("affectation présente", file, oa.uid().get(), &false, &true);
                continue;
            };
            let u = oa.uid().get();
            tally.check("affectation présente", file, u, &true, &true);
            tally.check(
                "affectation : tâche et ressource",
                file,
                u,
                &(a.task, a.resource),
                &(oa.task, oa.resource),
            );
            tally.check("affectation : unités", file, u, &a.units, &oa.units);
            tally.check("affectation : travail", file, u, &a.work, &oa.work);
            tally.check("affectation : retard", file, u, &a.delay, &oa.delay);
            tally.check("affectation : profil", file, u, &a.contour, &oa.contour);
        }
        for or in op.resources.iter().filter(|r| r.uid().get() != 0) {
            let Some(r) = p.resources.get(or.uid()) else {
                tally.check("ressource présente", file, or.uid().get(), &false, &true);
                continue;
            };
            let u = or.uid().get();
            tally.check("ressource présente", file, u, &true, &true);
            tally.check("ressource : nature", file, u, &r.kind, &or.kind);
            tally.check(
                "ressource : unités maximales",
                file,
                u,
                &r.max_units,
                &or.max_units,
            );
            tally.check("ressource : calendrier", file, u, &r.calendar, &or.calendar);
        }
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
        // Calendriers dérivés du corrigé (calendriers de ressource, R-296).
        for oc in op.calendars.iter() {
            let CalendarKind::Derived { base: ob, week: ow } = oc.kind() else {
                continue;
            };
            let u = oc.uid().get();
            let Some(c) = p.calendars.get(oc.uid()) else {
                tally.check("calendrier dérivé présent", file, u, &false, &true);
                continue;
            };
            tally.check("calendrier dérivé présent", file, u, &true, &true);
            if let CalendarKind::Derived { base, week } = c.kind() {
                tally.check("calendrier dérivé : base", file, u, base, ob);
                tally.check("calendrier dérivé : semaine", file, u, week, ow);
            }
            tally.check(
                "calendrier dérivé : exceptions (nombre)",
                file,
                u,
                &c.exceptions().len(),
                &oc.exceptions().len(),
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
        classes.0 += u64::from(cmp.tasks_identical);
        classes.1 += u64::from(cmp.tasks_rounding);
        classes.2 += u64::from(cmp.tasks_off);
        for (c, n) in &cmp.causes {
            *causes.entry(c).or_default() += u64::from(*n);
        }
        let so = schedule(op);
        let mut orec = plannercraft_mpp::Recorded::default();
        for (uid, r) in &o.recorded.tasks {
            orec.tasks.insert(
                *uid,
                plannercraft_mpp::RecordedTask {
                    start: r.start,
                    finish: r.finish,
                    sub_minute: false,
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
                "{rel}: {} tâches, moteur {}/{} (arrondi {}, écart réel {}) (corrigé relu par MSPDI : {}/{} ; sans ressources : {}/{})",
                p.tasks.len(),
                cmp.tasks_identical,
                cmp.tasks_compared,
                cmp.tasks_rounding,
                cmp.tasks_off,
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
    println!(
        "Classes (lecture .mpp) : identiques à la minute {} ; à deux minutes près {} ; écart réel {} ; dont causes non recalculables {causes:?}",
        classes.0, classes.1, classes.2
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

/// Écart d'une tâche : (début, fin) calculé − enregistré, en minutes.
fn task_gaps(
    p: &Project,
    s: &plannercraft_core::Schedule,
    rec: &plannercraft_mpp::Recorded,
) -> BTreeMap<TaskUid, (i64, i64)> {
    let mut out = BTreeMap::new();
    for t in p.tasks.iter() {
        if t.is_null {
            continue;
        }
        let (Some(r), Some(sc)) = (rec.tasks.get(&t.uid()), s.scheduled(t.uid())) else {
            continue;
        };
        let (Some(rs), Some(rf)) = (r.start, r.finish) else {
            continue;
        };
        out.insert(
            t.uid(),
            (
                sc.start.minutes() - rs.minutes(),
                sc.finish.minutes() - rf.minutes(),
            ),
        );
    }
    out
}

/// Diagnostic des écarts du moteur par tâche racine (lot M2) : une tâche en
/// écart dont les prédécesseurs (successeurs à rebours) et ceux de ses
/// récapitulatives sont justes (à deux minutes près) porte la cause ; une
/// récapitulative en écart dont toutes les tâches filles sont justes aussi.
/// Une ligne TSV par tâche en écart (`MPP_DIAG_OUT`), avec les indices du
/// corrigé.
#[test]
#[ignore = "diagnostic local, hors CI"]
fn diagnose_roots() {
    use plannercraft_core::progress::{ProgressState, facts};
    use plannercraft_core::{ConstraintKind, SchedulingMode, TimeBasis};
    use std::fmt::Write as _;
    let dir = PathBuf::from(std::env::var("MPP_MEASURE_DIR").expect("MPP_MEASURE_DIR"));
    let oracle_dir = PathBuf::from(std::env::var("MPP_ORACLE_DIR").expect("MPP_ORACLE_DIR"));
    let out_path = std::env::var("MPP_DIAG_OUT").expect("MPP_DIAG_OUT");
    let now: Moment = "2026-11-14T08:00".parse().unwrap();
    let mut files = Vec::new();
    walk(&dir, &mut files);
    let mut out = String::from(
        "file\tuid\tid\trole\tsummary\tds\tdf\tmilestone\tmanual\tconstraint\tcalendar\tstate\tresume\tlevel_delay\telapsed\tinactive\tdeadline\ttype\teffort\tassignments\tres_cal\tok_oracle\tok_bare\tdur\tfrom_finish\trs\tcs\tpreds\tname\n",
    );
    for path in &files {
        let rel = path.strip_prefix(&dir).unwrap().to_string_lossy().into_owned();
        let Ok(doc) = read(
            &std::fs::read(path).unwrap(),
            &ReadOptions::new("m".to_owned(), now),
        ) else {
            continue;
        };
        let Ok(xml) = std::fs::read(oracle_dir.join(&rel).with_extension("xml")) else {
            continue;
        };
        let o = plannercraft_mspdi::read(&xml, &plannercraft_mspdi::ReadOptions::new(now)).unwrap();
        let p = &doc.project;
        let op = &o.project;
        let from_finish = matches!(p.anchor, plannercraft_core::ScheduleAnchor::FromFinish(_));
        let s = schedule(p);
        let gaps = task_gaps(p, &s, &doc.recorded);
        let mut orec = plannercraft_mpp::Recorded::default();
        for (uid, r) in &o.recorded.tasks {
            orec.tasks.insert(
                *uid,
                plannercraft_mpp::RecordedTask {
                    start: r.start,
                    finish: r.finish,
                    sub_minute: false,
                },
            );
        }
        let go = task_gaps(op, &schedule(op), &orec);
        let mut bare = op.clone();
        bare.resources = plannercraft_core::Arena::new();
        bare.assignments = plannercraft_core::Arena::new();
        let gb = task_gaps(&bare, &schedule(&bare), &orec);
        let ok = |g: Option<&(i64, i64)>| g.is_none_or(|(a, b)| a.abs() <= 2 && b.abs() <= 2);
        // Hiérarchie : parent et enfants par l'ordre et les niveaux.
        let tasks: Vec<_> = p.tasks.iter().filter(|t| !t.is_null).collect();
        let mut parent: BTreeMap<TaskUid, TaskUid> = BTreeMap::new();
        let mut children: BTreeMap<TaskUid, Vec<TaskUid>> = BTreeMap::new();
        let mut stack: Vec<(u16, TaskUid)> = Vec::new();
        for t in &tasks {
            let lvl = t.outline_level.get();
            while stack.last().is_some_and(|(l, _)| *l >= lvl) {
                stack.pop();
            }
            if let Some((_, pu)) = stack.last() {
                parent.insert(t.uid(), *pu);
                children.entry(*pu).or_default().push(t.uid());
            }
            stack.push((lvl, t.uid()));
        }
        let mut successors: BTreeMap<TaskUid, Vec<TaskUid>> = BTreeMap::new();
        for t in &tasks {
            for l in &t.predecessors {
                successors.entry(l.predecessor).or_default().push(t.uid());
            }
        }
        let drivers = |u: TaskUid| -> Vec<TaskUid> {
            if from_finish {
                successors.get(&u).cloned().unwrap_or_default()
            } else {
                p.tasks
                    .get(u)
                    .map(|t| t.predecessors.iter().map(|l| l.predecessor).collect())
                    .unwrap_or_default()
            }
        };
        for t in &tasks {
            let u = t.uid();
            let Some(g) = gaps.get(&u) else { continue };
            let is_summary = children.contains_key(&u);
            let role = if *g == (0, 0) {
                "juste"
            } else if ok(Some(g)) {
                "arrondi"
            } else if is_summary {
                let kids_ok = children[&u].iter().all(|c| ok(gaps.get(c)));
                if kids_ok { "racine" } else { "héritée" }
            } else {
                let mut all_ok = drivers(u).iter().all(|d| ok(gaps.get(d)));
                let mut a = parent.get(&u).copied();
                while let Some(pu) = a {
                    all_ok &= drivers(pu).iter().all(|d| ok(gaps.get(d)));
                    a = parent.get(&pu).copied();
                }
                if all_ok { "racine" } else { "héritée" }
            };
            let ot = op.tasks.get(u);
            let assigned: Vec<_> = op.assignments.iter().filter(|a| a.task == u).collect();
            let res_cal = assigned
                .iter()
                .filter_map(|a| a.resource)
                .filter_map(|r| op.resources.get(r))
                .filter_map(|r| r.calendar)
                .filter_map(|c| op.calendars.get(c))
                .any(|c| {
                    !c.exceptions().is_empty()
                        || matches!(c.kind(), plannercraft_core::calendar::model::CalendarKind::Derived { week, .. } if format!("{week:?}").contains("Set"))
                });
            let st = match facts(t).state {
                ProgressState::NotStarted => "non",
                ProgressState::Started => "commencée",
                ProgressState::Complete => "achevée",
            };
            let mut line = String::new();
            writeln!(
                line,
                "{rel}\t{}\t{}\t{role}\t{is_summary}\t{}\t{}\t{}\t{}\t{}\t{}\t{st}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}\t{res_cal}\t{}\t{}\t{}\t{from_finish}\t{}\t{}\t{}\t{}",
                u.get(),
                p.task_id(u).map_or(0, plannercraft_core::TaskId::get),
                g.0,
                g.1,
                t.milestone,
                !matches!(t.mode, SchedulingMode::Auto),
                if t.constraint.kind() == ConstraintKind::AsSoonAsPossible {
                    String::new()
                } else {
                    format!("{:?}", t.constraint.kind())
                },
                t.calendar.is_some(),
                t.progress.resume.is_some(),
                t.leveling_delay.is_some(),
                t.duration.basis() == TimeBasis::Elapsed,
                !t.active,
                t.deadline.is_some(),
                ot.map(|x| x.task_type),
                ot.is_some_and(|x| x.effort_driven),
                assigned.len(),
                ok(go.get(&u)),
                ok(gb.get(&u)),
                t.duration.minutes().get(),
                doc.recorded.tasks.get(&u).and_then(|r| r.start).map(|m| m.to_string()).unwrap_or_default(),
                s.scheduled(u).map(|x| x.start.to_string()).unwrap_or_default(),
                t.predecessors
                    .iter()
                    .map(|l| format!("{}{:?}{:?}", l.predecessor.get(), l.kind, l.lag.value))
                    .collect::<Vec<_>>()
                    .join(","),
                t.name.replace('\t', " "),
            )
            .unwrap();
            out.push_str(&line);
        }
    }
    std::fs::write(out_path, out).unwrap();
}
