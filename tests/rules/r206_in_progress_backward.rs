//! R-206 — Passe arrière et marges d'une tâche en cours. Partie réelle
//! fixe ; fin au plus tard par les plafonds normaux ; début au plus tard du
//! reste = `backward_span(LF, D − réel).start` ; début au plus tard de la
//! tâche = début réel ; marge totale = celle du reste (reprise contre début
//! au plus tard du reste, fin contre fin au plus tard) sur l'échelle de
//! marge ; critique selon R-050 (une tâche en cours peut l'être) ; elle ne
//! plafonne ses prédécesseurs que par la partie restante (FS : borne sur
//! `Resume`). Hypothèse (aucun cas au corpus).

use plannercraft_core::{LateDriver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

use crate::r204_in_progress_leaf::started;

#[test]
fn r206_backward_remaining_only() {
    // T de R-204 (fin mer 17:00), U 1 j FS de T, V 5 j seule (fin du
    // projet ven 17:00) : LF(T) = jeu 17:00, TF(T) = 480 ; début au plus
    // tard = début réel.
    let run = started(3_333).task("U", 480).task("V", 2_400).fs("T", "U").run();
    let t = run.at("T");
    assert_eq!(t.late.start, moment("2026-10-19T08:00"));
    assert_eq!(t.late.finish, moment("2026-10-22T17:00"));
    assert_eq!(t.total_slack, Minutes::new(480));
    assert_eq!(t.free_slack, Minutes::ZERO);
    assert_eq!(
        t.late_driver,
        LateDriver::Successor {
            successor: run.uid("U"),
            link: LinkType::FinishToStart,
        }
    );
    assert!(!t.critical);
    // Prédécesseur P 1 j FS → T : plafonné par le début au plus tard du
    // reste (mer 08:00), LF(P) = mar 17:00, TF(P) = 480 — et non par le
    // début réel de T (lun 08:00), qui donnerait une marge négative.
    let run = started(3_333)
        .task("P", 480)
        .task("U", 480)
        .task("V", 2_400)
        .fs("P", "T")
        .fs("T", "U")
        .run();
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    let p = run.at("P");
    assert_eq!(p.late.finish, moment("2026-10-20T17:00"));
    assert_eq!(p.total_slack, Minutes::new(480));
    // Marge libre de P : jusqu'à la reprise de T (mar 08:00), soit 0.
    assert_eq!(p.free_slack, Minutes::ZERO);
}

#[test]
fn r206_critical_in_progress() {
    // T en cours tire la fin du projet : TF 0, critique ; U après T aussi.
    let run = started(3_333).task("U", 480).fs("T", "U").run();
    let t = run.at("T");
    assert_eq!(t.total_slack, Minutes::ZERO);
    assert!(t.critical);
    assert!(run.at("U").critical);
    assert_eq!(run.schedule.project_finish, moment("2026-10-22T17:00"));
    // Implémentations rapide et de référence identiques.
    let project = started(3_333)
        .task("P", 480)
        .task("U", 480)
        .fs("P", "T")
        .fs("T", "U")
        .build();
    assert_eq!(
        plannercraft_core::schedule(&project),
        plannercraft_core::schedule::schedule_reference(&project)
    );
    // Sous-tâche en cours (durée réelle saisie, sans début réel) : la
    // récapitulative l'enveloppe.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("T", 1_440)
        .level("T", 2)
        .with_task("T", |t| {
            t.progress.actual_start = Some(moment("2026-10-19T08:00"));
            t.progress.actual_duration = Minutes::new(480);
        })
        .run();
    assert_eq!(run.at("S").finish, moment("2026-10-21T17:00"));
}
