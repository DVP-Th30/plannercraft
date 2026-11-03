//! R-146 — CSV : export. Colonnes par défaut, UTF-8 avec BOM, CRLF,
//! guillemets si nécessaire, durées par R-141 (récapitulative : durée du
//! planning), dates du planning par R-143, prédécesseurs par R-142, garde
//! de formule. Projet depuis lun 19/10/2026 08:00, Standard, 480 / 2 400 / 20.

use plannercraft_automation::csv::{
    CsvColumn, CsvExportOptions, CsvImportOptions, CsvLocale, export, import,
};
use plannercraft_core::{Project, ScheduleAnchor, TaskUid, schedule};
use plannercraft_mspdi::write::Newline;
use plannercraft_tests::moment;

fn imported(csv: &str) -> Project {
    let o = CsvImportOptions::new(
        ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        moment("2026-10-17T08:00"),
    );
    import(csv.as_bytes(), &o).unwrap().project
}

fn exported(p: &Project, options: &CsvExportOptions) -> String {
    String::from_utf8(export(p, &schedule(p), options)).unwrap()
}

const R145: &str = "N°;Nom;Durée;Début;Fin;Prédécesseurs;Niveau hiérarchique;Jalon\n\
1;Phase;10j;;;;1;Non\n2;A;3j;;;;2;Non\n3;B;2j;;;2;2;Non\n4;Fin;;;;3;1;Oui\n";

#[test]
fn r146_french_export_of_the_r145_project() {
    let p = imported(R145);
    let out = exported(&p, &CsvExportOptions::new(CsvLocale::Fr));
    // BOM, en-tête par défaut, CRLF ; Phase : durée du planning 5 j (lun
    // 08:00 – ven 17:00) ; B : « 2 » (FS sans décalage) ; jalon « Oui », 0j.
    let expected = "\u{feff}N°;Nom;Durée;Début;Fin;Prédécesseurs;Niveau hiérarchique;Jalon;Notes\r\n\
1;Phase;5j;19/10/2026 08:00;23/10/2026 17:00;;1;Non;\r\n\
2;A;3j;19/10/2026 08:00;21/10/2026 17:00;;2;Non;\r\n\
3;B;2j;22/10/2026 08:00;23/10/2026 17:00;2;2;Non;\r\n\
4;Fin;0j;23/10/2026 17:00;23/10/2026 17:00;3;1;Oui;\r\n";
    assert_eq!(out, expected);
}

#[test]
fn r146_english_export_other_columns_and_options() {
    let mut p = imported(R145);
    let b = p.tasks.get_mut(TaskUid::new(3)).unwrap();
    b.wbs = Some("1.2".to_owned());
    b.progress.percent_complete = plannercraft_core::Percent::new(3_350).unwrap();
    let options = CsvExportOptions {
        locale: CsvLocale::En,
        columns: vec![
            CsvColumn::UniqueId,
            CsvColumn::Name,
            CsvColumn::Duration,
            CsvColumn::Start,
            CsvColumn::UniquePredecessors,
            CsvColumn::Wbs,
            CsvColumn::PercentComplete,
            CsvColumn::Milestone,
        ],
        bom: false,
        newline: Newline::Lf,
    };
    let out = exported(&p, &options);
    let expected = "Unique ID,Name,Duration,Start,Unique ID Predecessors,WBS,% Complete,Milestone\n\
1,Phase,5d,10/19/2026 08:00,,,0%,No\n\
2,A,3d,10/19/2026 08:00,,,0%,No\n\
3,B,2d,10/22/2026 08:00,2,1.2,33.5%,No\n\
4,Fin,0d,10/23/2026 17:00,3,,0%,Yes\n";
    assert_eq!(out, expected);
}

#[test]
fn r146_quoting_formula_guard_null_rows() {
    let csv = "N°;Nom;Durée;Notes;Prédécesseurs\n\
1;=SOMME(A1);1j;\"a;b \"\"c\"\"\";\n2\n3;'=x;1,5j;\"l1\nl2\";\"1DD+1j;2\"\n4;@cmd;1j;+1;\n";
    let p = imported(csv);
    let out = exported(&p, &CsvExportOptions::new(CsvLocale::Fr));
    let lines: Vec<&str> = out.split("\r\n").collect();
    // « =SOMME(A1) » → « '=SOMME(A1) » ; notes avec `;` et guillemets entre
    // guillemets doublés ; ligne vide réduite à son N°.
    assert!(lines[1].starts_with("1;'=SOMME(A1);1j;"), "{}", lines[1]);
    assert!(lines[1].ends_with(";1;Non;\"a;b \"\"c\"\"\""), "{}", lines[1]);
    assert_eq!(lines[2], "2;;;;;;;;");
    // « '=x » (lu « =x ») → « '=x » ; saut de ligne dans les notes ;
    // prédécesseurs « 1DD+1j;2 » entre guillemets (séparateur `;`).
    assert!(lines[3].starts_with("3;'=x;1,5j;"), "{}", lines[3]);
    assert!(lines[3].contains(";\"1DD+1j;2\";"), "{}", lines[3]);
    assert!(lines[3].ends_with(";\"l1\nl2\""), "{}", lines[3]);
    assert!(lines[4].starts_with("4;'@cmd;1j;"), "{}", lines[4]);
    assert!(lines[4].ends_with(";'+1"), "{}", lines[4]);
    // Relu : mêmes textes (garde retirée), même ligne vide, mêmes liens.
    let back = imported(&out);
    let names: Vec<_> = back
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.is_null, t.notes.clone()))
        .collect();
    let original: Vec<_> = p
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.is_null, t.notes.clone()))
        .collect();
    assert_eq!(names, original);
    assert_eq!(
        back.tasks.get(TaskUid::new(3)).unwrap().predecessors,
        p.tasks.get(TaskUid::new(3)).unwrap().predecessors
    );
}

#[test]
fn r146_inactive_and_excluded_dates() {
    let mut p = imported("Nom;Durée\nA;1j\nB;2j\n");
    p.tasks.get_mut(TaskUid::new(2)).unwrap().active = false;
    let out = exported(&p, &CsvExportOptions::new(CsvLocale::Fr));
    // B inactive : dates d'affichage (R-062), lun 08:00 – mar 17:00.
    assert!(
        out.contains("\r\n2;B;2j;19/10/2026 08:00;20/10/2026 17:00;;1;Non;\r\n"),
        "{out}"
    );
}
