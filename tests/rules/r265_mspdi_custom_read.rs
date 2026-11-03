//! R-265 — Lecture MSPDI des `ExtendedAttribute` : définitions simples d'un
//! champ de R-263 (sans formule ni table de choix) ⇒ alias dans le modèle ;
//! valeurs des tâches lues selon le type ; tout le reste (autre `FieldID`,
//! formule, table de choix, valeur d'une autre forme) en `Extras` à
//! l'identique (R-087) ; valeur illisible d'un champ modélisé ⇒ `Extras` et
//! `mspdi.custom_unreadable` (info). Source : S90 (table des `FieldID`),
//! S91 (éléments de la définition et de la valeur).

use plannercraft_core::{CustomFieldId, CustomKind, CustomValue, Severity, TaskUid};
use plannercraft_mspdi::custom::{field_id, from_field_id};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::{reread, text, written};

fn id(kind: CustomKind, n: u8) -> CustomFieldId {
    CustomFieldId::new(kind, n).unwrap()
}

/// Document : conteneur de définitions `defs` (absent si vide), tâches 1 et
/// 2 avec les valeurs `v1` et `v2`.
fn doc(defs: &str, v1: &str, v2: &str) -> MspdiDocument {
    let container = if defs.is_empty() {
        String::new()
    } else {
        format!("<ExtendedAttributes>{defs}</ExtendedAttributes>")
    };
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>{container}\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
         <Tasks><Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration>{v1}</Task>\
         <Task><UID>2</UID><ID>2</ID><Name>B</Name><Duration>PT8H0M0S</Duration>{v2}</Task></Tasks></Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

fn value(fid: i64, v: &str) -> String {
    format!("<ExtendedAttribute><FieldID>{fid}</FieldID><Value>{v}</Value></ExtendedAttribute>")
}

#[test]
fn r265_field_id_table() {
    // S90, learn.microsoft.com « PjField enumeration (Project) », relue le
    // 2026-10-28 : « Represents the Text1 (Task) field. » pjTaskText1 =
    // 188743731 ; Text2 188743734, Text3 188743737, Text4 188743740, Text5
    // 188743743, Text6 à Text10 188743746 à 188743750, Text11 à Text30
    // 188743997 à 188744016 ; Number1 à Number5 188743767 à 188743771,
    // Number6 à Number20 188743982 à 188743996 ; Date1 à Date10 188743945 à
    // 188743954 ; Flag1 à Flag10 188743752 à 188743761, Flag11 à Flag20
    // 188743972 à 188743981.
    let mut expected: Vec<(CustomFieldId, i64)> = Vec::new();
    for (n, fid) in [
        (1, 188_743_731),
        (2, 188_743_734),
        (3, 188_743_737),
        (4, 188_743_740),
        (5, 188_743_743),
    ] {
        expected.push((id(CustomKind::Text, n), fid));
    }
    for n in 6..=10 {
        expected.push((id(CustomKind::Text, n), 188_743_746 + i64::from(n - 6)));
    }
    for n in 11..=30 {
        expected.push((id(CustomKind::Text, n), 188_743_997 + i64::from(n - 11)));
    }
    for n in 1..=5 {
        expected.push((id(CustomKind::Number, n), 188_743_767 + i64::from(n - 1)));
    }
    for n in 6..=20 {
        expected.push((id(CustomKind::Number, n), 188_743_982 + i64::from(n - 6)));
    }
    for n in 1..=10 {
        expected.push((id(CustomKind::Date, n), 188_743_945 + i64::from(n - 1)));
    }
    for n in 1..=10 {
        expected.push((id(CustomKind::Flag, n), 188_743_752 + i64::from(n - 1)));
    }
    for n in 11..=20 {
        expected.push((id(CustomKind::Flag, n), 188_743_972 + i64::from(n - 11)));
    }
    assert_eq!(expected.len(), 80);
    for (f, fid) in &expected {
        assert_eq!(field_id(*f), *fid, "{f}");
        assert_eq!(from_field_id(*fid), Some(*f), "{fid}");
    }
    // Hors table : Text1 + 1, Cost1 (188743786), Duration1 (188743783),
    // codes hiérarchiques et champs de ressource.
    for other in [
        188_743_732,
        188_743_786,
        188_743_783,
        188_744_096,
        205_521_023,
        0,
        -1,
    ] {
        assert_eq!(from_field_id(other), None, "{other}");
    }
}

#[test]
fn r265_read_alias_values() {
    // Exemple : Text1 alias « Lot » et deux valeurs ⇒ modèle.
    let d = doc(
        "<ExtendedAttribute><FieldID>188743731</FieldID><FieldName>Text1</FieldName><Alias>Lot</Alias>\
         <Guid>000039B7-8BBE-4CEB-82C4-FA8C0B400033</Guid></ExtendedAttribute>\
         <ExtendedAttribute><FieldID>188743767</FieldID><FieldName>Number1</FieldName></ExtendedAttribute>",
        &[
            value(188_743_731, "Gros œuvre &amp; VRD"),
            value(188_743_767, "12.5"),
            value(188_743_945, "2026-11-02T08:00:00"),
            value(188_743_752, "1"),
            value(188_743_753, "0"),
            value(188_743_734, ""),
        ]
        .concat(),
        &[value(188_743_731, "GO"), value(188_743_768, "-0.00005")].concat(),
    );
    let p = &d.project;
    assert_eq!(
        p.custom_fields
            .get(&CustomFieldId::TEXT1)
            .and_then(|f| f.alias.as_deref()),
        Some("Lot")
    );
    // Définition sans alias : remontée, rien à garder dans le modèle.
    assert_eq!(p.custom_fields.len(), 1);
    assert!(p.extras.0.iter().all(|e| e.key != "ExtendedAttributes"));
    let t1 = p.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(
        t1.custom.get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("Gros œuvre & VRD".to_owned()))
    );
    assert_eq!(
        t1.custom.get(&id(CustomKind::Number, 1)),
        Some(&CustomValue::Number(125_000))
    );
    assert_eq!(
        t1.custom.get(&id(CustomKind::Date, 1)),
        Some(&CustomValue::Date(moment("2026-11-02T08:00")))
    );
    assert_eq!(
        t1.custom.get(&id(CustomKind::Flag, 1)),
        Some(&CustomValue::Flag(true))
    );
    // Indicateur à 0 et texte vide : ni stockés ni gardés.
    assert_eq!(t1.custom.len(), 4);
    assert!(t1.extras.0.iter().all(|e| e.key != "ExtendedAttribute"));
    let t2 = p.tasks.get(TaskUid::new(2)).unwrap();
    assert_eq!(
        t2.custom.get(&CustomFieldId::TEXT1),
        Some(&CustomValue::Text("GO".to_owned()))
    );
    // −0,00005 ⇒ −1 dix-millième (R-001, demi éloigné de zéro).
    assert_eq!(
        t2.custom.get(&id(CustomKind::Number, 2)),
        Some(&CustomValue::Number(-1))
    );
    assert!(
        d.diagnostics
            .iter()
            .all(|x| x.code != ImportCode::CustomUnreadable && !x.element.contains("ExtendedAttribute")),
        "{:?}",
        d.diagnostics
    );
}

#[test]
fn r265_formula_field_stays_extras() {
    // Exemple : Cost1 et Text2 à formule ⇒ `Extras` inchangés, relus puis
    // réécrits octet pour octet ; Text1 simple remonté à côté.
    let text2 = "<ExtendedAttribute><FieldID>188743734</FieldID><FieldName>Text2</FieldName>\
                 <Formula>[Name] &amp; \"!\"</Formula></ExtendedAttribute>";
    let cost1 = "<ExtendedAttribute><FieldID>188743786</FieldID><FieldName>Cost1</FieldName>\
                 <Alias>Budget</Alias></ExtendedAttribute>";
    let lookup = "<ExtendedAttribute><FieldID>188743737</FieldID><FieldName>Text3</FieldName>\
                  <Ltuid>3fa3</Ltuid><ValueList><Value><ID>1</ID><Value>A</Value></Value></ValueList>\
                  </ExtendedAttribute>";
    let text1 = "<ExtendedAttribute><FieldID>188743731</FieldID><FieldName>Text1</FieldName>\
                 <Alias>Lot</Alias></ExtendedAttribute>";
    let v_text2 = value(188_743_734, "calculé");
    let v_cost1 = value(188_743_786, "1250");
    let v_text3 = "<ExtendedAttribute><FieldID>188743737</FieldID><Value>A</Value>\
                   <ValueGUID>3fa3</ValueGUID></ExtendedAttribute>";
    let d = doc(
        &[text2, cost1, lookup, text1].concat(),
        &[
            v_text2.as_str(),
            v_cost1.as_str(),
            value(188_743_731, "GO").as_str(),
        ]
        .concat(),
        v_text3,
    );
    let p = &d.project;
    assert_eq!(p.custom_fields.len(), 1);
    let container = p.extras.0.iter().find(|e| e.key == "ExtendedAttributes").unwrap();
    assert_eq!(
        container.payload,
        format!("<ExtendedAttributes>{text2}{cost1}{lookup}</ExtendedAttributes>")
    );
    let t1 = p.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t1.custom.len(), 1);
    let kept: Vec<&str> = t1
        .extras
        .0
        .iter()
        .filter(|e| e.key == "ExtendedAttribute")
        .map(|e| e.payload.as_str())
        .collect();
    assert_eq!(kept, [v_text2.as_str(), v_cost1.as_str()]);
    let t2 = p.tasks.get(TaskUid::new(2)).unwrap();
    assert!(t2.custom.is_empty());
    assert_eq!(t2.extras.0.len(), 1);

    // Réécriture : définitions et valeurs gardées octet pour octet, Text1
    // écrit par le modèle avant elles.
    let out = text(&written(p));
    for kept in [text2, cost1, lookup, v_text2.as_str(), v_cost1.as_str(), v_text3] {
        assert!(out.contains(kept), "{kept}");
    }
    let model = out.find("<FieldName>Text1</FieldName>").unwrap();
    assert!(model < out.find(text2).unwrap());
    let back = reread(out.as_bytes());
    assert_eq!(back.project.custom_fields, p.custom_fields);
    let back_container = back
        .project
        .extras
        .0
        .iter()
        .find(|e| e.key == "ExtendedAttributes")
        .unwrap();
    assert_eq!(back_container, container);
    assert_eq!(back.project.tasks, p.tasks);

    // Sans aucune définition simple, le conteneur reste tel quel (blancs
    // compris).
    let raw = format!("\n\t\t{text2}\n\t");
    let d = doc(&raw, "", "");
    let container = d
        .project
        .extras
        .0
        .iter()
        .find(|e| e.key == "ExtendedAttributes")
        .unwrap();
    assert_eq!(
        container.payload,
        format!("<ExtendedAttributes>{raw}</ExtendedAttributes>")
    );
}

#[test]
fn r265_unreadable_value_diagnostic() {
    // Nombre « abc », date « demain », indicateur « oui », texte de 256
    // caractères, valeur absente : gardés en `Extras`, un diagnostic info
    // chacun ; la valeur lisible voisine est lue.
    let long = "x".repeat(256);
    let v1 = [
        value(188_743_767, "abc"),
        value(188_743_945, "demain"),
        value(188_743_752, "oui"),
        value(188_743_731, &long),
        "<ExtendedAttribute><FieldID>188743768</FieldID></ExtendedAttribute>".to_owned(),
        value(188_743_769, "7"),
    ]
    .concat();
    let d = doc("", &v1, "");
    let t1 = d.project.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t1.custom.len(), 1);
    assert_eq!(
        t1.custom.get(&id(CustomKind::Number, 3)),
        Some(&CustomValue::Number(70_000))
    );
    assert_eq!(t1.extras.0.len(), 5);
    let diags: Vec<_> = d
        .diagnostics
        .iter()
        .filter(|x| x.code == ImportCode::CustomUnreadable)
        .collect();
    assert_eq!(diags.len(), 5);
    for x in diags {
        assert_eq!(x.code.code(), "mspdi.custom_unreadable");
        assert_eq!(x.severity, Severity::Info);
        assert_eq!(x.element, "Tasks/Task/ExtendedAttribute");
        assert_eq!(x.subject, Some(plannercraft_core::Subject::Task(TaskUid::new(1))));
    }
    // Relue après écriture : mêmes valeurs gardées, mêmes diagnostics.
    let back = reread(text(&written(&d.project)).as_bytes());
    let b1 = back.project.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(b1.extras, t1.extras);
    assert_eq!(b1.custom, t1.custom);
}
