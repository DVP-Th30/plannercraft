//! R-047 — Passe arrière : transfert adjoint. Plafond de fin FS `h(LS_s)`,
//! FF `h(LF_s)` ; plafond de début SS `h(LS_s)`, SF `h(LF_s)` converti par
//! `forward_span(y, d).finish` ; `late = backward_span(min, d)` ;
//! `h(y) = next_working_σ(add_σ(y, −L))`. Source : S22 ; adjoint :
//! hypothèse (aucun décalage dans le corpus).

use plannercraft_core::{LateDriver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_e, lag_w};

#[test]
fn r047_fs_network() {
    // N1 : C mer 08:00 – 17:00 ; B plafond h(mer 08:00) = mer 08:00 →
    // backward_span(mer 08:00, 960) = lun 08:00 – mar 17:00 ; A mar 08:00 –
    // 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("C").late.start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("B").late.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("B").late.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-20T17:00"));
}

#[test]
fn r047_mixed_calendars_with_lead() {
    // Début ven 23/10 08:00 ; P (24 heures) 960 min : ven 08:00 → sam 24/10
    // 00:00 ; Q (Standard) 1 j FS −60 min : add_std(sam 00:00, −60) =
    // ven 16:00 → Q ven 16:00 – lun 26/10 16:00 = fin du projet.
    // Retour : h(ven 16:00) = next_std(add_std(ven 16:00, +60)) =
    // next_std(ven 17:00) = lun 08:00 → P LF lun 08:00, TF(P) = 3 360 (56 h).
    // Le miroir naïf donnerait LF ven 17:00 et TF −420.
    let run = Net::new("2026-10-23T08:00")
        .task("P", 960)
        .calendar("P", 2)
        .task("Q", 480)
        .link("P", "Q", LinkType::FinishToStart, lag_w(-60))
        .run();
    assert_eq!(run.at("P").early.finish, moment("2026-10-24T00:00"));
    assert_eq!(run.at("Q").early.start, moment("2026-10-23T16:00"));
    assert_eq!(run.at("Q").early.finish, moment("2026-10-26T16:00"));
    assert_eq!(run.at("P").late.finish, moment("2026-10-26T08:00"));
    assert_eq!(run.at("P").total_slack, Minutes::new(3_360));
    assert_eq!(run.at("Q").total_slack, Minutes::ZERO);
}

#[test]
fn r047_ff_cap() {
    // A 2 j ; B FF 1 j (mar 08:00 – 17:00) ; D 3 j seul : fin du projet
    // mer 17:00. B LF mer 17:00 ; A plafond h(LF_B) = mer 17:00 →
    // A mar 08:00 – mer 17:00, TF(A) = 480.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .task("D", 1_440)
        .link("A", "B", LinkType::FinishToFinish, lag_w(0))
        .run();
    assert_eq!(run.at("B").late.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
}

#[test]
fn r047_ss_cap_final_successor() {
    // A 1 j ; B SS+4 h 1 j (lun 13:00 – mar 12:00), tâche finale : fin du
    // projet mar 12:00. B LS lun 13:00 ; A plafond de début
    // h(lun 13:00) = next(add(lun 13:00, −240)) = lun 08:00 → plafond de fin
    // forward_span(lun 08:00, 480).finish = lun 17:00 → A lun 08:00 – 17:00,
    // TF 0.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .link("A", "B", LinkType::StartToStart, lag_w(240))
        .run();
    assert_eq!(run.at("B").late.start, moment("2026-10-19T13:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
}

#[test]
fn r047_ss_cap_with_slack() {
    // Même lien, D 3 j seul : fin du projet mer 17:00. B LS = mer 08:00 ;
    // A plafond de début h(mer 08:00) = add(mer 08:00, −240) = mar 13:00 →
    // plafond de fin forward_span(mar 13:00, 480).finish = mer 12:00 →
    // A mar 13:00 – mer 12:00, TF(A) = 720 = TF(B).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .task("D", 1_440)
        .link("A", "B", LinkType::StartToStart, lag_w(240))
        .run();
    assert_eq!(run.at("A").late.start, moment("2026-10-20T13:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-21T12:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(720));
    assert_eq!(run.at("B").total_slack, Minutes::new(720));
}

#[test]
fn r047_sf_cap() {
    // A 2 j ; B SF+3 j 1 j (mer 08:00 – 17:00) ; D 4 j seul : fin du projet
    // jeu 22/10 17:00. B LF jeu 17:00 ; A plafond de début
    // h(jeu 17:00) = add(jeu 17:00, −1 440) = mar 08:00 → plafond de fin
    // forward_span(mar 08:00, 960).finish = mer 17:00 → A mar 08:00 –
    // mer 17:00, TF(A) = 480 = TF(B).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .task("D", 1_920)
        .link("A", "B", LinkType::StartToFinish, lag_w(1_440))
        .run();
    assert_eq!(run.at("A").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(480));
    assert_eq!(run.at("B").total_slack, Minutes::new(480));
}

#[test]
fn r047_elapsed_lag_cap() {
    // Début ven 23/10 08:00 ; A 1 j ; B FS+2 ej (lun 26/10 08:00 – 17:00).
    // Retour : h(lun 08:00) = lun 08:00 − 2 880 = sam 24/10 08:00 →
    // backward_span(sam 08:00, 480) = ven 08:00 – 17:00 : TF(A) = 0.
    let run = Net::new("2026-10-23T08:00")
        .task("A", 480)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToStart, lag_e(2_880))
        .run();
    assert_eq!(run.at("A").late.finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
}

#[test]
fn r047_ff_elapsed_lag_into_working_task() {
    // A 1 j ; B FF+6 em 1 j : borne lun 17:06 (temps civil), B =
    // backward_span(lun 17:06, 480) = lun 08:00 – 17:00 : sa fin est
    // ramenée à 17:00, de même temps ouvré que la borne. Retour : l'ancre
    // de B est portée à next(lun 17:00) = mar 08:00, h = mar 07:54 →
    // TF(A) = FS(A) = 0 (sans ce report : −6).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .link("A", "B", LinkType::FinishToFinish, lag_e(6))
        .run();
    assert_eq!(run.at("B").early.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("A").late.finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
}

/// Fin du projet quand `name` est placée en manuel à `start` – `finish`
/// (même réseau) : un retard jusqu'aux dates au plus tard ne doit pas la
/// décaler (S16, S17).
fn finish_with(
    net: impl Fn() -> Net,
    name: &str,
    start: plannercraft_core::Moment,
    finish: plannercraft_core::Moment,
) -> plannercraft_core::Moment {
    let run = net()
        .with_task(name, |t| {
            t.mode = plannercraft_core::SchedulingMode::Manual {
                start: Some(start),
                finish: Some(finish),
                duration: None,
            };
        })
        .run();
    assert!(
        !run.codes().contains(&"link.manual_not_honored"),
        "{:?}",
        run.codes()
    );
    run.schedule.project_finish
}

#[test]
fn r047_start_cap_off_working_time_elapsed_lag() {
    // A 1 j ; B 1 j SS+600 em de A ; C 4 j seule : B ≥ lun 18:00 → mar
    // 08:00 – 17:00 ; fin du projet jeu 22/10 17:00. B LS jeu 08:00 ;
    // plafond de début de A h(jeu 08:00) = mer 22:00, hors temps ouvré : le
    // dernier début plaçable est mer 16:59 (placée à jeu 08:00, A commencerait
    // après la borne et B passerait à ven). LS(A) = mer 16:59, TF 1 439 ;
    // FS : h(ES_B = mar 08:00) = lun 22:00 → lun 16:59 → 479.
    let net = || {
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 480)
            .task("C", 1_920)
            .link("A", "B", LinkType::StartToStart, lag_e(600))
    };
    let run = net().run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-22T17:00"));
    let a = *run.at("A");
    assert_eq!(a.late.start, moment("2026-10-21T16:59"));
    assert_eq!(a.late.finish, moment("2026-10-22T16:59"));
    assert_eq!(a.total_slack, Minutes::new(1_439));
    assert_eq!(a.free_slack, Minutes::new(479));
    // A placée à ses dates au plus tard : la fin du projet ne bouge pas.
    assert_eq!(
        finish_with(net, "A", a.late.start, a.late.finish),
        moment("2026-10-22T17:00")
    );
}

#[test]
fn r047_start_cap_from_another_calendar() {
    // A (Standard) 1 j ; B (24 heures) 2 400 min SS+0 de A ; C 5 j seule :
    // fin du projet ven 23/10 17:00. B LS = ven 17:00 − 40 h = jeu 01:00 :
    // plafond de début de A jeu 01:00, hors temps ouvré Standard → mer
    // 16:59 (et non jeu 08:00, qui pousserait B et la fin du projet au sam
    // 24/10 00:00).
    let net = || {
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 2_400)
            .calendar("B", 2)
            .task("C", 2_400)
            .link("A", "B", LinkType::StartToStart, lag_w(0))
    };
    let run = net().run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("B").late.start, moment("2026-10-22T01:00"));
    let a = *run.at("A");
    assert_eq!(a.late.start, moment("2026-10-21T16:59"));
    assert_eq!(
        finish_with(net, "A", a.late.start, a.late.finish),
        moment("2026-10-23T17:00")
    );
}

#[test]
fn r047_lead_into_elapsed_successor_is_adjoint_in_raw_time() {
    // A 2 j (lun – mar) ; B à durée écoulée 2 000 min, FS −1 j de A (σ =
    // calendrier du projet, R-041) : B mar 08:00 – mer 17:20 ; C 1 j FS de
    // B : jeu 08:00 – 17:00 = fin du projet. LS(B) = jeu 08:00 − 2 000 =
    // mar 22:40. B lit sa borne en brut : h ramène mar 22:40 au début de la
    // dernière minute ouvrée (mar 16:59), puis add(+480) = mer 16:59. LF(A)
    // = mer 16:59, TF 479 (l'adjoint en temps ouvré donnait jeu 08:00 →
    // LF mer 17:00, TF 480 : A placée là, B finirait jeu 17:20 et C ven).
    let net = || {
        Net::new("2026-10-19T08:00")
            .task("A", 960)
            .elapsed("B", 2_000)
            .task("C", 480)
            .link("A", "B", LinkType::FinishToStart, lag_w(-480))
            .fs("B", "C")
    };
    let run = net().run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T17:20"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-22T17:00"));
    assert_eq!(run.at("B").late.start, moment("2026-10-20T22:40"));
    let a = *run.at("A");
    assert_eq!(a.late.finish, moment("2026-10-21T16:59"));
    assert_eq!(a.total_slack, Minutes::new(479));
    assert_eq!(a.free_slack, Minutes::ZERO);
    assert_eq!(
        finish_with(net, "A", a.late.start, a.late.finish),
        moment("2026-10-22T17:00")
    );
}

#[test]
fn r047_lead_into_milestone_reads_raw_time() {
    // A 1 j (lun) ; M jalon FS −60 min de A : borne add(lun 17:00, −60) =
    // lun 16:00 → M lun 16:00 ; E à durée écoulée 1 440 em FS de M ; C 3 j
    // seule : fin du projet mer 17:00. LS(E) = mar 17:00, donc LS(M) =
    // mar 17:00, fin d'une minute ouvrée. Un jalon lit sa borne en brut
    // (R-031) : h ramène mar 17:00 à mar 16:59, add(+60) = mer 08:59 →
    // LF(A) = mer 08:59, TF 539. (Adjoint en temps ouvré : LF mer 09:00 ; A
    // placée là pousserait M à mer 08:00 et E au jeu 08:00.)
    let net = || {
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("M", 0)
            .elapsed("E", 1_440)
            .task("C", 1_440)
            .link("A", "M", LinkType::FinishToStart, lag_w(-60))
            .fs("M", "E")
    };
    let run = net().run();
    assert_eq!(run.at("M").early.start, moment("2026-10-19T16:00"));
    assert_eq!(run.at("M").late.start, moment("2026-10-20T17:00"));
    let a = *run.at("A");
    assert_eq!(a.late.finish, moment("2026-10-21T08:59"));
    assert_eq!(a.total_slack, Minutes::new(539));
    assert_eq!(
        finish_with(net, "A", a.late.start, a.late.finish),
        moment("2026-10-21T17:00")
    );
}

#[test]
fn r047_finish_link_into_milestone_anchor() {
    // A à durée écoulée 480 em (lun 08:00 – 16:00) ; M jalon FF de A : borne
    // lun 16:00 → M lun 16:00 ; E 1 440 em FS de M ; C 3 j seule : fin du
    // projet mer 17:00, LF(M) = mar 17:00. Une borne de fin b place le jalon
    // en b si b commence une minute ouvrée, sinon à l'instant ouvré
    // précédent : l'ancre est next(mar 17:00) − 1 = mer 07:59 (mer 08:00
    // placerait M à mer 08:00). LF(A) = mer 07:59, TF = 2 399 en temps civil.
    let net = || {
        Net::new("2026-10-19T08:00")
            .elapsed("A", 480)
            .task("M", 0)
            .elapsed("E", 1_440)
            .task("C", 1_440)
            .link("A", "M", LinkType::FinishToFinish, lag_w(0))
            .fs("M", "E")
    };
    let run = net().run();
    assert_eq!(run.at("M").early.finish, moment("2026-10-19T16:00"));
    assert_eq!(run.at("M").late.finish, moment("2026-10-20T17:00"));
    let a = *run.at("A");
    assert_eq!(a.late.finish, moment("2026-10-21T07:59"));
    assert_eq!(a.total_slack, Minutes::new(2_399));
    assert_eq!(
        finish_with(net, "A", a.late.start, a.late.finish),
        moment("2026-10-21T17:00")
    );
}

#[test]
fn r047_late_driver_project_finish_and_successor() {
    // N1 : C et D finissent à la fin du projet (aucun successeur) ; B et A
    // sont plafonnés par leur lien FS vers C (premier minimum strict).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    let c = run.uid("C");
    for name in ["C", "D"] {
        assert_eq!(run.at(name).late_driver, LateDriver::ProjectFinish, "{name}");
    }
    for name in ["A", "B"] {
        assert_eq!(
            run.at(name).late_driver,
            LateDriver::Successor {
                successor: c,
                link: LinkType::FinishToStart
            },
            "{name}"
        );
    }
    // Récapitulative du projet : agrégat.
    assert_eq!(
        run.schedule.project_summary.unwrap().late_driver,
        LateDriver::Rollup
    );
}

#[test]
fn r047_late_driver_equal_caps_keep_the_first() {
    // A 1 j FS → B 1 j, B finale ; la fin du projet (mar 17:00) est aussi le
    // plafond de B : égalité, la fin du projet (première) reste la cause.
    // A : plafond h(LS_B = mar 08:00) = mar 08:00 < fin du projet → B.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .run();
    assert_eq!(run.at("B").late_driver, LateDriver::ProjectFinish);
    assert_eq!(
        run.at("A").late_driver,
        LateDriver::Successor {
            successor: run.uid("B"),
            link: LinkType::FinishToStart
        }
    );
}

#[test]
fn r047_late_driver_summary_cap_and_rollup() {
    // S {A 1 j, B 2 j} FS → Y 1 j : C(S) = h(LS_Y = mer 08:00) = mer 08:00
    // plafonne A et B (fin du projet mer 17:00) ; S : agrégat.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .task("B", 960)
        .task("Y", 480)
        .level("A", 2)
        .level("B", 2)
        .fs("S", "Y")
        .run();
    let s = run.uid("S");
    for name in ["A", "B"] {
        assert_eq!(
            run.at(name).late_driver,
            LateDriver::SummaryCap { summary: s },
            "{name}"
        );
    }
    assert_eq!(run.at("S").late_driver, LateDriver::Rollup);
    // Plafond hérité d'un ancêtre : la cause nomme la récapitulative qui l'a
    // posé. P {S {A 1 j}} FS → Y : A plafonnée par C(P), posé par P.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 0)
        .task("S", 0)
        .task("A", 480)
        .task("Y", 480)
        .level("S", 2)
        .level("A", 3)
        .fs("P", "Y")
        .run();
    assert_eq!(
        run.at("A").late_driver,
        LateDriver::SummaryCap {
            summary: run.uid("P")
        }
    );
}

#[test]
fn r047_late_driver_multiple_critical_paths() {
    // R-051 : A 1 j sans successeur, C 3 j, option active → LF(A) = EF(A).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_440)
        .options(|o| o.multiple_critical_paths = true)
        .run();
    assert_eq!(run.at("A").late_driver, LateDriver::MultipleCriticalPaths);
    // C finit à la fin du projet : égalité, la fin du projet reste la cause.
    assert_eq!(run.at("C").late_driver, LateDriver::ProjectFinish);
}
