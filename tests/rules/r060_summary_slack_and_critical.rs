//! R-060 — Dates au plus tard, marges et critique d'une récapitulative :
//! LS = min LS, LF = max LF des enfants actifs ; TF = `min(between(ES, LS),
//! between(EF, LF))` sur ces dates agrégées et le calendrier de la
//! récapitulative ou du projet ; FS par R-049 (sans lien sortant propre ni
//! hérité : FS = TF) ; critique par R-050, jamais « parce qu'un enfant
//! l'est ». Source : observé O3 (A1 UID 108), O4, O5 ; reste : hypothèse.

use plannercraft_core::Minutes;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r060_summary_slack_on_aggregated_dates() {
    // P 1 j (lun) ; S {A 1 j sans lien, B 1 j FS de P} ; Q 1 j FS de B :
    // B mar, Q mer, fin du projet mer 17:00 ; A LF mer 17:00, TF 960 ;
    // B TF 0, critique.
    // S : ES lun 08:00, EF mar 17:00, LS = min(mer 08:00, mar 08:00) =
    // mar 08:00, LF = max(mer 17:00, mar 17:00) = mer 17:00 → TF(S) =
    // min(480, 480) = 480, FS(S) = 480, non critique malgré B.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 480)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .task("Q", 480)
        .fs("P", "B")
        .fs("B", "Q")
        .run();
    assert_eq!(run.at("A").total_slack, Minutes::new(960));
    assert!(run.at("B").critical);
    let s = run.at("S");
    assert_eq!(s.early.start, moment("2026-10-19T08:00"));
    assert_eq!(s.early.finish, moment("2026-10-20T17:00"));
    assert_eq!(s.late.start, moment("2026-10-20T08:00"));
    assert_eq!(s.late.finish, moment("2026-10-21T17:00"));
    assert_eq!(s.total_slack, Minutes::new(480));
    assert_eq!(s.free_slack, Minutes::new(480));
    assert!(!s.critical);
}

#[test]
fn r060_summary_with_outgoing_link_free_slack() {
    // S {A 1 j, B 2 j} FS → Y 1 j ; Z 4 j seule : fin du projet jeu 22/10
    // 17:00. Y mer 21/10, LF jeu 17:00 → LS(Y) = jeu 08:00 ; plafond hérité
    // h(LS_Y) = jeu 08:00 → LS(A) = mer 08:00, LS(B) = mar 08:00.
    // S : LS = mar 08:00, LF = jeu 08:00 → TF(S) = min(480, 480) = 480 ;
    // FS(S) = écart à Y = between(mar 17:00, mer 08:00) = 0 (O5).
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 960)
        .level("B", 2)
        .task("Y", 480)
        .task("Z", 1_920)
        .fs("S", "Y")
        .run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-22T17:00"));
    let s = run.at("S");
    assert_eq!(s.total_slack, Minutes::new(480));
    assert_eq!(s.free_slack, Minutes::ZERO);
    assert!(!s.critical);
}

#[test]
fn r060_summary_critical_by_threshold() {
    // Même réseau que le premier cas, seuil 1 j : TF(S) = 480 ≤ 480 →
    // critique.
    let run = Net::new("2026-10-19T08:00")
        .options(|o| o.critical_slack_limit_days = 1)
        .task("P", 480)
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .task("Q", 480)
        .fs("P", "B")
        .fs("B", "Q")
        .run();
    assert!(run.at("S").critical);
}
