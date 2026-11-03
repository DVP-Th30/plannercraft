//! R-101 — Marge négative, marge libre bornée : la marge totale est signée
//! et conservée, une tâche est critique si `TF ≤ seuil` (négative
//! comprise) ; `FS = max(0, min(TF, écarts))`. Sans contrainte ni
//! échéance, la borne est sans effet (FS ≥ 0 de R-049 reste une propriété).
//! Sources : S16 (« Negative slack … is usually caused by constraint dates
//! or task dependencies »), S31, Q10.

use plannercraft_core::{Constraint, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r101_negative_total_slack_and_bounded_free_slack() {
    // Q 1 j FS → P 3 j FS → A 1 j SNLT mar 20/10 08:00 (option active) :
    // A mar ; P au plus tard jeu 15/10 08:00 – lun 17:00, TF(P) −1 440 ; Q
    // au plus tard mer 14/10, TF(Q) −1 440 ; FS(Q) = FS(P) = 0 (écarts
    // négatifs bornés) ; P et Q critiques.
    let run = Net::new("2026-10-19T08:00")
        .task("Q", 480)
        .task("P", 1_440)
        .task("A", 480)
        .fs("Q", "P")
        .fs("P", "A")
        .constraint(
            "A",
            Constraint::StartNoLaterThan {
                date: moment("2026-10-20T08:00"),
            },
        )
        .run();
    for name in ["P", "Q"] {
        let t = run.at(name);
        assert_eq!(t.total_slack, Minutes::new(-1_440), "{name}");
        assert_eq!(t.free_slack, Minutes::ZERO, "{name}");
        assert!(t.critical, "{name}");
    }
    assert_eq!(run.at("Q").late.start, moment("2026-10-14T08:00"));
    // Récapitulative du projet : marge totale négative, marge libre 0.
    let ps = run.schedule.project_summary.unwrap();
    assert!(ps.total_slack < Minutes::ZERO);
    assert_eq!(ps.free_slack, Minutes::ZERO);
}

#[test]
fn r101_same_chain_with_mso() {
    // Même chaîne, A MSO mar 20/10 08:00 : mêmes marges.
    let run = Net::new("2026-10-19T08:00")
        .task("Q", 480)
        .task("P", 1_440)
        .task("A", 480)
        .fs("Q", "P")
        .fs("P", "A")
        .constraint(
            "A",
            Constraint::MustStartOn {
                date: moment("2026-10-20T08:00"),
            },
        )
        .run();
    for name in ["P", "Q"] {
        assert_eq!(run.at(name).total_slack, Minutes::new(-1_440), "{name}");
        assert_eq!(run.at(name).free_slack, Minutes::ZERO, "{name}");
        assert!(run.at(name).critical, "{name}");
    }
}
