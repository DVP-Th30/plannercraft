//! R-256 — Collage d'un texte externe (empreinte différente du dernier
//! texte émis) : lignes séparées par `\n` ou `\r\n`, cellules par
//! tabulation ; (a) première ligne reconnue par `csv::recognize` (colonne
//! Nom) ⇒ lignes nouvelles par le chemin de l'import CSV ; (b) une seule
//! colonne sans tabulation ⇒ une tâche par ligne non vide ; (c) sinon
//! `paste.unrecognized`. Plafonds : 10 000 lignes, 4 Mio
//! (`paste.too_large`). Repère : projet « P ». Source : hypothèse.

use plannercraft_automation::clip::{
    ClipError, MAX_PASTE_BYTES, MAX_PASTE_LINES, RowClip, fingerprint, paste_text,
};
use plannercraft_automation::csv::{CsvColumn, Recognized, recognize};
use plannercraft_automation::text::{Language, TextContext};
use plannercraft_automation::{ApplyMode, Placement, Session, TaskRef};
use plannercraft_tests::daily::{plan_p, uid, uids};
use plannercraft_tests::task_edit::{get, names};

/// Colle `text` avant 6 et applique ; rend le nombre de tâches collées.
fn paste_before_6(s: &mut Session, text: &str) -> Result<u32, ClipError> {
    let at = Placement::Before(TaskRef::Uid(uid(6)));
    let (batch, report) = paste_text(s, text, &at, Language::Fr)?;
    s.apply(&batch, ApplyMode::Commit).unwrap();
    Ok(report.tasks)
}

#[test]
fn r256_header_recognized() {
    // Exemple : « Nom\tDurée\nCoffrage\t3j\nFerraillage\t2j » ⇒ 2 tâches
    // de 3 j et 2 j, avant 6, au niveau de 6.
    let mut s = plan_p();
    assert_eq!(recognize("Durée"), Recognized::Column(CsvColumn::Duration));
    assert_eq!(recognize(" nom "), Recognized::Column(CsvColumn::Name));
    assert_eq!(recognize("Couleur"), Recognized::Unknown);
    let n = paste_before_6(&mut s, "Nom\tDurée\r\nCoffrage\t3j\r\nFerraillage\t2j\r\n").unwrap();
    assert_eq!(n, 2);
    let got = names(&s);
    assert_eq!(got[5], ("Coffrage".to_owned(), 2));
    assert_eq!(got[6], ("Ferraillage".to_owned(), 2));
    assert_eq!(get(&s, 8).duration.minutes().get(), 3 * 480);
    assert_eq!(get(&s, 9).duration.minutes().get(), 2 * 480);
    // En anglais, avec niveaux et prédécesseurs internes (par ID du texte).
    let mut s = plan_p();
    let text = "Name\tDuration\tOutline Level\tPredecessors\nLot\t\t1\t\nA\t2d\t2\t\nB\t1d\t2\t2\n";
    assert_eq!(paste_before_6(&mut s, text).unwrap(), 3);
    let got = names(&s);
    assert_eq!(got[5], ("Lot".to_owned(), 2));
    assert_eq!(got[6], ("A".to_owned(), 3));
    assert_eq!(got[7], ("B".to_owned(), 3));
    let b = get(&s, 10);
    assert_eq!(b.predecessors.len(), 1);
    assert_eq!(b.predecessors[0].predecessor.get(), 9);
}

#[test]
fn r256_single_column_names() {
    // Exemple : « A\nB\n\nC » ⇒ 3 tâches A, B, C (espaces de bord
    // retirées), durée par défaut.
    let mut s = plan_p();
    assert_eq!(paste_before_6(&mut s, "  A \nB\n\n   \nC").unwrap(), 3);
    let got = names(&s);
    let pasted: Vec<&str> = got.iter().skip(5).take(3).map(|(n, _)| n.as_str()).collect();
    assert_eq!(pasted, ["A", "B", "C"]);
    assert!(got.iter().skip(5).take(3).all(|(_, l)| *l == 2));
    let default = plannercraft_core::Task::new(uid(99), String::new(), &s.project().options).duration;
    assert_eq!(get(&s, 8).duration, default);
    // Texte vide ou blanc : rien à coller.
    let s = plan_p();
    assert_eq!(
        paste_text(&s, "\n  \n", &Placement::End, Language::Fr).map(|_| ()),
        Err(ClipError::Unrecognized)
    );
}

#[test]
fn r256_multi_column_without_header_refused() {
    // Exemple : « A\t3j\nB\t2j » ⇒ `paste.unrecognized`, rien écrit ; le
    // texte tabulé émis par PlannerCraft lui-même (sans en-tête) aussi.
    let s = plan_p();
    let err = paste_text(&s, "A\t3j\nB\t2j", &Placement::End, Language::Fr).unwrap_err();
    assert_eq!(err, ClipError::Unrecognized);
    assert_eq!(err.code(), "paste.unrecognized");
    let clip = RowClip::capture(&s, &uids(&[2, 3])).unwrap();
    let ctx = TextContext::for_project(s.project(), Language::Fr);
    let tsv = clip.to_tsv(&[CsvColumn::Id, CsvColumn::Name, CsvColumn::Duration], &ctx);
    // Durées par R-141 (relisibles par R-140), pas les libellés longs de
    // la table.
    assert_eq!(tsv, "2\tFondations\t5j\n3\tMurs\t10j");
    assert_eq!(
        paste_text(&s, &tsv, &Placement::End, Language::Fr).map(|_| ()),
        Err(ClipError::Unrecognized)
    );
}

#[test]
fn r256_line_cap() {
    // 10 000 lignes : acceptées ; 10 001 : `paste.too_large`.
    let s = plan_p();
    let ok = "x\n".repeat(MAX_PASTE_LINES);
    let (batch, report) = paste_text(&s, &ok, &Placement::End, Language::Fr).unwrap();
    assert_eq!(report.tasks as usize, MAX_PASTE_LINES);
    assert_eq!(batch.ops.len(), MAX_PASTE_LINES);
    let over = "x\n".repeat(MAX_PASTE_LINES + 1);
    let err = paste_text(&s, &over, &Placement::End, Language::Fr).unwrap_err();
    assert_eq!(err, ClipError::TooLarge);
    assert_eq!(err.code(), "paste.too_large");
}

#[test]
fn r256_byte_cap() {
    // Au-delà de 4 Mio : `paste.too_large`, avant toute lecture.
    let s = plan_p();
    let big = "a".repeat(MAX_PASTE_BYTES + 1);
    assert_eq!(
        paste_text(&s, &big, &Placement::End, Language::Fr).map(|_| ()),
        Err(ClipError::TooLarge)
    );
    let fits = "a".repeat(MAX_PASTE_BYTES);
    assert!(paste_text(&s, &fits, &Placement::End, Language::Fr).is_ok());
}

#[test]
fn clip_fingerprint_stable_crlf() {
    // Le texte émis par `to_tsv` (fins `\n`) et le même relu du
    // presse-papiers sous Windows (`\r\n`) ont la même empreinte ; un
    // autre texte, non.
    let s = plan_p();
    let clip = RowClip::capture(&s, &uids(&[1])).unwrap();
    let ctx = TextContext::for_project(s.project(), Language::Fr);
    let tsv = clip.to_tsv(&[CsvColumn::Name, CsvColumn::Duration], &ctx);
    let crlf = tsv.replace('\n', "\r\n");
    assert_eq!(fingerprint(&tsv), fingerprint(&crlf));
    assert_eq!(fingerprint(&tsv), fingerprint(&tsv.clone()));
    assert_ne!(fingerprint(&tsv), fingerprint(&format!("{tsv}x")));
    assert_eq!(fingerprint("Murs\n"), fingerprint("Murs\r\n"));
}
