//! Rendu Markdown des réponses de l'assistant (lot A1b d'ADR-0024,
//! H-UI-146), derrière une seule fonction remplaçable :
//! [`render_markdown`].
//!
//! Moteur : `egui_commonmark` 0.26 (MIT OR Apache-2.0, `pulldown-cmark`),
//! sans images ni coloration syntaxique. Titres ramenés à deux niveaux
//! visuels : la taille `Heading` du style est fixée à celle du texte plus
//! 2 points le temps du rendu, de sorte que `#` est à peine plus grand que
//! le texte et les niveaux suivants sont du texte appuyé. Le texte du
//! modèle n'est jamais interprété comme du HTML (pas de rendu HTML).

use std::sync::{Arc, Mutex};

use egui::{FontId, TextStyle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// Écart de taille entre un titre `#` et le texte.
const HEADING_EXTRA: f32 = 2.0;

type SharedCache = Arc<Mutex<CommonMarkCache>>;

/// Dessine `text` (Markdown) dans `ui`, sur toute la largeur disponible.
pub fn render_markdown(ui: &mut egui::Ui, text: &str) {
    let id = egui::Id::unique("plannercraft.markdown.cache");
    let cache: SharedCache = ui.data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(CommonMarkCache::default())))
            .clone()
    });
    let body = ui
        .style()
        .text_styles
        .get(&TextStyle::Body)
        .map_or(14.0, |f| f.size);
    ui.scope(|ui| {
        ui.style_mut()
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(body + HEADING_EXTRA));
        let Ok(mut cache) = cache.lock() else {
            // Cache empoisonné (panique d'un autre fil, jamais attendue) :
            // texte brut plutôt que rien.
            ui.label(text);
            return;
        };
        CommonMarkViewer::new()
            .indentation_spaces(2)
            .show(ui, &mut cache, text);
    });
}

/// Texte brut de `text` (Markdown) : texte et code des évènements de
/// `pulldown-cmark`, une ligne par bloc, sans marques (`**`, `#`, `- `).
/// Sert au texte accessible publié en fin de message.
#[must_use]
pub fn plain_text(text: &str) -> String {
    use pulldown_cmark::{Event, Parser, TagEnd};
    let mut out = String::with_capacity(text.len());
    for event in Parser::new(text) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::Heading(_) | TagEnd::TableRow)
                if !out.ends_with('\n') =>
            {
                out.push('\n');
            }
            _ => {}
        }
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests : un échec doit s'arrêter net")]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    /// Gras et liste rendus : le texte est présent sans les marques `**`
    /// ni `- `.
    #[test]
    fn h_ui_146_markdown_bold_and_list() {
        let harness = Harness::new_ui(|ui| {
            super::render_markdown(ui, "Le **chemin critique** :\n\n- Études\n- Fondations\n");
        });
        assert!(harness.query_by_label_contains("**").is_none());
        harness.get_by_label_contains("chemin critique");
        harness.get_by_label_contains("Fondations");
    }

    #[test]
    fn plain_text_drops_marks() {
        assert_eq!(
            super::plain_text("Le **chemin** :\n\n- A\n- `B`\n\n# Titre"),
            "Le chemin :\nA\nB\nTitre"
        );
    }
}
