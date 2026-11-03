//! Panneau repliable des diagnostics d'import et de calcul (version
//! minimale) : une ligne par diagnostic, triée par gravité (erreurs, puis
//! avertissements, puis informations), dans l'ordre d'émission à gravité
//! égale.
//!
//! Les entrées viennent toutes prêtes de `FrameCtx::diagnostics`
//! (`DiagnosticsView`, construite une fois par document et par langue par
//! `App`) : aucune mise en texte ni aucun tri pendant l'image. Les textes
//! sont le `Display` des diagnostics (en français, non traduits ; « → »
//! remplacé par « -> », absent des polices d'egui : voir `docs/ui-needs.md`,
//! N5).
//!
//! Seconde passe UX (lot L10, H-UI-119) : le volet n'est dessiné que si le
//! document a au moins un diagnostic (la case Affichage → Diagnostics peut
//! rester cochée sans volet : rien à montrer, les pastilles de la barre
//! d'état disent déjà « aucune erreur ») ; une pastille de la barre d'état
//! l'ouvre filtré sur sa gravité ([`set_filter`]), « Tout afficher » retire
//! le filtre.

use plannercraft_core::Severity;

use super::FrameCtx;
use crate::i18n::{Arg, TextId};

/// Clé, dans la mémoire d'egui, du filtre de gravité du volet.
fn filter_id() -> egui::Id {
    egui::Id::unique("plannercraft.diagnostics.filter")
}

/// Clé, dans la mémoire d'egui, du relevé « volet dessiné à cette image ».
fn drawn_id() -> egui::Id {
    egui::Id::unique("plannercraft.diagnostics.drawn")
}

/// Filtre de gravité courant du volet (`None` : tous les diagnostics).
#[must_use]
pub fn filter(ctx: &egui::Context) -> Option<Severity> {
    ctx.data(|data| data.get_temp::<Option<Severity>>(filter_id()))
        .flatten()
}

/// Pose le filtre de gravité du volet (pastille de la barre d'état ;
/// `None` : tout afficher). Préférence d'affichage de la session, jamais
/// enregistrée.
pub fn set_filter(ctx: &egui::Context, severity: Option<Severity>) {
    ctx.data_mut(|data| data.insert_temp(filter_id(), severity));
}

/// Volet fermé (relevé par la barre d'état à chaque image) : filtre retiré
/// (la case du menu Affichage rouvre le volet sur tous les diagnostics)
/// et relevé « dessiné » remis à faux.
pub fn closed(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.insert_temp::<Option<Severity>>(filter_id(), None);
        data.insert_temp(drawn_id(), false);
    });
}

/// Le volet a-t-il été dessiné à la dernière image (H-UI-119, relevé des
/// tests) ?
#[must_use]
pub fn drawn(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(drawn_id()))
        .unwrap_or(false)
}

/// Plage des entrées de gravité `severity` dans `entries` (triées par
/// gravité, la plus grave d'abord : la plage est contiguë) ; vide si
/// aucune.
#[must_use]
pub fn severity_range(entries: &[(Severity, String)], severity: Severity) -> std::ops::Range<usize> {
    let rank = |s: Severity| match s {
        Severity::Error => 0_u8,
        Severity::Warning => 1,
        Severity::Info => 2,
    };
    let wanted = rank(severity);
    let start = entries.partition_point(|(s, _)| rank(*s) < wanted);
    let end = entries.partition_point(|(s, _)| rank(*s) <= wanted);
    start..end.max(start)
}

/// Panneau des diagnostics, en bas de la fenêtre (au-dessus de la barre
/// d'état), redimensionnable ; absent sans document ou sans diagnostic
/// (H-UI-119). Filtré sur la gravité de [`filter`] s'il en reste ; sinon
/// tous.
pub fn panel(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let ctx = ui.ctx().clone();
    let entries = &fc.diagnostics.entries;
    if fc.doc.is_none() || entries.is_empty() {
        ctx.data_mut(|data| data.insert_temp(drawn_id(), false));
        return;
    }
    let texts = fc.texts;
    // Filtre posé par une pastille ; une gravité sans entrée ⇒ tout.
    let filtered = filter(&ctx)
        .map(|severity| (severity, severity_range(entries, severity)))
        .filter(|(_, range)| !range.is_empty());
    let range = filtered.as_ref().map_or(0..entries.len(), |(_, r)| r.clone());
    let shown = entries.get(range).unwrap_or_default();
    let mut show_all = false;
    egui::Panel::bottom("plannercraft_diagnostics")
        .resizable(true)
        .default_size(fc.metrics.row_h * 7.0)
        .show(ui, |ui| {
            if let Some((severity, _)) = filtered {
                ui.horizontal(|ui| {
                    let id = match severity {
                        Severity::Error => TextId::StatusErrors,
                        Severity::Warning => TextId::StatusWarnings,
                        Severity::Info => TextId::MenuDiagnostics,
                    };
                    let count = i64::try_from(shown.len()).unwrap_or(i64::MAX);
                    ui.strong(texts.get_args(id, &[("count", Arg::Count(count))]));
                    if ui.small_button(texts.get(TextId::DiagnosticsShowAll)).clicked() {
                        show_all = true;
                    }
                });
                ui.separator();
            }
            // Lignes virtualisées (une ligne de texte chacune, tronquée ; le
            // texte entier s'affiche au survol) : coût par image borné par la
            // hauteur du panneau, pas par le nombre de diagnostics.
            let row_h = ui.text_style_height(&egui::TextStyle::Body);
            egui::ScrollArea::vertical().auto_shrink([false, true]).show_rows(
                ui,
                row_h,
                shown.len(),
                |ui, range| {
                    for (severity, text) in shown.get(range).unwrap_or_default() {
                        let color = match severity {
                            Severity::Error => ui.visuals().error_fg_color,
                            Severity::Warning => ui.visuals().warn_fg_color,
                            Severity::Info => ui.visuals().text_color(),
                        };
                        ui.add(egui::Label::new(egui::RichText::new(text.as_str()).color(color)).truncate());
                    }
                },
            );
        });
    if show_all {
        set_filter(&ctx, None);
    }
    ctx.data_mut(|data| data.insert_temp(drawn_id(), true));
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::Severity;

    use super::severity_range;

    #[test]
    fn h_ui_119_severity_ranges() {
        let entries: Vec<(Severity, String)> = [
            Severity::Error,
            Severity::Error,
            Severity::Warning,
            Severity::Info,
            Severity::Info,
        ]
        .into_iter()
        .map(|s| (s, String::new()))
        .collect();
        assert_eq!(severity_range(&entries, Severity::Error), 0..2);
        assert_eq!(severity_range(&entries, Severity::Warning), 2..3);
        assert_eq!(severity_range(&entries, Severity::Info), 3..5);
        assert!(severity_range(&entries[2..], Severity::Error).is_empty());
        assert!(severity_range(&[], Severity::Warning).is_empty());
    }
}
