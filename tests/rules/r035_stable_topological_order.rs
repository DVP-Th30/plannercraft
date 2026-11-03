//! R-035 — Ordre topologique stable. Nœuds numérotés canoniquement (porte,
//! feuille, agrégat), ordre de Kahn départagé par l'indice canonique ;
//! identité si l'ordre des ID est déjà topologique. Indépendant de l'ordre de
//! stockage des liens et de toute table de hachage. Hypothèse (règle 5).

use plannercraft_core::TaskUid;
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn uids(v: &[u32]) -> Vec<TaskUid> {
    v.iter().map(|u| TaskUid::new(*u)).collect()
}

fn three() -> Net {
    // T1 (UID 10, ID 1), T2 (UID 5, ID 2), T3 (UID 7, ID 3), 1 j chacune.
    Net::new("2026-10-19T08:00")
        .task_with_uid("T1", 10, 480)
        .task_with_uid("T2", 5, 480)
        .task_with_uid("T3", 7, 480)
}

#[test]
fn r035_order_follows_links_then_ids() {
    // Lien T3 → T1 (FS) : T1 n'est prête qu'après T3. Kahn sur le tas-min
    // de l'indice canonique : T2 (indice 1), T3 (2), puis T1 (0).
    let run = three().fs("T3", "T1").run();
    assert_eq!(run.schedule.order, uids(&[5, 7, 10]));
    // T1 commence après T3 : mar 20/10 08:00.
    assert_eq!(run.at("T1").early.start, moment("2026-10-20T08:00"));
}

#[test]
fn r035_order_is_id_order_without_links() {
    // Sans lien : chemin rapide, ordre des ID (et non des UID).
    let run = three().run();
    assert_eq!(run.schedule.order, uids(&[10, 5, 7]));
}

#[test]
fn r035_order_ignores_link_storage_order() {
    // C a deux prédécesseurs, stockés dans les deux ordres : même ordre,
    // mêmes dates (seul le pilote peut changer, R-044).
    let base = || {
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 480)
            .task("C", 480)
            .task("D", 480)
            .fs("D", "A")
    };
    let ab = base().fs("A", "C").fs("B", "C").run();
    let ba = base().fs("B", "C").fs("A", "C").run();
    assert_eq!(ab.schedule.order, ba.schedule.order);
    // D (ID 4) remonte la liste : B (2), D (4), A (1), C (3) ; Kahn tire B
    // avant D (indice 1 < 3), puis A, puis C.
    assert_eq!(ab.schedule.order, uids(&[2, 4, 1, 3]));
    for name in ["A", "B", "C", "D"] {
        assert_eq!(ab.at(name).early, ba.at(name).early, "{name}");
        assert_eq!(ab.at(name).late, ba.at(name).late, "{name}");
    }
}
