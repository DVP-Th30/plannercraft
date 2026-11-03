//! R-087 — Lecture : éléments non modélisés conservés en `Extras`.

use plannercraft_core::{Extras, TaskUid};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::moment;

fn read_xml(xml: &str, keep_extras: bool) -> MspdiDocument {
    let options = ReadOptions {
        keep_extras,
        ..ReadOptions::new(moment("2026-01-01T00:00"))
    };
    read(xml.as_bytes(), &options).unwrap()
}

/// Projet minimal autour de `tasks` (contenu de `Tasks`) et `calendars`.
fn project(tasks: &str, calendars: &str) -> String {
    format!(
        "<Project xmlns=\"http://schemas.microsoft.com/project\">\
         <SaveVersion>14</SaveVersion><Name>P</Name><StartDate>2026-10-19T08:00:00</StartDate>\
         <CurrencyCode>EUR</CurrencyCode><CalendarUID>1</CalendarUID><Views><View/></Views>\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar>{calendars}\
         </Calendar></Calendars><Tasks>{tasks}</Tasks></Project>"
    )
}

/// Clés et charges des `Extras`.
fn pairs(extras: &Extras) -> Vec<(&str, &str)> {
    extras
        .0
        .iter()
        .map(|e| (e.key.as_str(), e.payload.as_str()))
        .collect()
}

fn task_extras(doc: &MspdiDocument, uid: u32) -> Vec<(&str, &str)> {
    pairs(&doc.project.tasks.get(TaskUid::new(uid)).unwrap().extras)
}

#[test]
fn r087_out_of_schema_element_is_anchored_on_the_previous_always_written_sibling() {
    // `FreeformDurationFormat` (hors schéma, MS Project 2013+) suit
    // `DurationFormat` (M) : clé `DurationFormat>FreeformDurationFormat`.
    let xml = project(
        "<Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration>\
         <DurationFormat>7</DurationFormat><FreeformDurationFormat>7</FreeformDurationFormat>\
         <Work>PT0H0M0S</Work></Task>",
        "",
    );
    let doc = read_xml(&xml, true);
    assert_eq!(
        task_extras(&doc, 1),
        [(
            "DurationFormat>FreeformDurationFormat",
            "<FreeformDurationFormat>7</FreeformDurationFormat>"
        )]
    );
    assert_eq!(
        doc.project.tasks.get(TaskUid::new(1)).unwrap().extras.0[0].origin,
        "mspdi"
    );
}

#[test]
fn r087_schema_elements_keep_their_name_and_order() {
    // `ExtendedAttribute` (P) deux fois après `CommitmentType` (P) : trois
    // `Extras` nommés, ordre du document gardé. Champs hors de R-263 (Cost1,
    // Cost2) : Texte1 et Texte2 sont lus dans le modèle depuis L8 (R-265).
    let xml = project(
        "<Task><UID>1</UID><Name>A</Name><CommitmentType>0</CommitmentType>\
         <ExtendedAttribute><FieldID>188743786</FieldID><Value>a</Value></ExtendedAttribute>\
         <ExtendedAttribute><FieldID>188743787</FieldID><Value>b</Value></ExtendedAttribute></Task>",
        "",
    );
    let doc = read_xml(&xml, true);
    assert_eq!(
        task_extras(&doc, 1),
        [
            ("CommitmentType", "<CommitmentType>0</CommitmentType>"),
            (
                "ExtendedAttribute",
                "<ExtendedAttribute><FieldID>188743786</FieldID><Value>a</Value></ExtendedAttribute>"
            ),
            (
                "ExtendedAttribute",
                "<ExtendedAttribute><FieldID>188743787</FieldID><Value>b</Value></ExtendedAttribute>"
            ),
        ]
    );
}

#[test]
fn r087_computed_elements_are_neither_kept_nor_anchors() {
    // `OverAllocated` (X) entre `Recurring` (M) et `Zzz` (hors schéma) :
    // non conservé, compté ; `Zzz` s'ancre sur `Recurring`. `Start` (C) et
    // `Duration` (M?) ne servent pas d'ancre non plus.
    let xml = project(
        "<Task><UID>1</UID><Name>A</Name><Recurring>0</Recurring><OverAllocated>0</OverAllocated>\
         <Zzz>1</Zzz><Start>2026-10-19T08:00:00</Start><Yyy/><Cost>0</Cost><Cost>0</Cost></Task>",
        "",
    );
    let doc = read_xml(&xml, true);
    assert_eq!(
        task_extras(&doc, 1),
        [("Recurring>Zzz", "<Zzz>1</Zzz>"), ("Recurring>Yyy", "<Yyy/>")]
    );
    assert_eq!(doc.source.dropped_computed.get("OverAllocated"), Some(&1));
    assert_eq!(doc.source.dropped_computed.get("Cost"), Some(&2));
}

#[test]
fn r087_first_child_out_of_schema_is_anchored_at_the_start() {
    let xml = project("<Task><Zzz/><UID>1</UID><Name>A</Name></Task>", "");
    let doc = read_xml(&xml, true);
    assert_eq!(task_extras(&doc, 1), [("^>Zzz", "<Zzz/>")]);
}

#[test]
fn r087_canonical_order_follows_the_observed_profile() {
    // `Contact` (P) précède `Hyperlink` (P) dans le profil ; le document les
    // donne dans l'autre ordre : l'ordre canonique (tri stable par
    // emplacement d'écriture) les remet en place.
    let xml = project(
        "<Task><UID>1</UID><Name>A</Name><Hyperlink>h</Hyperlink><Contact>c</Contact></Task>",
        "",
    );
    let doc = read_xml(&xml, true);
    let keys: Vec<&str> = task_extras(&doc, 1).into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["Contact", "Hyperlink"]);
}

#[test]
fn r087_project_and_calendar_extras() {
    // Projet : `CurrencyCode` (P) nommé ; section d'interface `Views` (hors
    // schéma) ancrée sur `CalendarUID` (M). Calendrier : `IsBaselineCalendar`
    // (P).
    let xml = project("", "<IsBaselineCalendar>0</IsBaselineCalendar>");
    let doc = read_xml(&xml, true);
    assert_eq!(
        pairs(&doc.project.extras),
        [
            ("CurrencyCode", "<CurrencyCode>EUR</CurrencyCode>"),
            ("CalendarUID>Views", "<Views><View/></Views>"),
        ]
    );
    let cal = doc.project.calendars.at(0).unwrap();
    assert_eq!(
        pairs(&cal.extras),
        [("IsBaselineCalendar", "<IsBaselineCalendar>0</IsBaselineCalendar>")]
    );
    assert_eq!(doc.source.ui_sections, ["Views"]);
}

#[test]
fn r087_children_without_extras_are_lost_and_reported() {
    // Un jour de calendrier, une exception, un lien n'ont pas d'`Extras` :
    // leurs enfants non lus sont perdus, comptés, signalés (info).
    let xml = project(
        "<Task><UID>1</UID><Name>A</Name></Task><Task><UID>2</UID><Name>B</Name><PredecessorLink>\
         <PredecessorUID>1</PredecessorUID><Type>1</Type><CrossProjectName>x</CrossProjectName></PredecessorLink></Task>",
        "<WeekDays><WeekDay><DayType>1</DayType><DayWorking>0</DayWorking><Foo/></WeekDay></WeekDays>",
    );
    let doc = read_xml(&xml, true);
    assert_eq!(doc.source.not_preserved, 2);
    let lost: Vec<&str> = doc
        .diagnostics
        .iter()
        .filter(|d| d.code == ImportCode::NotPreserved)
        .map(|d| d.element.as_str())
        .collect();
    assert_eq!(
        lost,
        [
            "Calendars/Calendar/WeekDays/WeekDay/Foo",
            "Tasks/Task/PredecessorLink/CrossProjectName"
        ]
    );
    assert_eq!(
        doc.diagnostics
            .iter()
            .find(|d| d.code == ImportCode::NotPreserved)
            .unwrap()
            .severity,
        plannercraft_core::Severity::Info
    );
    // Le lien est lu.
    assert_eq!(
        doc.project.tasks.get(TaskUid::new(2)).unwrap().predecessors.len(),
        1
    );
}

#[test]
fn r087_prefixed_root_declarations_are_kept() {
    let xml = "<p:Project xmlns:p=\"http://schemas.microsoft.com/project\"><p:StartDate>2026-10-19T08:00:00\
               </p:StartDate><p:CurrencyCode>EUR</p:CurrencyCode></p:Project>";
    let doc = read_xml(xml, true);
    assert_eq!(
        pairs(&doc.project.extras),
        [
            ("@xmlns:p", "http://schemas.microsoft.com/project"),
            ("CurrencyCode", "<p:CurrencyCode>EUR</p:CurrencyCode>"),
        ]
    );
}

#[test]
fn r087_without_extras_nothing_is_kept() {
    let xml = project(
        "<Task><UID>1</UID><Name>A</Name><Contact>c</Contact><Zzz/></Task>",
        "<IsBaselineCalendar>0</IsBaselineCalendar>",
    );
    let doc = read_xml(&xml, false);
    assert!(doc.project.extras.is_empty());
    assert!(doc.project.tasks.get(TaskUid::new(1)).unwrap().extras.is_empty());
    assert!(doc.project.calendars.at(0).unwrap().extras.is_empty());
}

#[test]
fn r087_scheduled_work_distribution_of_an_assignment_is_dropped() {
    // Affectation à profil uniforme (`WorkContour` 0) : répartitions de type
    // 1 (travail planifié, calculé par MS Project) écartées et comptées ;
    // type 2 (réel) conservé. Profil modifié (`WorkContour` 8) : tout est
    // conservé (fixture `resources.xml`).
    let bytes = std::fs::read(plannercraft_tests::tests_path("fixtures/mspdi/resources.xml")).unwrap();
    let doc = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let extras = |uid: u32| {
        pairs(
            &doc.project
                .assignments
                .get(plannercraft_core::AssignmentUid::new(uid))
                .unwrap()
                .extras,
        )
        .into_iter()
        .map(|(k, p)| (k.to_owned(), p.contains("<Type>1</Type>")))
        .collect::<Vec<_>>()
    };
    assert_eq!(extras(1), [("TimephasedData".to_owned(), false)]);
    assert_eq!(extras(2), [("TimephasedData".to_owned(), true)]);
    assert_eq!(doc.source.dropped_computed.get("TimephasedData"), Some(&2));
}

#[test]
fn r087_payload_with_an_undefined_entity_is_not_rewritten() {
    // `&nbsp;` n'est pas défini en XML sans DTD : le lecteur le tolère
    // (`mspdi.inconsistent`) et capture la charge telle quelle, mais la
    // réécrire produirait un document mal formé. L'écriture l'écarte
    // (`mspdi.write.extra_invalid`) ; `normalize` décrit la perte.
    use plannercraft_mspdi::write::{Computed, WriteCode, WriteOptions, write};
    let xml = project(
        "<Task><UID>1</UID><ID>1</ID><Name>A</Name><Hyperlink>a&nbsp;b</Hyperlink>\
         <Contact>x &amp; &#233;</Contact></Task>",
        "",
    );
    let doc = read_xml(&xml, true);
    assert_eq!(
        task_extras(&doc, 1),
        [
            ("Contact", "<Contact>x &amp; &#233;</Contact>"),
            ("Hyperlink", "<Hyperlink>a&nbsp;b</Hyperlink>"),
        ]
    );
    let options = WriteOptions::default();
    let out = write(&doc.project, Computed::Omit, &options);
    let text = String::from_utf8(out.xml.clone()).unwrap();
    assert!(!text.contains("&nbsp;"));
    assert!(text.contains("<Contact>x &amp; &#233;</Contact>"));
    let invalid: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == WriteCode::ExtraInvalid)
        .map(|d| d.element.as_str())
        .collect();
    assert_eq!(invalid, ["Tasks/Task/Hyperlink"]);
    let back = read(&out.xml, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(
        back.project,
        plannercraft_mspdi::normalize(&doc.project, &options)
    );
}
