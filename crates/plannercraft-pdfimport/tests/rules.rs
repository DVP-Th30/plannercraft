//! Règles de l'import PDF (`docs/scheduling-rules.md`, R-270 à R-273) :
//! les dates lues ne sont jamais recopiées comme dates calculées (règle 3).
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::text::{Language, TextContext};
use plannercraft_core::{Moment, ProjectOptions};
use plannercraft_pdfimport::extract::{Extracted, PageContent, Word};
use plannercraft_pdfimport::plan::{DateOrderSource, ImportStats, SourceInfo, build};
use plannercraft_pdfimport::{
    ConvertOptions, ImportOptions, ImportedPlan, ImportedTask, NumericOrder, WarningCode, convert, values,
};

fn ctx() -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), Language::Fr)
}

fn moment(s: &str) -> Moment {
    s.parse().unwrap()
}

/// Tâche lue : dates `AAAA-MM-JJ` (début 08:00, fin 17:00).
fn task(
    id: u32,
    level: u16,
    summary: bool,
    duration: &str,
    start: &str,
    finish: &str,
    preds: &str,
) -> ImportedTask {
    let predecessors = values::predecessors(preds, &ctx())
        .into_iter()
        .map(Result::unwrap)
        .collect();
    ImportedTask {
        id,
        name: format!("T{id}"),
        level,
        summary,
        duration: values::duration(duration, &ctx()),
        duration_text: duration.to_owned(),
        start: (!start.is_empty()).then(|| moment(&format!("{start}T08:00"))),
        start_text: start.to_owned(),
        finish: (!finish.is_empty()).then(|| moment(&format!("{finish}T17:00"))),
        finish_text: finish.to_owned(),
        predecessors,
        predecessors_text: preds.to_owned(),
        resources: String::new(),
        percent_complete: None,
        bold: summary,
        page: 0,
        confidence: 100,
    }
}

fn plan(tasks: Vec<ImportedTask>) -> ImportedPlan {
    ImportedPlan {
        language: None,
        date_order: Some(NumericOrder::DayMonth),
        date_order_source: DateOrderSource::Values,
        tasks,
        warnings: Vec::new(),
        source: SourceInfo::default(),
        stats: ImportStats::default(),
    }
}

fn options(pin: bool) -> ConvertOptions {
    ConvertOptions {
        name: "essai".to_owned(),
        now: moment("2026-01-01T08:00"),
        lang: Language::Fr,
        pin_read_starts: pin,
    }
}

/// R-270 : le projet commence au plus petit début lu, même s'il n'est pas
/// sur la première ligne.
#[test]
fn r270_project_start_is_the_earliest_read_start() {
    let p = plan(vec![
        task(1, 1, false, "2 jours", "2026-01-07", "2026-01-08", ""),
        task(2, 1, false, "1 jour", "2026-01-05", "2026-01-05", ""),
    ]);
    let c = convert(&p, &options(false)).unwrap();
    assert_eq!(c.project_start, moment("2026-01-05T08:00"));
    // Sans début lu : plus petite fin lue, à l'heure de début par défaut.
    let p = plan(vec![task(1, 1, false, "1 jour", "", "2026-01-09", "")]);
    assert_eq!(
        convert(&p, &options(false)).unwrap().project_start,
        moment("2026-01-09T08:00")
    );
    // Aucune date : pas de projet.
    let p = plan(vec![task(1, 1, false, "1 jour", "", "", "")]);
    assert!(convert(&p, &options(false)).is_err());
}

/// R-271 : « Début au plus tôt le » au début lu, sur une feuille sans
/// prédécesseur que le moteur placerait plus tôt ; jamais sur une
/// récapitulative ni sur une feuille liée (par défaut).
#[test]
fn r271_snet_only_where_the_engine_would_start_earlier() {
    let p = plan(vec![
        task(1, 1, true, "10 jours", "2026-01-05", "2026-01-16", ""),
        task(2, 2, false, "3 jours", "2026-01-05", "2026-01-07", ""),
        task(3, 2, false, "2 jours", "2026-01-12", "2026-01-13", ""),
        task(4, 2, false, "2 jours", "2026-01-15", "2026-01-16", "3"),
    ]);
    let c = convert(&p, &options(false)).unwrap();
    // 2 démarre au début du projet : rien ; 3 est posée au 12/01 ; 4 est
    // liée : pas de contrainte, le moteur la place après 3 (14/01) et
    // l'écart avec le 15/01 lu est listé (R-272).
    assert_eq!(c.constraints, 1);
    let s = c.session.schedule();
    assert_eq!(s.scheduled(c.uids[2]).unwrap().start, moment("2026-01-12T08:00"));
    assert_eq!(s.scheduled(c.uids[3]).unwrap().start, moment("2026-01-14T08:00"));
    assert!(
        c.gaps
            .iter()
            .any(|g| g.id == 4 && g.field == "start" && g.days == -1)
    );
}

/// R-271 étendue (`pin_read_starts`) : la feuille liée placée trop tôt est
/// contrainte à son début lu, en commençant par la source de l'écart ; une
/// tâche en aval rattrapée par le décalage n'est pas contrainte.
#[test]
fn r271_pin_read_starts_constrains_the_source_of_the_gap_only() {
    let p = plan(vec![
        task(1, 1, false, "2 jours", "2026-01-05", "2026-01-06", ""),
        // Lue au 08/01 (un jour de battement invisible dans le PDF).
        task(2, 1, false, "2 jours", "2026-01-08", "2026-01-09", "1"),
        task(3, 1, false, "1 jour", "2026-01-12", "2026-01-12", "2"),
    ]);
    let c = convert(&p, &options(true)).unwrap();
    assert_eq!(c.constraints, 1);
    assert!(c.gaps.is_empty(), "{:?}", c.gaps);
    let strict = convert(&p, &options(false)).unwrap();
    assert_eq!(strict.constraints, 0);
    assert_eq!(strict.gaps.len(), 4);
}

/// R-272 : comparaison au jour, écart signé en jours calendaires (calculé
/// moins lu) ; une date lue n'est jamais recopiée : une durée lue qui ne
/// tombe pas sur la fin lue donne un écart.
#[test]
fn r272_gaps_are_signed_calendar_days() {
    let p = plan(vec![
        // 3 jours ouvrés depuis le ven 09/01 : fin le mar 13/01, lue au 12/01.
        task(1, 1, false, "3 jours", "2026-01-09", "2026-01-12", ""),
    ]);
    let c = convert(&p, &options(false)).unwrap();
    assert_eq!(c.dates_compared, 2);
    assert_eq!(c.dates_equal, 1);
    assert_eq!(c.gaps.len(), 1);
    let g = &c.gaps[0];
    assert_eq!(
        (g.field, g.read.as_str(), g.computed.as_str(), g.days),
        ("finish", "2026-01-12", "2026-01-13", 1)
    );
    let s = c.session.schedule().scheduled(c.uids[0]).unwrap();
    assert_eq!(s.finish, moment("2026-01-13T17:00"));
}

fn word(text: &str, x0: i32, baseline: i32) -> Word {
    let n = i32::try_from(text.chars().count()).unwrap();
    Word {
        text: text.to_owned(),
        x0,
        x1: x0 + n * 420,
        baseline,
        size: 800,
        bold: false,
        glyphs: Vec::new(),
    }
}

/// Une page : en-tête puis lignes N°, Nom, Durée, Début, Fin.
fn page(header: [&str; 5], rows: &[[&str; 5]]) -> Extracted {
    let xs = [400, 2000, 20000, 25000, 32000];
    let mut words = Vec::new();
    for (r, cells) in std::iter::once(&header).chain(rows).enumerate() {
        let y = 1000 + 1200 * i32::try_from(r).unwrap();
        for (k, cell) in cells.iter().enumerate() {
            let mut x = xs[k];
            for w in cell.split_whitespace() {
                let wd = word(w, x, y);
                x = wd.x1 + 300;
                words.push(wd);
            }
        }
    }
    Extracted {
        pages: vec![PageContent {
            words,
            glyphs: 2000,
            width: 84_000,
            height: 59_500,
            ..PageContent::default()
        }],
        page_count: 1,
        producer: None,
        creator: None,
    }
}

/// R-273 : l'ordre jour/mois des dates numériques se décide sur tout le
/// document : un jour > 12 tranche pour toutes les dates, même
/// ambiguës ; à défaut, la langue de l'en-tête avec un avertissement.
#[test]
fn r273_date_order_is_decided_on_the_whole_document() {
    let fr = ["N°", "Nom de la tâche", "Durée", "Début", "Fin"];
    let en = ["ID", "Task Name", "Duration", "Start", "Finish"];
    // En-tête anglais, mais « 25/03/26 » n'existe qu'en JJ/MM : « 03/04/26 »
    // est le 3 avril.
    let ex = page(
        en,
        &[
            ["1", "A", "1 day", "03/04/26", "03/04/26"],
            ["2", "B", "1 day", "25/03/26", "25/03/26"],
        ],
    );
    let p = build(&ex, &ImportOptions::default()).unwrap();
    assert_eq!(p.date_order, Some(NumericOrder::DayMonth));
    assert_eq!(p.tasks[0].start.unwrap(), moment("2026-04-03T08:00"));
    // Rien ne tranche : langue de l'en-tête, avertissement.
    let ex = page(fr, &[["1", "A", "1 jour", "03/04/26", "03/04/26"]]);
    let p = build(&ex, &ImportOptions::default()).unwrap();
    assert_eq!(p.date_order_source, DateOrderSource::Language);
    assert_eq!(p.tasks[0].start.unwrap(), moment("2026-04-03T08:00"));
    assert!(p.warnings.iter().any(|w| w.code == WarningCode::DateOrderAssumed));
    // Incompatibles dans les deux ordres : avertissement, dates fautives
    // ignorées.
    let ex = page(
        fr,
        &[
            ["1", "A", "1 jour", "25/03/26", "25/03/26"],
            ["2", "B", "1 jour", "03/25/26", "03/25/26"],
        ],
    );
    let p = build(&ex, &ImportOptions::default()).unwrap();
    assert!(
        p.warnings
            .iter()
            .any(|w| w.code == WarningCode::DateOrderConflict)
    );
    assert!(p.tasks[1].start.is_none());
}
