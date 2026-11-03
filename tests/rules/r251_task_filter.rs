//! R-251 — `TaskFilter` partagé (interface, `task.list`, `list_tasks`) :
//! une tâche passe si elle satisfait **tous** les critères posés ;
//! `incomplete` = feuille planifiée à moins de 100 % ; `name_contains`
//! sans casse ni accents (NFD, marques retirées, minuscules) ; `resource`
//! par nom exact ; `show_related_summaries` ajoute les récapitulatives
//! ancêtres d'une tâche retenue (vrai pour les préréglages de l'interface,
//! faux pour `task.list`). Repère : projet « P » de `docs/plans/j8.md` § 2.
//! Hypothèse (S79 relue : rien sur les récapitulatives liées).

use plannercraft_automation::Session;
use plannercraft_automation::command::params::TaskListParams;
use plannercraft_automation::filter::{FilterCtx, FilterHits, FilterPreset, TaskFilter, fold_text};
use plannercraft_automation::view;
use plannercraft_tests::daily::plan_p;
use plannercraft_tests::task_edit::apply;
use serde_json::json;

fn hits(s: &Session, f: &TaskFilter) -> FilterHits {
    let p = s.project();
    f.hits(&FilterCtx {
        project: p,
        schedule: s.schedule(),
        status: p.effective_status_date(),
    })
}

/// UID des lignes montrées.
fn shown(s: &Session, f: &TaskFilter) -> Vec<u32> {
    let h = hits(s, f);
    s.project()
        .tasks
        .iter()
        .zip(&h.shown)
        .filter(|(_, on)| **on)
        .map(|(t, _)| t.uid().get())
        .collect()
}

/// UID listés par `task.list`.
fn listed(s: &Session, p: &TaskListParams) -> Vec<u32> {
    view::list(s, p).unwrap().tasks.iter().map(|t| t.uid).collect()
}

fn with_2_done() -> Session {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([{ "op": "progress.update", "task": 2, "percent_complete": { "text": "100%" } }]),
    );
    s
}

#[test]
fn r251_incomplete_leaf_only() {
    let s = with_2_done();
    let f = TaskFilter {
        incomplete: Some(true),
        ..TaskFilter::default()
    };
    // Feuilles < 100 %, jalon 4 compris ; ni 2 (achevée) ni les
    // récapitulatives 1 et 5.
    assert_eq!(shown(&s, &f), [3, 4, 6, 7]);
    let h = hits(&s, &f);
    assert_eq!((h.direct, h.related), (4, 0));
    let p = TaskListParams {
        incomplete: Some(true),
        ..TaskListParams::default()
    };
    assert_eq!(listed(&s, &p), [3, 4, 6, 7]);
    // `false` : le complément (récapitulatives et feuille achevée).
    let p = TaskListParams {
        incomplete: Some(false),
        ..TaskListParams::default()
    };
    assert_eq!(listed(&s, &p), [1, 2, 5]);
}

#[test]
fn r251_related_summaries() {
    let s = with_2_done();
    // Exemple de R-251 : « Inachevées » ⇒ 3, 4, 6, 7, récapitulatives
    // liées 1 et 5 affichées.
    let f = TaskFilter::from_preset(&FilterPreset::Incomplete);
    assert!(f.show_related_summaries);
    assert_eq!(shown(&s, &f), [1, 3, 4, 5, 6, 7]);
    let h = hits(&s, &f);
    assert_eq!((h.direct, h.related), (4, 2));
    // Une seule feuille retenue : seule sa récapitulative est ajoutée.
    let f = TaskFilter {
        name_contains: Some("peint".to_owned()),
        show_related_summaries: true,
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &f), [5, 7]);
    // `task.list` : jamais de récapitulatives liées (sortie J4-J7).
    let p = TaskListParams {
        name_contains: Some("peint".to_owned()),
        ..TaskListParams::default()
    };
    assert_eq!(listed(&s, &p), [7]);
    // Filtre « Toutes » : tout est montré, rien d'ajouté.
    let all = TaskFilter::from_preset(&FilterPreset::All);
    assert_eq!(shown(&s, &all), [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(hits(&s, &all).related, 0);
}

#[test]
fn r251_name_contains_folds_accents() {
    let s = plan_p();
    assert_eq!(fold_text("Plâtrerie"), "platrerie");
    assert_eq!(fold_text("ÉCRÊTÉ"), "ecrete");
    assert_eq!(fold_text("Œuvre"), "œuvre");
    for needle in ["platre", "PLÂTRE", "Platr"] {
        let f = TaskFilter {
            name_contains: Some(needle.to_owned()),
            ..TaskFilter::default()
        };
        assert_eq!(shown(&s, &f), [6], "{needle}");
    }
    let mut ws = plannercraft_automation::Workspace::new(
        plannercraft_tests::moment("2026-10-19T08:00"),
        plannercraft_automation::text::Language::Fr,
    );
    ws.call("project.new", json!({ "name": "P", "start": "2026-10-19T08:00" }))
        .unwrap();
    ws.call(
        "batch.apply",
        json!({ "ops": [
            { "op": "task.add", "name": "Plâtrerie" },
            { "op": "task.add", "name": "Peinture" }
        ] }),
    )
    .unwrap();
    let v = ws
        .call("task.list", json!({ "name_contains": "platre" }))
        .unwrap();
    assert_eq!(v["total"], json!(1));
    assert_eq!(v["tasks"][0]["name"], json!("Plâtrerie"));
    // Requête bornée à 256 caractères.
    let err = ws
        .call("task.list", json!({ "name_contains": "a".repeat(257) }))
        .unwrap_err();
    assert_eq!(err.code, plannercraft_automation::error::codes::LIMIT_EXCEEDED);
}

#[test]
fn r251_resource_exact_name() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon" },
            { "op": "resource.add", "name": "Maçon B" },
            { "op": "assignment.add", "task": 2, "resource": 1 },
            { "op": "assignment.add", "task": 3, "resource": 2 }
        ]),
    );
    let by = |name: &str| TaskFilter {
        resource: Some(name.to_owned()),
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &by("Maçon")), [2]);
    assert_eq!(shown(&s, &by("Maçon B")), [3]);
    // Comparaison exacte : ni casse ni sous-chaîne.
    assert_eq!(shown(&s, &by("maçon")), Vec::<u32>::new());
    assert_eq!(shown(&s, &by("Maç")), Vec::<u32>::new());
    let f = TaskFilter::from_preset(&FilterPreset::UsingResource("Maçon".to_owned()));
    assert_eq!(shown(&s, &f), [1, 2]);
}

/// Relecture finale de J8 : un nom de ressource portant des espaces de
/// bord (MSPDI, `resource.add`) est retrouvé par son nom sans eux, celui
/// que propose la fenêtre « Utilisant la ressource… ».
#[test]
fn r251_resource_name_edge_spaces() {
    let mut s = plan_p();
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon " },
            { "op": "assignment.add", "task": 2, "resource": 1 }
        ]),
    );
    assert_eq!(s.project().resources.iter().next().unwrap().name, "Maçon ");
    let by = |name: &str| TaskFilter {
        resource: Some(name.to_owned()),
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &by("Maçon")), [2]);
    assert_eq!(shown(&s, &by(" Maçon ")), [2]);
    assert_eq!(shown(&s, &by("maçon")), Vec::<u32>::new());
}

#[test]
fn r251_conjunction() {
    let mut s = plan_p();
    let feuilles_critiques = TaskFilter {
        critical: Some(true),
        summary: Some(false),
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &feuilles_critiques), [2, 3, 6, 7]);
    // Critique ET « m » dans le nom : Murs seulement.
    let f = TaskFilter {
        name_contains: Some("M".to_owned()),
        ..feuilles_critiques.clone()
    };
    assert_eq!(shown(&s, &f), [3]);
    // Critique ET jalon : 4 a de la marge, rien.
    let f = TaskFilter {
        critical: Some(true),
        milestone: Some(true),
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &f), Vec::<u32>::new());
    // `matches` et `hits` disent la même chose, tâche par tâche.
    let p = s.project();
    let ctx = FilterCtx {
        project: p,
        schedule: s.schedule(),
        status: p.effective_status_date(),
    };
    let h = feuilles_critiques.hits(&ctx);
    for (pos, t) in p.tasks.iter().enumerate() {
        assert_eq!(feuilles_critiques.matches(&ctx, pos, t), h.shown[pos], "{pos}");
    }
    // Actives / inactives ; plage de dates (semaine du 09/11 : 3 finit le
    // 06/11, 6 du 09 au 12/11, 7 du 13 au 17/11).
    apply(
        &mut s,
        json!([{ "op": "task.update", "uid": 7, "active": false }]),
    );
    let f = TaskFilter {
        active: Some(false),
        ..TaskFilter::default()
    };
    assert_eq!(shown(&s, &f), [7]);
    let f = TaskFilter::from_preset(&FilterPreset::DateRange {
        from: plannercraft_tests::day("2026-11-09"),
        to: plannercraft_tests::day("2026-11-13"),
    });
    assert_eq!(shown(&s, &f), [5, 6]);
    let p = TaskListParams {
        active: Some(true),
        summary: Some(false),
        outline_max: Some(2),
        ..TaskListParams::default()
    };
    assert_eq!(listed(&s, &p), [2, 3, 4, 6]);
}
