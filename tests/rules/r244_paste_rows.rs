//! R-244 — Coller un `RowClip` à `Placement::Before(ligne sélectionnée)`
//! (fin du projet si la ligne vide est sélectionnée) : une tâche nouvelle
//! par tâche du tampon, UID neufs, niveaux relatifs gardés, seuls les liens
//! internes recréés, affectations reprises par nom de ressource ;
//! contraintes, échéances, notes, mode, actif gardés ; ni avancement, ni
//! réels, ni ligne de base, ni UID d'origine. Repère : projet « P ».
//! Source : hypothèse.

use plannercraft_automation::clip::{PasteOptions, PasteReport, ResourcePaste, RowClip, paste_batch};
use plannercraft_automation::{ApplyMode, Placement, Session, TaskRef};
use plannercraft_core::{Constraint, SchedulingMode, TaskUid};
use plannercraft_tests::daily::{plan_p, uid, uids};
use plannercraft_tests::task_edit::{apply, get, names};
use serde_json::json;

/// Copie `ns`, colle à `at`, applique ; rend le compte rendu et les UID
/// créés.
fn paste(s: &mut Session, ns: &[u32], at: &Placement, o: PasteOptions) -> (PasteReport, Vec<u32>) {
    let clip = RowClip::capture(s, &uids(ns)).unwrap();
    let (batch, report) = paste_batch(s, &clip, at, &o).unwrap();
    let before: Vec<u32> = s.project().tasks.iter().map(|t| t.uid().get()).collect();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    let created = s
        .project()
        .tasks
        .iter()
        .map(|t| t.uid().get())
        .filter(|u| !before.contains(u))
        .collect();
    (report, created)
}

fn before(n: u32) -> Placement {
    Placement::Before(TaskRef::Uid(uid(n)))
}

fn level_names(s: &Session) -> Vec<(String, u16)> {
    names(s)
}

#[test]
fn r244_relative_levels() {
    // Copier la récapitulative 1 (descendants inclus), coller avant 6
    // (niveau 2) : 1′ au niveau 2 sous 5, ses enfants au niveau 3.
    let mut s = plan_p();
    let clip = RowClip::capture(&s, &uids(&[1])).unwrap();
    let rel: Vec<u16> = clip.tasks.iter().map(|t| t.rel_level).collect();
    assert_eq!(rel, [0, 1, 1, 1]);
    let (report, created) = paste(&mut s, &[1], &before(6), PasteOptions::default());
    assert_eq!(report.tasks, 4);
    assert_eq!(report.tracking_dropped, 0, "aucun suivi dans P");
    assert_eq!(created, [8, 9, 10, 11]);
    let got = level_names(&s);
    let tail: Vec<(&str, u16)> = got.iter().skip(4).map(|(n, l)| (n.as_str(), *l)).collect();
    assert_eq!(
        tail,
        [
            ("Second œuvre", 1),
            ("Gros œuvre", 2),
            ("Fondations", 3),
            ("Murs", 3),
            ("Livraison", 3),
            ("Plâtrerie", 2),
            ("Peinture", 2),
        ]
    );
    // Deux feuilles de récapitulatives différentes : deux racines sœurs.
    let clip = RowClip::capture(&s, &uids(&[2, 7])).unwrap();
    let rel: Vec<u16> = clip.tasks.iter().map(|t| t.rel_level).collect();
    assert_eq!(rel, [0, 0]);
    // Une sous-tâche sélectionnée avec son parent : une seule fois.
    let clip = RowClip::capture(&s, &uids(&[1, 3, 3])).unwrap();
    assert_eq!(clip.tasks.len(), 4);
    // Coller en premier enfant d'une feuille : la racine descend d'un cran.
    let mut s = plan_p();
    paste(
        &mut s,
        &[5],
        &Placement::LastChildOf(TaskRef::Uid(uid(4))),
        PasteOptions::default(),
    );
    let got = level_names(&s);
    assert_eq!(got[4], ("Second œuvre".to_owned(), 3));
    assert_eq!(got[5], ("Plâtrerie".to_owned(), 4));
}

#[test]
fn r244_internal_links_only() {
    // Exemple : copier 3 et 4 (lien 3→4 interne, 2→3 et 3→6 externes),
    // coller avant 6 ⇒ « Murs′ » et « Livraison′ » au niveau de 6, lien
    // Murs′→Livraison′ FS, `links_dropped = 2`.
    let mut s = plan_p();
    let clip = RowClip::capture(&s, &uids(&[3, 4])).unwrap();
    assert_eq!(clip.links.len(), 1);
    assert_eq!(clip.links_dropped, 2);
    let (report, created) = paste(&mut s, &[3, 4], &before(6), PasteOptions::default());
    assert_eq!(report.links_dropped, 2);
    assert_eq!(created, [8, 9]);
    assert_eq!(
        (get(&s, 8).name.as_str(), get(&s, 8).outline_level.get()),
        ("Murs", 2)
    );
    assert_eq!(
        (get(&s, 9).name.as_str(), get(&s, 9).outline_level.get()),
        ("Livraison", 2)
    );
    assert_eq!(get(&s, 8).predecessors.len(), 0);
    let links: Vec<(u32, plannercraft_core::LinkType)> = get(&s, 9)
        .predecessors
        .iter()
        .map(|l| (l.predecessor.get(), l.kind))
        .collect();
    assert_eq!(links, [(8, plannercraft_core::LinkType::FinishToStart)]);
    // Tâches d'origine intactes (4 garde son prédécesseur 3).
    assert_eq!(get(&s, 4).predecessors.len(), 1);
    // Sans les liens : tous comptés écartés.
    let mut s = plan_p();
    let o = PasteOptions {
        links: false,
        ..PasteOptions::default()
    };
    let (report, created) = paste(&mut s, &[3, 4], &before(6), o);
    assert_eq!(report.links_dropped, 3);
    assert_eq!(get(&s, created[1]).predecessors.len(), 0);
}

#[test]
fn r244_resources_match_create_drop() {
    // « Maçon » existe à la cible, « Grue » non : Match la compte omise,
    // MatchOrCreate la crée (travail), Drop omet tout.
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon", "as": "$m" },
            { "op": "resource.add", "name": "Grue", "as": "$g" },
            { "op": "assignment.add", "task": 3, "resource": "$m", "units": { "text": "50%" } },
            { "op": "assignment.add", "task": 2, "resource": "$g" }
        ]),
    );
    let clip = RowClip::capture(&s, &uids(&[2, 3])).unwrap();
    assert_eq!(clip.assignments.len(), 2);
    apply(
        &mut s,
        json!([{ "op": "resource.delete", "uid": 2, "cascade": true }]),
    );
    let resources = s.project().resources.len();
    let at = before(6);

    let mut matched = s.clone();
    let (batch, rep) = paste_batch(&matched, &clip, &at, &PasteOptions::default()).unwrap();
    matched.apply(&batch, ApplyMode::Commit).unwrap();
    assert_eq!((rep.assignments_dropped, rep.resources_created), (1, 0));
    assert_eq!(matched.project().resources.len(), resources);
    let pasted: Vec<_> = matched
        .project()
        .assignments
        .iter()
        .filter(|a| a.task.get() >= 8)
        .map(|a| {
            (
                a.task.get(),
                a.resource.map(plannercraft_core::ResourceUid::get),
                a.units.get(),
            )
        })
        .collect();
    assert_eq!(pasted, [(9, Some(1), 5_000)]);

    let mut created = s.clone();
    let o = PasteOptions {
        resources: ResourcePaste::MatchOrCreate,
        ..PasteOptions::default()
    };
    let (batch, rep) = paste_batch(&created, &clip, &at, &o).unwrap();
    created.apply(&batch, ApplyMode::Commit).unwrap();
    assert_eq!((rep.assignments_dropped, rep.resources_created), (0, 1));
    assert_eq!(created.project().resources.len(), resources + 1);
    let grue = created
        .project()
        .resources
        .iter()
        .find(|x| x.name == "Grue")
        .unwrap();
    assert_eq!(grue.kind, plannercraft_core::ResourceKind::Work);
    assert!(
        created
            .project()
            .assignments
            .iter()
            .any(|a| a.task.get() == 8 && a.resource == Some(grue.uid()))
    );

    let mut dropped = s.clone();
    let o = PasteOptions {
        resources: ResourcePaste::Drop,
        ..PasteOptions::default()
    };
    let (batch, rep) = paste_batch(&dropped, &clip, &at, &o).unwrap();
    dropped.apply(&batch, ApplyMode::Commit).unwrap();
    assert_eq!((rep.assignments_dropped, rep.resources_created), (2, 0));
    assert!(dropped.project().assignments.iter().all(|a| a.task.get() < 8));
}

#[test]
fn r244_no_progress_no_baseline() {
    // 2 à 50 %, ligne de base posée, contrainte, échéance, notes ; 6
    // manuelle ; 7 inactive : la copie garde les champs du planning, pas
    // l'avancement, les réels, la ligne de base ni l'UID.
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "baseline.set" },
            { "op": "progress.update", "task": 2, "percent_complete": { "text": "50%" } },
            { "op": "task.set_constraint", "uid": 2, "kind": "snet", "date": "2026-10-20T08:00" },
            { "op": "task.set_deadline", "uid": 2, "deadline": "2026-10-30T17:00" },
            { "op": "task.update", "uid": 2, "notes": "Béton C25" },
            { "op": "task.update", "uid": 6, "mode": "manual" },
            { "op": "task.update", "uid": 6, "start": "2026-11-16T08:00" },
            { "op": "task.update", "uid": 7, "active": false }
        ]),
    );
    let (report, created) = paste(&mut s, &[2, 6, 7], &Placement::End, PasteOptions::default());
    assert_eq!(report.tasks, 3);
    // Ligne de base posée sur toutes : les trois perdent un suivi
    // (relecture finale de J8, compté pour l'avertissement de l'interface).
    assert_eq!(report.tracking_dropped, 3);
    assert_eq!(created, [8, 9, 10]);
    let (src, new) = (get(&s, 2), get(&s, 8));
    assert_eq!(new.progress.percent_complete.get(), 0);
    assert_eq!(
        new.progress,
        plannercraft_core::Task::new(TaskUid::new(8), String::new(), &s.project().options).progress
    );
    assert!(new.baselines.is_empty());
    assert!(!src.baselines.is_empty());
    assert_eq!(new.constraint, src.constraint);
    assert!(matches!(new.constraint, Constraint::StartNoEarlierThan { .. }));
    assert_eq!(new.deadline, src.deadline);
    assert_eq!(new.notes, "Béton C25");
    assert_eq!(new.duration, src.duration);
    // Manuelle : mêmes valeurs manuelles (début lun 16/11, 4 j).
    assert_eq!(get(&s, 9).mode, get(&s, 6).mode);
    assert!(matches!(
        get(&s, 9).mode,
        SchedulingMode::Manual { start: Some(_), .. }
    ));
    // Inactive gardée inactive.
    assert!(!get(&s, 10).active);
    assert!(get(&s, 8).active);
}

#[test]
fn r244_paste_before_blank_row_appends() {
    // Ligne vide sélectionnée ⇒ `Placement::End` : à la fin, niveau 1,
    // structure relative gardée.
    let mut s = plan_p();
    let (_, created) = paste(&mut s, &[5], &Placement::End, PasteOptions::default());
    assert_eq!(created, [8, 9, 10]);
    let got = level_names(&s);
    let tail: Vec<(&str, u16)> = got.iter().skip(7).map(|(n, l)| (n.as_str(), *l)).collect();
    assert_eq!(tail, [("Second œuvre", 1), ("Plâtrerie", 2), ("Peinture", 2)]);
    // Le lien interne 6→7 recréé, 3→6 écarté.
    assert_eq!(get(&s, 10).predecessors.len(), 1);
    assert_eq!(get(&s, 10).predecessors[0].predecessor.get(), 9);
    assert_eq!(get(&s, 9).predecessors.len(), 0);
    // Une entrée d'annulation : annuler retire tout le collage.
    s.undo().unwrap();
    assert_eq!(s.project().tasks.len(), 7);
}

#[test]
fn r244_unknown_task_and_empty_clip() {
    let s = plan_p();
    assert_eq!(
        RowClip::capture(&s, &[TaskUid::new(99)]),
        Err(plannercraft_automation::clip::ClipError::UnknownTask(
            TaskUid::new(99)
        ))
    );
    let empty = RowClip::default();
    assert_eq!(
        paste_batch(&s, &empty, &Placement::End, &PasteOptions::default()).map(|_| ()),
        Err(plannercraft_automation::clip::ClipError::Unrecognized)
    );
}

/// Relecture finale de J8 : coller 300 feuilles manuelles dans un projet de
/// 5 000 tâches passe en un lot. Le collage donne le début de chaque tâche
/// manuelle : ses dates ne sont plus figées d'abord (un recalcul du
/// planning par tâche, refusé par le budget `schedule_recomputes` au-delà
/// d'environ 200) ; début et durée manuels de la source.
#[test]
fn r244_paste_many_manual_within_budget() {
    use plannercraft_automation::helpers;
    use plannercraft_tests::generate::large_network;
    use plannercraft_tests::session::session;
    let network = large_network(11);
    let leaves: Vec<TaskUid> = network
        .tasks
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            !t.is_null
                && network
                    .tasks
                    .iter()
                    .nth(i + 1)
                    .is_none_or(|n| n.outline_level <= t.outline_level)
        })
        .map(|(_, t)| t.uid())
        .take(300)
        .collect();
    let mut s = session(network);
    let freeze = helpers::toggle_mode(&s, &leaves).unwrap().unwrap();
    s.apply(&freeze, ApplyMode::Commit).unwrap();
    let clip = RowClip::capture(&s, &leaves).unwrap();
    assert_eq!(clip.tasks.len(), 300);
    let (batch, report) = paste_batch(&s, &clip, &Placement::End, &PasteOptions::default()).unwrap();
    assert_eq!(report.tasks, 300);
    let before = s.project().tasks.len();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    let project = s.project();
    assert_eq!(project.tasks.len(), before + 300);
    for (src, new) in leaves.iter().zip(project.tasks.iter().skip(before)) {
        let src = project.tasks.get(*src).unwrap();
        // Début et durée de la source (la fin suit le calendrier de la
        // tâche collée, que R-244 ne recolle pas).
        let (
            SchedulingMode::Manual {
                start: pasted_start,
                duration: pasted_duration,
                ..
            },
            SchedulingMode::Manual {
                start: source_start,
                duration: source_duration,
                ..
            },
        ) = (new.mode, src.mode)
        else {
            panic!("« {} » : manuelle attendue", src.name);
        };
        assert_eq!(
            (pasted_start, pasted_duration),
            (source_start, source_duration),
            "« {} »",
            src.name
        );
    }
}
