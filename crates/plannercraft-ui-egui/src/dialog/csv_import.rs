//! Fenêtre « Importer CSV… » (H-UI-40) : aperçu (séparateur, encodage,
//! langue modifiables ; colonnes reconnues, ignorées, non prises en charge ;
//! 20 lignes), puis ouverture d'un nouveau document. Contrat posé par le lot
//! L0, implémenté en L10.
//!
//! Fichier → Importer CSV… ouvre le sélecteur ([`crate::FileService::pick_bytes`] :
//! desktop par rfd, web par téléversement) ; les octets reçus (64 Mio au
//! plus) ouvrent cette fenêtre. L'aperçu est celui de
//! `plannercraft_automation::csv::preview` (lot L3), recalculé à chaque
//! changement d'option ; « Importer » lit le fichier entier
//! ([`crate::io::load_csv`]) et ouvre le résultat comme un nouveau document,
//! après le bandeau « Enregistrer les modifications ? » si le document
//! courant est modifié. Un refus de l'import reste affiché dans la fenêtre.

use std::sync::Arc;

use plannercraft_automation::csv::{
    CsvEncoding, CsvError, CsvImportOptions, CsvLocale, CsvPreview, Recognized, preview,
};
use plannercraft_core::{Moment, ScheduleAnchor, Severity};

use crate::action::DialogKind;
use crate::app::{App, DocOrigin, next_monday_morning};
use crate::i18n::{Arg, TextId, Texts, error_text_id};
use crate::io::{PickError, PickRequest, PickedFile};

/// Octets du fichier choisi, partagés (la fenêtre est clonable) ; leur
/// forme de débogage ne donne que la taille.
#[derive(Clone, PartialEq, Eq)]
pub struct CsvBytes(pub Arc<[u8]>);

impl std::fmt::Debug for CsvBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CsvBytes({} octets)", self.0.len())
    }
}

/// Séparateur choisi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeparatorChoice {
    /// Détecté sur la ligne d'en-tête.
    Auto,
    /// `;`.
    Semicolon,
    /// `,`.
    Comma,
    /// Tabulation.
    Tab,
}

impl SeparatorChoice {
    /// Octet imposé (`None` : détecté).
    #[must_use]
    pub const fn byte(self) -> Option<u8> {
        match self {
            Self::Auto => None,
            Self::Semicolon => Some(b';'),
            Self::Comma => Some(b','),
            Self::Tab => Some(b'\t'),
        }
    }
}

/// Choix de séparateur proposés.
const SEPARATORS: [(SeparatorChoice, TextId); 4] = [
    (SeparatorChoice::Auto, TextId::CiAuto),
    (SeparatorChoice::Semicolon, TextId::CiSepSemicolon),
    (SeparatorChoice::Comma, TextId::CiSepComma),
    (SeparatorChoice::Tab, TextId::CiSepTab),
];

/// Choix d'encodage proposés (`None` : détecté).
const ENCODINGS: [(Option<CsvEncoding>, TextId); 5] = [
    (None, TextId::CiAuto),
    (Some(CsvEncoding::Utf8), TextId::CiEncUtf8),
    (Some(CsvEncoding::Utf16Le), TextId::CiEncUtf16Le),
    (Some(CsvEncoding::Utf16Be), TextId::CiEncUtf16Be),
    (Some(CsvEncoding::Windows1252), TextId::CiEncWindows1252),
];

/// Choix de langue proposés (`None` : déduite des en-têtes).
const LOCALES: [(Option<CsvLocale>, TextId); 3] = [
    (None, TextId::CiAuto),
    (Some(CsvLocale::Fr), TextId::LangFr),
    (Some(CsvLocale::En), TextId::LangEn),
];

/// Caractères au plus d'une cellule affichée dans l'aperçu.
const CELL_CHARS: usize = 40;

/// Options d'un aperçu (séparateur, encodage, langue).
type Shown = (Option<u8>, Option<CsvEncoding>, Option<CsvLocale>);

/// Brouillon de la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImportState {
    /// Nom du fichier choisi.
    pub name: String,
    /// Octets du fichier.
    pub bytes: CsvBytes,
    /// Séparateur.
    pub separator: SeparatorChoice,
    /// Encodage (`None` : détecté).
    pub encoding: Option<CsvEncoding>,
    /// Langue des en-têtes et des valeurs (`None` : déduite).
    pub locale: Option<CsvLocale>,
    /// Aperçu des options [`CsvImportState::shown`], ou refus du fichier.
    pub preview: Result<CsvPreview, CsvError>,
    /// Options de l'aperçu affiché.
    shown: Shown,
    /// « Maintenant » à l'ouverture (date courante du projet importé).
    now: Moment,
    /// Refus de l'import (fichier entier), affiché dans la fenêtre.
    pub error: Option<String>,
}

impl CsvImportState {
    /// Fenêtre sur le fichier `name`, options détectées, aperçu calculé.
    #[must_use]
    pub fn open(name: String, bytes: Arc<[u8]>, now: Moment) -> Self {
        let mut state = Self {
            name,
            bytes: CsvBytes(bytes),
            separator: SeparatorChoice::Auto,
            encoding: None,
            locale: None,
            preview: Err(CsvError::Empty),
            shown: (None, None, None),
            now,
            error: None,
        };
        state.preview = preview(&state.bytes.0, &state.options());
        state
    }

    /// Options de l'import : choix de la fenêtre, projet planifié depuis le
    /// lundi qui suit « maintenant », 08:00 (comme « Nouveau »).
    #[must_use]
    pub fn options(&self) -> CsvImportOptions {
        let anchor = ScheduleAnchor::FromStart(next_monday_morning(self.now));
        let mut opts = CsvImportOptions::new(anchor, self.now);
        opts.separator = self.separator.byte();
        opts.encoding = self.encoding;
        opts.locale = self.locale;
        opts
    }

    /// Recalcule l'aperçu si une option a changé depuis le dernier.
    pub fn refresh(&mut self) {
        let wanted = (self.separator.byte(), self.encoding, self.locale);
        if wanted != self.shown {
            self.preview = preview(&self.bytes.0, &self.options());
            self.shown = wanted;
            self.error = None;
        }
    }
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsvOutcome {
    /// Rien.
    Open,
    /// « Importer ».
    Import,
    /// « Annuler » ou Échap.
    Cancel,
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.csv-import")
}

/// Liste déroulante nommée `label` sur `choices`.
fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    texts: &Texts,
    salt: &str,
    label: TextId,
    value: &mut T,
    choices: &[(T, TextId)],
) {
    let name = texts.get(label);
    ui.label(&name);
    let selected = choices
        .iter()
        .find(|(v, _)| v == value)
        .map_or_else(String::new, |(_, t)| texts.get(*t));
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (v, t) in choices {
                ui.selectable_value(value, *v, texts.get(*t));
            }
        })
        .response;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
}

/// Libellé d'un séparateur retenu.
fn separator_label(texts: &Texts, byte: u8) -> String {
    match byte {
        b';' => texts.get(TextId::CiSepSemicolon),
        b',' => texts.get(TextId::CiSepComma),
        b'\t' => texts.get(TextId::CiSepTab),
        other => char::from(other).to_string(),
    }
}

/// Libellé d'un encodage retenu.
fn encoding_label(texts: &Texts, encoding: CsvEncoding) -> String {
    ENCODINGS
        .iter()
        .find(|(e, _)| *e == Some(encoding))
        .map_or_else(String::new, |(_, t)| texts.get(*t))
}

/// Libellé d'une langue retenue.
fn locale_label(texts: &Texts, locale: CsvLocale) -> String {
    texts.get(match locale {
        CsvLocale::Fr => TextId::LangFr,
        CsvLocale::En => TextId::LangEn,
    })
}

/// Sort d'une colonne, en texte.
pub fn recognized_label(texts: &Texts, sort: Recognized) -> String {
    texts.get(match sort {
        Recognized::Column(_) => TextId::CiRecognized,
        Recognized::Unknown => TextId::CiUnknown,
        Recognized::Unsupported => TextId::CiUnsupported,
        Recognized::Duplicate => TextId::CiDuplicate,
    })
}

/// Texte d'une cellule de l'aperçu, coupé à [`CELL_CHARS`] caractères.
fn cell_text(raw: &str) -> String {
    let mut chars = raw.chars();
    let mut out: String = chars.by_ref().take(CELL_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// Aperçu : options retenues, colonnes et leur sort, lignes, remarques.
fn preview_body(ui: &mut egui::Ui, texts: &Texts, p: &CsvPreview) {
    let detected = [
        separator_label(texts, p.detected.separator),
        encoding_label(texts, p.detected.encoding),
        locale_label(texts, p.detected.locale),
    ]
    .join(", ");
    ui.label(texts.get_args(TextId::CiDetected, &[("detail", Arg::Str(detected.into()))]));
    ui.add_space(4.0);
    ui.strong(texts.get(TextId::CiColumns));
    egui::ScrollArea::vertical()
        .id_salt("plannercraft.csv-import.columns")
        .max_height(140.0)
        .show(ui, |ui| {
            egui::Grid::new("plannercraft.csv-import.columns.grid")
                .striped(true)
                .show(ui, |ui| {
                    for (header, sort) in &p.columns {
                        ui.label(cell_text(header));
                        ui.label(recognized_label(texts, *sort));
                        ui.end_row();
                    }
                });
        });
    ui.add_space(4.0);
    let count = i64::try_from(p.rows.len()).unwrap_or(i64::MAX);
    ui.strong(texts.get_args(TextId::CiRows, &[("count", Arg::Count(count))]));
    egui::ScrollArea::both()
        .id_salt("plannercraft.csv-import.rows")
        .max_height(220.0)
        .show(ui, |ui| {
            egui::Grid::new("plannercraft.csv-import.rows.grid")
                .striped(true)
                .show(ui, |ui| {
                    for (header, _) in &p.columns {
                        ui.strong(cell_text(header));
                    }
                    ui.end_row();
                    for row in &p.rows {
                        for cell in row {
                            ui.label(cell_text(cell));
                        }
                        ui.end_row();
                    }
                });
        });
    if !p.diagnostics.is_empty() {
        let count = i64::try_from(p.diagnostics.len()).unwrap_or(i64::MAX);
        ui.colored_label(
            ui.visuals().warn_fg_color,
            texts.get_args(TextId::CiDiagnostics, &[("count", Arg::Count(count))]),
        );
        for d in p.diagnostics.iter().take(5) {
            let line = i64::try_from(d.line).unwrap_or(i64::MAX);
            ui.label(texts.get_args(
                TextId::CiDiagnosticLine,
                &[
                    ("count", Arg::Count(line)),
                    ("detail", Arg::Str(d.code.code().into())),
                ],
            ));
        }
    }
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut CsvImportState) -> CsvOutcome {
    let mut outcome = CsvOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(640.0);
        ui.heading(texts.get(TextId::CiTitle));
        ui.label(texts.get_args(TextId::CiFile, &[("name", Arg::Str(state.name.as_str().into()))]));
        egui::Grid::new("plannercraft.csv-import.options").show(ui, |ui| {
            combo(
                ui,
                texts,
                "plannercraft.csv-import.separator",
                TextId::CiSeparator,
                &mut state.separator,
                &SEPARATORS,
            );
            ui.end_row();
            combo(
                ui,
                texts,
                "plannercraft.csv-import.encoding",
                TextId::CiEncoding,
                &mut state.encoding,
                &ENCODINGS,
            );
            ui.end_row();
            combo(
                ui,
                texts,
                "plannercraft.csv-import.language",
                TextId::CiLanguage,
                &mut state.locale,
                &LOCALES,
            );
            ui.end_row();
        });
        state.refresh();
        ui.separator();
        match &state.preview {
            Ok(p) => preview_body(ui, texts, p),
            Err(error) => super::error_line(ui, Some(&texts.get(error_text_id(error.code())))),
        }
        super::error_line(ui, state.error.as_deref());
        ui.separator();
        ui.horizontal(|ui| {
            let ready = state.preview.is_ok();
            if ui
                .add_enabled(ready, egui::Button::new(texts.get(TextId::CiImport)))
                .clicked()
            {
                outcome = CsvOutcome::Import;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = CsvOutcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = CsvOutcome::Cancel;
    }
    if outcome == CsvOutcome::Open
        && state.preview.is_ok()
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = CsvOutcome::Import;
    }
    outcome
}

/// Nom du document importé : nom du fichier sans son extension (« taches.csv »
/// ⇒ « taches ») ; « Enregistrer » proposera alors « taches.xml ».
#[must_use]
pub fn document_name(file: &str) -> String {
    std::path::Path::new(file)
        .file_stem()
        .map_or_else(|| file.to_owned(), |s| s.to_string_lossy().into_owned())
}

impl App {
    /// Fichier → Importer CSV… : sélecteur de fichiers ; la réponse arrive
    /// par [`App::poll_picked`].
    pub(crate) fn start_csv_import(&mut self, ctx: &egui::Context) {
        let request = PickRequest {
            title: self.texts.get(TextId::CiPickTitle),
            filter: self.texts.get(TextId::CiPickFilter),
            extensions: vec!["csv".to_owned(), "txt".to_owned()],
        };
        let tx = self.pick_tx.clone();
        self.env.files.pick_bytes(ctx, request, tx);
    }

    /// Relève les fichiers choisis pour l'import : aperçu ouvert, ou refus
    /// de lecture au bandeau.
    pub(crate) fn poll_picked(&mut self) {
        while let Ok(event) = self.pick_rx.try_recv() {
            match event {
                PickedFile::Cancelled => {}
                PickedFile::Picked { name, result } => match result {
                    Ok(bytes) => self.open_csv_import(name, bytes),
                    Err(error) => {
                        let text = match error {
                            PickError::Read(detail) => self.texts.get_args(
                                TextId::OpenErrorReadFile,
                                &[
                                    ("name", Arg::Str(name.into())),
                                    ("detail", Arg::Str(detail.into())),
                                ],
                            ),
                            PickError::TooLarge => self.texts.get(error_text_id(CsvError::TooLarge.code())),
                        };
                        self.push_titled(Severity::Error, Some(TextId::OpenErrorTitle), text);
                    }
                },
            }
        }
    }

    /// Ouvre l'aperçu de l'import sur `bytes` (une fenêtre déjà ouverte est
    /// remplacée ; une saisie de cellule est abandonnée).
    pub fn open_csv_import(&mut self, name: String, bytes: Vec<u8>) {
        let now = (self.env.now)();
        let state = CsvImportState::open(name, Arc::from(bytes), now);
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(super::DialogState {
            kind: DialogKind::CsvImport,
            body: super::DialogBody::CsvImport(Box::new(state)),
        });
    }

    /// Dessine l'aperçu de l'import et applique sa réponse.
    pub(crate) fn show_csv_import(&mut self, ctx: &egui::Context) {
        let Some(super::DialogState {
            body: super::DialogBody::CsvImport(state),
            ..
        }) = &mut self.view.dialog
        else {
            return;
        };
        match show(ctx, &self.texts, state) {
            CsvOutcome::Open => {}
            CsvOutcome::Cancel => self.close_dialog(),
            CsvOutcome::Import => self.import_csv(ctx),
        }
    }

    /// « Importer » : fichier entier lu ; succès ⇒ fenêtre fermée et
    /// nouveau document (après confirmation si le courant est modifié) ;
    /// refus ⇒ affiché dans la fenêtre.
    fn import_csv(&mut self, ctx: &egui::Context) {
        let now = (self.env.now)();
        let Some(super::DialogState {
            body: super::DialogBody::CsvImport(state),
            ..
        }) = &mut self.view.dialog
        else {
            return;
        };
        match crate::io::load_csv(&state.bytes.0, &state.options(), now) {
            Ok(loaded) => {
                let origin = DocOrigin {
                    name: document_name(&state.name),
                    path: None,
                    demo: false,
                };
                let remarks = loaded.diagnostics.len();
                let file = state.name.clone();
                self.close_dialog();
                self.request_install_csv(loaded.document, origin, (file, remarks), ctx);
            }
            Err(error) => state.error = Some(self.texts.get(error_text_id(error.code()))),
        }
    }
}
