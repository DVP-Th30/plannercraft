//! R-044 — Plusieurs prédécesseurs et pilote. ES0 = max brut des bornes ;
//! pilote = première borne maximale dans l'ordre : début du projet, porte du
//! parent, liens dans l'ordre de stockage. Source : S22 ; départage :
//! hypothèse.

use plannercraft_core::{Constraint, ConstraintKind, Driver, LinkType};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r044_latest_predecessor_drives() {
    // A 1 j (lun), B 2 j (lun – mar), C FS de A et de B, 1 j → mer 08:00 –
    // 17:00, pilote B.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("C").early.start, moment("2026-10-21T08:00"));
    assert_eq!(
        run.at("C").driver,
        Driver::Link {
            predecessor: run.uid("B"),
            link: LinkType::FinishToStart
        }
    );
}

#[test]
fn r044_tie_keeps_first_stored_link() {
    // A et B 1 j, mêmes bornes : le pilote est le premier lien stocké.
    let net = || {
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 480)
            .task("C", 480)
    };
    let ab = net().fs("A", "C").fs("B", "C").run();
    let ba = net().fs("B", "C").fs("A", "C").run();
    let driver = |r: &plannercraft_tests::net::Run, n: &str| Driver::Link {
        predecessor: r.uid(n),
        link: LinkType::FinishToStart,
    };
    assert_eq!(ab.at("C").driver, driver(&ab, "A"));
    assert_eq!(ba.at("C").driver, driver(&ba, "B"));
    assert_eq!(ab.at("C").early, ba.at("C").early);
}

#[test]
fn r044_constraint_driver() {
    // Amendement J3 : une contrainte n'est pas une borne brute ; elle ne
    // pilote que si elle déplace le début placé. A 1 j FS → B 1 j SNET mar
    // 20/10 07:00 : la date (mar 07:00) dépasse en brut la borne du lien
    // (lun 17:00), mais les deux se placent mar 08:00 → B mar 08:00, pilote
    // le lien.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint(
            "B",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-20T07:00"),
            },
        )
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(
        run.at("B").driver,
        Driver::Link {
            predecessor: run.uid("A"),
            link: LinkType::FinishToStart
        }
    );
    // SNET mar 09:00 : s* = mar 09:00 > mar 08:00 → B mar 09:00 – mer
    // 09:00, pilote la contrainte.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint(
            "B",
            Constraint::StartNoEarlierThan {
                date: moment("2026-10-20T09:00"),
            },
        )
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T09:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-21T09:00"));
    assert_eq!(
        run.at("B").driver,
        Driver::Constraint {
            kind: ConstraintKind::StartNoEarlierThan
        }
    );
}
