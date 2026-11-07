//! Rendu Markdown des réponses de l'assistant (lot A1b d'ADR-0024,
//! H-UI-146 ; lot A2 : vrai gras), derrière une seule fonction
//! remplaçable : [`render_markdown`].
//!
//! Lot A2 : rendu écrit ici sur `pulldown-cmark` (MIT) à la place
//! d'`egui_commonmark`, qui ne rendait le gras que par la couleur (egui
//! n'a pas de graisse dans ses polices d'interface). Corps en Inter
//! Regular et gras en Inter `SemiBold`, les deux familles déjà embarquées
//! pour le planning (lot P2, [`crate::theme::install_plan_fonts`]) ; tant
//! qu'elles ne sont pas chargées (première image, harnais de test), la
//! police d'egui et la couleur appuyée servent de repli. Interligne ≈ 1,45
//! (cahier `assistant-chat.md` § 1 : ≈ 1,5).
//!
//! Sous-ensemble rendu : paragraphes, titres ramenés à deux niveaux
//! visuels (`#` : texte + 2 points en gras, les suivants : gras), gras,
//! italique, barré, code en ligne et en bloc, listes à puces et numérotées
//! (imbriquées, retrait suspendu), citations, filets, tableaux, liens
//! (texte souligné, non cliquable). Le texte du modèle n'est jamais
//! interprété comme du HTML (balises affichées telles quelles).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Écart de taille entre un titre `#` et le texte.
const HEADING_EXTRA: f32 = 2.0;
/// Interligne (multiple de la taille du texte).
const LINE_HEIGHT: f32 = 1.45;
/// Retrait d'un niveau de liste ou de citation.
const INDENT: f32 = 18.0;
/// Espace après un bloc.
const BLOCK_GAP: f32 = 6.0;

/// Polices du rendu : Inter si chargée, sinon celles d'egui.
#[derive(Clone)]
struct Fonts {
    regular: FontFamily,
    strong: FontFamily,
    /// Le gras a sa graisse (sinon : couleur appuyée).
    weighted: bool,
    size: f32,
    text: Color32,
    strong_color: Color32,
    weak: Color32,
    link: Color32,
    code_bg: Color32,
}

/// Style courant d'un passage.
#[derive(Clone, Copy, Default)]
struct Span {
    strong: u8,
    emphasis: u8,
    strike: u8,
    link: u8,
    heading: Option<HeadingLevel>,
}

/// Tableau en construction.
#[derive(Default)]
struct TableBuf {
    rows: Vec<Vec<LayoutJob>>,
    head_rows: usize,
    in_head: bool,
}

/// Rendu en cours.
struct Renderer<'f> {
    fonts: &'f Fonts,
    span: Span,
    job: LayoutJob,
    /// Puce ou numéro de l'élément de liste dont le texte n'a pas encore
    /// été dessiné.
    bullet: Option<String>,
    /// Listes ouvertes : prochain numéro (`None` : puces).
    lists: Vec<Option<u64>>,
    quotes: usize,
    table: Option<TableBuf>,
    code_block: Option<String>,
}

impl Renderer<'_> {
    fn format(&self) -> TextFormat {
        let f = self.fonts;
        let strong = self.span.strong > 0 || self.span.heading.is_some();
        let size = match self.span.heading {
            Some(HeadingLevel::H1) => f.size + HEADING_EXTRA,
            _ => f.size,
        };
        let family = if strong {
            f.strong.clone()
        } else {
            f.regular.clone()
        };
        let mut color = if strong && !f.weighted {
            f.strong_color
        } else {
            f.text
        };
        if self.span.link > 0 {
            color = f.link;
        }
        TextFormat {
            font_id: FontId::new(size, family),
            line_height: Some((size * LINE_HEIGHT).round()),
            color,
            italics: self.span.emphasis > 0,
            strikethrough: if self.span.strike > 0 {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            underline: if self.span.link > 0 {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            ..TextFormat::default()
        }
    }

    fn text(&mut self, text: &str) {
        let format = self.format();
        self.job.append(text, 0.0, format);
    }

    fn code(&mut self, text: &str) {
        let f = self.fonts;
        let format = TextFormat {
            font_id: FontId::monospace((f.size - 1.0).max(8.0)),
            line_height: Some((f.size * LINE_HEIGHT).round()),
            color: f.text,
            background: f.code_bg,
            ..TextFormat::default()
        };
        self.job.append(text, 0.0, format);
    }

    fn indent(&self) -> f32 {
        let depth = self.lists.len().saturating_add(self.quotes);
        #[allow(clippy::cast_precision_loss, reason = "profondeur d'imbrication petite")]
        let depth = depth.min(8) as f32;
        depth * INDENT
    }

    /// Dessine le passage accumulé (avec la puce en attente, retrait
    /// suspendu) ; rien s'il est vide.
    fn flush(&mut self, ui: &mut egui::Ui) {
        if let Some(table) = &mut self.table {
            // Cellule de tableau : gardée pour la grille.
            let job = std::mem::take(&mut self.job);
            if let Some(row) = table.rows.last_mut() {
                row.push(job);
            }
            return;
        }
        let bullet = self.bullet.take();
        // Élément sans texte (flux en cours, ou sous-liste seule) : pas de
        // puce orpheline.
        if self.job.is_empty() {
            return;
        }
        let mut job = std::mem::take(&mut self.job);
        job.wrap.max_width = f32::INFINITY;
        let indent = self.indent();
        let quoted = self.quotes > 0;
        let rule = self.fonts.weak;
        let line = (self.fonts.size * LINE_HEIGHT).round();
        let format = self.format();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let top = ui.cursor().top();
            match bullet {
                Some(mark) => {
                    ui.add_space((indent - INDENT).max(0.0));
                    let mut head = LayoutJob::default();
                    head.append(&mark, 0.0, format);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(INDENT, line), egui::Sense::hover());
                    let galley = ui.painter().layout_job(head);
                    let x = rect.right() - galley.size().x - 4.0;
                    ui.painter().galley(
                        egui::pos2(x.max(rect.left()), rect.top()),
                        galley,
                        Color32::PLACEHOLDER,
                    );
                }
                None => ui.add_space(indent),
            }
            let response = ui.add(egui::Label::new(job).wrap());
            if quoted {
                let x = ui.min_rect().left() + (indent - INDENT).max(0.0) + 4.0;
                ui.painter()
                    .vline(x, top..=response.rect.bottom(), Stroke::new(2.0, rule));
            }
        });
    }

    fn start(&mut self, ui: &mut egui::Ui, tag: &Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush(ui);
                ui.add_space(BLOCK_GAP / 2.0);
                self.span.heading = Some(*level);
            }
            Tag::BlockQuote(_) => {
                self.flush(ui);
                self.quotes = self.quotes.saturating_add(1);
            }
            Tag::CodeBlock(_) => {
                self.flush(ui);
                self.code_block = Some(String::new());
            }
            Tag::List(start) => {
                self.flush(ui);
                self.lists.push(*start);
            }
            Tag::Item => {
                self.flush(ui);
                let mark = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let mark = format!("{n}.");
                        *n = n.saturating_add(1);
                        mark
                    }
                    _ => "•".to_owned(),
                };
                self.bullet = Some(mark);
            }
            Tag::Table(_) => {
                self.flush(ui);
                self.table = Some(TableBuf::default());
            }
            Tag::TableHead => {
                if let Some(t) = &mut self.table {
                    t.in_head = true;
                    t.rows.push(Vec::new());
                }
            }
            Tag::TableRow => {
                if let Some(t) = &mut self.table {
                    t.rows.push(Vec::new());
                }
            }
            Tag::TableCell => {
                if self.table.as_ref().is_some_and(|t| t.in_head) {
                    self.span.strong = self.span.strong.saturating_add(1);
                }
            }
            Tag::Emphasis => self.span.emphasis = self.span.emphasis.saturating_add(1),
            Tag::Strong => self.span.strong = self.span.strong.saturating_add(1),
            Tag::Strikethrough => self.span.strike = self.span.strike.saturating_add(1),
            Tag::Link { .. } => self.span.link = self.span.link.saturating_add(1),
            _ => {}
        }
    }

    fn end(&mut self, ui: &mut egui::Ui, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush(ui);
                if self.lists.is_empty() && self.table.is_none() {
                    ui.add_space(BLOCK_GAP);
                }
            }
            TagEnd::Heading(_) => {
                self.flush(ui);
                self.span.heading = None;
                ui.add_space(BLOCK_GAP / 2.0);
            }
            TagEnd::BlockQuote(_) => {
                self.flush(ui);
                self.quotes = self.quotes.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code_block.take() {
                    code_block(ui, self.fonts, code.trim_end(), self.indent());
                    ui.add_space(BLOCK_GAP);
                }
            }
            TagEnd::List(_) => {
                self.flush(ui);
                self.lists.pop();
                if self.lists.is_empty() {
                    ui.add_space(BLOCK_GAP);
                }
            }
            TagEnd::Item => self.flush(ui),
            TagEnd::TableCell => {
                self.flush(ui);
                if self.table.as_ref().is_some_and(|t| t.in_head) {
                    self.span.strong = self.span.strong.saturating_sub(1);
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = &mut self.table {
                    t.in_head = false;
                    t.head_rows = t.rows.len();
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    table(ui, t, self.indent());
                    ui.add_space(BLOCK_GAP);
                }
            }
            TagEnd::Emphasis => self.span.emphasis = self.span.emphasis.saturating_sub(1),
            TagEnd::Strong => self.span.strong = self.span.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.span.strike = self.span.strike.saturating_sub(1),
            TagEnd::Link => self.span.link = self.span.link.saturating_sub(1),
            _ => {}
        }
    }
}

/// Bloc de code : fond discret, police à chasse fixe.
fn code_block(ui: &mut egui::Ui, fonts: &Fonts, code: &str, indent: f32) {
    ui.horizontal_top(|ui| {
        ui.add_space(indent);
        egui::Frame::new()
            .fill(fonts.code_bg)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(code)
                        .font(FontId::monospace((fonts.size - 1.0).max(8.0)))
                        .color(fonts.text),
                );
            });
    });
}

/// Tableau : grille rayée, en-tête en gras.
fn table(ui: &mut egui::Ui, table: TableBuf, indent: f32) {
    let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    ui.horizontal_top(|ui| {
        ui.add_space(indent);
        let id = ui.next_auto_id();
        egui::Grid::new(id)
            .striped(true)
            .num_columns(columns)
            .spacing(egui::vec2(12.0, 4.0))
            .show(ui, |ui| {
                for row in table.rows {
                    for cell in row {
                        ui.add(egui::Label::new(cell).wrap());
                    }
                    ui.end_row();
                }
            });
    });
}

/// Dessine `text` (Markdown) dans `ui`, sur toute la largeur disponible.
pub fn render_markdown(ui: &mut egui::Ui, text: &str) {
    let size = ui
        .style()
        .text_styles
        .get(&TextStyle::Body)
        .map_or(14.0, |f| f.size);
    let weighted = crate::theme::plan_fonts_ready(ui.ctx());
    let visuals = ui.visuals();
    let (regular, strong) = if weighted {
        (
            FontFamily::Name(crate::theme::PLAN_FAMILY.into()),
            FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into()),
        )
    } else {
        (FontFamily::Proportional, FontFamily::Proportional)
    };
    let fonts = Fonts {
        regular,
        strong,
        weighted,
        size,
        text: visuals.widgets.noninteractive.fg_stroke.color,
        strong_color: visuals.widgets.active.fg_stroke.color,
        weak: visuals.widgets.noninteractive.bg_stroke.color,
        link: visuals.hyperlink_color,
        code_bg: visuals.code_bg_color,
    };
    let mut r = Renderer {
        fonts: &fonts,
        span: Span::default(),
        job: LayoutJob::default(),
        bullet: None,
        lists: Vec::new(),
        quotes: 0,
        table: None,
        code_block: None,
    };
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for event in Parser::new_ext(text, options) {
            match event {
                Event::Start(tag) => r.start(ui, &tag),
                Event::End(tag) => r.end(ui, tag),
                Event::Text(t) => match &mut r.code_block {
                    Some(code) => code.push_str(&t),
                    None => r.text(&t),
                },
                Event::Code(t) => r.code(&t),
                Event::Html(t) | Event::InlineHtml(t) => r.text(&t),
                Event::SoftBreak => r.text(" "),
                Event::HardBreak => r.text("\n"),
                Event::Rule => {
                    r.flush(ui);
                    ui.separator();
                }
                Event::TaskListMarker(done) => r.text(if done { "[x] " } else { "[ ] " }),
                _ => {}
            }
        }
        r.flush(ui);
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

    /// Lot A2 : le gras prend la famille Inter `SemiBold` (vraie graisse,
    /// couleur du texte) quand les polices du planning sont chargées ;
    /// sinon repli sur la couleur appuyée.
    #[test]
    fn h_ui_146_bold_uses_semibold_family() {
        use egui::{Color32, FontFamily};
        let fonts = |weighted: bool| super::Fonts {
            regular: FontFamily::Name(crate::theme::PLAN_FAMILY.into()),
            strong: FontFamily::Name(crate::theme::PLAN_FAMILY_STRONG.into()),
            weighted,
            size: 14.0,
            text: Color32::from_gray(40),
            strong_color: Color32::BLACK,
            weak: Color32::GRAY,
            link: Color32::BLUE,
            code_bg: Color32::WHITE,
        };
        for weighted in [true, false] {
            let f = fonts(weighted);
            let mut r = super::Renderer {
                fonts: &f,
                span: super::Span::default(),
                job: egui::text::LayoutJob::default(),
                bullet: None,
                lists: Vec::new(),
                quotes: 0,
                table: None,
                code_block: None,
            };
            let plain = r.format();
            assert_eq!(plain.font_id.family, f.regular);
            r.span.strong = 1;
            let bold = r.format();
            assert_eq!(bold.font_id.family, f.strong);
            let expected = if weighted { f.text } else { f.strong_color };
            assert_eq!(bold.color, expected);
            assert_eq!(bold.line_height, Some((14.0_f32 * 1.45).round()));
        }
    }

    /// Le rendu complet (listes imbriquées, tableau, code, citation) ne
    /// perd aucun texte.
    #[test]
    fn h_ui_146_markdown_blocks() {
        let harness = Harness::new_ui(|ui| {
            super::render_markdown(
                ui,
                "# Titre\n\n1. un\n2. deux\n   - imbriqué\n\n> citée\n\n| A | B |\n|---|---|\n| x | y |\n\n`code`\n\n---\n\n```\nbloc\n```\n",
            );
        });
        for text in ["Titre", "un", "imbriqué", "citée", "x", "y", "code", "bloc"] {
            assert!(harness.query_all_by_label_contains(text).count() >= 1, "{text}");
        }
    }

    #[test]
    fn plain_text_drops_marks() {
        assert_eq!(
            super::plain_text("Le **chemin** :\n\n- A\n- `B`\n\n# Titre"),
            "Le chemin :\nA\nB\nTitre"
        );
    }
}
