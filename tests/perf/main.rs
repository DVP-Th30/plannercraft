//! Budget de performance du moteur (spécification J2 § 6, ADR-0009 A18).
//!
//! `schedule` sur le grand réseau (5 000 tâches, 10 000 liens, compilation
//! des calendriers comprise), et sur ses variantes J3 (spécification J3
//! § 9.2) : `5000_constrained` (20 % de contraintes datées, 5 % d'ALAP, 5 %
//! d'échéances) et `5000_from_finish` (planification à rebours, mêmes
//! proportions) : 3 exécutions de chauffe, puis médiane de 15.
//! Échec au-delà de 50 ms, avertissement au-delà de 25 ms ; cible locale
//! ≤ 10 ms. J5 : `5000_assigned` (10 000 affectations, ADR-0013), même
//! budget de 50 ms ; `5000_assigned_leaves` (1 000 ressources ayant chacune
//! ses congés, 10 % de feuilles à calendrier propre), garde-fou provisoire
//! de 250 ms ; projets hostiles (calendriers combinés, 100 000 parts) en
//! temps borné. Hiérarchie profonde hostile (ADR-0012, lot L15) : 256 niveaux
//! de récapitulatives imbriquées, 5 000 tâches, 10 000 liens dont des FS,
//! SS et FF depuis chaque récapitulative ; budget 30 ms de médiane.
//! J6 : `5000_progress` (20 % de feuilles achevées, 30 % en cours dont la
//! moitié hors séquence), budget bloquant de 50 ms ; `5000_assigned_progress`
//! journalisé (garde-fou de 250 ms).
//! Mesure seulement en release, par le job CI `perf` :
//! `cargo test -p plannercraft-tests --release --test perf -- --ignored --nocapture`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::schedule::schedule_reference;
use plannercraft_core::{
    Diagnostic, Duration as TaskDuration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Moment,
    OutlineLevel, Percent, Project, ProjectInit, ScheduleAnchor, Severity, Task, TaskOutcome, TaskRole,
    TaskUid, TimeBasis, schedule,
};
use plannercraft_tests::generate::{
    assigned_leaves_network, assigned_network, assigned_progress_network, constrained_network,
    from_finish_network, hostile_combined_calendars, hostile_parts, large_network, progress_network,
};

/// Seuil d'échec de la médiane.
const BUDGET: Duration = Duration::from_millis(50);
/// Seuil d'avertissement.
const WARN: Duration = Duration::from_millis(25);

#[test]
fn large_network_shape() {
    // Le réseau mesuré a la forme annoncée et se calcule sans erreur.
    let p = large_network(2026);
    assert_eq!(p.tasks.len(), 5_000);
    let links: usize = p.tasks.iter().map(|t| t.predecessors.len()).sum();
    assert_eq!(links, 10_000);
    let backward = p
        .tasks
        .iter()
        .flat_map(|t| t.predecessors.iter().map(move |l| (l.predecessor, t.uid())))
        .filter(|(pred, succ)| pred > succ)
        .count();
    // Environ 20 % des liens remontent la liste (chemin de Kahn forcé).
    assert!((1_500..=2_500).contains(&backward), "{backward} liens remontants");
    let s = schedule(&p);
    let summaries = s
        .tasks
        .values()
        .filter(|o| matches!(o, TaskOutcome::Scheduled(ts) if ts.role == TaskRole::Summary))
        .count();
    assert_eq!(summaries, 525);
    assert!(
        s.tasks
            .values()
            .all(|o| matches!(o, TaskOutcome::Scheduled(ts) if !ts.degraded))
    );
    let errors: Vec<&Diagnostic> = s
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    // Déterministe.
    assert_eq!(large_network(2026), p);
}

/// Les variantes J3 se calculent sans erreur ni tâche dégradée, avec leurs
/// contraintes (et, à rebours, leur ancre), de façon déterministe.
#[test]
fn constrained_networks_shape() {
    for (name, p) in [
        ("5000_constrained", constrained_network(2026)),
        ("5000_from_finish", from_finish_network(2026)),
    ] {
        let dated = p.tasks.iter().filter(|t| t.constraint.kind().is_dated()).count();
        assert!(
            (700..=1_200).contains(&dated),
            "{name} : {dated} contraintes datées"
        );
        let deadlines = p.tasks.iter().filter(|t| t.deadline.is_some()).count();
        assert!((150..=300).contains(&deadlines), "{name} : {deadlines} échéances");
        let s = schedule(&p);
        assert!(
            s.tasks
                .values()
                .all(|o| matches!(o, TaskOutcome::Scheduled(ts) if !ts.degraded)),
            "{name}"
        );
        let errors: Vec<&Diagnostic> = s
            .diagnostics
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{name} : {errors:?}");
    }
    assert_eq!(constrained_network(2026), constrained_network(2026));
    assert_eq!(from_finish_network(2026), from_finish_network(2026));
}

/// Médiane de 15 exécutions après 3 de chauffe ; échec au-delà du budget.
fn budget(name: &str, p: &Project) {
    budget_of(name, p, BUDGET);
}

/// [`budget`] avec un seuil explicite.
fn budget_of(name: &str, p: &Project, limit: Duration) {
    for _ in 0..3 {
        black_box(schedule(black_box(p)));
    }
    let mut times: Vec<Duration> = (0..15)
        .map(|_| {
            let t = Instant::now();
            black_box(schedule(black_box(p)));
            t.elapsed()
        })
        .collect();
    times.sort_unstable();
    let median = times[times.len() / 2];
    eprintln!(
        "schedule ({name}) : médiane {median:?}, min {:?}, max {:?}",
        times.first().copied().unwrap_or_default(),
        times.last().copied().unwrap_or_default()
    );
    if median > WARN {
        eprintln!("avertissement : médiane {median:?} au-delà de {WARN:?} (cible locale ≤ 10 ms)");
    }
    assert!(
        median <= limit,
        "{name} : médiane {median:?} au-delà du budget {limit:?}"
    );
}

#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn schedule_budget_5000_tasks_10000_links() {
    budget("5 000 tâches, 10 000 liens", &large_network(2026));
}

#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn schedule_budget_5000_constrained() {
    budget("5000_constrained", &constrained_network(2026));
}

#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn schedule_budget_5000_from_finish() {
    budget("5000_from_finish", &from_finish_network(2026));
}

/// Le réseau affecté (J5, ADR-0013) a la forme annoncée : 10 000
/// affectations de travail placées, aucune erreur ni tâche dégradée,
/// charges calculées pour les 20 ressources ; déterministe.
#[test]
fn assigned_network_shape() {
    let p = assigned_network(2026);
    assert_eq!(p.tasks.len(), 5_000);
    assert_eq!(p.assignments.len(), 10_000);
    assert_eq!(p.resources.len(), 20);
    let s = schedule(&p);
    assert!(
        s.tasks
            .values()
            .all(|o| matches!(o, TaskOutcome::Scheduled(ts) if !ts.degraded))
    );
    let errors: Vec<&Diagnostic> = s
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(s.resources.len(), 20);
    assert_eq!(s.assignments.len(), 10_000);
    assert_eq!(assigned_network(2026), p);
}

/// Budget J5 (ADR-0013) : 5 000 tâches, 10 000 liens, 10 000 affectations
/// (placement par affectation, cumuls et surutilisation compris), 50 ms.
#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn perf_5000_assigned() {
    budget("5000_assigned", &assigned_network(2026));
}

/// Réseau avec avancement (J6, ADR-0014 D10) : 5 000 tâches, 10 000 liens,
/// ~20 % de feuilles achevées et ~30 % en cours dont la moitié hors
/// séquence ; aucune erreur, aucune tâche dégradée ; déterministe.
#[test]
fn progress_network_shape() {
    let p = progress_network(2026);
    assert_eq!(p.tasks.len(), 5_000);
    let s = schedule(&p);
    let (mut done, mut running) = (0, 0);
    for o in s.tasks.values() {
        assert!(
            matches!(o, TaskOutcome::Scheduled(ts) if !ts.degraded),
            "tâche non planifiée ou dégradée : {o:?}"
        );
        let TaskOutcome::Scheduled(ts) = o else { continue };
        if ts.role == TaskRole::Leaf {
            match ts.progress.map(|p| p.state) {
                Some(plannercraft_core::ProgressState::Complete) => done += 1,
                Some(plannercraft_core::ProgressState::Started) => running += 1,
                _ => {}
            }
        }
    }
    assert!((700..=1_200).contains(&done), "{done} achevées");
    assert!((1_100..=1_700).contains(&running), "{running} en cours");
    let errors: Vec<&Diagnostic> = s
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(progress_network(2026), p);
}

/// Budget J6 (ADR-0014 D10) : `5000_progress`, bloquant à 50 ms.
#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn perf_5000_progress() {
    budget("5000_progress", &progress_network(2026));
}

/// `5000_assigned_progress` (J6) : journalisé, sans seuil bloquant au-delà
/// du garde-fou de `5000_assigned_leaves`.
#[test]
#[ignore = "mesuré en release, job CI perf"]
fn perf_5000_assigned_progress() {
    budget_of(
        "5000_assigned_progress",
        &assigned_progress_network(2026),
        LEAVES_BUDGET,
    );
}

/// Variante de la relecture finale J5 : chaque ressource a ses congés
/// (1 000 calendriers de ressource distincts), 10 % des feuilles sur un
/// calendrier propre : placement par parts sur deux échelles, unions et
/// intersections compilées ; mêmes formes que `5000_assigned`, aucune
/// erreur, aucune tâche dégradée, aucun calendrier combiné refusé.
#[test]
fn assigned_leaves_network_shape() {
    let p = assigned_leaves_network(2026);
    assert_eq!(p.tasks.len(), 5_000);
    assert_eq!(p.assignments.len(), 10_000);
    assert_eq!(p.resources.len(), 1_000);
    let s = schedule(&p);
    assert!(
        s.tasks
            .values()
            .all(|o| matches!(o, TaskOutcome::Scheduled(ts) if !ts.degraded))
    );
    let refused: Vec<&Diagnostic> = s
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error || d.code() == "calendar.too_many_segments")
        .collect();
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(s.resources.len(), 1_000);
    assert_eq!(s.assignments.len(), 10_000);
    assert_eq!(assigned_leaves_network(2026), p);
}

/// Garde-fou provisoire de `5000_assigned_leaves` (relecture finale J5) :
/// la cible de 50 ms d'ADR-0013 n'est pas tenue (médiane locale 158 ms :
/// compilation des 1 011 calendriers, environ 50 ms ; 5 000 unions et
/// 1 000 intersections, environ 40 ms ; placement par parts sur deux
/// échelles, environ 65 ms). Seuil bloquant de 250 ms en attendant
/// l'arbitrage (`docs/decisions.md`, correctifs de la relecture finale
/// (11)) : il rattrape une régression quadratique (O(S²) sur les échelles,
/// O(P²) sur les parts), pas une dérive de quelques pour cent. J6, lot L9 :
/// les 1 010 calendriers dérivés sans semaine propre sont compilés par
/// surcharge de leur base (compilation 104 → 11 à 18 ms, calcul 178 → 110
/// à 117 ms au minimum, binaires alternés sur serveurdvp chargé).
const LEAVES_BUDGET: Duration = Duration::from_millis(250);

/// Budget de `5000_assigned_leaves` : [`LEAVES_BUDGET`].
#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn perf_5000_assigned_leaves() {
    budget_of(
        "5000_assigned_leaves",
        &assigned_leaves_network(2026),
        LEAVES_BUDGET,
    );
}

/// Un calcul chronométré ; échec au-delà de `limit`.
fn once(name: &str, p: &Project, limit: Duration) -> plannercraft_core::Schedule {
    let t = Instant::now();
    let s = schedule(p);
    let took = t.elapsed();
    eprintln!("schedule ({name}) : {took:?}");
    assert!(took <= limit, "{name} : {took:?} au-delà de {limit:?}");
    s
}

/// Adversaire F2 de la relecture finale J5 : 1 000 paires (tâche,
/// ressource) de calendriers à 60 000 segments. Le budget des calendriers
/// combinés (`Limits::combined_segments`, 4 M segments gardés, 32 M lus)
/// refuse les intersections au-delà : `calendar.too_many_segments`, repli
/// sur le calendrier de la tâche, mémoire bornée (environ 128 Mo de
/// segments combinés au lieu de 4 Go), calcul fini.
#[test]
#[ignore = "projet hostile, mesuré en release, job CI perf"]
fn hostile_combined_calendars_bounded() {
    let s = once(
        "calendriers combinés hostiles",
        &hostile_combined_calendars(),
        Duration::from_secs(10),
    );
    // Budget des combinaisons atteint (deux calendriers distincts), en plus
    // de celui de la compilation (J6, lot L9 : 6,6 M segments pour un budget
    // de 2 M, calendrier refusé = `calendar` égal à `other`).
    assert!(
        s.diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::CalendarTooManySegments { calendar, other, .. } if calendar != other
        )),
        "budget des combinaisons jamais atteint"
    );
    assert_eq!(s.assignments.len(), 1_000);
    eprintln!("pic mémoire du processus : {}", peak_memory());
}

/// Pic de mémoire résidente du processus (`VmHWM`, Linux), pour le
/// journal du job `perf` ; « inconnu » ailleurs. À lire sur un test lancé
/// seul (`--exact`) : le pic est celui du processus entier.
fn peak_memory() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .map(|l| l.trim_start_matches("VmHWM:").trim().to_owned())
        })
        .unwrap_or_else(|| "inconnu".to_owned())
}

/// Calendriers hostiles du modèle (J6, lot L9, dette de l'ADR-0013) :
/// 1 000 calendriers dérivés du Standard, chacun chômé un jour sur deux sur
/// tout le domaine (environ 60 000 segments, 2 Mo compilés : 2 Go sans
/// budget) ; 1 000 tâches d'un jour, chacune sur l'un d'eux.
fn hostile_model_calendars() -> Project {
    use plannercraft_core::calendar::{Calendar, CalendarException, DaySetting, Recurrence};
    use plannercraft_core::{CalendarUid, Day, DayRange};
    let start: Moment = "2026-10-19T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Calendriers du modèle hostiles".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let domain = DayRange::new(Day::new(0).unwrap(), Day::new(60_630).unwrap()).unwrap();
    for k in 0..1_000u32 {
        let mut c = Calendar::new_derived(CalendarUid::new(100 + k), format!("C{k}"), CalendarUid::new(1));
        c.add_exception(CalendarException {
            name: "Un jour sur deux".to_owned(),
            period: domain,
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Daily { every: 2 },
            occurrences: None,
        })
        .unwrap();
        p.calendars.push(c).unwrap();
    }
    let options = p.options;
    for i in 0..1_000u32 {
        let mut t = Task::new(TaskUid::new(i + 1), format!("T{i}"), &options);
        t.duration =
            TaskDuration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        t.calendar = Some(CalendarUid::new(100 + i));
        p.tasks.push(t).unwrap();
    }
    p
}

/// Budget de compilation des calendriers du modèle (J6, lot L9, R-172
/// amendée) : `Limits::compiled_segments` (2 M segments gardés, 16 M de
/// travail) refuse les calendriers au-delà, `calendar.too_many_segments`,
/// tâches en repli R-066 ; mémoire et temps bornés.
#[test]
#[ignore = "projet hostile, mesuré en release, job CI perf"]
fn hostile_calendars_bounded() {
    let s = once(
        "calendriers du modèle hostiles",
        &hostile_model_calendars(),
        Duration::from_secs(10),
    );
    let refused = s
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::CalendarTooManySegments { calendar, other, .. } if calendar == other
            )
        })
        .count();
    assert!(refused > 900, "{refused} calendriers refusés");
    assert!(s.diagnostics.iter().any(|d| d.code() == "task.calendar_fallback"));
    assert!(s.tasks.len() >= 1_000);
    eprintln!(
        "{refused} calendriers refusés ; pic mémoire du processus : {}",
        peak_memory()
    );
}

/// Adversaire F1 de la relecture finale J5 : 100 000 affectations
/// distinctes sur une seule feuille. Mesure exacte de l'union bornée à
/// 1 000 parts (`assignment.too_many_parts`, durée = plus grand `d_a`) ;
/// le reste du calcul est linéaire en parts.
#[test]
#[ignore = "projet hostile, mesuré en release, job CI perf"]
fn hostile_parts_bounded() {
    let s = once("100 000 parts", &hostile_parts(100_000), Duration::from_secs(5));
    assert!(
        s.diagnostics
            .iter()
            .any(|d| d.code() == "assignment.too_many_parts")
    );
    // 1 000 parts : sous le plafond, mesure exacte par balayage
    // (O(P log P)) ; la plus décalée finit 999 min après la première.
    let s = once("1 000 parts", &hostile_parts(1_000), Duration::from_millis(500));
    assert!(
        !s.diagnostics
            .iter()
            .any(|d| d.code() == "assignment.too_many_parts")
    );
    let duration = match s.tasks.get(&TaskUid::new(1)) {
        Some(TaskOutcome::Scheduled(a)) => Some(a.duration),
        _ => None,
    };
    assert_eq!(duration, Some(Minutes::new(480 + 999)));
}

/// Profondeur de la hiérarchie hostile.
const DEPTH: u32 = 256;
/// Tâches de la hiérarchie hostile.
const DEEP_TASKS: u32 = 5_000;
/// Liens de la hiérarchie hostile.
const DEEP_LINKS: usize = 10_000;

/// Hiérarchie hostile (lot L15) : récapitulative `S_i` au niveau `i` (1 à
/// 256), chacune suivie de ses feuilles au niveau `i + 1` (environ 17 par
/// niveau, de profondeur moyenne 128) ; puis 256 tâches de premier niveau
/// `Z_i`. Liens : `S_i → Z_i` (FS, +1 h), `S_i → Z_(i+1)` (SS), `S_i →
/// Z_(i+2)` (FF) ; chaque feuille a des prédécesseurs FS ou SS parmi les
/// 40 feuilles qui la précèdent, jusqu'à 10 000 liens. Chaque feuille hérite
/// donc des arcs de tous ses ancêtres : la référence (J3) remonte jusqu'à
/// 256 agrégats et 768 arcs par nœud. Avec `progress`, les feuilles ont un
/// % achevé de `(uid mod 11) × 10 %` (environ 9 sur 10 en cours, R-200) :
/// variante `deep_hierarchy_progress` (relecture finale J6, J6-PERF-1) ;
/// sans, la hiérarchie de J5.
fn deep_hierarchy(progress: bool) -> Project {
    let start: Moment = "2026-11-02T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Hiérarchie profonde".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let options = p.options;
    let day = |n: i64| {
        TaskDuration::new(
            Minutes::new(n * 480),
            DurationUnit::Day,
            TimeBasis::Working,
            false,
        )
        .unwrap()
    };
    let leaves_total = DEEP_TASKS - 2 * DEPTH;
    let mut uid = 0u32;
    let mut spine = Vec::new();
    let mut leaves: Vec<TaskUid> = Vec::new();
    let mut tasks = Vec::new();
    for i in 1..=DEPTH {
        uid += 1;
        let mut s = Task::new(TaskUid::new(uid), format!("S{i}"), &options);
        s.outline_level = OutlineLevel::new(u16::try_from(i).unwrap()).unwrap();
        spine.push(s.uid());
        tasks.push(s);
        let k = leaves_total / DEPTH + u32::from(i <= leaves_total % DEPTH);
        for j in 0..k {
            uid += 1;
            let mut t = Task::new(TaskUid::new(uid), format!("F{i}.{j}"), &options);
            t.outline_level = OutlineLevel::new(u16::try_from(i + 1).unwrap()).unwrap();
            t.duration = day(i64::from(1 + (uid % 5)));
            if progress {
                t.progress.percent_complete = Percent::new(i64::from(uid % 11) * 1_000).unwrap();
            }
            leaves.push(t.uid());
            tasks.push(t);
        }
    }
    let mut links = 0usize;
    // Prédécesseurs des feuilles : jusqu'à 40 en arrière, déterministes.
    let mut leaf_preds: Vec<Vec<Link>> = vec![Vec::new(); leaves.len()];
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        x
    };
    let summary_links = 3 * usize::try_from(DEPTH).unwrap();
    while links < DEEP_LINKS - summary_links {
        let s = usize::try_from(next() % u64::try_from(leaves.len()).unwrap()).unwrap();
        if s == 0 {
            continue;
        }
        let back = 1 + usize::try_from(next() % 40).unwrap();
        let q = s.saturating_sub(back);
        if leaf_preds[s].iter().any(|l| l.predecessor == leaves[q]) {
            continue;
        }
        let kind = if next() % 4 == 0 {
            LinkType::StartToStart
        } else {
            LinkType::FinishToStart
        };
        leaf_preds[s].push(Link {
            predecessor: leaves[q],
            kind,
            lag: Lag::ZERO,
        });
        links += 1;
    }
    let mut z = Vec::new();
    for i in 0..DEPTH {
        uid += 1;
        let mut t = Task::new(TaskUid::new(uid), format!("Z{i}"), &options);
        t.duration = day(2);
        z.push(t);
    }
    let n = spine.len();
    for (i, s) in spine.iter().enumerate() {
        let hour = Lag::new(
            TimeBasis::Working,
            LagValue::Time {
                minutes: Minutes::new(60),
                unit: DurationUnit::Hour,
            },
        )
        .unwrap();
        z[i].predecessors.push(Link {
            predecessor: *s,
            kind: LinkType::FinishToStart,
            lag: hour,
        });
        z[(i + 1) % n].predecessors.push(Link {
            predecessor: *s,
            kind: LinkType::StartToStart,
            lag: Lag::ZERO,
        });
        z[(i + 2) % n].predecessors.push(Link {
            predecessor: *s,
            kind: LinkType::FinishToFinish,
            lag: Lag::ZERO,
        });
    }
    let mut leaf_preds = leaf_preds.into_iter();
    for mut t in tasks {
        if t.outline_level.get() > 1 && !spine.contains(&t.uid()) {
            t.predecessors = leaf_preds.next().unwrap_or_default();
        }
        p.tasks.push(t).unwrap();
    }
    for t in z {
        p.tasks.push(t).unwrap();
    }
    p
}

/// La hiérarchie hostile a la forme annoncée, se calcule sans erreur, et
/// les deux implémentations des marges et des % achevés (lot L15) y sont
/// égales, sans avancement et avec (feuilles en cours, R-206, J6-PERF-1).
#[test]
fn deep_hierarchy_shape() {
    for progress in [false, true] {
        let p = deep_hierarchy(progress);
        assert_eq!(p.tasks.len(), 5_000);
        let links: usize = p.tasks.iter().map(|t| t.predecessors.len()).sum();
        assert_eq!(links, DEEP_LINKS);
        assert_eq!(p.tasks.iter().map(|t| t.outline_level.get()).max(), Some(257));
        let s = schedule(&p);
        let errors: Vec<&Diagnostic> = s
            .diagnostics
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        let summaries = s
            .tasks
            .values()
            .filter(
                |o| matches!(o, TaskOutcome::Scheduled(ts) if ts.role == TaskRole::Summary && !ts.degraded),
            )
            .count();
        assert_eq!(summaries, 256);
        let running = s
            .tasks
            .values()
            .filter(|o| {
                matches!(o, TaskOutcome::Scheduled(ts)
                    if ts.progress.is_some_and(|p| p.state == plannercraft_core::ProgressState::Started))
            })
            .count();
        if progress {
            assert!(running > 3_000, "{running} feuilles en cours");
        } else {
            assert_eq!(running, 0);
        }
        assert_eq!(s, schedule_reference(&p), "avancement : {progress}");
    }
}

/// Budget de la hiérarchie hostile (lot L15) : 30 ms de médiane.
fn deep_budget(name: &str, progress: bool) {
    let p = deep_hierarchy(progress);
    // Référence (J3), informative : le coût que la stratégie rapide évite.
    let t = Instant::now();
    black_box(schedule_reference(black_box(&p)));
    eprintln!("schedule_reference ({name}) : {:?}", t.elapsed());
    budget_of(name, &p, Duration::from_millis(30));
}

#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn perf_deep_hierarchy_256_5000() {
    deep_budget("hiérarchie de 256 niveaux, 5 000 tâches", false);
}

/// Même hiérarchie, feuilles en cours (relecture finale J6, J6-PERF-1) :
/// une feuille en cours passe par la stratégie rapide des marges, même
/// budget bloquant.
#[test]
#[ignore = "budget mesuré en release, job CI perf"]
fn perf_deep_hierarchy_progress_256_5000() {
    deep_budget("deep_hierarchy_progress, 256 niveaux, 5 000 tâches", true);
}

/// Lot de saisies d'avancement (relecture finale J6, J6-PERF-2) sur le
/// grand réseau (5 000 tâches) : chaque `progress.update` qui suit une
/// écriture recalcule le clone (le début implicite est le début planifié
/// courant) ; 200 saisies, journalisé, garde-fou de 20 s. Un lot de 10 000
/// saisies est refusé par le budget de recalcul du lot (`limit.exceeded`,
/// `schedule_recomputes`, 200 recalculs au plus) en temps borné.
#[test]
#[ignore = "mesuré en release, job CI perf"]
fn perf_batch_progress_updates() {
    use plannercraft_automation::error::codes;
    use plannercraft_automation::{ApplyMode, Batch};
    let p = large_network(2026);
    let plan = schedule(&p);
    let uids: Vec<u32> = p
        .tasks
        .iter()
        .map(plannercraft_core::Task::uid)
        .filter(|u| matches!(plan.get(*u), Some(TaskOutcome::Scheduled(ts)) if ts.role == TaskRole::Leaf))
        .map(TaskUid::get)
        .collect();
    let mut s = plannercraft_tests::session::session(p);
    let batch = |n: usize| -> Batch {
        let ops: Vec<serde_json::Value> = uids
            .iter()
            .cycle()
            .take(n)
            .map(|u| serde_json::json!({ "op": "progress.update", "task": u, "percent_complete": { "text": "10%" } }))
            .collect();
        plannercraft_tests::task_edit::batch(serde_json::Value::Array(ops))
    };
    let t = Instant::now();
    let out = s.apply(&batch(200), ApplyMode::DryRun);
    let took = t.elapsed();
    eprintln!("lot de 200 progress.update (5 000 tâches) : {took:?}");
    assert!(out.is_ok(), "{out:?}");
    assert!(took <= Duration::from_secs(20), "{took:?}");
    let t = Instant::now();
    let err = s.apply(&batch(10_000), ApplyMode::DryRun).unwrap_err();
    let took = t.elapsed();
    eprintln!("lot de 10 000 progress.update refusé : {took:?}");
    assert_eq!(err.code, codes::LIMIT_EXCEEDED);
    assert!(took <= Duration::from_secs(60), "{took:?}");
}

/// `baseline_variance` page par page (relecture finale J6, J6-PERF-3) :
/// 20 000 tâches à ligne de base 0, dont une tâche d'arrivée de 19 999
/// prédécesseurs (FS). Chaque page ne construit que ses lignes (sans liens
/// ni ressources) : médiane de 15 pages de 50 lignes, budget de 50 ms
/// (avant : toutes les vues de tâches, liens compris, à chaque page ;
/// serveurdvp : 92 ms avant, 17,6 ms après).
#[test]
#[ignore = "mesuré en release, job CI perf"]
fn perf_baseline_variance_page() {
    use plannercraft_automation::ApplyMode;
    use plannercraft_automation::analysis::baseline_variance;
    use plannercraft_automation::command::params::VarianceParams;
    let start: Moment = "2026-11-02T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Variance".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let options = p.options;
    let n = 20_000u32;
    let day = TaskDuration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
    for i in 1..n {
        let mut t = Task::new(TaskUid::new(i), format!("T{i}"), &options);
        t.duration = day;
        p.tasks.push(t).unwrap();
    }
    let mut hub = Task::new(TaskUid::new(n), "Arrivée".to_owned(), &options);
    hub.duration = day;
    hub.predecessors = (1..n)
        .map(|i| Link {
            predecessor: TaskUid::new(i),
            kind: LinkType::FinishToStart,
            lag: Lag::ZERO,
        })
        .collect();
    p.tasks.push(hub).unwrap();
    let mut s = plannercraft_tests::session::session(p);
    s.apply(
        &plannercraft_tests::task_edit::batch(serde_json::json!([{ "op": "baseline.set", "number": 0 }])),
        ApplyMode::Commit,
    )
    .unwrap();
    let mut cursor = None;
    let mut times = Vec::new();
    for _ in 0..15 {
        let params = VarianceParams {
            baseline: Some(0),
            only_varied: false,
            cursor,
            limit: Some(50),
        };
        let t = Instant::now();
        let view = baseline_variance(&s, &params).unwrap();
        times.push(t.elapsed());
        assert_eq!(view.total, 20_000);
        assert_eq!(view.tasks.len(), 50);
        cursor = view.next_cursor;
    }
    times.sort_unstable();
    let median = times[times.len() / 2];
    eprintln!("baseline_variance (20 000 tâches, page de 50) : médiane {median:?}");
    assert!(median <= Duration::from_millis(50), "{median:?}");
}
