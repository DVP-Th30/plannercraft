//! Galerie des modèles de mise en forme (H-UI-52) : aperçu réel sur le
//! projet ouvert, Appliquer = une entrée d'annulation ; volet de mise en
//! forme directe (H-UI-59 à H-UI-61). Contrat posé par le lot L0,
//! implémenté en L15.
//!
//! Format → Modèles de mise en forme… : une carte par modèle intégré
//! (vignette des 12 premières lignes du projet ouvert, dessinée depuis la
//! scène du rendu, même géométrie que l'écran, H-UI-42), le modèle actuel
//! marqué, « Appliquer » (`style.apply { template, patch }`, une entrée
//! d'annulation ; le patch garde les couleurs de phase et la mise en page
//! enregistrées du document, relecture finale de J8, F5), « Définir par défaut » (modèle des documents sans
//! style, H-UI-43) ; modèles personnels (préférences) applicables de même ;
//! « Enregistrer comme modèle personnel » (nom, 64 caractères au plus,
//! 32 modèles au plus). Écart au cahier : la vignette vient de la scène
//! (`layout`) et non de l'écran stylé ; même géométrie et mêmes couleurs,
//! sans dépendance nouvelle.

use plannercraft_automation::Session;
use plannercraft_automation::render_text::SessionTexts;
use plannercraft_automation::text::Language;
use plannercraft_render::geometry::{Scene, Viewport, layout};
use plannercraft_render::style::{DocStyle, TemplateId, Variant, template};
use plannercraft_render::{Lang, RenderLimits, Zoom};

use crate::i18n::{Arg, TextId, Texts, template_name};
use crate::state::NamedStyle;

/// Lignes montrées par une vignette.
pub const THUMB_ROWS: u32 = 12;

/// Taille d'une vignette à l'écran.
const THUMB_SIZE: egui::Vec2 = egui::vec2(250.0, 120.0);

/// Réponse de la galerie à une image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GalleryOutcome {
    /// Rien.
    Open,
    /// Fermer.
    Close,
    /// Appliquer un modèle intégré.
    Apply(TemplateId),
    /// Appliquer le modèle personnel d'indice donné.
    ApplyUser(usize),
    /// Définir le modèle par défaut.
    MakeDefault(TemplateId),
    /// Enregistrer le style du document comme modèle personnel.
    Save(String),
}

/// Vignettes gardées : clé (document, langue, variante) et scènes.
type Thumbs = ((u64, u64), Language, Variant, Vec<(TemplateId, Scene)>);

/// Brouillon de la galerie.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GalleryState {
    /// Nom saisi pour « Enregistrer comme modèle personnel ».
    pub name: String,
    /// Retour du dernier enregistrement (texte affiché).
    pub message: Option<String>,
    /// Vignettes, relues quand le document, la langue ou le thème changent.
    thumbs: Option<Thumbs>,
}

/// Langue du rendu.
const fn render_lang(lang: Language) -> Lang {
    match lang {
        Language::Fr => Lang::Fr,
        Language::En => Lang::En,
    }
}

/// Scène d'une vignette : `THUMB_ROWS` premières lignes du projet, plage
/// du projet ajustée à 600 px, textes de la session (règle 3).
#[must_use]
pub fn thumb_scene(session: &Session, id: TemplateId, lang: Language, variant: Variant) -> Scene {
    let st = template(id);
    let vp = Viewport {
        zoom: Zoom::Auto { width_px: 600 },
        limits: RenderLimits {
            max_rows: THUMB_ROWS,
            ..RenderLimits::default()
        },
        ..Viewport::new(variant, render_lang(lang))
    };
    let texts = SessionTexts { session, lang };
    layout(session.project(), session.schedule(), &st, &texts, &vp).0
}

impl GalleryState {
    /// Vignettes du document `key` (relues si la clé a changé).
    fn thumbs(
        &mut self,
        session: &Session,
        key: (u64, u64),
        lang: Language,
        variant: Variant,
    ) -> &[(TemplateId, Scene)] {
        let fresh = self
            .thumbs
            .as_ref()
            .is_some_and(|(k, l, v, _)| *k == key && *l == lang && *v == variant);
        if !fresh {
            let scenes = TemplateId::ALL
                .into_iter()
                .map(|id| (id, thumb_scene(session, id, lang, variant)))
                .collect();
            self.thumbs = Some((key, lang, variant, scenes));
        }
        self.thumbs.as_ref().map_or(&[], |(_, _, _, s)| s.as_slice())
    }
}

/// Entrées de la galerie lues par la fenêtre.
pub struct GalleryInput<'a> {
    /// Session du document.
    pub session: &'a Session,
    /// Clé du document (ouverture, génération).
    pub key: (u64, u64),
    /// Langue des textes du rendu.
    pub lang: Language,
    /// Modèle du document (style choisi), s'il en a un.
    pub current: Option<DocStyle>,
    /// Modèle par défaut des préférences.
    pub default: TemplateId,
    /// Modèles personnels.
    pub user: &'a [NamedStyle],
}

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.gallery")
}

/// Dessine la galerie (une image) ; rend la réponse de l'utilisateur.
#[allow(
    clippy::too_many_lines,
    reason = "cartes, modèles personnels, enregistrement : un bloc chacun"
)]
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    state: &mut GalleryState,
    input: &GalleryInput<'_>,
) -> GalleryOutcome {
    let mut outcome = GalleryOutcome::Open;
    let variant = match ctx.theme() {
        egui::Theme::Light => Variant::Light,
        egui::Theme::Dark => Variant::Dark,
    };
    let inter = ctx.fonts(|f| {
        f.families()
            .contains(&egui::FontFamily::Name(crate::theme::PLAN_FAMILY.into()))
    });
    let current_base = input.current.as_ref().map(|d| d.base);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_max_width(840.0);
        ui.heading(texts.get(TextId::GalleryTitle));
        let thumbs = state
            .thumbs(input.session, input.key, input.lang, variant)
            .to_vec();
        egui::Grid::new("plannercraft.gallery-grid")
            .spacing(egui::vec2(16.0, 12.0))
            .show(ui, |ui| {
                for (n, (id, scene)) in thumbs.iter().enumerate() {
                    ui.vertical(|ui| {
                        let (rect, _) = ui.allocate_exact_size(THUMB_SIZE, egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        crate::view::scene_paint::thumbnail(&painter, scene, rect, inter);
                        if scene.rows.is_empty() {
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                texts.get(TextId::GalleryPreviewEmpty),
                                egui::FontId::proportional(13.0),
                                ui.visuals().weak_text_color(),
                            );
                        }
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                            egui::StrokeKind::Outside,
                        );
                        let mut title = texts.get(template_name(*id));
                        let current =
                            current_base == Some(*id) || (current_base.is_none() && input.default == *id);
                        if current {
                            title = format!("{title} — {}", texts.get(TextId::GalleryCurrent));
                        }
                        ui.strong(title);
                        ui.horizontal(|ui| {
                            let apply = texts.get(TextId::GalleryApply);
                            let name = texts.get(template_name(*id));
                            let button = ui.button(apply.as_str());
                            // Nom accessible distinct par carte (« Appliquer Sobre »).
                            button.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{apply} {name}"))
                            });
                            if button.clicked() {
                                outcome = GalleryOutcome::Apply(*id);
                            }
                            if input.default == *id {
                                ui.label(texts.get(TextId::GalleryDefault))
                                    .on_hover_text(texts.get(TextId::GalleryDefaultHint));
                            } else {
                                let label = texts.get(TextId::GalleryMakeDefault);
                                let b = ui.button(label.as_str());
                                b.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        format!("{label} {name}"),
                                    )
                                });
                                if b.on_hover_text(texts.get(TextId::GalleryDefaultHint)).clicked() {
                                    outcome = GalleryOutcome::MakeDefault(*id);
                                }
                            }
                        });
                    });
                    if n % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        ui.separator();
        ui.strong(texts.get(TextId::GalleryUserTemplates));
        if input.user.is_empty() {
            ui.label(texts.get(TextId::GalleryNoUserTemplate));
        }
        for (i, t) in input.user.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(t.name.as_str());
                let apply = texts.get(TextId::GalleryApply);
                let b = ui.button(apply.as_str());
                let name = t.name.clone();
                b.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{apply} {name}"))
                });
                if b.clicked() {
                    outcome = GalleryOutcome::ApplyUser(i);
                }
            });
        }
        ui.horizontal(|ui| {
            super::text_field(
                ui,
                &texts.get(TextId::GallerySaveName),
                &mut state.name,
                true,
                220.0,
            );
            if ui.button(texts.get(TextId::GallerySave)).clicked() {
                outcome = GalleryOutcome::Save(state.name.clone());
            }
        });
        if let Some(message) = &state.message {
            ui.label(message.as_str());
        }
        ui.separator();
        if ui.button(texts.get(TextId::GalleryClose)).clicked() {
            outcome = GalleryOutcome::Close;
        }
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = GalleryOutcome::Close;
    }
    outcome
}

/// Texte du retour d'un enregistrement de modèle personnel.
#[must_use]
pub fn saved_message(texts: &Texts, result: Result<&str, crate::state::UserTemplateError>) -> String {
    match result {
        Ok(name) => texts.get_args(
            TextId::GallerySaved,
            &[("name", Arg::Str(name.to_owned().into()))],
        ),
        Err(crate::state::UserTemplateError::BadName) => texts.get(TextId::GalleryNameInvalid),
        Err(crate::state::UserTemplateError::Full) => texts.get(TextId::GalleryFull),
    }
}

impl crate::app::App {
    /// Ouvre la galerie (sans document : rien).
    pub(crate) fn open_gallery(&mut self) {
        if self.document.is_none() {
            return;
        }
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(super::DialogState {
            kind: crate::action::DialogKind::Gallery,
            body: super::DialogBody::Gallery(Box::default()),
        });
    }

    /// Dessine la galerie et applique sa réponse.
    pub(crate) fn show_gallery(&mut self, ctx: &egui::Context) {
        use plannercraft_automation::command::Command;
        use plannercraft_automation::command::style::{StyleApply, TemplateJson, doc_style};
        let lang = self.text_language();
        let (
            Some(document),
            Some(super::DialogState {
                body: super::DialogBody::Gallery(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            return;
        };
        let input = GalleryInput {
            session: &document.session,
            key: document.cache_key(),
            lang,
            current: doc_style(document.project()),
            default: self.state.default_template,
            user: &self.state.user_templates,
        };
        // Relecture finale de J8 (F5) : changer de modèle garde ce qui
        // appartient au projet et non au modèle, couleurs de phase (H-UI-65)
        // et mise en page enregistrée (colonnes d'impression, en-tête et
        // pied, cartouche) ; le reste des surcharges suit le modèle.
        // ADR-0017 (lot L6) : la liste des tâches de la chronologie
        // (H-UI-79) appartient au projet aussi, gardée telle quelle.
        let kept = input
            .current
            .as_ref()
            .map(|d| plannercraft_render::style::StylePatch {
                phases: d.over.phases.clone(),
                page: d.over.page.clone(),
                timeline: d.over.timeline.clone(),
                ..plannercraft_render::style::StylePatch::default()
            })
            .unwrap_or_default();
        let outcome = show(ctx, &self.texts, state, &input);
        let apply = |template: TemplateId, patch: Option<serde_json::Value>| {
            super::batch_of(
                vec![Command::StyleApply(StyleApply {
                    template: Some(TemplateJson::from(template)),
                    patch,
                    reset: false,
                })],
                lang,
            )
        };
        match outcome {
            GalleryOutcome::Open => {}
            GalleryOutcome::Close => self.close_dialog(),
            GalleryOutcome::Apply(id) => {
                let patch = (!kept.is_empty())
                    .then(|| serde_json::to_value(&kept).ok())
                    .flatten();
                let batch = apply(id, patch);
                drop(self.execute(&batch, crate::app::EditOrigin::User));
            }
            GalleryOutcome::ApplyUser(i) => {
                if let Some(t) = self.state.user_templates.get(i) {
                    // Surcharges du modèle personnel par-dessus celles que
                    // garde le projet (les siennes l'emportent).
                    let mut over = plannercraft_automation::command::style::merge_patch(&kept, &t.style.over)
                        .unwrap_or_else(|| t.style.over.clone());
                    // Chronologie : UID du projet ouvert, jamais ceux du
                    // projet d'où vient le modèle (ADR-0017, lot L6).
                    over.timeline.clone_from(&kept.timeline);
                    let patch = (!over.is_empty())
                        .then(|| serde_json::to_value(&over).ok())
                        .flatten();
                    let batch = apply(t.style.base, patch);
                    drop(self.execute(&batch, crate::app::EditOrigin::User));
                }
            }
            GalleryOutcome::MakeDefault(id) => self.state.default_template = id,
            GalleryOutcome::Save(name) => {
                let message = self.save_user_template_named(&name);
                if let Some(super::DialogState {
                    body: super::DialogBody::Gallery(state),
                    ..
                }) = &mut self.view.dialog
                {
                    state.message = Some(message);
                }
            }
        }
    }

    /// « Enregistrer comme modèle personnel » : style du document (sinon le
    /// modèle par défaut sans surcharge) sous `name` ; rend le texte du
    /// retour (enregistré, nom invalide, liste pleine).
    pub(crate) fn save_user_template_named(&mut self, name: &str) -> String {
        use plannercraft_render::style::STYLE_VERSION;
        let Some(document) = &self.document else {
            return String::new();
        };
        let mut style =
            plannercraft_automation::command::style::doc_style(document.project()).unwrap_or(DocStyle {
                v: STYLE_VERSION,
                base: self.state.default_template,
                over: plannercraft_render::style::StylePatch::default(),
            });
        // Un modèle ne porte pas la chronologie : ses UID n'ont de sens que
        // dans ce projet (ADR-0017, lot L6).
        style.over.timeline = None;
        let trimmed = name.trim().to_owned();
        let result = self.state.save_user_template(name, style);
        saved_message(&self.texts, result.map(|()| trimmed.as_str()))
    }
}
