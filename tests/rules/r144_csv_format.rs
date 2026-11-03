//! R-144 — CSV : encodage (BOM, UTF-8, sinon Windows-1252), séparateur
//! détecté sur l'en-tête hors guillemets, locale à la majorité des
//! en-têtes, en-têtes reconnus en français et en anglais, lecture RFC 4180,
//! bornes et numéros de ligne physiques.

use plannercraft_automation::csv::{
    CsvCode, CsvEncoding, CsvError, CsvImport, CsvImportOptions, CsvLimits, CsvLocale, import,
};
use plannercraft_core::{Duration, DurationUnit, Minutes, ScheduleAnchor, TimeBasis};
use plannercraft_tests::moment;

fn options() -> CsvImportOptions {
    CsvImportOptions::new(
        ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        moment("2026-10-17T08:00"),
    )
}

fn read(bytes: &[u8]) -> CsvImport {
    import(bytes, &options()).unwrap()
}

fn names(i: &CsvImport) -> Vec<String> {
    i.project.tasks.iter().map(|t| t.name.clone()).collect()
}

fn codes(i: &CsvImport) -> Vec<(u64, &'static str, Option<String>)> {
    i.diagnostics
        .iter()
        .map(|d| (d.line, d.code.code(), d.column.clone()))
        .collect()
}

fn days(n: i64) -> Duration {
    Duration::new(
        Minutes::new(n * 480),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap()
}

#[test]
fn r144_french_header_and_separator() {
    // « N°;Nom;Durée » : séparateur `;`, en-têtes français → locale FR,
    // virgule décimale : « 1,5j » = 720.
    let i = read("N°;Nom;Durée\r\n1;Tâche;1,5j\r\n".as_bytes());
    assert_eq!(i.detected.separator, b';');
    assert_eq!(i.detected.locale, CsvLocale::Fr);
    assert_eq!(i.detected.encoding, CsvEncoding::Utf8);
    assert_eq!(names(&i), vec!["Tâche"]);
    assert_eq!(
        i.project.tasks.iter().next().unwrap().duration.minutes(),
        Minutes::new(720)
    );
    assert_eq!(codes(&i), vec![]);
}

#[test]
fn r144_english_header_comma_and_tab() {
    // « ID,Name,Duration » : `,` et anglais ; tabulation détectée de même.
    let i = read(b"ID,Name,Duration\n1,Task,\"1.5d\"\n");
    assert_eq!((i.detected.separator, i.detected.locale), (b',', CsvLocale::En));
    assert_eq!(
        i.project.tasks.iter().next().unwrap().duration.minutes(),
        Minutes::new(720)
    );
    let i = read(b"Name\tDuration\nA\t2d\n");
    assert_eq!((i.detected.separator, i.detected.locale), (b'\t', CsvLocale::En));
    assert_eq!(i.project.tasks.iter().next().unwrap().duration, days(2));
    // Séparateurs hors guillemets seulement ; à égalité, `;` l'emporte.
    let i = read(b"\"Name;x\",Duration\nA,2d\n");
    assert_eq!(i.detected.separator, b',');
    let i = read(b"Nom;Dur\xc3\xa9e,x\nA;2j\n");
    assert_eq!(i.detected.separator, b';');
    // Locale à la majorité des en-têtes, même avec `;` : anglais.
    let i = read(b"Name;Duration\nA;1.5d\n");
    assert_eq!(i.detected.locale, CsvLocale::En);
    // Égalité des en-têtes propres à chaque langue (WBS et Notes sont
    // communs) : la locale suit le séparateur.
    let i = read(b"WBS,Notes,Nom,Name\n1,n,A,B\n");
    assert_eq!(i.detected.locale, CsvLocale::En);
    let i = read(b"WBS;Notes;Nom;Name\n1;n;A;B\n");
    assert_eq!(i.detected.locale, CsvLocale::Fr);
}

#[test]
fn r144_encodings() {
    // BOM UTF-8 retiré.
    let i = read(b"\xef\xbb\xbfNom;Dur\xc3\xa9e\nA;1j\n");
    assert_eq!(i.detected.encoding, CsvEncoding::Utf8);
    assert_eq!(names(&i), vec!["A"]);
    // Octet 0xE9 hors UTF-8 → Windows-1252 → « é ».
    let i = read(b"Nom;Dur\xe9e\nCaf\xe9;1j\n");
    assert_eq!(i.detected.encoding, CsvEncoding::Windows1252);
    assert_eq!(names(&i), vec!["Café"]);
    assert_eq!(codes(&i), vec![]);
    // BOM FF FE → UTF-16 petit-boutiste ; FE FF → gros-boutiste.
    let text = "Nom;Durée\r\nÉté;2j\r\n";
    let mut le = vec![0xFF, 0xFE];
    let mut be = vec![0xFE, 0xFF];
    for u in text.encode_utf16() {
        le.extend(u.to_le_bytes());
        be.extend(u.to_be_bytes());
    }
    for (bytes, enc) in [(le, CsvEncoding::Utf16Le), (be, CsvEncoding::Utf16Be)] {
        let i = read(&bytes);
        assert_eq!(i.detected.encoding, enc);
        assert_eq!(names(&i), vec!["Été"]);
        assert_eq!(i.project.tasks.iter().next().unwrap().duration, days(2));
    }
    // Encodage imposé contredit par les octets : refus.
    let forced = CsvImportOptions {
        encoding: Some(CsvEncoding::Utf8),
        ..options()
    };
    assert_eq!(import(b"Nom;Dur\xe9e\n", &forced), Err(CsvError::Encoding));
    let forced = CsvImportOptions {
        encoding: Some(CsvEncoding::Windows1252),
        ..options()
    };
    assert_eq!(names(&import(b"Nom\nCaf\xe9\n", &forced).unwrap()), vec!["Café"]);
}

#[test]
fn r144_headers_unknown_unsupported_duplicate() {
    // En-têtes : casse, accents, espaces et ponctuation ignorés.
    let i = read(
        "n° ; NOM DE LA TÂCHE ; durée ; Couleur ; Noms des ressources ; Nom\n1;A;2j;rouge;Paul;B\n"
            .as_bytes(),
    );
    assert_eq!(names(&i), vec!["A"]);
    assert_eq!(
        codes(&i),
        vec![
            (1, "csv.unknown_column", Some("Couleur".to_owned())),
            (
                1,
                "csv.unsupported_column",
                Some("Noms des ressources".to_owned())
            ),
            (1, "csv.duplicate_column", Some("Nom".to_owned())),
        ]
    );
    // Locale imposée : les en-têtes anglais restent reconnus.
    let fr = CsvImportOptions {
        locale: Some(CsvLocale::Fr),
        ..options()
    };
    let i = import(b"Name;Duration\nA;1,5\n", &fr).unwrap();
    assert_eq!(
        i.project.tasks.iter().next().unwrap().duration.minutes(),
        Minutes::new(720)
    );
}

#[test]
fn r144_rfc4180_quotes_multiline_and_lines() {
    // Champ entre guillemets avec séparateur, guillemets doublés, saut de
    // ligne ; lignes vides sautées ; numéro de ligne physique gardé : la
    // ligne de « C » est la 7 (en-tête 1, A 2, B 3 à 5, ligne vide 6).
    let csv = "Nom;Durée;Notes\r\n\"A;1\";1j;\"dit \"\"oui\"\"\"\r\nB;2j;\"l1\r\nl2\r\nl3\"\r\n\r\nC;3x;\r\n";
    let i = read(csv.as_bytes());
    assert_eq!(names(&i), vec!["A;1", "B", "C"]);
    let notes: Vec<_> = i.project.tasks.iter().map(|t| t.notes.clone()).collect();
    assert_eq!(notes, vec!["dit \"oui\"", "l1\r\nl2\r\nl3", ""]);
    assert_eq!(codes(&i), vec![(7, "csv.bad_value", Some("Durée".to_owned()))]);
    // Fins de ligne CR seules ou LF seules : mêmes numéros de ligne.
    for csv in ["Nom;Durée\rA;1j\r\rB;2x\r", "Nom;Durée\nA;1j\n\nB;2x\n"] {
        let i = read(csv.as_bytes());
        assert_eq!(names(&i), vec!["A", "B"]);
        assert_eq!(codes(&i), vec![(4, "csv.bad_value", Some("Durée".to_owned()))]);
    }
    // Ligne faite de séparateurs : sautée. Lignes de longueurs inégales admises.
    let i = read(b"Nom;Dur\xc3\xa9e\n;;;\nA\nB;1j;extra;extra\n");
    assert_eq!(names(&i), vec!["A", "B"]);
    // Guillemet non fermé : le reste du fichier est un champ, sans panique.
    let i = read(b"Nom;Dur\xc3\xa9e\n\"A;1j\nB;2j\n");
    assert_eq!(names(&i), vec!["A;1j\nB;2j\n"]);
}

#[test]
fn r144_file_refusals() {
    // 64 Mio + 1 octet → trop grand (avant tout décodage).
    let big = vec![b' '; 64 * 1_024 * 1_024 + 1];
    assert_eq!(import(&big, &options()), Err(CsvError::TooLarge));
    assert_eq!(import(b"", &options()), Err(CsvError::Empty));
    assert_eq!(import(b" \r\n\t", &options()), Err(CsvError::Empty));
    assert_eq!(import(b";;;\n;;\n", &options()), Err(CsvError::NoHeader));
    assert_eq!(
        import(b"ID;Notes\n1;x\n", &options()),
        Err(CsvError::NoKnownColumn)
    );
    let small = CsvImportOptions {
        limits: CsvLimits {
            max_rows: 2,
            max_columns: 3,
            ..CsvLimits::default()
        },
        ..options()
    };
    assert!(import(b"Nom\nA\nB\n", &small).is_ok());
    assert_eq!(import(b"Nom\nA\nB\nC\n", &small), Err(CsvError::TooManyRows));
    assert_eq!(import(b"Nom;a;b;c\nA\n", &small), Err(CsvError::TooManyColumns));
    assert_eq!(import(b"Nom\nA;1;2;3\n", &small), Err(CsvError::TooManyColumns));
    // Champ au-delà de la borne : valeur ignorée, diagnostic.
    let tiny = CsvImportOptions {
        limits: CsvLimits {
            max_field_bytes: 4,
            ..CsvLimits::default()
        },
        ..options()
    };
    let i = import(b"Nom;Notes\nA;12345\n", &tiny).unwrap();
    assert_eq!(i.project.tasks.iter().next().unwrap().notes, "");
    assert_eq!(codes(&i), vec![(2, "csv.bad_value", Some("Notes".to_owned()))]);
}

#[test]
fn r144_leading_blank_lines_and_excel_sep_line() {
    // Lignes vides de tête : le séparateur est détecté sur l'en-tête, pas sur
    // la première ligne physique (vide).
    let i = read(b"\n\r\nName,Duration\nA,2d\nB,5x\n");
    assert_eq!(i.detected.separator, b',');
    assert_eq!(names(&i), ["A", "B"]);
    assert_eq!(codes(&i), vec![(5, "csv.bad_value", Some("Duration".to_owned()))]);
    // Ligne Excel `sep=X` : fixe le séparateur, n'est pas l'en-tête ; les
    // numéros de ligne restent physiques.
    let i = read(b"\nsep=;\nName;Duration\nA;2d\nB;5x\n");
    assert_eq!(i.detected.separator, b';');
    assert_eq!(names(&i), ["A", "B"]);
    assert_eq!(i.project.tasks.iter().next().unwrap().duration, days(2));
    assert_eq!(codes(&i), vec![(5, "csv.bad_value", Some("Duration".to_owned()))]);
    // Le séparateur fixé l'emporte sur la détection : « Name;Duration » est
    // alors une seule colonne inconnue.
    assert_eq!(
        import(b"SEP=,\nName;Duration\nA;2d\n", &options()),
        Err(CsvError::NoKnownColumn)
    );
}

#[test]
fn r144_diagnostics_are_capped() {
    // Règle 10 : 3 000 prédécesseurs inconnus → 1 000 gardés pour ce code,
    // les autres codes ne sont pas masqués, un `csv.truncated` final.
    use plannercraft_automation::csv::{CsvCode, MAX_DIAGNOSTICS_PER_CODE};
    use std::fmt::Write as _;
    let mut csv = String::from("Nom;Durée;Prédécesseurs\n");
    for k in 0..3_000 {
        writeln!(csv, "T{k};1j;99999").unwrap();
    }
    csv.push_str("X;5x;\n");
    let i = read(csv.as_bytes());
    let count = |code: &str| i.diagnostics.iter().filter(|d| d.code.code() == code).count();
    assert_eq!(
        count("csv.unknown_predecessor"),
        MAX_DIAGNOSTICS_PER_CODE as usize
    );
    assert_eq!(count("csv.bad_value"), 1);
    let last = i.diagnostics.last().unwrap();
    assert_eq!(
        (last.line, &last.code),
        (0, &CsvCode::Truncated { dropped: 2_000 })
    );
    assert_eq!(i.diagnostics.len(), 1_002);
}

/// Diagnostics `csv.date_ambiguous` : (ligne, colonne, valeur, gravité).
fn ambiguous(i: &CsvImport) -> Vec<(u64, Option<String>, Option<String>, plannercraft_core::Severity)> {
    i.diagnostics
        .iter()
        .filter(|d| d.code == CsvCode::DateAmbiguous)
        .map(|d| (d.line, d.column.clone(), d.value.clone(), d.code.severity()))
        .collect()
}

#[test]
fn r144_date_ambiguous_warning_once_per_column() {
    // En-têtes français, « 03/04/2026 » partout : lu le 3 avril (JJ/MM de la
    // locale des en-têtes), le 4 mars dans l'autre ordre ; aucune valeur ne
    // lève l'ambiguïté → un avertissement par colonne, à la première valeur.
    let csv = "N°;Nom;Durée;Début;Fin\r\n\
               1;A;1j;03/04/2026 08:00;03/04/2026 17:00\r\n\
               2;B;1j;03/04/2026;05/06/2026\r\n\
               3;C;1j;2026-04-03;\r\n";
    let i = read(csv.as_bytes());
    assert_eq!(i.detected.locale, CsvLocale::Fr);
    let first = i.project.tasks.iter().next().unwrap();
    assert_eq!(first.name, "A");
    let warn = plannercraft_core::Severity::Warning;
    assert_eq!(
        ambiguous(&i),
        vec![
            (
                2,
                Some("Début".to_owned()),
                Some("03/04/2026 08:00".to_owned()),
                warn
            ),
            (
                2,
                Some("Fin".to_owned()),
                Some("03/04/2026 17:00".to_owned()),
                warn
            ),
        ]
    );
    // Locale imposée : l'ordre ne vient plus des en-têtes, aucun avertissement.
    let mut opts = options();
    opts.locale = Some(CsvLocale::Fr);
    assert_eq!(ambiguous(&import(csv.as_bytes(), &opts).unwrap()), vec![]);
    // En-têtes anglais, « 03/04/2026 » : 4 mars → même avertissement.
    let en = read(b"Name,Duration,Start\r\nA,1d,03/04/2026\r\n");
    assert_eq!(en.detected.locale, CsvLocale::En);
    assert_eq!(
        ambiguous(&en),
        vec![(2, Some("Start".to_owned()), Some("03/04/2026".to_owned()), warn)]
    );
    // Code stable.
    assert_eq!(CsvCode::DateAmbiguous.code(), "csv.date_ambiguous");
}

#[test]
fn r144_no_warning_when_day_over_12() {
    // Une valeur dont le jour dépasse 12 (13/04/2026 en JJ/MM) ne se lit pas
    // dans l'autre ordre : la colonne n'est pas ambiguë.
    let i = read("N°;Nom;Durée;Début\r\n1;A;1j;03/04/2026\r\n2;B;1j;13/04/2026\r\n".as_bytes());
    assert_eq!(ambiguous(&i), vec![]);
    // Anglais : « 12/25/2026 » (jour 25 en second) lève l'ambiguïté.
    let en = read(b"Name,Duration,Start\r\nA,1d,04/03/2026\r\nB,1d,12/25/2026\r\n");
    assert_eq!(ambiguous(&en), vec![]);
    // Dates ISO, ou identiques dans les deux ordres : rien d'ambigu.
    let iso = read("N°;Nom;Durée;Début\r\n1;A;1j;2026-04-03\r\n2;B;1j;03/03/2026\r\n".as_bytes());
    assert_eq!(ambiguous(&iso), vec![]);
}
