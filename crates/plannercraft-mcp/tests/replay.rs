//! Rejeu du scénario `build_30` en mémoire (lot L18, critère de fin J4) :
//! « un agent construit un planning de 30 tâches via MCP », sans LLM.
//!
//! Le scénario (`tests/mcp/scenarios/build_30.jsonl`) est rejoué sur
//! `serve_on` + `tokio::io::duplex`, `--now 2026-10-19T08:00`, langue `fr`.
//! Vérifications du cahier (`docs/plans/j4.md`, L18) : 1 (résultats égaux
//! aux attendus), 2 (MSPDI enregistré), 3 (undo jusqu'au projet vide),
//! 5 (budget < 2 s). La 4 (vrai binaire en stdio) est dans
//! `apps/plannercraft-cli/tests/mcp_replay.rs`, la 6 (HTTP) dans
//! `tests/replay_http.rs`. `PLANNERCRAFT_BLESS=1` régénère les attendus (à relire).
//!
//! J6 (lot L10) : scénario de suivi `tracking_j6` (`--now 2026-11-02T08:00`)
//! et harnais de l'essai réel n° 3 (`replay_claude_run_3`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net"
)]

#[path = "common/replay.rs"]
mod replay;

use std::path::Path;
use std::time::{Duration, Instant};

use plannercraft_automation::text::Language;
use plannercraft_core::{Moment, Severity};
use plannercraft_mcp::{McpServeError, ServeOptions, serve_on};
use replay::{NOW, Scenario, TempDir, logged, normalized, repo_root, substitute};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

const SCENARIO: &str = "build_30";

/// Budget du rejeu complet en debug (vérification 5). Temps réel mesuré
/// pendant que les autres rejeux tournent en parallèle sur les runners de la
/// CI (2 à 4 cœurs) : 10 s, qui attrape une explosion de complexité sans
/// dépendre de la charge (0,2 à 0,5 s par rejeu en local, 2,3 s observé sur
/// la CI Linux le 2026-10-19 avec huit rejeux simultanés).
const BUDGET: Duration = Duration::from_secs(10);

/// Le budget n'est vérifié que si `PLANNERCRAFT_REPLAY_BUDGET` est posée
/// (job CI `mcp`, Linux, rejeux seuls) : sur les runners Windows et macOS de
/// `cargo test --workspace`, le temps réel dépend de la charge (20 à 24 s
/// observés sous Windows le 2026-11-03) sans rien dire du code.
fn budget_checked() -> bool {
    std::env::var_os("PLANNERCRAFT_REPLAY_BUDGET").is_some()
}

fn root() -> std::path::PathBuf {
    repo_root(env!("CARGO_MANIFEST_DIR"))
}

fn now() -> Moment {
    NOW.parse().unwrap()
}

/// « Maintenant » du rejeu du scénario `name`.
fn now_of(name: &str) -> Moment {
    replay::now_of(name).parse().unwrap()
}

fn opts(journal: Option<&Path>) -> ServeOptions {
    opts_at(now(), journal)
}

fn opts_at(now: Moment, journal: Option<&Path>) -> ServeOptions {
    let mut o = ServeOptions::new(now, Language::Fr);
    o.ignore_app_guard = true;
    o.journal = journal.map(Path::to_path_buf);
    o
}

type Client = RunningService<RoleClient, ()>;

async fn connect(o: ServeOptions) -> (Client, JoinHandle<Result<(), McpServeError>>) {
    let (client_io, server_io) = tokio::io::duplex(1 << 22);
    let (r, w) = tokio::io::split(server_io);
    let server = tokio::spawn(serve_on(r, w, o));
    let client = ().serve(client_io).await.unwrap();
    (client, server)
}

/// Appel d'outil ; résultat sous la forme du journal.
async fn call(client: &Client, tool: &str, params: &Value) -> Value {
    let mut req = CallToolRequestParams::new(tool.to_owned());
    if let Value::Object(args) = params {
        req = req.with_arguments(args.clone());
    }
    let r = client.call_tool(req).await.unwrap();
    logged(r.is_error, r.structured_content)
}

/// Rejoue tous les appels ; résultats ramenés au jeton `$TMP`.
async fn run(client: &Client, scenario: &Scenario, tmp: &Path) -> Vec<Value> {
    let mut got = Vec::with_capacity(scenario.calls.len());
    for c in &scenario.calls {
        let r = call(client, &c.tool, &substitute(&c.params, tmp)).await;
        got.push(normalized(&r, tmp));
    }
    got
}

/// Rejeu complet avec journal ; rend le journal produit et la durée.
async fn replay_with_journal(scenario: &Scenario, tmp: &TempDir) -> (Vec<Value>, String, Duration) {
    let journal = tmp.path().join("run.jsonl");
    replay_opened(scenario, tmp, journal, None).await
}

/// Comme [`replay_with_journal`], serveur lancé avec `--open open` si donné.
async fn replay_opened(
    scenario: &Scenario,
    tmp: &TempDir,
    journal: std::path::PathBuf,
    open: Option<std::path::PathBuf>,
) -> (Vec<Value>, String, Duration) {
    let mut o = opts_at(now_of(&scenario.name), Some(&journal));
    o.open = open;
    let (client, server) = connect(o).await;
    let t0 = Instant::now();
    let got = run(&client, scenario, tmp.path()).await;
    let elapsed = t0.elapsed();
    client.cancel().await.unwrap();
    server.await.unwrap().unwrap();
    let produced = std::fs::read_to_string(&journal).unwrap();
    (got, produced, elapsed)
}

/// Rejeu en mémoire du scénario `name` : résultats égaux aux attendus,
/// journal produit égal au scénario, budget de durée ; si `saved` (nom du
/// fichier enregistré sous `$TMP`), MSPDI `normalize` égal à l'attendu
/// relu. `PLANNERCRAFT_BLESS=1` régénère les attendus au lieu de comparer.
async fn replay_memory(name: &str, saved: Option<&str>) -> Scenario {
    let root = root();
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("memory");
    let (got, produced, elapsed) = replay_with_journal(&scenario, &tmp).await;
    let saved = saved.map(|f| tmp.path().join(f));
    if replay::blessing() {
        replay::bless(&root, &scenario, &produced, tmp.path(), saved.as_deref());
        return scenario;
    }
    replay::assert_results(&scenario, &got);
    replay::assert_journal(&scenario, &produced, tmp.path());
    assert!(
        !budget_checked() || elapsed < BUDGET,
        "rejeu de {name} en {elapsed:?}, budget {BUDGET:?}"
    );
    if let Some(saved) = saved {
        let read = |p: &Path| {
            let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
            plannercraft_mspdi::read(&bytes, &plannercraft_mspdi::ReadOptions::new(now_of(name)))
                .unwrap_or_else(|e| panic!("{} : {e}", p.display()))
                .project
        };
        let w = plannercraft_mspdi::write::WriteOptions::default();
        assert_eq!(
            plannercraft_mspdi::normalize(&read(&saved), &w),
            plannercraft_mspdi::normalize(&read(&replay::expected_xml_path(&root, name)), &w),
            "{name} : MSPDI enregistré différent de l'attendu"
        );
    }
    scenario
}

/// Scénario J8 (lot L6, `docs/plans/j8.md`) : modèles listés, Présentation
/// appliquée (simulée puis validée), style lu, mise en forme directe d'un
/// type de barre (contraste signalé) annulée, modèle nommé, refus de
/// `reset` avec `template`, enregistrement puis annulation du style.
#[tokio::test]
async fn replay_style_j8_memory() {
    let s = replay_memory("style_j8", Some("style_j8.xml")).await;
    if replay::blessing() {
        return;
    }
    let ok = |i: usize| ok_at(&s, i);
    assert_eq!(ok(2)["templates"].as_array().map(Vec::len), Some(6));
    assert_eq!(ok(3)["committed"], json!(false));
    assert_eq!(ok(5)["source"], json!("document"));
    assert_eq!(ok(5)["base"], json!("presentation"));
    let issues = ok(7)["contrast_issues"].as_array().cloned().unwrap_or_default();
    assert!(
        issues.iter().any(|i| i["role"] == json!("critical")),
        "{issues:?}"
    );
    assert_eq!(ok(9)["name"], json!("Client"));
    assert_eq!(
        error_code(s.calls[10].expected.as_ref().unwrap()),
        Some("param.invalid")
    );
    // Fichier enregistré avec le style choisi ; l'annulation qui suit le
    // retire de la session.
    let saved = std::fs::read_to_string(replay::expected_xml_path(&root(), "style_j8")).unwrap();
    assert!(saved.contains("<?plannercraft-style {\"base\":\"presentation\",\"v\":1}?>"));
    assert_eq!(ok(13)["source"], json!("default"));
}

/// Scénario J8 (lot L16, `docs/plans/j8.md`) : usage quotidien par un
/// agent sur le projet « P » du cahier — filtres de `list_tasks`
/// (inachevées, nom sans casse ni accents, jalons, inactives ; R-251),
/// bascule manuelle (R-253 : 6 ne bouge plus quand 3 s'allonge), dates
/// d'une tâche manuelle (R-254), retour en automatique, inactivation
/// (R-255), export PDF A3 paysage sur 2 pages en largeur, enregistrement.
#[tokio::test]
async fn replay_daily_j8_memory() {
    let s = replay_memory("daily_j8", Some("daily_j8.xml")).await;
    if replay::blessing() {
        return;
    }
    let ok = |i: usize| ok_at(&s, i);
    let uids = |i: usize| -> Vec<u64> {
        ok(i)["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["uid"].as_u64().unwrap())
            .collect()
    };
    // R-251 : feuilles < 100 % (jalon compris), sans récapitulatives liées.
    assert_eq!(uids(3), [3, 4, 6, 7]);
    assert_eq!(uids(4), [7], "« PEINT » ⇒ Peinture");
    assert_eq!(uids(5), [4]);
    let task_after = |i: usize| {
        ok(i)["diff"]["changed"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["uid"] == json!(6))
            .cloned()
            .unwrap_or_else(|| panic!("appel {} : 6 inchangée", i + 1))
    };
    // R-253 : bascule en manuel sans effet sur les dates ; 3 allongée de
    // 2 j ⇒ 6 manuelle garde son début (S73).
    assert_eq!(ok(6)["diff"]["changed_total"], json!(0));
    assert_eq!(ok(8)["start"], json!("2026-11-09T08:00"));
    // R-254 : début saisi ⇒ fin = début + 4 j ; fin avant le début refusée.
    assert_eq!(task_after(9)["start_after"], json!("2026-11-16T08:00"));
    assert_eq!(task_after(9)["finish_after"], json!("2026-11-19T17:00"));
    assert_eq!(
        error_code(s.calls[10].expected.as_ref().unwrap()),
        Some("task.manual_finish_before_start")
    );
    // Retour en automatique : 6 après la fin de 3 (mar 10/11).
    assert_eq!(task_after(11)["start_after"], json!("2026-11-11T08:00"));
    // R-255 et filtre « inactive ».
    assert_eq!(uids(13), [2]);
    // Export PDF : A3 paysage, 2 pages en largeur remplies, à 100 %.
    assert_eq!(ok(14)["pages"], json!(2));
    assert_eq!(ok(14)["scale_pct"], json!(100));
}

/// Vérifications 1 et 5, plus le journal produit égal au scénario.
#[tokio::test]
async fn replay_build_30_memory() {
    replay_memory(SCENARIO, Some("build_30.xml")).await;
}

/// Scénario J5 (lot L9, `docs/plans/j5.md`) : ressources, affectations,
/// types de tâche, pilotage par l'effort, surutilisation, rejoué en mémoire ;
/// MSPDI exporté égal à l'attendu par `normalize`.
///
/// Vérifications du cahier, sur les résultats attendus : `dry_run` sans
/// effet et même diff que l'appel validé ; surutilisation signalée par
/// `diff.overallocated` à l'affectation à 150 %, visible par semaine dans
/// `resource_usage`, résolue par `update_assignment` ; `delete_resource`
/// sans `cascade` refusé ; réouverture du MSPDI exporté : mêmes tâches
/// (durées comprises), mêmes ressources, même résumé (hors chemin, état
/// modifié, historique).
#[tokio::test]
async fn replay_resources_j5_memory() {
    let s = replay_memory("resources_j5", Some("resources_j5.xml")).await;
    if replay::blessing() {
        return;
    }
    let at = |tool: &str, params: &Value| index_of(&s, tool, params);
    let ok = |i: usize| ok_at(&s, i);
    assert!(
        s.calls
            .iter()
            .filter(|c| c.tool != "delete_resource")
            .all(|c| error_code(c.expected.as_ref().unwrap()).is_none())
    );
    // Quatre ressources, un calendrier propre pour chaque ressource de travail.
    let added: Vec<&Value> = (0..s.calls.len())
        .filter(|&i| s.calls[i].tool == "add_resource")
        .map(|i| &ok(i)["side_effects"])
        .collect();
    assert_eq!(
        added,
        [
            &json!([{ "calendar_created": 4 }]),
            &json!([{ "calendar_created": 5 }]),
            &json!([]),
            &json!([])
        ]
    );
    // Électricien ajouté à une tâche pilotée par l'effort : dry_run puis
    // validé, même diff ; 48 h gardées, 24 h chacun (3 j par affectation) ;
    // durée de la tâche = union des parts, 5 j du lun 23 au ven 27/11 (la
    // part de l'Électricien décalée par son congé, R-173, S14).
    let dry = at(
        "assign_resource",
        &json!({ "task": 4, "resource": 2, "dry_run": true }),
    );
    assert_eq!(ok(dry)["committed"], json!(false));
    assert_eq!(ok(dry)["diff"], ok(dry + 1)["diff"]);
    assert_eq!(ok(dry + 1)["committed"], json!(true));
    let reseaux = ok(at("get_task", &json!({ "uid": 4 })));
    assert_eq!(
        (&reseaux["work"]["text"], &reseaux["duration"]["text"]),
        (&json!("48h"), &json!("5j"))
    );
    // Durée fixe : unités à 50 % ⇒ travail 16 h ; travail fixe : 8 j ⇒ 50 %.
    let charpente = ok(at("get_task", &json!({ "uid": 3 })));
    assert_eq!(charpente["work"]["text"], json!("16h"));
    let enduits = ok(at("get_task", &json!({ "uid": 6 })));
    assert_eq!(
        (
            &enduits["work"]["text"],
            &enduits["resources"][0]["units"]["text"]
        ),
        (&json!("32h"), &json!("50%"))
    );
    // Béton (matériel, UID 3) quantité 12 sur Dalle (UID 7) : la quantité
    // rendue est celle saisie (portée par le travail, 12 h, O30), relisible
    // comme `quantity`, dans les vues des tâches et des ressources.
    let last = |tool: &str| s.calls.iter().rposition(|c| c.tool == tool).unwrap();
    let listed = ok(last("list_resources"));
    let beton = listed["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["uid"] == json!(3))
        .unwrap();
    let twelve = json!({ "text": "12", "value": 120_000 });
    assert_eq!(beton["assignments"][0]["units"], twelve, "{beton}");
    assert_eq!(beton["assignments"][0]["effective_units"], twelve, "{beton}");
    let tasks = ok(last("list_tasks"));
    let dalle = tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["uid"] == json!(7))
        .unwrap();
    let on_dalle = dalle["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["uid"] == json!(3))
        .unwrap();
    assert_eq!(on_dalle["units"], twelve, "{dalle}");
    // Surutilisation du Maçon (UID 1).
    let over = at(
        "assign_resource",
        &json!({ "task": 7, "resource": 1, "units": { "text": "150%" } }),
    );
    assert_eq!(
        ok(over)["diff"]["overallocated"],
        json!({ "new": [1], "resolved": [] })
    );
    let usage = ok(over + 1);
    assert_eq!(s.calls[over + 1].tool, "resource_usage");
    assert_eq!(
        usage["rows"][0]["overallocated"],
        json!([false, true, false, false])
    );
    let fix = at(
        "update_assignment",
        &json!({ "task": 7, "resource": 1, "units": { "text": "100%" } }),
    );
    assert_eq!(
        ok(fix)["diff"]["overallocated"],
        json!({ "new": [], "resolved": [1] })
    );
    let refused = s.calls.iter().find(|c| c.tool == "delete_resource").unwrap();
    assert_eq!(
        error_code(refused.expected.as_ref().unwrap()),
        Some("resource.has_assignments")
    );
    assert_reopened(&s);
}

/// Fichier de départ de l'essai réel n° 3 : export de l'étape « ligne de
/// base 0 » du scénario `tracking_j6`, versionné à côté de son MSPDI final.
const TRACKING_BASELINE0: &str = "extension-bureaux.xml";

/// Chemin versionné du fichier de départ de l'essai réel n° 3.
fn tracking_baseline0_path(root: &Path) -> std::path::PathBuf {
    root.join("tests/mcp/expected/tracking_j6_baseline0.xml")
}

/// MSPDI `normalize` d'un fichier relu.
fn normalized_mspdi(p: &Path, now: Moment) -> plannercraft_core::Project {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
    let project = plannercraft_mspdi::read(&bytes, &plannercraft_mspdi::ReadOptions::new(now))
        .unwrap_or_else(|e| panic!("{} : {e}", p.display()))
        .project;
    plannercraft_mspdi::normalize(&project, &plannercraft_mspdi::write::WriteOptions::default())
}

/// Scénario J7 (lot L5, `docs/plans/j7.md`) rejoué en mémoire : semaine
/// type (samedi matin du Maçon, R-247), copie d'un calendrier et nom
/// d'exception (R-248, R-165), calendrier du projet (R-249), taux stockés
/// (R-195), suppression refusée puis acceptée (R-248), ligne de base
/// effacée sur une sélection, puis rien à effacer (R-250) ; MSPDI
/// enregistré égal à l'attendu par `normalize`.
#[tokio::test]
async fn replay_calendars_j7_memory() {
    let s = replay_memory("calendars_j7", Some("calendars_j7.xml")).await;
    if replay::blessing() {
        return;
    }
    let refused: Vec<(&str, &str)> = s
        .calls
        .iter()
        .filter_map(|c| error_code(c.expected.as_ref().unwrap()).map(|e| (c.tool.as_str(), e)))
        .collect();
    assert_eq!(
        refused,
        [
            ("set_work_week", "param.invalid"),
            ("add_calendar", "calendar.name_taken"),
            ("delete_calendar", "calendar.in_use"),
        ]
    );
    let ok = |i: usize| ok_at(&s, i);
    let week = |dry: bool| {
        let mut p = json!({ "calendar": 4, "days": [{ "day": "saturday", "setting": { "ranges": [{ "from": "08:00", "to": "12:00" }] } }] });
        if dry {
            p["dry_run"] = json!(true);
        }
        index_of(&s, "set_work_week", &p)
    };
    // `dry_run` et appel validé : même diff (fin de Maçonnerie lun 26/10
    // 17:00 → 12:00, Enduits avancée d'une demi-journée).
    assert_eq!(ok(week(true))["diff"], ok(week(false))["diff"]);
    assert_eq!(ok(week(true))["committed"], json!(false));
    let add = index_of(&s, "add_calendar", &json!({ "name": "Équipe B", "copy_of": 1 }));
    assert_eq!(ok(add)["side_effects"], json!([{ "calendar_created": 5 }]));
    let none = index_of(&s, "clear_baseline", &json!({ "number": 3 }));
    assert_eq!(
        ok(none)["warnings"][0]["code"],
        json!("baseline.nothing_to_clear")
    );
}

/// Scénario de suivi J6 (lot L10, `docs/plans/j6.md`, chiffres relus dans
/// `docs/features/tracking.md` § 3) rejoué en mémoire, `--now
/// 2026-11-02T08:00` : ligne de base 0, avancement au 13/11, refus, tâche
/// commencée hors séquence, replanification du reste au 27/11, `what_if`,
/// ligne de base 1 sur une sélection, historique, aller-retour MSPDI.
#[tokio::test]
async fn replay_tracking_j6_memory() {
    let root = root();
    let name = "tracking_j6";
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("memory");
    let (got, produced, elapsed) = replay_with_journal(&scenario, &tmp).await;
    let saved = tmp.path().join("tracking_j6.xml");
    let start = tmp.path().join(TRACKING_BASELINE0);
    if replay::blessing() {
        replay::bless(&root, &scenario, &produced, tmp.path(), Some(&saved));
        std::fs::copy(&start, tracking_baseline0_path(&root)).unwrap();
        return;
    }
    replay::assert_results(&scenario, &got);
    replay::assert_journal(&scenario, &produced, tmp.path());
    assert!(
        !budget_checked() || elapsed < BUDGET,
        "rejeu de {name} en {elapsed:?}, budget {BUDGET:?}"
    );
    let now = now_of(name);
    for (file, expected) in [
        (&saved, replay::expected_xml_path(&root, name)),
        (&start, tracking_baseline0_path(&root)),
    ] {
        assert_eq!(
            normalized_mspdi(file, now),
            normalized_mspdi(&expected, now),
            "{name} : {} différent de l'attendu",
            file.display()
        );
    }
    assert_tracking(&scenario);
}

/// Vérifications du cahier (L10, point 3) sur les résultats attendus du
/// scénario `tracking_j6` (les résultats rejoués leur sont égaux).
fn assert_tracking(s: &Scenario) {
    let at = |tool: &str, params: &Value| index_of(s, tool, params);
    let ok = |i: usize| ok_at(s, i);
    // Seuls trois appels sont refusés, avec les codes attendus.
    let refused: Vec<(&str, &str)> = s
        .calls
        .iter()
        .filter_map(|c| error_code(c.expected.as_ref().unwrap()).map(|e| (c.tool.as_str(), e)))
        .collect();
    assert_eq!(
        refused,
        [
            ("set_baseline", "baseline.exists"),
            ("update_progress", "progress.summary_not_editable"),
            ("update_progress", "progress.invalid_order"),
        ]
    );
    // Planning initial : 20 tâches, fin mer 16/12/2026 17:00.
    let summary0 = ok(at("project_summary", &json!({})));
    assert_eq!(
        (
            &summary0["tasks"],
            &summary0["summaries"],
            &summary0["milestones"],
            &summary0["finish"]
        ),
        (&json!(20), &json!(3), &json!(2), &json!("2026-12-16T17:00"))
    );
    // Ligne de base 0 : tous les écarts nuls.
    let v0 = ok(at("baseline_variance", &json!({})));
    let zero = |v: &Value| v.as_object().unwrap().values().all(|x| minutes_of(x) == 0);
    assert!(zero(&v0["project"]["variance"]), "{v0}");
    assert!(
        v0["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| zero(&t["variance"])),
        "{v0}"
    );
    // Avancement au 13/11 : la tâche achevée sort du chemin critique.
    let t1 = at(
        "update_progress",
        &json!({ "task": 2, "percent_complete": { "text": "100%" } }),
    );
    assert_eq!(ok(t1)["diff"]["critical_left"], json!([2]));
    let cp = ok(at("critical_path", &json!({})));
    let critical: Vec<&Value> = cp["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["uid"])
        .collect();
    assert!(
        !critical.contains(&&json!(2)) && critical.contains(&&json!(3)),
        "{cp}"
    );
    // Écarts chiffrés à la main (tracking.md § 3) : un jour de retard sur la
    // maçonnerie, un jour d'avance sur les réseaux, fin du projet + 1 j.
    let i_v = index_of_nth(s, "baseline_variance", &json!({}), 1);
    let v = ok(i_v);
    assert_eq!(minutes_of(&v["project"]["variance"]["finish"]), 480, "{v}");
    let mac = task_of(v, 3);
    assert_eq!(
        (
            minutes_of(&mac["variance"]["start"]),
            minutes_of(&mac["variance"]["finish"])
        ),
        (480, 480)
    );
    let res = task_of(v, 4);
    assert_eq!(
        (
            minutes_of(&res["variance"]["finish"]),
            minutes_of(&res["variance"]["duration"])
        ),
        (-480, -480)
    );
    // Charpente tirée par la maçonnerie (lien FS), elle-même au début réel.
    let x5 = ok(at("explain_date", &json!({ "task": 8 })));
    assert_eq!(
        x5["steps"][0]["driver"],
        json!({ "kind": "link", "link": "fs", "predecessor": 3 })
    );
    assert_eq!(x5["chain"][0]["uid"], json!(3));
    assert_eq!(x5["chain"][0]["driver"], json!({ "kind": "actual" }));
    // Jalon à 50 % ramené à 100 %, avec avertissement (dry_run).
    let m = ok(at(
        "update_progress",
        &json!({ "task": 6, "percent_complete": { "text": "50%" }, "dry_run": true }),
    ));
    assert_eq!(m["committed"], json!(false));
    assert_eq!(m["warnings"][0]["code"], json!("progress.milestone_rounded"));
    // Menuiseries commencées hors séquence : réel, puis reprise tirée par le
    // lien FS de la charpente.
    let x6 = ok(at("explain_date", &json!({ "task": 9 })));
    let kinds: Vec<&Value> = x6["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|st| &st["step"])
        .collect();
    assert_eq!(kinds, [&json!("driver"), &json!("actual"), &json!("resume")]);
    assert_eq!(
        x6["steps"][2]["cause"],
        json!({ "kind": "link", "link": "fs", "predecessor": 8 })
    );
    assert_tracking_analysis(s);
}

/// Suite de [`assert_tracking`] : replanification, `what_if`, ligne de
/// base 1, retards, historique.
fn assert_tracking_analysis(s: &Scenario) {
    let at = |tool: &str, params: &Value| index_of(s, tool, params);
    let ok = |i: usize| ok_at(s, i);
    // Replanification du reste au 27/11 : dry_run sans effet, même diff.
    let dry = at("reschedule_remaining", &json!({ "dry_run": true }));
    assert_eq!(ok(dry)["committed"], json!(false));
    assert_eq!(ok(dry + 1)["committed"], json!(true));
    assert_eq!(ok(dry)["diff"], ok(dry + 1)["diff"]);
    let x2 = ok(at("explain_date", &json!({ "task": 3 })));
    assert_eq!(x2["steps"][2]["cause"], json!({ "kind": "entered" }));
    assert_eq!(x2["steps"][2]["resume"], json!("2026-11-30T08:00"));
    // what_if : projet inchangé ; + 3 j sur une tâche critique, puis un
    // second plaquiste, mesurés contre la ligne de base 0.
    let w1 = index_of(
        s,
        "what_if",
        &s.calls.iter().find(|c| c.tool == "what_if").unwrap().params,
    );
    assert_eq!(s.calls[w1 - 1].tool, "project_summary");
    assert_eq!(s.calls[w1 + 2].tool, "project_summary");
    assert_eq!(ok(w1 - 1), ok(w1 + 2), "what_if a modifié le projet");
    let base = minutes_of(
        &ok(index_of_nth(s, "baseline_variance", &json!({ "baseline": 0 }), 0))["project"]["variance"]["finish"],
    );
    let f1 = minutes_of(&ok(w1)["project_finish_variance"]);
    let f2 = minutes_of(&ok(w1 + 1)["project_finish_variance"]);
    assert_eq!((f1 - base, f2 - base), (1440, -960), "{base} {f1} {f2}");
    for w in [w1, w1 + 1] {
        assert_eq!(ok(w)["accepted"], json!(true));
        assert_eq!(ok(w)["explanations"][0]["task"]["uid"], json!(20));
    }
    // Ligne de base 1 sur une sélection, cumulée vers les récapitulatives.
    let v1 = ok(index_of_nth(s, "baseline_variance", &json!({ "baseline": 1 }), 0));
    for uid in [7, 10, 14] {
        assert!(!task_of(v1, uid)["baseline"].is_null(), "{uid} : {v1}");
    }
    for uid in [3, 8, 20] {
        assert!(task_of(v1, uid)["baseline"].is_null(), "{uid} : {v1}");
    }
    // Retards : la maçonnerie, pas la tâche achevée ni les réseaux en avance.
    let late = ok(at("list_tasks", &json!({ "late": true })));
    let late: Vec<&Value> = late["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["uid"])
        .collect();
    assert!(late.contains(&&json!(3)), "{late:?}");
    assert!(
        !late.contains(&&json!(2)) && !late.contains(&&json!(4)),
        "{late:?}"
    );
    // Trois undo, trois redo.
    let u = at("undo", &json!({}));
    let lens: Vec<&Value> = (u..u + 6).map(|i| &ok(i)["history_len"]).collect();
    assert_eq!(
        lens,
        [
            &json!(10),
            &json!(9),
            &json!(8),
            &json!(9),
            &json!(10),
            &json!(11)
        ]
    );
}

/// `tracking_j6` : `save_project`, `export_mspdi`, `open_project` sur
/// l'export ; mêmes écarts, mêmes tâches, même résumé. Porte sur les
/// résultats attendus du scénario, que `replay_tracking_j6_memory`,
/// `_stdio` et `_http` comparent aux résultats rejoués.
#[test]
fn replay_tracking_j6_reopen_same_variances() {
    let s = Scenario::load(&root(), "tracking_j6");
    assert_tracking_reopened(&s);
}

/// `tracking_j6` : la réouverture du MSPDI exporté redonne les mêmes écarts
/// (lignes 0 et 1), les mêmes tâches et le même résumé (hors chemin, état
/// modifié, historique).
fn assert_tracking_reopened(s: &Scenario) {
    let ok = |i: usize| ok_at(s, i);
    let opened = index_of(s, "open_project", &json!({ "path": "$TMP/export.xml" }));
    let before = opened - 6;
    let tools: Vec<&str> = (before..before + 4).map(|i| s.calls[i].tool.as_str()).collect();
    assert_eq!(
        tools,
        [
            "baseline_variance",
            "baseline_variance",
            "list_tasks",
            "project_summary"
        ]
    );
    for k in 0..3 {
        assert_eq!(s.calls[opened + 1 + k].tool, s.calls[before + k].tool);
        assert_eq!(s.calls[opened + 1 + k].params, s.calls[before + k].params);
        assert_eq!(ok(opened + 1 + k), ok(before + k), "appel {}", opened + 2 + k);
    }
    let strip = |v: &Value| {
        let mut v = v.clone();
        for k in ["dirty", "path", "history_len"] {
            v.as_object_mut().unwrap().remove(k);
        }
        v
    };
    assert_eq!(strip(ok(opened + 4)), strip(ok(before + 3)));
}

/// `tracking_j6`, vérification 2 du cahier J4 étendue : MSPDI enregistré
/// relu sans erreur, 20 tâches, avancement, lignes de base 0 et 1 et date
/// d'état relus, planning sans erreur, `normalize` égal à l'attendu.
#[tokio::test]
async fn replay_tracking_j6_saved_mspdi_matches() {
    let root = root();
    let name = "tracking_j6";
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("saved");
    let (client, _server) = connect(opts_at(now_of(name), None)).await;
    run(&client, &scenario, tmp.path()).await;
    client.cancel().await.unwrap();
    let bytes = std::fs::read(tmp.path().join("tracking_j6.xml")).unwrap();
    let saved =
        plannercraft_mspdi::read(&bytes, &plannercraft_mspdi::ReadOptions::new(now_of(name))).unwrap();
    let errors: Vec<_> = saved
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "import : {errors:?}");
    let p = &saved.project;
    let tasks: Vec<_> = p
        .tasks
        .iter()
        .filter(|t| !t.is_null && t.uid().get() != 0)
        .collect();
    assert_eq!(tasks.len(), 20);
    assert_eq!(
        p.status_date,
        Some("2026-11-27T17:00".parse().unwrap()),
        "date d'état relue"
    );
    let by_uid = |uid: u32| tasks.iter().find(|t| t.uid().get() == uid).unwrap();
    let mac = plannercraft_core::progress::facts(by_uid(3));
    assert_eq!(mac.actual_start, Some("2026-11-03T08:00".parse().unwrap()));
    assert_eq!(mac.actual_duration.get(), 2400);
    assert_eq!(
        plannercraft_core::progress::facts(by_uid(2)).state,
        plannercraft_core::progress::ProgressState::Complete
    );
    let sched = plannercraft_core::schedule(p);
    let errors: Vec<_> = sched
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "planning : {errors:?}");
    let w = plannercraft_mspdi::write::WriteOptions::default();
    let expected = normalized_mspdi(&replay::expected_xml_path(&root, name), now_of(name));
    assert_eq!(plannercraft_mspdi::normalize(p, &w), expected);
}

/// Essai réel n° 3 avec Claude (lot L10, point 5 ; `docs/features/mcp.md`
/// § 6) : harnais prêt, branché sur le journal versionné
/// `tests/mcp/scenarios/claude_run_3.jsonl` dès qu'il existe (l'essai est
/// mené par la session principale). Le fichier de départ (export de l'étape
/// « ligne de base 0 » de `tracking_j6`) est déposé en
/// `$TMP/extension-bureaux.xml` avant le rejeu, `--now 2026-11-13T17:00` ;
/// si le journal ne commence pas par son `open_project`, le serveur est
/// lancé avec `--open` sur ce fichier (comme pendant l'essai). Vérifié sur
/// le journal : `update_progress`, `explain_date` et `what_if` utilisés ; la
/// règle 3 (aucune date rédigée hors des résultats) se relit sur la réponse
/// de Claude, consignée dans `mcp.md`.
#[tokio::test]
async fn replay_claude_run_3() {
    let root = root();
    let name = "claude_run_3";
    if !replay::scenario_path(&root, name).exists() {
        eprintln!("replay_claude_run_3 : journal {name}.jsonl absent, essai n° 3 pas encore versionné");
        return;
    }
    let scenario = Scenario::load(&root, name);
    let tmp = TempDir::new("claude3");
    let start = tmp.path().join(TRACKING_BASELINE0);
    std::fs::copy(tracking_baseline0_path(&root), &start).unwrap();
    let opened_by_tool = scenario.calls.first().is_some_and(|c| {
        c.tool == "open_project" && c.params["path"] == json!(format!("$TMP/{TRACKING_BASELINE0}"))
    });
    let open = (!opened_by_tool).then_some(start);
    let journal = tmp.path().join("run.jsonl");
    let saved = tmp.path().join(TRACKING_BASELINE0);
    let (got, produced, elapsed) = replay_opened(&scenario, &tmp, journal, open).await;
    if replay::blessing() {
        replay::bless(&root, &scenario, &produced, tmp.path(), Some(&saved));
        return;
    }
    replay::assert_results(&scenario, &got);
    replay::assert_journal(&scenario, &produced, tmp.path());
    // MSPDI enregistré par l'agent (`save_project` sur le fichier ouvert) :
    // `normalize` égal à l'attendu relu.
    let read = |p: &Path| {
        let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
        plannercraft_mspdi::read(&bytes, &plannercraft_mspdi::ReadOptions::new(now_of(name)))
            .unwrap_or_else(|e| panic!("{} : {e}", p.display()))
            .project
    };
    let w = plannercraft_mspdi::write::WriteOptions::default();
    assert_eq!(
        plannercraft_mspdi::normalize(&read(&saved), &w),
        plannercraft_mspdi::normalize(&read(&replay::expected_xml_path(&root, name)), &w),
        "{name} : MSPDI enregistré différent de l'attendu"
    );
    assert!(
        !budget_checked() || elapsed < BUDGET,
        "rejeu de {name} en {elapsed:?}, budget {BUDGET:?}"
    );
    let tools: Vec<&str> = scenario.calls.iter().map(|c| c.tool.as_str()).collect();
    for t in ["update_progress", "explain_date", "what_if"] {
        assert!(tools.contains(&t), "{t} absent de l'essai n° 3 : {tools:?}");
    }
    claude_run_3_facts(&scenario, &tools);
}

/// Faits clés de l'essai n° 3 du 2026-10-20 (`docs/features/mcp.md` § 6).
fn claude_run_3_facts(scenario: &Scenario, tools: &[&str]) {
    assert_eq!(
        tools,
        [
            "open_project",
            "list_tasks",
            "list_resources",
            "set_status_date",
            "get_task",
            "get_task",
            "update_progress",
            "update_progress",
            "reschedule_remaining",
            "explain_date",
            "baseline_variance",
            "get_task",
            "what_if",
            "what_if",
            "what_if",
            "save_project"
        ]
    );
    assert!(
        scenario
            .calls
            .iter()
            .all(|c| error_code(c.expected.as_ref().unwrap()).is_none())
    );
    let ok = |i: usize| ok_at(scenario, i);
    // Date d'état posée au 13/11 17:00 par l'agent.
    assert_eq!(scenario.calls[3].params, json!({ "date": "2026-11-13T17:00" }));
    // Terrassement achevé (sort du chemin critique) ; Maçonnerie commencée
    // le 03/11 à 50 % : réception 16/12 → 17/12 ; reste replanifié après
    // la date d'état : 17/12 → 23/12 (+ 4 j).
    assert_eq!(ok(6)["diff"]["critical_left"], json!([2]));
    assert_eq!(ok(7)["diff"]["finish_after"], json!("2026-12-17T17:00"));
    assert_eq!(ok(8)["diff"]["finish_after"], json!("2026-12-23T17:00"));
    assert_eq!(minutes_of(&ok(8)["diff"]["finish_delta"]), 1_920);
    // Maçonnerie : partie réelle jusqu'au lun 09/11, reprise lun 16/11
    // (reprise saisie au 13/11 17:00), fin ven 20/11.
    let mac = &ok(11)["progress"];
    assert_eq!(mac["stop"], json!("2026-11-09T17:00"));
    assert_eq!(mac["resume"], json!("2026-11-16T08:00"));
    assert_eq!(ok(11)["finish"], json!("2026-11-20T17:00"));
    // Pourquoi la réception bouge : chaîne des pilotes jusqu'à la
    // Maçonnerie (FS de bout en bout).
    let chain: Vec<Value> = ok(9)["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["uid"].clone())
        .collect();
    assert_eq!(Value::Array(chain), json!([19, 17, 16, 14, 10, 9, 8, 3]));
    assert_eq!(ok(9)["date"], json!("2026-12-23T17:00"));
    // Écarts à la ligne 0 : projet (ligne déduite) et réception + 5 j.
    let variance = ok(10);
    assert_eq!(variance["project"]["derived"], json!(true));
    assert_eq!(minutes_of(&variance["project"]["variance"]["finish"]), 2_400);
    assert_eq!(minutes_of(&task_of(variance, 20)["variance"]["finish"]), 2_400);
    assert_eq!(minutes_of(&task_of(variance, 6)["variance"]["finish"]), 2_400);
    // Second plaquiste : simple ajout sans effet (tâches non pilotées par
    // l'effort) ; à 200 % ou piloté par l'effort, réception au 21/12
    // (− 2 j), Électricité puis Ventilation deviennent le chemin.
    assert_eq!(minutes_of(&ok(12)["diff"]["finish_delta"]), 0);
    for i in [13, 14] {
        assert_eq!(ok(i)["diff"]["finish_after"], json!("2026-12-21T17:00"));
        assert_eq!(minutes_of(&ok(i)["diff"]["finish_delta"]), -960);
    }
    let after: Vec<&Value> = ok(14)["explanations"][0]["chain"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["uid"])
        .collect();
    assert!(
        after.contains(&&json!(13)) && after.contains(&&json!(11)),
        "{after:?}"
    );
    // Simulations sans effet : enregistrement du seul avancement.
    assert_eq!(ok(15)["path"], json!(format!("$TMP/{TRACKING_BASELINE0}")));
}

/// Minutes d'une durée de vue (`{minutes, unit, text}`).
fn minutes_of(v: &Value) -> i64 {
    v["minutes"].as_i64().unwrap()
}

/// Tâche `uid` d'une vue paginée (`tasks`).
fn task_of(v: &Value, uid: u64) -> Value {
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["uid"] == json!(uid))
        .cloned()
        .unwrap_or_else(|| panic!("tâche {uid} absente : {v}"))
}

/// Index du n-ième (à partir de 0) appel `tool` de paramètres `params`.
fn index_of_nth(s: &Scenario, tool: &str, params: &Value, n: usize) -> usize {
    s.calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.tool == tool && &c.params == params)
        .nth(n)
        .map_or_else(|| panic!("{tool} {params} n° {n} absent du scénario"), |(i, _)| i)
}

/// Index du premier appel `tool` de paramètres `params`.
fn index_of(s: &Scenario, tool: &str, params: &Value) -> usize {
    s.calls
        .iter()
        .position(|c| c.tool == tool && &c.params == params)
        .unwrap_or_else(|| panic!("{tool} {params} absent du scénario"))
}

/// Résultat `ok` attendu de l'appel `i`.
fn ok_at(s: &Scenario, i: usize) -> &Value {
    let r = s.calls[i].expected.as_ref().unwrap();
    r.get("ok").unwrap_or_else(|| panic!("appel {} : {r}", i + 1))
}

/// `resources_j5` : la réouverture du MSPDI exporté redonne le projet.
fn assert_reopened(s: &Scenario) {
    let at = |tool: &str, params: &Value| index_of(s, tool, params);
    let ok = |i: usize| ok_at(s, i);
    // Réouverture : mêmes tâches (dates, durées, travail, marges), mêmes
    // ressources. La durée d'une feuille placée par plusieurs parts est la
    // même en session et relue : l'union des parts (Réseaux, UID 4 : 5 j,
    // du lun 23 au ven 27/11), reportée dans le modèle après chaque recalcul
    // (R-173) et écrite par l'écrivain MSPDI (R-199).
    let opened = at("open_project", &json!({ "path": "$TMP/resources_j5.xml" }));
    let before = opened - 4;
    assert_eq!(s.calls[before].tool, "list_tasks");
    assert_eq!(s.calls[opened + 1].tool, "list_tasks");
    let reseaux = ok(before)["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["uid"] == json!(4))
        .unwrap();
    assert_eq!(reseaux["duration"]["minutes"], json!(2400), "{reseaux}");
    assert_eq!(ok(opened + 1), ok(before));
    assert_eq!(s.calls[opened + 2].tool, "list_resources");
    assert_eq!(ok(opened + 2), ok(before + 1));
    let strip = |v: &Value| {
        let mut v = v.clone();
        for k in ["dirty", "path", "history_len"] {
            v.as_object_mut().unwrap().remove(k);
        }
        v
    };
    assert_eq!(strip(ok(opened + 3)), strip(ok(before + 2)));
}

/// Essai réel avec Claude du 2026-10-19 (`docs/features/mcp.md` § 6) :
/// journal de `claude -p` versionné, chemins ramenés à `$TMP`, rejoué tel
/// quel. Six appels, aucune erreur ; un lot atomique de 75 opérations.
#[tokio::test]
async fn replay_claude_run_1() {
    let scenario = replay_memory("claude_run_1", Some("renovation-maison.xml")).await;
    if replay::blessing() {
        return;
    }
    let tools: Vec<&str> = scenario.calls.iter().map(|c| c.tool.as_str()).collect();
    assert_eq!(
        tools,
        [
            "new_project",
            "apply_batch",
            "critical_path",
            "project_summary",
            "save_project",
            "render_gantt"
        ]
    );
    assert!(
        scenario
            .calls
            .iter()
            .all(|c| error_code(c.expected.as_ref().unwrap()).is_none())
    );
    assert_eq!(scenario.calls[1].params["ops"].as_array().unwrap().len(), 75);
    let summary = &scenario.calls[3].expected.as_ref().unwrap()["ok"];
    assert_eq!(
        (
            &summary["tasks"],
            &summary["summaries"],
            &summary["milestones"],
            &summary["links"],
            &summary["finish"],
            &summary["history_len"],
        ),
        (
            &json!(35),
            &json!(5),
            &json!(4),
            &json!(40),
            &json!("2027-03-10T17:00"),
            &json!(1)
        )
    );
}

/// Essai réel n° 2 avec Claude du 2026-10-19 (`docs/features/mcp.md` § 6,
/// ressources J5) : journal de `claude -p` versionné, chemins ramenés à
/// `$TMP`, rejoué tel quel. Seize appels, aucune erreur ; trois
/// `apply_batch` en `dry_run` comparés avant d'appliquer ; six artisans,
/// surutilisation détectée (Maçon, Carreleur) puis résolue par des liens,
/// aucune surcharge à la fin ; fin du projet mer 27/01/2027 17:00.
#[tokio::test]
async fn replay_claude_run_2() {
    let s = replay_memory("claude_run_2", Some("renovation-maison.xml")).await;
    if replay::blessing() {
        return;
    }
    let tools: Vec<&str> = s.calls.iter().map(|c| c.tool.as_str()).collect();
    assert_eq!(
        tools,
        [
            "new_project",
            "apply_batch",
            "apply_batch",
            "list_resources",
            "resource_usage",
            "list_tasks",
            "critical_path",
            "apply_batch",
            "apply_batch",
            "apply_batch",
            "apply_batch",
            "critical_path",
            "resource_usage",
            "list_resources",
            "save_project",
            "render_gantt"
        ]
    );
    assert!(
        s.calls
            .iter()
            .all(|c| error_code(c.expected.as_ref().unwrap()).is_none())
    );
    let ok = |i: usize| ok_at(&s, i);
    // Surutilisation détectée : Maçon (UID 1) et Carreleur (UID 5).
    let over: Vec<&Value> = ok(3)["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["uid"])
        .collect();
    assert_eq!(over, [&json!(1), &json!(5)]);
    // Trois essais en `dry_run`, rien d'appliqué ; le premier fait
    // apparaître une surcharge du Plaquiste (UID 4).
    for i in 7..=9 {
        assert_eq!(s.calls[i].params["dry_run"], json!(true));
        assert_eq!(ok(i)["committed"], json!(false));
    }
    assert_eq!(
        ok(7)["diff"]["overallocated"],
        json!({ "new": [4], "resolved": [1, 5] })
    );
    // Lot validé : surcharges résolues, aucune nouvelle.
    assert_eq!(ok(10)["committed"], json!(true));
    assert_eq!(
        ok(10)["diff"]["overallocated"],
        json!({ "new": [], "resolved": [1, 5] })
    );
    // Fin du projet et chemin critique après la résolution.
    assert_eq!(ok(11)["project_finish"], json!("2027-01-27T17:00"));
    assert_eq!(ok(11)["total"], json!(22));
    // Six artisans, aucune surcharge restante.
    let all = ok(13)["resources"].as_array().unwrap();
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|r| r["overallocated"] == json!(false)));
}

/// Vérification 2 : MSPDI enregistré relu, 30 tâches, `normalize` égal à
/// l'attendu relu, aucun diagnostic d'erreur (import et planning).
#[tokio::test]
async fn replay_build_30_saved_mspdi_matches() {
    let root = root();
    let scenario = Scenario::load(&root, SCENARIO);
    let tmp = TempDir::new("saved");
    let (client, _server) = connect(opts(None)).await;
    run(&client, &scenario, tmp.path()).await;
    client.cancel().await.unwrap();

    let read = |p: &Path| {
        let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
        plannercraft_mspdi::read(&bytes, &plannercraft_mspdi::ReadOptions::new(now()))
            .unwrap_or_else(|e| panic!("{} : {e}", p.display()))
    };
    let saved = read(&tmp.path().join("build_30.xml"));
    let expected = read(&replay::expected_xml_path(&root, SCENARIO));
    let tasks = saved
        .project
        .tasks
        .iter()
        .filter(|t| !t.is_null && t.uid().get() != 0)
        .count();
    assert_eq!(tasks, 30);
    let errors: Vec<_> = saved
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "import : {errors:?}");
    let sched = plannercraft_core::schedule(&saved.project);
    let errors: Vec<_> = sched
        .diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "planning : {errors:?}");
    let w = plannercraft_mspdi::write::WriteOptions::default();
    assert_eq!(
        plannercraft_mspdi::normalize(&saved.project, &w),
        plannercraft_mspdi::normalize(&expected.project, &w)
    );
}

/// Code d'erreur d'un résultat journalisé.
fn error_code(r: &Value) -> Option<&str> {
    r.get("error").and_then(|e| e.get("code")).and_then(Value::as_str)
}

/// Vérification 3 (P30 de bout en bout) : `undo` répété jusqu'à
/// `history.empty` redonne exactement le projet vide du `new_project`
/// (MSPDI exporté identique octet pour octet).
#[tokio::test]
async fn replay_build_30_undo_to_empty() {
    let root = root();
    let scenario = Scenario::load(&root, SCENARIO);
    let tmp = TempDir::new("undo");

    // Référence : le projet juste après `new_project`.
    let first = &scenario.calls[0];
    assert_eq!(first.tool, "new_project");
    let (client, _server) = connect(opts(None)).await;
    let r = call(&client, "new_project", &first.params).await;
    assert!(r.get("ok").is_some(), "{r}");
    let empty = tmp.path().join("empty.xml");
    let r = call(
        &client,
        "export_mspdi",
        &json!({ "path": empty.display().to_string() }),
    )
    .await;
    assert!(r.get("ok").is_some(), "{r}");
    client.cancel().await.unwrap();

    let (client, _server) = connect(opts(None)).await;
    run(&client, &scenario, tmp.path()).await;
    let mut undone = 0_u32;
    loop {
        let r = call(&client, "undo", &json!({})).await;
        if r.get("ok").is_some() {
            undone += 1;
            assert!(undone <= 1_000, "undo sans fin");
            continue;
        }
        assert_eq!(error_code(&r), Some("history.empty"), "{r}");
        break;
    }
    // 30 tâches, 25 liens, 2 contraintes, 1 exception de calendrier.
    assert_eq!(undone, 58);
    let tasks = call(&client, "list_tasks", &json!({})).await;
    assert_eq!(tasks["ok"]["tasks"], json!([]), "{tasks}");
    let after = tmp.path().join("undone.xml");
    let r = call(
        &client,
        "export_mspdi",
        &json!({ "path": after.display().to_string() }),
    )
    .await;
    assert!(r.get("ok").is_some(), "{r}");
    client.cancel().await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&after).unwrap(),
        std::fs::read_to_string(&empty).unwrap(),
        "projet après undo complet différent du projet vide"
    );
}
