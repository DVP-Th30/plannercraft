//! Contrat J6 (lot L0, ADR-0014) : types, squelettes et registre. Les
//! commandes nouvelles (`progress.update`, `progress.reschedule`,
//! `baseline.set`) se lisent et se relisent, hors du schéma des lots, et
//! s'exécutent depuis le lot L6 (jamais `command.not_yet_available`) ; les
//! six entrées J6 du registre sont réservées avec leur schéma ; les
//! squelettes du cœur répondent sans paniquer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::collections::BTreeSet;

use plannercraft_automation::command::params::{ExplainParams, VarianceParams, WhatIfParams};
use plannercraft_automation::command::{BaselineSet, Command, PercentJson, RollupJson, SideJson, TaskRef};
use plannercraft_automation::error::codes;
use plannercraft_automation::registry::{Availability, Kind, by_mcp_name, lookup};
use plannercraft_automation::text::Language;
use plannercraft_automation::{
    AnchorSpec, ApplyMode, ApplyOptions, Batch, NewProject, REGISTRY, Session, Workspace,
};
use plannercraft_automation::{CommandError, CommandErrorKind};
use plannercraft_core::baseline::{self, BaselineError, BaselineSpec, Rollup, Scope};
use plannercraft_core::calendar::CalendarSet;
use plannercraft_core::schedule::explain::{ExplainError, Side, Step, explain};
use plannercraft_core::{BaselineNumber, Driver, Moment, ProgressState, TaskOutcome, TaskUid};
use serde_json::json;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn session_with_task() -> Session {
    let init = NewProject {
        name: "Projet1".to_owned(),
        anchor: AnchorSpec::Start("2026-10-19T08:00".parse().unwrap()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [{ "op": "task.add", "name": "A", "duration": { "text": "2j" } }]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

/// Vrai si le lot est refusé `command.not_yet_available` (simulation).
fn not_yet(s: &mut Session, batch: &Batch) -> bool {
    s.apply(batch, ApplyMode::DryRun)
        .is_err_and(|e| e.code == codes::COMMAND_NOT_YET_AVAILABLE)
}

/// Les six entrées J6 (ADR-0014) : cinq réservées depuis J4, une créée
/// en L0 (`progress.reschedule`).
const J6_ENTRIES: [(&str, &str, Kind); 6] = [
    ("analysis.baseline_variance", "baseline_variance", Kind::Analysis),
    ("analysis.explain_date", "explain_date", Kind::Analysis),
    ("analysis.what_if", "what_if", Kind::Analysis),
    ("baseline.set", "set_baseline", Kind::Write),
    ("progress.reschedule", "reschedule_remaining", Kind::Write),
    ("progress.update", "update_progress", Kind::Write),
];

/// Entrées J6 activées : toutes depuis le lot L8 (`analysis.explain_date`
/// au lot L7).
const ACTIVE: [&str; 6] = [
    "analysis.baseline_variance",
    "analysis.explain_date",
    "analysis.what_if",
    "baseline.set",
    "progress.reschedule",
    "progress.update",
];

#[test]
fn contract_compiles() {
    let mut s = session_with_task();
    let before = s.project().clone();
    let generation = s.generation();

    // Commandes nouvelles : lues, relues à l'identique, exécutées (lot L6)
    // en simulation, sans effet sur la session.
    let ops = [
        json!({"op": "progress.update", "task": 1, "percent_complete": {"value": 4_000},
               "actual_start": "2026-10-19T08:00", "remaining_duration": {"text": "1j"},
               "percent_work_complete": {"text": "40%"}, "physical_percent_complete": {"value": 0}}),
        json!({"op": "progress.update", "task": 1, "actual_finish": "2026-10-20T12:00",
               "actual_duration": {"minutes": 720}, "actual_work": {"text": "6h"}}),
        json!({"op": "progress.reschedule"}),
        json!({"op": "progress.reschedule", "after": "2026-11-13T17:00", "tasks": [1]}),
        json!({"op": "baseline.set"}),
        json!({"op": "baseline.set", "number": 1, "tasks": [1], "rollup": "to_all_summaries", "overwrite": true}),
    ];
    for (i, op) in ops.iter().enumerate() {
        let batch = Batch::from_json(json!({ "format_version": 1, "ops": [op] })).unwrap();
        let again: Batch = serde_json::from_value(serde_json::to_value(&batch).unwrap()).unwrap();
        assert_eq!(again, batch, "aller-retour {i}");
        assert_eq!(batch.ops[0].id(), op["op"].as_str().unwrap());
        assert!(!not_yet(&mut s, &batch), "{i}");
    }
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": [ops[5]] })).unwrap();
    let Command::BaselineSet(set) = &batch.ops[0] else {
        panic!("baseline.set attendu");
    };
    assert_eq!(
        set,
        &BaselineSet {
            number: Some(1),
            tasks: Some(vec![TaskRef::Uid(TaskUid::new(1))]),
            rollup: Some(RollupJson::ToAllSummaries),
            overwrite: true,
        }
    );
    // Champ inconnu refusé ; `overwrite` absent du JSON quand faux.
    assert!(Batch::from_json(json!({"format_version": 1, "ops": [{"op": "baseline.set", "x": 1}]})).is_err());
    let text = serde_json::to_string(&BaselineSet::default()).unwrap();
    assert_eq!(text, "{}");
    let p: PercentJson = serde_json::from_value(json!({"text": "40%"})).unwrap();
    assert_eq!(p.value, None);
    assert!(serde_json::from_value::<PercentJson>(json!({"value": 1, "x": 2})).is_err());
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);

    // Simulation (lot L8) : lot vide accepté, état inchangé, session
    // intacte.
    let batch = Batch::from_json(json!({ "format_version": 1, "ops": [] })).unwrap();
    let sim = s.simulate(&batch, ApplyOptions::default()).unwrap();
    assert!(!sim.outcome.committed);
    assert_eq!(&sim.project, s.project());
    assert_eq!(s.generation(), generation);

    // Paramètres des analyses : lus, champs inconnus refusés.
    let e: ExplainParams = serde_json::from_value(json!({"task": 1, "side": "finish", "depth": 5})).unwrap();
    assert_eq!(e.side, Some(SideJson::Finish));
    assert_eq!(Side::from(SideJson::Finish), Side::Finish);
    let w: WhatIfParams = serde_json::from_value(json!({
        "ops": [{"op": "task.update", "uid": 1, "duration": {"text": "7j"}}],
        "baseline": 0, "explain": [1]
    }))
    .unwrap();
    assert_eq!(w.ops.len(), 1);
    assert!(!w.detail);
    let v: VarianceParams = serde_json::from_value(json!({"only_varied": true})).unwrap();
    assert!(v.only_varied && v.baseline.is_none());
    assert!(serde_json::from_value::<VarianceParams>(json!({"bogus": 1})).is_err());
    assert_eq!(Rollup::from(RollupJson::FromSubtasks), Rollup::FromSubtasks);

    // Codes d'erreur J6 : messages dans les deux langues.
    for (code, kind) in [
        (
            codes::PROGRESS_SUMMARY_NOT_EDITABLE,
            CommandErrorKind::ProgressSummaryNotEditable,
        ),
        (codes::PROGRESS_INACTIVE, CommandErrorKind::ProgressInactive),
        (
            codes::PROGRESS_INVALID_ORDER,
            CommandErrorKind::ProgressInvalidOrder,
        ),
        (
            codes::BASELINE_EXISTS,
            CommandErrorKind::BaselineExists { number: 0, count: 26 },
        ),
        (
            codes::BASELINE_ROLLUP_NEEDS_SELECTION,
            CommandErrorKind::BaselineRollupNeedsSelection,
        ),
        (
            codes::EXPLAIN_NOT_SCHEDULED,
            CommandErrorKind::ExplainNotScheduled(TaskUid::new(3)),
        ),
    ] {
        let e = CommandError::new(code, kind);
        let (fr, en) = (e.message(Language::Fr), e.message(Language::En));
        assert!(!fr.is_empty() && fr != en, "{code}");
    }

    core_contract(&s);

    // Surfaces (lot L8) : toutes les entrées J6 sont routées ; sans projet
    // ouvert, l'espace de travail répond `session.none`, jamais
    // `command.not_yet_available`.
    let mut ws = Workspace::new(now(), Language::Fr);
    for (id, _, _) in J6_ENTRIES {
        assert!(ACTIVE.contains(&id), "{id}");
        let err = ws.call(id, json!({})).unwrap_err();
        assert_ne!(err.code, codes::COMMAND_NOT_YET_AVAILABLE, "{id}");
    }
}

/// Types du cœur (contrat J6) : planning sans avancement, JSON inchangé,
/// squelettes qui répondent sans paniquer (lignes de base remplies en L4).
fn core_contract(s: &Session) {
    let schedule = s.schedule();
    let Some(TaskOutcome::Scheduled(t)) = schedule.get(TaskUid::new(1)) else {
        panic!("tâche 1 planifiée attendue");
    };
    assert_eq!(t.progress, None);
    // R-230 (lot L1) : la passe avant pose la borne brute du pilote ; seul
    // ce champ s'ajoute au JSON d'un planning sans avancement.
    assert_eq!(t.driver_bound, Some(t.early.start));
    let text = serde_json::to_string(schedule).unwrap();
    assert!(text.contains("driver_bound") && !text.contains("\"progress\""));
    assert!(!text.contains("remaining_start"));

    // Faits d'avancement (R-200) : tâche neuve non commencée.
    let task = s.project().tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(
        plannercraft_core::progress::facts(task).state,
        ProgressState::NotStarted
    );

    // Explication : contrôles d'entrée, première étape.
    let p = s.project();
    let x = explain(p, schedule, TaskUid::new(1), Side::Start, 5).unwrap();
    assert_eq!(x.date, t.start);
    assert_eq!(
        x.steps.first(),
        Some(&Step::Driver {
            driver: Driver::ProjectStart,
            raw: t.driver_bound
        })
    );
    assert_eq!(
        explain(p, schedule, TaskUid::new(9), Side::Start, 5),
        Err(ExplainError::UnknownTask(TaskUid::new(9)))
    );
    assert_eq!(
        explain(p, schedule, TaskUid::new(1), Side::Finish, 0),
        Err(ExplainError::DepthOutOfRange(0))
    );
    assert_eq!(
        explain(p, schedule, TaskUid::new(1), Side::Finish, 51),
        Err(ExplainError::DepthOutOfRange(51))
    );

    // Lignes de base (lot L4) : contrôles de portée, capture de la tâche 1,
    // écriture dans une copie du projet.
    let spec = |scope, rollup| BaselineSpec {
        number: BaselineNumber::PRIMARY,
        scope,
        rollup,
        overwrite: false,
    };
    let edit = baseline::capture(p, schedule, &spec(Scope::Project, Rollup::None)).unwrap();
    assert!(edit.tasks.contains_key(&TaskUid::new(1)) && !edit.cost_copied);
    assert_eq!(
        baseline::capture(p, schedule, &spec(Scope::Project, Rollup::ToAllSummaries)),
        Err(BaselineError::RollupNeedsSelection)
    );
    assert_eq!(
        baseline::capture(
            p,
            schedule,
            &spec(Scope::Tasks(vec![TaskUid::new(7)]), Rollup::None)
        ),
        Err(BaselineError::UnknownTask(TaskUid::new(7)))
    );
    let (set, _) = CalendarSet::compile(p);
    let v = baseline::variance(p, schedule, &set, BaselineNumber::PRIMARY);
    assert_eq!(v.project.uid, TaskUid::PROJECT_SUMMARY);
    assert_eq!(v.project.baseline, None);
    let mut copy = p.clone();
    let dropped = baseline::apply(&mut copy, BaselineNumber::PRIMARY, edit);
    assert_eq!(dropped.len(), 0);
    assert!(
        copy.tasks
            .get(TaskUid::new(1))
            .unwrap()
            .baselines
            .contains_key(&BaselineNumber::PRIMARY)
    );
    assert!(p.tasks.get(TaskUid::new(1)).unwrap().baselines.is_empty());
}

/// Les entrées J6 : présentes, triées avec le reste, noms MCP uniques et
/// conformes, réservées au jalon 6 avec un schéma d'objet réel.
#[test]
fn registry_sorted_and_unique() {
    let ids: Vec<&str> = REGISTRY.iter().map(|s| s.id).collect();
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "REGISTRY non trié ou en double : {pair:?}");
    }
    let names: BTreeSet<&str> = REGISTRY.iter().map(|s| s.mcp_name).collect();
    assert_eq!(names.len(), REGISTRY.len());
    for (id, name, kind) in J6_ENTRIES {
        let spec = lookup(id).unwrap();
        assert_eq!(spec.mcp_name, name);
        assert_eq!(by_mcp_name(name).unwrap().id, id);
        assert_eq!(spec.kind, kind, "{id}");
        if ACTIVE.contains(&id) {
            assert_eq!(spec.available, Availability::Now, "{id}");
            assert!(spec.is_listed());
        } else {
            assert_eq!(spec.available, Availability::Milestone(6), "{id}");
            assert!(!spec.is_listed());
        }
        let schema = serde_json::to_value((spec.schema)()).unwrap();
        assert_eq!(schema["type"], json!("object"), "{id}");
        assert!(schema["properties"].is_object(), "{id} : schéma vide");
    }
}
