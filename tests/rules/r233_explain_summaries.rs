//! R-233 — Résumés par gabarit : `summary_fr` et `summary_en` sont
//! construits par un gabarit fixe à partir des seuls champs de
//! l'`Explanation` (noms des tâches et des calendriers lus dans le projet),
//! dates au format de R-143 précédées du jour de semaine abrégé ; aucune
//! date n'est calculée hors du cœur (règle 3). Hypothèse (forme du texte).

#![allow(
    clippy::many_single_char_names,
    reason = "tests : noms des tâches des exemples (A, B, C…)"
)]

use plannercraft_automation::explain::summary;
use plannercraft_automation::text::Language;
use plannercraft_core::schedule::explain::{Side, explain};
use plannercraft_tests::net::Net;

use crate::r204_in_progress_leaf::started;
use crate::r230_explain_start::{abc, plan};

#[test]
fn r233_templates_golden() {
    // Exemple de R-233 : C ← B ← début du projet.
    let (p, s, [c]) = plan(abc(), ["C"]);
    let x = explain(&p, &s, c, Side::Start, 5).unwrap();
    assert_eq!(
        summary(&p, &x, Language::Fr),
        "C commence le mer 21/10/2026 08:00 : fin de B (mar 20/10/2026 17:00, lien FS), reportée au \
         premier instant ouvré du calendrier Standard. B commence au début du projet."
    );
    assert_eq!(
        summary(&p, &x, Language::En),
        "C starts on Wed 10/21/2026 08:00: finish of B (Tue 10/20/2026 17:00, FS link), moved to the next \
         working time of calendar Standard. B starts at the project start."
    );
    // Fin (R-231) : durée sur le calendrier, puis le début.
    let x = explain(&p, &s, c, Side::Finish, 5).unwrap();
    assert_eq!(
        summary(&p, &x, Language::Fr),
        "C finit le mer 21/10/2026 17:00 : 1j depuis son début sur le calendrier Standard. C commence le \
         mer 21/10/2026 08:00 : fin de B (mar 20/10/2026 17:00, lien FS), reportée au premier instant \
         ouvré du calendrier Standard. B commence au début du projet."
    );
    // Tâche en cours (R-204, R-205) : réel, reprise et sa cause ;
    // hypothèse signalée (R-206).
    let (p, s, [t]) = plan(started(3_333).task("P", 960).fs("P", "T"), ["T"]);
    let x = explain(&p, &s, t, Side::Start, 5).unwrap();
    assert_eq!(
        summary(&p, &x, Language::Fr),
        "T commence le lun 19/10/2026 08:00 : début réel le lun 19/10/2026 08:00 ; partie réelle jusqu'au \
         lun 19/10/2026 17:00, reprise le mer 21/10/2026 08:00 (fin de P, lien FS). Les dates au plus tard \
         reposent sur une hypothèse (tâche en cours, R-206)."
    );
    assert_eq!(
        summary(&p, &x, Language::En),
        "T starts on Mon 10/19/2026 08:00: actual start on Mon 10/19/2026 08:00; actual part until Mon \
         10/19/2026 17:00, resumes on Wed 10/21/2026 08:00 (finish of P, FS link). Late dates rely on an \
         assumption (task in progress, R-206)."
    );
    // Chaîne coupée.
    let (p, s, [d]) = plan(
        Net::new("2026-10-19T08:00")
            .task("A", 480)
            .task("B", 480)
            .task("D", 480)
            .fs("A", "B")
            .fs("B", "D"),
        ["D"],
    );
    let x = explain(&p, &s, d, Side::Start, 1).unwrap();
    assert_eq!(
        summary(&p, &x, Language::Fr),
        "D commence le mer 21/10/2026 08:00 : fin de B (mar 20/10/2026 17:00, lien FS), reportée au \
         premier instant ouvré du calendrier Standard. B commence le mar 20/10/2026 08:00 : fin de A \
         (lun 19/10/2026 17:00, lien FS), reportée au premier instant ouvré. La chaîne continue au-delà \
         de la profondeur demandée."
    );
    // Les résumés ne contiennent que des dates de l'explication.
    let dates = [x.date, x.chain[0].start, x.chain[0].raw.unwrap()];
    let fr = summary(&p, &x, Language::Fr);
    for m in dates {
        let text = m.to_string();
        let (day, time) = text.split_once('T').unwrap();
        let mut parts = day.split('-');
        let (y, mo, dd) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        assert!(fr.contains(&format!("{dd}/{mo}/{y} {time}")), "{fr}");
    }
}
