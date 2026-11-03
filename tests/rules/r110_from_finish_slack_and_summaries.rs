//! R-110 — Marges, chemins critiques et récapitulatives à rebours : R-048 à
//! R-050 inchangées ; R-051 en miroir (option « plusieurs chemins
//! critiques » : une feuille sans prédécesseur effectif commence au plus tôt
//! à son début au plus tard) ; `K(S)` (lien SS/SF sortant d'une
//! récapitulative) plafonne toutes ses feuilles, avec
//! `link.start_from_summary` (miroir de R-058) ; décalage en % depuis une
//! récapitulative : lien ignoré (`link.unschedulable`) ; SNET et FNLT d'une
//! récapitulative (R-103) jugés sur ses dates planifiées. Source : hypothèse
//! (miroir de R-051, R-058).

use plannercraft_core::{Constraint, Diagnostic, Driver, LateDriver, LinkType, Minutes, TaskOutcome};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_pct, lag_w};

/// F ven 23/10 17:00 ; A 1 j → B 1 j (FS), C 3 j.
fn chain(multiple: bool) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .options(|o| o.multiple_critical_paths = multiple)
        .task("A", 480)
        .task("B", 480)
        .task("C", 1_440)
        .fs("A", "B")
        .run()
}

#[test]
fn r110_slack_without_multiple_critical_paths() {
    // S* = mer 08:00 (C) ; au plus tôt A mer, B jeu ; au plus tard A jeu, B
    // ven → TF(A) = TF(B) = 480 ; C critique.
    let run = chain(false);
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").total_slack, Minutes::new(480));
    assert_eq!(run.at("C").total_slack, Minutes::ZERO);
}

#[test]
fn r110_multiple_critical_paths_mirror() {
    // Option active : A sans prédécesseur effectif → ES = LS (jeu) ; B ven ;
    // C mer – ven ; toutes critiques.
    let run = chain(true);
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
    assert_eq!(run.at("B").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("C").early.start, moment("2026-10-21T08:00"));
    for name in ["A", "B", "C"] {
        assert_eq!(run.at(name).total_slack, Minutes::ZERO, "{name}");
        assert!(run.at(name).critical, "{name}");
    }
}

#[test]
fn r110_start_link_from_summary_caps_every_leaf() {
    // S {A 1 j, B 3 j} SS+1 j → Y 1 j ; D 5 j ; F ven : Y ven ; K(S) =
    // h(ven 08:00) = jeu 08:00 sur toutes les feuilles → A au plus tard jeu
    // 08:00 – 17:00 (cause `SummaryCap`) ; B mer 08:00 – ven 17:00 (le
    // plafond jeu 08:00 + 3 j dépasse F) ; `link.start_from_summary`.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("S", 0)
        .task("A", 480)
        .task("B", 1_440)
        .task("Y", 480)
        .task("D", 2_400)
        .level("A", 2)
        .level("B", 2)
        .link("S", "Y", LinkType::StartToStart, lag_w(480))
        .run();
    assert_eq!(run.at("Y").start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-22T17:00"));
    assert_eq!(
        run.at("A").late_driver,
        LateDriver::SummaryCap {
            summary: run.uid("S")
        }
    );
    assert_eq!(run.at("B").late.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("B").late.finish, moment("2026-10-23T17:00"));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::LinkStartFromSummary {
            predecessor: run.uid("S"),
            successor: run.uid("Y")
        }]
    );
}

#[test]
fn r110_percent_lag_from_summary_is_ignored() {
    // S {A 1 j} FS+50 % → Y 1 j ; F ven : la durée de S dépend des dates
    // de A, que ce lien plafonne : lien ignoré (`link.unschedulable`), A
    // dégradée ; Y et A au plus tard ven.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("S", 0)
        .task("A", 480)
        .task("Y", 480)
        .level("A", 2)
        .link("S", "Y", LinkType::FinishToStart, lag_pct(50))
        .run();
    assert_eq!(run.codes(), ["link.unschedulable"]);
    assert!(run.at("A").degraded);
    assert_eq!(run.at("A").start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("Y").start, moment("2026-10-23T08:00"));
}

#[test]
fn r110_summary_constraints_from_finish() {
    // S {A 1 j} FNLT mer 21/10 17:00, F ven → C(S) mer 17:00 → A mer.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .constraint(
            "S",
            Constraint::FinishNoLaterThan {
                date: moment("2026-10-21T17:00"),
            },
        )
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("S").start, moment("2026-10-21T08:00"));
    assert_eq!(run.codes(), [""; 0]);
    // S {A 1 j} SNET ven 23/10 13:00 : A planifiée ven 08:00, avant la date
    // brute → `constraint.not_honored { S }` ; au plus tôt, la porte (13:00)
    // repousse A à ven 13:00 – lun 12:00, TF(A) −240.
    let date = moment("2026-10-23T13:00");
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .constraint("S", Constraint::StartNoEarlierThan { date })
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").early.start, moment("2026-10-23T13:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(-240));
    assert_eq!(
        run.schedule.diagnostics,
        [Diagnostic::ConstraintNotHonored {
            task: run.uid("S"),
            kind: plannercraft_core::ConstraintKind::StartNoEarlierThan,
            date
        }]
    );
    assert!(matches!(run.outcome("S"), TaskOutcome::Scheduled(_)));
}
