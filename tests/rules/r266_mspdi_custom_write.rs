//! R-266 — Écriture MSPDI des champs personnalisés et aller-retour :
//! définition écrite pour tout champ qui a un alias ou une valeur
//! (`FieldID`, `FieldName` canonique de S90, `Alias`), dans l'ordre des
//! `FieldID`, avant les définitions gardées en `Extras` ; valeurs des
//! tâches dans l'ordre des `FieldID` à la place d'`ExtendedAttribute` ;
//! nombre sans zéro final ; `read(write(p)).project == normalize(p)`.
//! Source : S91 (éléments, exemple `<FieldID>188743731</FieldID>
//! <FieldName>Text1</FieldName><Alias>Information</Alias>`) ; forme écrite
//! du nombre et de la date : hypothèse (R-120).

use plannercraft_core::{CustomFieldDef, CustomFieldId, CustomKind, CustomValue, Project, Task, TaskUid};
use plannercraft_mspdi::write::{Computed, WriteOptions, write};
use plannercraft_mspdi::{ReadOptions, normalize, read};
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};
use plannercraft_tests::{moment, tests_path};

fn id(kind: CustomKind, n: u8) -> CustomFieldId {
    CustomFieldId::new(kind, n).unwrap()
}

/// Projet de deux tâches (UID 1 et 2).
fn two_tasks() -> Project {
    let mut p = new_project("2026-10-19T08:00");
    let o = p.options;
    for (uid, name) in [(1, "A"), (2, "B")] {
        p.tasks
            .push(Task::new(TaskUid::new(uid), name.to_owned(), &o))
            .unwrap();
    }
    p
}

#[test]
fn r266_write_order_and_format() {
    let mut p = two_tasks();
    p.custom_fields.insert(
        CustomFieldId::TEXT1,
        CustomFieldDef {
            alias: Some("Lot".to_owned()),
        },
    );
    // Alias sans valeur : définition écrite quand même.
    p.custom_fields.insert(
        id(CustomKind::Date, 2),
        CustomFieldDef {
            alias: Some("Réception".to_owned()),
        },
    );
    let t = p.tasks.get_mut(TaskUid::new(2)).unwrap();
    t.custom
        .insert(id(CustomKind::Number, 6), CustomValue::Number(-30_000));
    t.custom
        .insert(CustomFieldId::TEXT1, CustomValue::Text("GO".to_owned()));
    t.custom.insert(id(CustomKind::Flag, 1), CustomValue::Flag(true));
    t.custom
        .insert(id(CustomKind::Number, 1), CustomValue::Number(125_000));
    t.custom.insert(
        id(CustomKind::Date, 1),
        CustomValue::Date(moment("2026-11-02T08:00")),
    );
    let out = compact(&text(&written(&p)));
    // Définitions dans l'ordre des `FieldID` : Text1 (188743731), Number1
    // (188743767), Date1 (188743945), Date2 (188743946), Number6
    // (188743982) ; Flag1 (188743752) entre Text1 et Number1.
    let defs = "<ExtendedAttributes>\
        <ExtendedAttribute><FieldID>188743731</FieldID><FieldName>Text1</FieldName><Alias>Lot</Alias></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743752</FieldID><FieldName>Flag1</FieldName></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743767</FieldID><FieldName>Number1</FieldName></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743945</FieldID><FieldName>Date1</FieldName></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743946</FieldID><FieldName>Date2</FieldName><Alias>Réception</Alias></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743982</FieldID><FieldName>Number6</FieldName></ExtendedAttribute>\
        </ExtendedAttributes>";
    assert!(out.contains(defs), "{out}");
    // Le conteneur est à sa place du schéma : avant `Calendars`.
    assert!(out.find("<ExtendedAttributes>").unwrap() < out.find("<Calendars>").unwrap());
    let values = "<ExtendedAttribute><FieldID>188743731</FieldID><Value>GO</Value></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743752</FieldID><Value>1</Value></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743767</FieldID><Value>12.5</Value></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743945</FieldID><Value>2026-11-02T08:00:00</Value></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743982</FieldID><Value>-3</Value></ExtendedAttribute>";
    assert!(out.contains(values), "{out}");
    // La tâche 1 n'a aucune valeur.
    let task1 = out
        .get(out.find("<UID>1</UID>").unwrap()..out.find("<UID>2</UID>").unwrap())
        .unwrap();
    assert!(!task1.contains("ExtendedAttribute"));
    // Exemple : Text1 « Lot », tâche 2 « GO » ⇒ relu identique.
    let back = reread(text(&written(&p)).as_bytes());
    assert_eq!(back.project, normalize(&p, &WriteOptions::default()));
    assert_eq!(back.project.custom_fields, p.custom_fields);
    assert_eq!(
        back.project.tasks.get(TaskUid::new(2)).unwrap().custom,
        p.tasks.get(TaskUid::new(2)).unwrap().custom
    );
    // Aucun champ personnalisé : aucun conteneur écrit.
    assert!(!text(&written(&two_tasks())).contains("ExtendedAttribute"));
}

#[test]
fn r266_normalize_text_alias_and_empty() {
    // Texte nettoyé (U+FFFD) et alias tronqué à 50 caractères ; texte vide
    // et indicateur faux non écrits.
    let mut p = two_tasks();
    p.custom_fields.insert(
        CustomFieldId::TEXT1,
        CustomFieldDef {
            alias: Some("é".repeat(60)),
        },
    );
    p.custom_fields
        .insert(id(CustomKind::Text, 2), CustomFieldDef { alias: None });
    let t = p.tasks.get_mut(TaskUid::new(1)).unwrap();
    t.custom
        .insert(CustomFieldId::TEXT1, CustomValue::Text("a\u{1}b".to_owned()));
    t.custom
        .insert(id(CustomKind::Text, 3), CustomValue::Text(String::new()));
    t.custom.insert(id(CustomKind::Flag, 2), CustomValue::Flag(false));
    let n = normalize(&p, &WriteOptions::default());
    assert_eq!(
        n.custom_fields.get(&CustomFieldId::TEXT1).unwrap().alias,
        Some("é".repeat(50))
    );
    assert_eq!(n.custom_fields.len(), 1);
    let t = n.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t.custom.len(), 1);
    assert_eq!(
        t.custom.get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("a\u{FFFD}b".to_owned()))
    );
    let back = reread(text(&written(&p)).as_bytes());
    assert_eq!(back.project, n);
}

#[test]
fn r266_roundtrip_corpus() {
    // Fichiers du corpus à `ExtendedAttribute` (`cargo xtask corpus fetch`,
    // absents ⇒ rien à vérifier) : cogito (Text1 sans alias, une valeur
    // « Construct » par tâche, `Guid` et `SecondaryPID` non gardés) et
    // projinteg (Text1 alias « Responsavel », aucune valeur). Relus,
    // écrits et relus : image de `normalize`, `Extras` inchangés en dehors
    // des champs modélisés, aucune valeur illisible.
    let opts = ReadOptions::new(moment("2026-01-01T00:00"));
    let mut seen = 0;
    for (name, alias, values) in [
        ("cogito-advanced-sample-msp2010.xml", None, 50_usize),
        ("projinteg.xml", Some("Responsavel"), 0),
    ] {
        let Ok(bytes) = std::fs::read(tests_path("corpus/files").join(name)) else {
            continue;
        };
        seen += 1;
        let doc = read(&bytes, &opts).unwrap();
        let p = &doc.project;
        assert_eq!(
            p.custom_fields
                .get(&CustomFieldId::TEXT1)
                .and_then(|d| d.alias.as_deref()),
            alias,
            "{name}"
        );
        let with_values = p
            .tasks
            .iter()
            .chain(p.summary_task.iter())
            .filter(|t| t.custom.contains_key(&CustomFieldId::TEXT1))
            .count();
        assert_eq!(with_values, values, "{name}");
        assert!(
            doc.diagnostics
                .iter()
                .all(|d| d.code != plannercraft_mspdi::ImportCode::CustomUnreadable),
            "{name}"
        );
        // Plus aucun `ExtendedAttribute` du champ modélisé en `Extras`.
        assert!(p.extras.0.iter().all(|e| e.key != "ExtendedAttributes"), "{name}");
        assert!(
            p.tasks
                .iter()
                .all(|t| t.extras.0.iter().all(|e| e.key != "ExtendedAttribute")),
            "{name}"
        );
        let options = WriteOptions::default();
        let out = write(p, Computed::Omit, &options);
        let back = read(&out.xml, &opts).unwrap();
        assert!(back.project == normalize(p, &options), "{name}");
        let back_text = String::from_utf8(out.xml).unwrap();
        assert!(back_text.contains("<FieldName>Text1</FieldName>"), "{name}");
    }
    println!("r266_roundtrip_corpus : {seen} fichier(s) du corpus vérifié(s)");
}
