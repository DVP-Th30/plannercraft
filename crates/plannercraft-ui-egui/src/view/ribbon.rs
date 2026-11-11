//! Moteur de dessin du ruban (ADR-0017 D2, H-UI-69 à H-UI-77) : barre
//! d'accès rapide, bande d'onglets, corps des groupes de
//! [`crate::ribbon::spec::RIBBON`], repli et surimpression.
//!
//! Un seul moteur dessine les cinq onglets à partir de la table : chaque
//! élément lit son activation, son état et son action dans sa
//! [`RibbonCmd`], comme l'entrée jumelle de la barre de menus
//! (`view::chrome`, mêmes fonctions). Gabarit (H-UI-71, amendé par les
//! finitions P4) : bande d'onglets de [`TAB_BAND_H`], corps de [`BODY_H`] ;
//! grands boutons (icône 24 px, libellé sur deux lignes au plus dessous),
//! petits boutons (icône 16 px, libellé à droite, trois par colonne, deux
//! sur une rangée quand [`RibbonCmd::joins_previous`]), listes (libellé et
//! flèche), cases ; nom du groupe dessous, séparateur vertical court entre
//! groupes.
//!
//! Apparence (H-UI-132, H-UI-133) : jetons [`ChromeTokens`] du thème
//! résolu (rien d'écrit en dur ici) ; ruban sur la surface de la fenêtre,
//! filet bas ; symbole de l'application dessiné devant la barre d'accès
//! rapide ; onglets en texte seul, l'actif à l'accent souligné de 2 px ;
//! éléments à coins de 6 px, survol et appui en fond teinté, bascule
//! enfoncée en fond violet très clair et icône à l'accent, grisé opaque et
//! lisible.
//!
//! Largeur insuffisante (H-UI-75) : la disposition d'un onglet
//! ([`plan`], fonction pure des largeurs de texte) est calculée une fois
//! par onglet, langue, largeur arrondie au pixel et échelle, gardée dans la
//! mémoire d'egui ; les groupes sont réduits par rang de
//! [`GroupSpec::shrink_rank`] (petits boutons sans libellé, puis groupe
//! replié en un bouton à menu).
//!
//! Bouton de l'assistant (lot A5, H-UI-161, H-UI-166) : en haut à droite
//! de la bande d'onglets, toujours visible (ruban replié, accueil sans
//! document) ; bascule le volet latéral, enfoncé quand il est ouvert,
//! pastille des nouveautés arrivées volet fermé, pulsation discrète
//! pendant une réponse tant que le volet est fermé ([`AssistantButton`]).
//!
//! Repli (H-UI-72) : seule la bande d'onglets reste ; un clic sur un onglet
//! ouvre le corps dans une `egui::Area` au premier plan, sous la bande
//! (la vue ne bouge pas), refermée par une action du ruban, Échap ou un
//! clic ailleurs.

use std::sync::Arc;

use egui::{
    Align, Align2, Color32, FontId, Id, Key, Layout, Modifiers, Order, Pos2, Rect, RichText, Role, Sense,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use crate::action::Action;
use crate::i18n::{Arg, Language, TextId, Texts};
use crate::keymap::{Binding, shortcut_for_os, shortcut_text};
use crate::ribbon::spec::{
    CompactGroup, GroupSpec, ItemSize, ItemSpec, QUICK_ACCESS, RibbonCmd, compact_split, percent_value,
};
use crate::ribbon::{Icon, RibbonMode, RibbonOp, RibbonPrefs, RibbonTab, paint_icon};
use crate::theme::{ChromeTokens, logo_shapes};

use super::FrameCtx;
use super::chrome::{dropdown_contents, item_checked, push_cmd, twin_item};

/// Hauteur du bouton de l'assistant (lot A5 ; lot U2 : 30 points, icône et
/// libellé sur l'accent doux).
const ASSISTANT_H: f32 = 30.0;
/// Côté de l'icône du bouton de l'assistant.
const ASSISTANT_ICON: f32 = 15.0;
/// Marge horizontale du libellé du bouton de l'assistant.
const ASSISTANT_PAD: f32 = 12.0;
/// Pastille au-delà de ce nombre : « 9+ ».
const BADGE_MAX: u32 = 9;

/// État du bouton de l'assistant à l'image (lot A5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssistantButton {
    /// Volet latéral ouvert (bouton enfoncé).
    pub open: bool,
    /// Nouveautés arrivées volet fermé (pastille si > 0, volet fermé).
    pub badge: u32,
    /// Réponse en cours (pulsation, volet fermé seulement).
    pub busy: bool,
}

/// Hauteur de la barre du haut (interface adoucie, lot U2, H-UI-173) :
/// symbole, menu, accès rapide, onglets et bouton de l'assistant.
pub const TAB_BAND_H: f32 = 40.0;
/// Hauteur du ruban compact, bordure comprise (lot U2, H-UI-174).
pub const COMPACT_H: f32 = 52.0;
/// Hauteur du corps du ruban (masqué quand le ruban est replié).
pub const BODY_H: f32 = 92.0;
/// Hauteur de la zone des éléments (le nom du groupe est dessous).
const ITEMS_H: f32 = 76.0;
/// Marge haute de la zone des éléments.
const ITEMS_TOP: f32 = 2.0;
/// Hauteur d'un petit bouton, d'une case, d'une liste ou d'un pourcentage.
const SMALL_H: f32 = 22.0;
/// Pas vertical des trois emplacements d'une colonne de petits éléments.
const SLOT_PITCH: f32 = 27.0;
/// Emplacements par colonne.
const SLOTS: usize = 3;
/// Côté de l'icône d'un grand bouton (finitions P4 : 24 px).
const LARGE_ICON: f32 = 24.0;
/// Marge au-dessus de l'icône d'un grand bouton.
const LARGE_ICON_TOP: f32 = 8.0;
/// Écart entre l'icône d'un grand bouton et son libellé.
const LARGE_LABEL_GAP: f32 = 6.0;
/// Côté de l'icône d'un petit bouton.
const SMALL_ICON: f32 = 16.0;
/// Largeur minimale d'un grand bouton.
const LARGE_MIN_W: f32 = 52.0;
/// Marge intérieure horizontale d'un élément.
const PAD: f32 = 4.0;
/// Écart entre deux colonnes d'un groupe (grille de 4 px).
const COL_GAP: f32 = 4.0;
/// Écart entre deux petits éléments d'une même rangée.
const INLINE_GAP: f32 = 2.0;
/// Écart entre deux boutons de la rangée des pourcentages.
const PERCENT_GAP: f32 = 0.0;
/// Marge intérieure de part et d'autre du texte d'un pourcentage.
const PERCENT_PAD: f32 = 5.0;
/// Marge de part et d'autre du contenu d'un groupe.
const GROUP_PAD: f32 = 4.0;
/// Largeur occupée par un séparateur de groupes (espace, trait, espace) :
/// 16 px entre les contenus de deux groupes avec les marges.
const SEP_W: f32 = 8.0;
/// Marge haute et basse du trait séparateur (trait court).
const SEP_INSET: f32 = 12.0;
/// Rayon des coins d'un élément (H-UI-133).
const CORNER: f32 = 6.0;
/// Taille du texte des onglets.
const TAB_TEXT: f32 = 13.0;
/// Marge horizontale du texte d'un onglet (pastille de la barre du haut).
const TAB_PAD: f32 = 12.0;
/// Hauteur d'un onglet, d'un bouton de la barre du haut.
const BAR_ITEM: f32 = 30.0;
/// Côté des icônes de la barre du haut et du ruban compact.
const BAR_ICON: f32 = 17.0;
/// Rayon des coins des boutons de la barre du haut.
const BAR_CORNER: f32 = 7.0;
/// Marge horizontale de la barre du haut.
const BAR_PAD: f32 = 2.0;
/// Écart entre l'accès rapide et les onglets.
const TABS_GAP: f32 = 14.0;
/// Côté du symbole de l'application.
const LOGO: f32 = 26.0;
/// Marge horizontale du corps.
const BODY_PAD: f32 = 4.0;
/// Taille du texte des libellés : 11,5 points en Inter (lot U1, H-UI-169),
/// même œil que les 12 points d'Ubuntu Light d'avant (hauteur d'x d'Inter
/// plus grande) ; à 12 points, Inter (5 à 8 % plus large) faisait perdre
/// ses libellés à Propriétés à 1 280 px (H-UI-75).
const LABEL_SIZE: f32 = 11.5;
/// Taille du nom des groupes.
const GROUP_LABEL_SIZE: f32 = 11.0;
/// Hauteur d'une ligne de libellé d'un grand bouton.
const LINE_H: f32 = 14.0;
/// Largeur de la flèche d'une liste (triangle dessiné, pas un glyphe).
const ARROW_W: f32 = 7.0;
/// Côté de la case d'une case à cocher sans icône.
const BOX: f32 = 13.0;
/// Côté d'un bouton de la barre d'accès rapide (barre du haut, lot U2).
const QUICK: f32 = 30.0;
/// Largeur de fenêtre retenue au plus (règle 10 : bornage avant tout
/// calcul entier).
const MAX_WIDTH_PX: u32 = 100_000;

/// Largeur egui (`f32`) bornée en pixels entiers : non finie ou négative ⇒
/// 0 ; au plus [`MAX_WIDTH_PX`] (clé du cache de la disposition).
#[must_use]
pub fn width_key(width: f32) -> u32 {
    if !width.is_finite() || width <= 0.0 {
        return 0;
    }
    let px = crate::geom::f32_to_bounded_i64(width.round(), i64::from(MAX_WIDTH_PX));
    u32::try_from(px).unwrap_or(0)
}

/// Palier de réduction d'un groupe (H-UI-75).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Tous les libellés.
    Full,
    /// Petits boutons sans libellé (icône et info-bulle gardées).
    Compact,
    /// Groupe replié en un bouton à menu (nom et icône du groupe).
    Folded,
}

/// Forme dessinée d'un élément.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Grand bouton (icône 32 px, libellé dessous).
    Large,
    /// Grande liste (seul élément d'un groupe : icône, libellé, flèche).
    LargeDropdown,
    /// Petit bouton ou bascule à icône ; `compact` : sans libellé.
    Small {
        /// Sans libellé.
        compact: bool,
    },
    /// Petite liste ; `compact` : icône et flèche seulement.
    Dropdown {
        /// Sans libellé.
        compact: bool,
    },
    /// Case à cocher sans icône (libellé toujours affiché).
    CheckBox,
    /// Bouton texte de la rangée des pourcentages.
    Percent,
    /// Bouton à icône seule de la barre d'accès rapide.
    Quick,
    /// Bouton du ruban compact (lot U2) : icône, libellé si `text` ;
    /// `primary` : sur l'accent doux.
    CompactButton {
        /// Libellé affiché.
        text: bool,
        /// Bouton principal.
        primary: bool,
    },
    /// Liste du ruban compact : icône, libellé si `text`, flèche.
    CompactDropdown {
        /// Libellé affiché.
        text: bool,
    },
    /// Pourcentage du groupe segmenté du ruban compact.
    CompactPercent,
    /// Bouton « … » d'un groupe compact.
    More,
}

/// Élément placé dans un groupe : rectangle relatif au coin haut gauche du
/// groupe (zone des éléments), forme et lignes du libellé.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// Indice de l'élément dans `GroupSpec::items`.
    pub index: usize,
    /// Rectangle relatif.
    pub rect: Rect,
    /// Forme.
    pub kind: Kind,
    /// Lignes du libellé (une ou deux pour un grand bouton, une sinon).
    pub lines: Vec<String>,
}

/// Disposition d'un groupe.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPlan {
    /// Palier retenu.
    pub stage: Stage,
    /// Largeur du groupe (marges comprises).
    pub width: f32,
    /// Éléments placés (palier `Full` ou `Compact`) ; vide si replié.
    pub items: Vec<Placed>,
    /// Lignes du nom du bouton du groupe replié.
    pub folded: Vec<String>,
}

/// Disposition d'un onglet : un plan par groupe, dans l'ordre.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabPlan {
    /// Groupes.
    pub groups: Vec<GroupPlan>,
    /// Largeur totale (séparateurs compris).
    pub width: f32,
}

/// Libellé affiché d'un élément (pourcentages : « 50 % »).
fn item_label(texts: &Texts, item: &ItemSpec) -> String {
    match item.cmd {
        RibbonCmd::MarkPercent(step) => {
            texts.get_args(item.label, &[("percent", Arg::Count(percent_value(step)))])
        }
        _ => texts.get(item.label),
    }
}

/// Phrase d'aide d'un élément (H-UI-73).
fn item_tip(texts: &Texts, item: &ItemSpec) -> String {
    match item.cmd {
        RibbonCmd::MarkPercent(step) => {
            texts.get_args(item.tip, &[("percent", Arg::Count(percent_value(step)))])
        }
        _ => texts.get(item.tip),
    }
}

/// Coupe un libellé de grand bouton en deux lignes au plus, à l'espace qui
/// minimise la plus longue ligne (une ligne si elle tient dans la largeur
/// minimale ou s'il n'y a pas d'espace).
fn split_label(text: &str, measure: &mut dyn FnMut(&str, f32) -> f32) -> Vec<String> {
    let whole = measure(text, LABEL_SIZE);
    if whole <= LARGE_MIN_W - 2.0 * PAD {
        return vec![text.to_owned()];
    }
    let mut best: Option<(f32, usize)> = None;
    for (at, ch) in text.char_indices() {
        if ch != ' ' && ch != '\u{a0}' {
            continue;
        }
        let (head, tail) = text.split_at(at);
        let tail = tail.trim_start_matches([' ', '\u{a0}']);
        if head.is_empty() || tail.is_empty() {
            continue;
        }
        let worst = measure(head, LABEL_SIZE).max(measure(tail, LABEL_SIZE));
        if best.is_none_or(|(w, _)| worst < w) {
            best = Some((worst, at));
        }
    }
    match best {
        Some((worst, at)) if worst < whole => {
            let (head, tail) = text.split_at(at);
            vec![
                head.to_owned(),
                tail.trim_start_matches([' ', '\u{a0}']).to_owned(),
            ]
        }
        _ => vec![text.to_owned()],
    }
}

/// Largeur d'un grand bouton (flèche de liste sur la dernière ligne).
fn large_width(lines: &[String], arrow: bool, measure: &mut dyn FnMut(&str, f32) -> f32) -> f32 {
    let count = lines.len();
    let widest = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let extra = if arrow && i.saturating_add(1) == count {
                3.0 + ARROW_W
            } else {
                0.0
            };
            measure(line, LABEL_SIZE) + extra
        })
        .fold(0.0_f32, f32::max);
    (widest + 2.0 * PAD).max(LARGE_MIN_W)
}

/// Forme d'un élément au palier `stage` ; `alone` : seul élément du groupe.
fn kind_of(item: &ItemSpec, stage: Stage, alone: bool) -> Kind {
    // Groupe Texte (finitions P2) : icônes seules à tous les paliers.
    let compact = stage != Stage::Full || item.cmd.icon_only();
    match item.size {
        ItemSize::Large => Kind::Large,
        ItemSize::Dropdown if alone => Kind::LargeDropdown,
        ItemSize::Dropdown => Kind::Dropdown { compact },
        ItemSize::PercentRow => Kind::Percent,
        ItemSize::Check if item.icon.is_none() => Kind::CheckBox,
        ItemSize::Small | ItemSize::Check => Kind::Small { compact },
    }
}

/// Largeur d'un élément d'un emplacement (petit, liste, case, pourcentage).
fn slot_width(kind: Kind, label: &str, measure: &mut dyn FnMut(&str, f32) -> f32) -> f32 {
    match kind {
        Kind::Small { compact: true } => PAD + SMALL_ICON + PAD,
        Kind::Small { compact: false } => PAD + SMALL_ICON + PAD + measure(label, LABEL_SIZE) + 6.0,
        Kind::Dropdown { compact: true } => PAD + SMALL_ICON + 3.0 + ARROW_W + PAD,
        Kind::Dropdown { compact: false } => {
            PAD + SMALL_ICON + PAD + measure(label, LABEL_SIZE) + 4.0 + ARROW_W + PAD
        }
        Kind::CheckBox => PAD + BOX + 5.0 + measure(label, LABEL_SIZE) + 6.0,
        Kind::Percent => measure(label, LABEL_SIZE) + 2.0 * PERCENT_PAD,
        Kind::Quick => QUICK,
        Kind::Large | Kind::LargeDropdown => LARGE_MIN_W,
        Kind::CompactButton { .. } | Kind::CompactDropdown { .. } | Kind::CompactPercent | Kind::More => {
            compact_width(kind, label, measure)
        }
    }
}

/// Ordonnée relative de l'emplacement `slot` d'une colonne.
fn slot_y(slot: usize) -> f32 {
    match slot {
        0 => 0.0,
        1 => SLOT_PITCH,
        _ => 2.0 * SLOT_PITCH,
    }
}

/// Colonne de petits éléments en cours de remplissage.
#[derive(Default)]
struct Column {
    x: f32,
    width: f32,
    slots: usize,
}

/// Disposition d'un groupe au palier `stage`.
#[allow(
    clippy::too_many_lines,
    reason = "grands boutons, colonnes, rangée des pourcentages et rangées partagées, dans l'ordre"
)]
fn group_plan(
    group: &GroupSpec,
    stage: Stage,
    texts: &Texts,
    measure: &mut dyn FnMut(&str, f32) -> f32,
) -> GroupPlan {
    let name = texts.get(group.label);
    if stage == Stage::Folded {
        let folded = split_label(&name, measure);
        let width = large_width(&folded, true, measure) + 2.0 * GROUP_PAD;
        return GroupPlan {
            stage,
            width,
            items: Vec::new(),
            folded,
        };
    }
    let alone = group.items.len() == 1;
    let mut items = Vec::with_capacity(group.items.len());
    let mut x = 0.0_f32;
    let mut col: Option<Column> = None;
    let close = |col: &mut Option<Column>, x: &mut f32| {
        if let Some(c) = col.take() {
            *x = c.x + c.width + COL_GAP;
        }
    };
    let mut index = 0;
    while let Some(item) = group.items.get(index) {
        let kind = kind_of(item, stage, alone);
        let label = item_label(texts, item);
        match kind {
            Kind::Large | Kind::LargeDropdown => {
                close(&mut col, &mut x);
                let lines = split_label(&label, measure);
                let w = large_width(&lines, kind == Kind::LargeDropdown, measure);
                items.push(Placed {
                    index,
                    rect: Rect::from_min_size(pos2(x, 0.0), vec2(w, ITEMS_H)),
                    kind,
                    lines,
                });
                x += w + COL_GAP;
                index = index.saturating_add(1);
            }
            _ => {
                if col.as_ref().is_some_and(|c| c.slots >= SLOTS) {
                    close(&mut col, &mut x);
                }
                let c = col.get_or_insert_with(|| Column {
                    x,
                    ..Column::default()
                });
                let y = slot_y(c.slots);
                if kind == Kind::Percent {
                    // Rangée des pourcentages : un seul emplacement.
                    let mut px = c.x;
                    while let Some(p) = group.items.get(index).filter(|p| p.size == ItemSize::PercentRow) {
                        let text = item_label(texts, p);
                        let w = slot_width(Kind::Percent, &text, measure);
                        items.push(Placed {
                            index,
                            rect: Rect::from_min_size(pos2(px, y), vec2(w, SMALL_H)),
                            kind: Kind::Percent,
                            lines: vec![text],
                        });
                        px += w + PERCENT_GAP;
                        index = index.saturating_add(1);
                    }
                    c.width = c.width.max(px - PERCENT_GAP - c.x);
                } else {
                    let w = slot_width(kind, &label, measure);
                    items.push(Placed {
                        index,
                        rect: Rect::from_min_size(pos2(c.x, y), vec2(w, SMALL_H)),
                        kind,
                        lines: vec![label],
                    });
                    c.width = c.width.max(w);
                    index = index.saturating_add(1);
                    // Finitions P4 (H-UI-133) : petits éléments posés sur la
                    // même rangée (Lier | Délier, Indenter | Désindenter).
                    let mut px = c.x + w;
                    while let Some(next) = group.items.get(index).filter(|n| {
                        n.cmd.joins_previous() && !matches!(n.size, ItemSize::Large | ItemSize::PercentRow)
                    }) {
                        let kind = kind_of(next, stage, alone);
                        let label = item_label(texts, next);
                        let w = slot_width(kind, &label, measure);
                        px += INLINE_GAP;
                        items.push(Placed {
                            index,
                            rect: Rect::from_min_size(pos2(px, y), vec2(w, SMALL_H)),
                            kind,
                            lines: vec![label],
                        });
                        px += w;
                        index = index.saturating_add(1);
                    }
                    c.width = c.width.max(px - c.x);
                }
                c.slots = c.slots.saturating_add(1);
            }
        }
    }
    close(&mut col, &mut x);
    let content = (x - COL_GAP).max(0.0);
    let inner = content.max(measure(&name, GROUP_LABEL_SIZE));
    // Contenu centré quand le nom du groupe est plus large.
    let shift = GROUP_PAD + (inner - content) / 2.0;
    for placed in &mut items {
        placed.rect = placed.rect.translate(vec2(shift, 0.0));
    }
    GroupPlan {
        stage,
        width: inner + 2.0 * GROUP_PAD,
        items,
        folded: Vec::new(),
    }
}

/// Paliers des groupes pour tenir dans `available` (H-UI-75), par rang de
/// réduction croissant (de droite à gauche dans la table), avec un rang
/// d'écart entre les deux paliers : le groupe de rang k est replié juste
/// après que celui de rang k + 1 a perdu ses libellés de petits boutons ;
/// on s'arrête dès que la largeur tient. Les groupes de rang élevé
/// (Planification, Insérer…) gardent ainsi leurs libellés tant qu'un
/// groupe de rang plus faible peut encore être replié.
#[must_use]
pub fn choose_stages(ranks: &[u8], widths: &[[f32; 3]], available: f32) -> Vec<Stage> {
    choose_stages_with(ranks, widths, available, SEP_W)
}

/// [`choose_stages`] avec des séparateurs de `sep` points (ruban compact :
/// filet d'un point).
#[must_use]
pub fn choose_stages_with(ranks: &[u8], widths: &[[f32; 3]], available: f32, sep: f32) -> Vec<Stage> {
    let mut stages = vec![Stage::Full; widths.len()];
    let total = |stages: &[Stage]| -> f32 {
        let groups: f32 = stages
            .iter()
            .zip(widths)
            .map(|(stage, w)| match stage {
                Stage::Full => w[0],
                Stage::Compact => w[1],
                Stage::Folded => w[2],
            })
            .sum();
        let count = u16::try_from(stages.len()).unwrap_or(u16::MAX);
        groups + sep * f32::from(count.saturating_sub(1))
    };
    let mut order: Vec<usize> = (0..widths.len()).collect();
    order.sort_by_key(|&i| (ranks.get(i).copied().unwrap_or(u8::MAX), i));
    // Pas : (r1, compact), (r2, compact), (r1, replié), (r3, compact),
    // (r2, replié)… : un groupe n'est replié qu'après que le suivant a
    // perdu ses libellés de petits boutons.
    let mut steps: Vec<(usize, Stage)> = Vec::with_capacity(order.len().saturating_mul(2));
    let mut previous = None;
    for &i in &order {
        steps.push((i, Stage::Compact));
        if let Some(p) = previous.replace(i) {
            steps.push((p, Stage::Folded));
        }
    }
    if let Some(p) = previous {
        steps.push((p, Stage::Folded));
    }
    let width_at = |i: usize, stage: Stage| {
        widths.get(i).map_or(0.0, |w| match stage {
            Stage::Full => w[0],
            Stage::Compact => w[1],
            Stage::Folded => w[2],
        })
    };
    for (i, target) in steps {
        if total(&stages) <= available {
            return stages;
        }
        // Finitions P4 : un pas qui n'étroitise pas le groupe (groupe
        // d'icônes seules, bouton replié plus large que le groupe compact)
        // est sauté : la réduction reste progressive.
        if let Some(stage) = stages.get_mut(i)
            && width_at(i, target) < width_at(i, *stage)
        {
            *stage = target;
        }
    }
    stages
}

/// Disposition d'un onglet dans `available` points (fonction pure des
/// largeurs de texte données par `measure(texte, taille)`).
#[must_use]
pub fn plan(
    tab: RibbonTab,
    texts: &Texts,
    available: f32,
    measure: &mut dyn FnMut(&str, f32) -> f32,
) -> TabPlan {
    let groups = tab.groups();
    let widths: Vec<[f32; 3]> = groups
        .iter()
        .map(|g| [Stage::Full, Stage::Compact, Stage::Folded].map(|s| group_plan(g, s, texts, measure).width))
        .collect();
    let ranks: Vec<u8> = groups.iter().map(|g| g.shrink_rank).collect();
    let stages = choose_stages(&ranks, &widths, available);
    let plans: Vec<GroupPlan> = groups
        .iter()
        .zip(stages)
        .map(|(g, s)| group_plan(g, s, texts, measure))
        .collect();
    let count = u16::try_from(plans.len()).unwrap_or(u16::MAX);
    let width = plans.iter().map(|p| p.width).sum::<f32>() + SEP_W * f32::from(count.saturating_sub(1));
    TabPlan { groups: plans, width }
}

/// Clé du cache de disposition (H-UI-75).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlanKey {
    tab: RibbonTab,
    language: Language,
    width: u32,
    ppp: u32,
}

/// Cache des dispositions (au plus [`PLAN_CACHE`] entrées, la plus
/// récente en tête).
#[derive(Clone, Default)]
struct PlanCache(Vec<(PlanKey, Arc<TabPlan>)>);

/// Entrées du cache : cinq onglets dans deux langues.
const PLAN_CACHE: usize = 10;

/// Disposition de `tab` pour la largeur du corps, depuis le cache de la
/// mémoire d'egui ou calculée (galleys d'egui).
fn cached_plan(ui: &Ui, texts: &Texts, tab: RibbonTab, available: f32) -> Arc<TabPlan> {
    let key = PlanKey {
        tab,
        language: texts.language(),
        width: width_key(available),
        ppp: ui.ctx().pixels_per_point().to_bits(),
    };
    let id = Id::unique("plannercraft.ribbon.plans");
    if let Some(found) = ui.ctx().data(|data| {
        data.get_temp::<PlanCache>(id).and_then(|cache| {
            cache
                .0
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, p)| Arc::clone(p))
        })
    }) {
        return found;
    }
    let painter = ui.painter();
    let mut measure = |text: &str, size: f32| {
        painter
            .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::PLACEHOLDER)
            .size()
            .x
    };
    let fresh = Arc::new(plan(tab, texts, available, &mut measure));
    ui.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PlanCache>(id);
        cache.0.insert(0, (key, Arc::clone(&fresh)));
        cache.0.truncate(PLAN_CACHE);
    });
    fresh
}

/// Élément dessiné à l'image (relevé pour les tests : géométrie,
/// débordement, chevauchement).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnItem {
    /// Nom accessible.
    pub label: String,
    /// Rectangle à l'écran.
    pub rect: Rect,
    /// Commande (`None` : onglet ou groupe replié).
    pub cmd: Option<RibbonCmd>,
    /// Forme (`None` : onglet ou groupe replié).
    pub kind: Option<Kind>,
}

/// Relevé du ruban de la dernière image.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonDrawn {
    /// Rectangle du panneau du ruban (barre du haut, carte du ruban et
    /// gouttières ; lot U2).
    pub panel: Rect,
    /// Barre du haut (lot U2, H-UI-173).
    pub bar: Rect,
    /// Rectangle du corps (carte du ruban, ou surimpression si le ruban est
    /// replié).
    pub body: Option<Rect>,
    /// Forme du ruban dessinée (lot U2, H-UI-174).
    pub mode: RibbonMode,
    /// Éléments du corps et de la barre d'accès rapide.
    pub items: Vec<DrawnItem>,
    /// Groupes : nom, palier, rectangle.
    pub groups: Vec<(String, Stage, Rect)>,
    /// Bouton « Menu » de la barre du haut (lot U2, H-UI-173).
    pub menu: Option<Rect>,
    /// Chevron compact ⇄ complet (lot U2, H-UI-174).
    pub chevron: Option<Rect>,
    /// Bouton de l'assistant (lot A5) et sa pastille, si dessinée.
    pub assistant: Option<(Rect, Option<Rect>)>,
    /// Contour « réponse en cours » dessiné autour du bouton (H-UI-166).
    pub assistant_ring: bool,
    /// Champ « Rechercher une commande… » (lot U3, H-UI-177), s'il est
    /// dessiné.
    pub palette: Option<Rect>,
}

impl Default for RibbonDrawn {
    fn default() -> Self {
        Self {
            panel: Rect::NOTHING,
            bar: Rect::NOTHING,
            body: None,
            mode: RibbonMode::default(),
            items: Vec::new(),
            groups: Vec::new(),
            menu: None,
            chevron: None,
            assistant: None,
            assistant_ring: false,
            palette: None,
        }
    }
}

/// Clé du relevé dans la mémoire d'egui.
fn drawn_id() -> Id {
    Id::unique("plannercraft.ribbon.drawn")
}

/// Relevé du ruban de la dernière image (vide s'il n'est pas dessiné).
#[must_use]
pub fn drawn(ctx: &egui::Context) -> RibbonDrawn {
    ctx.data(|data| data.get_temp::<RibbonDrawn>(drawn_id()))
        .unwrap_or_default()
}

/// Rectangles gardés d'une image à l'autre (bande d'onglets et
/// surimpression) : un clic hors d'eux referme la surimpression.
fn keep_id() -> Id {
    Id::unique("plannercraft.ribbon.keep")
}

/// Hauteur du panneau du ruban pour `mode`, marge extérieure `outer`
/// comprise (lot U2) : marge, barre du haut, gouttière, carte (compacte
/// ou complète, aucune si repliée) et gouttière sous la carte.
#[must_use]
pub fn panel_height(mode: RibbonMode, outer: f32) -> f32 {
    let gutter = super::cards::GUTTER;
    let bar = outer + TAB_BAND_H + gutter;
    match mode {
        RibbonMode::Collapsed => bar,
        RibbonMode::Compact => bar + COMPACT_H + gutter,
        RibbonMode::Full => bar + BODY_H + 2.0 + gutter,
    }
}

/// Dessine la barre du haut et le ruban (interface adoucie, lot U2,
/// H-UI-173, H-UI-174). `prefs` : onglet et forme persistés ; `overlay` :
/// onglet ouvert en surimpression (ruban replié) ; `assistant` : état du
/// bouton de l'assistant (lot A5).
pub fn show(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    prefs: &RibbonPrefs,
    overlay: Option<RibbonTab>,
    assistant: AssistantButton,
) {
    let ctx = ui.ctx().clone();
    ctx.data_mut(|data| data.insert_temp(drawn_id(), RibbonDrawn::default()));
    if overlay.is_some() {
        close_overlay_on_escape_or_outside(ui, fc);
    }
    let tokens = ChromeTokens::for_theme(ctx.theme());
    let outer = super::cards::outer_margin(&ctx);
    let gutter = super::cards::GUTTER;
    let height = panel_height(prefs.mode, outer);
    // Barre du haut et ruban sur le fond de l'application (peint par
    // `App::show`) ; la carte du ruban est dessinée ici.
    let frame = egui::Frame::NONE.inner_margin(0.0);
    let bar = egui::Panel::top("plannercraft_ribbon")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(height)
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            record(&ctx, |d| {
                d.panel = rect;
                d.mode = prefs.mode;
            });
            let bar = Rect::from_min_max(
                pos2(rect.left() + outer, rect.top() + outer),
                pos2(rect.right() - outer, rect.top() + outer + TAB_BAND_H),
            );
            record(&ctx, |d| d.bar = bar);
            let tabs_end = top_bar(ui, fc, bar, *prefs, overlay);
            let right = assistant_button(ui, fc, bar, tabs_end, assistant).map_or(bar.right(), |r| r.left());
            // Lot U3 (H-UI-177) : champ de la palette, dans la place que le
            // bouton de l'assistant laisse.
            let field = super::palette::top_field(ui, fc, bar, tabs_end, right);
            record(&ctx, |d| d.palette = field);
            let card_h = match prefs.mode {
                RibbonMode::Collapsed => None,
                RibbonMode::Compact => Some(COMPACT_H),
                RibbonMode::Full => Some(BODY_H + 2.0),
            };
            if let Some(card_h) = card_h {
                let card =
                    Rect::from_min_size(pos2(bar.left(), bar.bottom() + gutter), vec2(bar.width(), card_h));
                paint_card(ui.painter(), &tokens, card);
                let inner = super::cards::inner(card);
                if prefs.mode == RibbonMode::Full {
                    let body =
                        Rect::from_min_max(inner.min, pos2(inner.right() - CHEVRON_ROOM, inner.bottom()));
                    body_ui(ui, fc, body, prefs.tab);
                } else {
                    compact_ui(ui, fc, inner, prefs.tab);
                }
                record(&ctx, |d| d.body = Some(card));
                chevron(ui, fc, inner, prefs.mode);
            }
            bar
        })
        .inner;
    let mut keep = vec![bar];
    if prefs.collapsed()
        && let Some(tab) = overlay
    {
        let width = bar.width();
        let area = egui::Area::new(Id::unique("plannercraft_ribbon_overlay"))
            .order(Order::Foreground)
            .fixed_pos(pos2(bar.left(), bar.bottom() + gutter))
            .show(&ctx, |ui| {
                let (card, _) = ui.allocate_exact_size(vec2(width, COMPACT_H), Sense::hover());
                ui.painter().add(
                    ui.style()
                        .visuals
                        .popup_shadow
                        .as_shape(card, super::cards::RADIUS),
                );
                paint_card(ui.painter(), &tokens, card);
                let before = fc.actions.len();
                compact_ui(ui, fc, super::cards::inner(card), tab);
                record(ui.ctx(), |d| d.body = Some(card));
                // Action du ruban ⇒ surimpression refermée (H-UI-72).
                if fc.actions.len() > before {
                    fc.actions.push(Action::Ribbon(RibbonOp::CloseOverlay));
                }
            });
        keep.push(area.response.rect);
    }
    ctx.data_mut(|data| data.insert_temp(keep_id(), keep));
}

/// Carte du ruban (lot U2, H-UI-172) : surface, coins de 10 points,
/// bordure d'un point ; son contenu ne déborde jamais (pas de masque des
/// coins).
fn paint_card(painter: &egui::Painter, tokens: &ChromeTokens, card: Rect) {
    if !card.is_finite() {
        return;
    }
    painter.rect(
        card,
        super::cards::RADIUS,
        tokens.surface,
        Stroke::new(1.0, tokens.rule),
        StrokeKind::Inside,
    );
}

/// Surimpression ouverte : Échap ou un clic hors de la bande d'onglets et
/// du corps la referment (pas pendant qu'une liste du ruban est ouverte).
fn close_overlay_on_escape_or_outside(ui: &Ui, fc: &mut FrameCtx<'_>) {
    let escape = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    let keep: Vec<Rect> = ui
        .ctx()
        .data(|data| data.get_temp::<Vec<Rect>>(keep_id()))
        .unwrap_or_default();
    let outside = ui.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .press_origin()
                .is_some_and(|p| !keep.iter().any(|r| r.contains(p)))
    }) && !egui::Popup::is_any_open(ui.ctx());
    if escape || outside {
        fc.actions.push(Action::Ribbon(RibbonOp::CloseOverlay));
    }
}

/// Modifie le relevé de l'image.
fn record(ctx: &egui::Context, f: impl FnOnce(&mut RibbonDrawn)) {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<RibbonDrawn>(drawn_id())));
}

/// Barre du haut (lot U2, H-UI-173) : symbole de l'application, bouton
/// « Menu » (menus Fichier à Aide en sous-menus), accès rapide (H-UI-76),
/// puis les cinq onglets (H-UI-69) en pastilles ; double-clic sur un
/// onglet ⇒ repli (H-UI-72). Rend l'abscisse de fin des onglets.
fn top_bar(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    bar: Rect,
    prefs: RibbonPrefs,
    overlay: Option<RibbonTab>,
) -> f32 {
    let texts = fc.texts;
    let tokens = ChromeTokens::for_theme(ui.ctx().theme());
    let builder = UiBuilder::new()
        .max_rect(bar.shrink2(vec2(BAR_PAD, 0.0)))
        .layout(Layout::left_to_right(Align::Center));
    ui.scope_builder(builder, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Symbole de l'application (H-UI-132), dessiné, décoratif.
        let (logo, _) = ui.allocate_exact_size(Vec2::splat(LOGO), Sense::hover());
        ui.painter().extend(logo_shapes(logo));
        if tokens.logo_ring != tokens.surface {
            let radius = LOGO * 185.0 / 824.0;
            ui.painter().rect_stroke(
                logo,
                radius,
                Stroke::new(1.0, tokens.logo_ring),
                StrokeKind::Outside,
            );
        }
        ui.add_space(6.0);
        menu_button(ui, fc, &tokens);
        ui.add_space(2.0);
        for (i, item) in QUICK_ACCESS.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(QUICK), Sense::hover());
            let lines = [texts.get(item.label)];
            draw_item(
                ui,
                fc,
                item,
                rect,
                Kind::Quick,
                &lines,
                Id::unique(("plannercraft.ribbon.quick", i)),
            );
            ui.add_space(2.0);
        }
        ui.add_space(TABS_GAP - 2.0);
        for tab in RibbonTab::ALL {
            let selected = if prefs.collapsed() {
                overlay == Some(tab)
            } else {
                prefs.tab == tab
            };
            let label = texts.get(tab.label());
            let response = tab_button(ui, &tokens, &label, selected);
            response.widget_info(|| WidgetInfo::selected(Role::Tab, true, selected, &label));
            let op = if response.double_clicked() {
                Some(RibbonOp::ToggleCollapsed)
            } else if response.clicked() {
                Some(match (prefs.collapsed(), overlay) {
                    (true, Some(open)) if open == tab => RibbonOp::CloseOverlay,
                    (true, _) => RibbonOp::OpenOverlay(tab),
                    (false, _) => RibbonOp::Select(tab),
                })
            } else {
                None
            };
            if let Some(op) = op {
                fc.actions.push(Action::Ribbon(op));
            }
            ui.add_space(2.0);
        }
        ui.cursor().left()
    })
    .inner
}

/// Bouton « Menu » de la barre du haut (lot U2, H-UI-173) : icône à trois
/// traits ; ouvre les menus Fichier à Aide (`chrome::main_menu`).
fn menu_button(ui: &mut Ui, fc: &mut FrameCtx<'_>, tokens: &ChromeTokens) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(QUICK), Sense::hover());
    let id = Id::unique("plannercraft.topbar.menu");
    let response = ui.interact(rect, id, Sense::click());
    let name = fc.texts.get(TextId::TopMenu);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let look = ItemLook::of(tokens, true, &response, false, open);
    let painter = ui.painter().clone();
    if let Some(bg) = look.bg {
        painter.rect_filled(rect, BAR_CORNER, bg);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            BAR_CORNER,
            Stroke::new(1.5, tokens.accent),
            StrokeKind::Inside,
        );
    }
    // Trois traits (icône « menu » dessinée : pas de SVG de plus).
    let icon = Rect::from_center_size(rect.center(), Vec2::splat(BAR_ICON));
    let stroke = Stroke::new(BAR_ICON * 1.8 / 24.0, tokens.text_weak);
    for y in [6.0, 12.0, 18.0] {
        let y = icon.top() + icon.height() * y / 24.0;
        painter.line_segment(
            [
                pos2(icon.left() + icon.width() * 4.0 / 24.0, y),
                pos2(icon.left() + icon.width() * 20.0 / 24.0, y),
            ],
            stroke,
        );
    }
    record(ui.ctx(), |d| d.menu = Some(rect));
    let tip = fc.texts.get(TextId::TopMenuTip);
    let response = response.on_hover_ui(|ui| tooltip(ui, &name, &tip, None));
    egui::Popup::menu(&response).show(|ui| super::chrome::main_menu(ui, fc));
}

/// Bouton de l'assistant (lot A5, H-UI-161, H-UI-166 ; lot U2 : icône et
/// libellé sur l'accent doux, 30 points de haut), au bord droit de la
/// barre `bar` (jamais sur les onglets, qui finissent à `tabs_end`) :
/// survol et appui comme les éléments du ruban, volet ouvert ⇒ fond plus
/// soutenu ; pastille à l'accent (nombre au-delà d'une nouveauté, « 9+ »
/// au-delà de neuf) si le volet est fermé ; pendant une réponse, volet
/// fermé, contour à l'accent qui pulse (fixe si « Réduire les
/// animations ») : volet ouvert, la réponse s'écrit sous les yeux et le
/// bouton reste fixe. Clic ⇒ `SideOp::Toggle`. Rend le rectangle du bouton.
#[allow(
    clippy::too_many_lines,
    reason = "bouton, pastille et pulsation dans l'ordre du dessin (lot A5)"
)]
fn assistant_button(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    bar: Rect,
    tabs_end: f32,
    b: AssistantButton,
) -> Option<Rect> {
    if !bar.is_finite() {
        return None;
    }
    let ctx = ui.ctx().clone();
    let tokens = ChromeTokens::for_theme(ctx.theme());
    let texts = fc.texts;
    let name = texts.get(TextId::AssistantButton);
    let galley = ui.painter().layout_no_wrap(
        name.clone(),
        crate::theme::strong_font(&ctx, TAB_TEXT),
        tokens.soft_text,
    );
    let full_w = ASSISTANT_PAD + ASSISTANT_ICON + 6.0 + galley.size().x + ASSISTANT_PAD;
    // Place comptée : libellé si la barre le permet, sinon icône seule.
    let room = bar.right() - BAR_PAD - tabs_end - 8.0;
    let with_text = room >= full_w;
    let width = if with_text { full_w } else { BAR_ITEM };
    let rect = Rect::from_min_size(
        pos2(bar.right() - BAR_PAD - width, bar.center().y - ASSISTANT_H / 2.0),
        vec2(width, ASSISTANT_H),
    );
    let response = ui.interact(rect, Id::unique("plannercraft.ribbon.assistant"), Sense::click());
    let badge = if b.open { 0 } else { b.badge };
    let news = (badge > 0).then(|| {
        texts.get_args(
            TextId::AssistantButtonBadge,
            &[("count", Arg::Count(i64::from(badge)))],
        )
    });
    let label = match &news {
        Some(news) => format!("{name} ({news})"),
        None => name.clone(),
    };
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, b.open, &label));
    let down = response.is_pointer_button_down_on();
    let bg = if down || b.open {
        tokens.selection_bg
    } else if response.hovered() {
        tokens.pressed
    } else {
        tokens.toggle_bg
    };
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 8.0, bg);
    if response.has_focus() {
        painter.rect_stroke(rect, 8.0, Stroke::new(1.5, tokens.accent), StrokeKind::Inside);
    }
    let icon_x = if with_text {
        rect.left() + ASSISTANT_PAD
    } else {
        rect.center().x - ASSISTANT_ICON / 2.0
    };
    let icon = Rect::from_min_size(
        pos2(icon_x, rect.center().y - ASSISTANT_ICON / 2.0),
        Vec2::splat(ASSISTANT_ICON),
    );
    paint_icon(&painter, icon, Icon::Sparkles, tokens.soft_text);
    if with_text {
        let size = galley.size();
        painter.galley(
            pos2(icon.right() + 6.0, rect.center().y - size.y / 2.0),
            galley,
            tokens.soft_text,
        );
    }
    let ring = b.busy && !b.open;
    if ring {
        let reduce = fc.state.reduce_motion;
        let alpha = if reduce {
            0.7
        } else {
            let t = ctx.input(|i| i.time);
            // Une pulsation toutes les 1,6 s environ, entre 25 et 85 %.
            #[allow(clippy::cast_possible_truncation, reason = "phase d'une animation")]
            let phase = ((t * 4.0).sin() * 0.5 + 0.5) as f32;
            0.25 + 0.6 * phase
        };
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.5, tokens.accent.gamma_multiply(alpha)),
            StrokeKind::Inside,
        );
        if !reduce {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
    let dot = (badge > 0).then(|| {
        let text = if badge > BADGE_MAX {
            format!("{BADGE_MAX}+")
        } else {
            badge.to_string()
        };
        let galley = painter.layout_no_wrap(text, FontId::proportional(9.0), tokens.on_accent);
        // Une nouveauté : point de 8 px ; plus : pastille de 13 px de haut
        // portant le nombre.
        let c = pos2(rect.right() - 4.0, rect.top() + 5.0);
        let disk = if badge == 1 {
            Rect::from_center_size(c, Vec2::splat(8.0))
        } else {
            Rect::from_center_size(c, vec2((galley.size().x + 6.0).max(13.0), 13.0))
        };
        painter.rect_filled(disk, disk.height() / 2.0, tokens.accent);
        if badge > 1 {
            painter.galley(
                Rect::from_center_size(c, galley.size()).min,
                galley,
                tokens.on_accent,
            );
        }
        disk
    });
    record(&ctx, |d| {
        d.assistant = Some((rect, dot));
        d.assistant_ring = ring;
    });
    let binding = crate::keymap::Binding::Ui(crate::keymap::UiBinding::ToggleAssistant);
    let title = tooltip_title(&ctx, &name, Some(binding));
    let mut tip = texts.get(TextId::AssistantButtonTip);
    if b.busy {
        tip = format!("{tip}\n{}", texts.get(TextId::AssistantButtonBusy));
    }
    let response = response.on_hover_ui(|ui| tooltip(ui, &title, &tip, news.as_deref()));
    if response.clicked() {
        fc.actions
            .push(Action::SidePanel(crate::view::side_panel::SideOp::Toggle));
    }
    Some(rect)
}

/// Onglet de la barre du haut (lot U2, H-UI-173) : pastille de 30 points ;
/// actif sur la surface, texte appuyé ; les autres en texte atténué, fond
/// au survol ; focus clavier : contour à l'accent.
fn tab_button(ui: &mut Ui, tokens: &ChromeTokens, label: &str, selected: bool) -> egui::Response {
    let (font, color) = if selected {
        (crate::theme::strong_font(ui.ctx(), TAB_TEXT), tokens.text)
    } else {
        (FontId::proportional(TAB_TEXT), tokens.text_weak)
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let width = galley.size().x + 2.0 * TAB_PAD;
    let (pill, response) = ui.allocate_exact_size(vec2(width, BAR_ITEM), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(pill, BAR_CORNER, tokens.surface);
    } else if response.hovered() {
        painter.rect_filled(pill, BAR_CORNER, tokens.hover);
    }
    if response.has_focus() {
        painter.rect_stroke(
            pill,
            BAR_CORNER,
            Stroke::new(1.5, tokens.accent),
            StrokeKind::Inside,
        );
    }
    let size = galley.size();
    painter.galley(
        pos2(pill.left() + TAB_PAD, pill.center().y - size.y / 2.0),
        galley,
        color,
    );
    response
}

/// Place réservée au chevron à droite de la carte du ruban (lot U2).
const CHEVRON_ROOM: f32 = 36.0;
/// Côté du chevron.
const CHEVRON: f32 = 28.0;
/// Hauteur d'un élément du ruban compact.
const COMPACT_ITEM_H: f32 = 30.0;
/// Marge horizontale du libellé d'un élément compact.
const COMPACT_PAD: f32 = 9.0;
/// Écart entre l'icône et le libellé d'un élément compact.
const COMPACT_ICON_GAP: f32 = 6.0;
/// Écart entre deux éléments compacts.
const COMPACT_GAP: f32 = 2.0;
/// Marge de part et d'autre du contenu d'un groupe compact.
const COMPACT_GROUP_PAD: f32 = 9.0;
/// Marge intérieure de la carte du ruban compact.
const COMPACT_CARD_PAD: f32 = 6.0;
/// Marge intérieure du groupe segmenté (pourcentages).
const SEGMENT_PAD: f32 = 2.0;
/// Marge de part et d'autre du texte d'un pourcentage compact.
const COMPACT_PERCENT_PAD: f32 = 8.0;
/// Largeur du bouton « … » d'un groupe compact.
const MORE_W: f32 = 24.0;
/// Largeur d'un séparateur de groupes compacts (filet d'un point).
const COMPACT_SEP_W: f32 = 1.0;

/// Élément placé d'un groupe compact (rectangle relatif au bord gauche du
/// groupe, ordonnée relative au haut de la rangée).
#[derive(Clone, Debug, PartialEq)]
pub struct CompactPlaced {
    /// Indice de l'élément dans `GroupSpec::items`.
    pub index: usize,
    /// Rectangle relatif.
    pub rect: Rect,
    /// Forme.
    pub kind: Kind,
    /// Libellé affiché (vide : icône seule).
    pub text: String,
}

/// Disposition d'un groupe compact.
#[derive(Clone, Debug, PartialEq)]
pub struct CompactGroupPlan {
    /// Palier retenu (libellés, icônes seules, groupe en un bouton).
    pub stage: Stage,
    /// Largeur du groupe (marges comprises).
    pub width: f32,
    /// Éléments visibles (vide si replié).
    pub items: Vec<CompactPlaced>,
    /// Bouton « … » (éléments hors de la rangée), s'il y en a.
    pub more: Option<Rect>,
    /// Fond du groupe segmenté (pourcentages), s'il y en a.
    pub segment: Option<Rect>,
}

/// Disposition d'un onglet compact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompactPlan {
    /// Groupes, dans l'ordre.
    pub groups: Vec<CompactGroupPlan>,
    /// Largeur totale (séparateurs compris).
    pub width: f32,
}

/// Largeur d'un bouton de groupe replié compact (icône du groupe et
/// flèche).
const FOLDED_COMPACT_W: f32 = 6.0 + BAR_ICON + 4.0 + ARROW_W + 6.0;

/// Forme compacte d'un élément visible ; `labelled` : libellé affiché.
fn compact_kind(item: &ItemSpec, labelled: bool, primary: bool) -> Kind {
    match item.size {
        ItemSize::PercentRow => Kind::CompactPercent,
        ItemSize::Check if item.icon.is_none() => Kind::CheckBox,
        ItemSize::Dropdown => Kind::CompactDropdown { text: labelled },
        ItemSize::Large | ItemSize::Small | ItemSize::Check => Kind::CompactButton {
            text: labelled,
            primary,
        },
    }
}

/// Largeur d'un élément compact de libellé `label`.
fn compact_width(kind: Kind, label: &str, measure: &mut dyn FnMut(&str, f32) -> f32) -> f32 {
    match kind {
        Kind::CompactButton { text: true, .. } => {
            COMPACT_PAD + BAR_ICON + COMPACT_ICON_GAP + measure(label, LABEL_SIZE) + COMPACT_PAD
        }
        Kind::CompactDropdown { text: false } => 6.0 + BAR_ICON + 4.0 + ARROW_W + 6.0,
        Kind::CompactDropdown { text: true } => {
            COMPACT_PAD
                + BAR_ICON
                + COMPACT_ICON_GAP
                + measure(label, LABEL_SIZE)
                + 6.0
                + ARROW_W
                + COMPACT_PAD
        }
        Kind::CheckBox => slot_width(kind, label, measure),
        Kind::CompactPercent => measure(label, LABEL_SIZE) + 2.0 * COMPACT_PERCENT_PAD,
        _ => COMPACT_ITEM_H,
    }
}

/// Disposition d'un groupe compact au palier `stage` (H-UI-174) : éléments
/// visibles de [`spec::compact_split`](crate::ribbon::spec::compact_split)
/// côte à côte (pourcentages dans un groupe segmenté), puis « … » s'il
/// reste des éléments ; replié : un bouton.
fn compact_group_plan(
    group: &'static GroupSpec,
    compact: Option<&CompactGroup>,
    stage: Stage,
    texts: &Texts,
    measure: &mut dyn FnMut(&str, f32) -> f32,
) -> CompactGroupPlan {
    if stage == Stage::Folded {
        return CompactGroupPlan {
            stage,
            width: FOLDED_COMPACT_W + 2.0 * COMPACT_GROUP_PAD,
            items: Vec::new(),
            more: None,
            segment: None,
        };
    }
    let (shown, more) = compact_split(group, compact);
    let top = (COMPACT_H - 2.0 - COMPACT_ITEM_H) / 2.0;
    let mut x = COMPACT_GROUP_PAD;
    let mut items = Vec::with_capacity(shown.len());
    let mut segment: Option<Rect> = None;
    // Début du groupe segmenté en cours (pourcentages contigus).
    let mut seg_start: Option<f32> = None;
    let mut first = true;
    for (item, c) in shown {
        let Some(index) = group.items.iter().position(|i| core::ptr::eq(i, item)) else {
            continue;
        };
        let labelled = c.text && (stage == Stage::Full || item.icon.is_none());
        let kind = compact_kind(item, labelled, c.primary);
        let text = if labelled || matches!(kind, Kind::CheckBox | Kind::CompactPercent) {
            item_label(texts, item)
        } else {
            String::new()
        };
        let w = compact_width(kind, &text, measure);
        let rect = if kind == Kind::CompactPercent {
            if seg_start.is_none() {
                if !first {
                    x += COMPACT_GAP;
                }
                seg_start = Some(x);
                x += SEGMENT_PAD;
            }
            Rect::from_min_size(
                pos2(x, top + SEGMENT_PAD),
                vec2(w, COMPACT_ITEM_H - 2.0 * SEGMENT_PAD),
            )
        } else {
            if let Some(start) = seg_start.take() {
                x += SEGMENT_PAD;
                segment = Some(Rect::from_min_max(
                    pos2(start, top),
                    pos2(x, top + COMPACT_ITEM_H),
                ));
            }
            if !first {
                x += COMPACT_GAP;
            }
            Rect::from_min_size(pos2(x, top), vec2(w, COMPACT_ITEM_H))
        };
        items.push(CompactPlaced {
            index,
            rect,
            kind,
            text,
        });
        x += w;
        first = false;
    }
    if let Some(start) = seg_start.take() {
        x += SEGMENT_PAD;
        segment = Some(Rect::from_min_max(
            pos2(start, top),
            pos2(x, top + COMPACT_ITEM_H),
        ));
    }
    let more = (!more.is_empty()).then(|| {
        if !first {
            x += COMPACT_GAP;
        }
        let r = Rect::from_min_size(pos2(x, top), vec2(MORE_W, COMPACT_ITEM_H));
        x += MORE_W;
        r
    });
    CompactGroupPlan {
        stage,
        width: x + COMPACT_GROUP_PAD,
        items,
        more,
        segment,
    }
}

/// Disposition compacte d'un onglet dans `available` points (H-UI-174 ;
/// fonction pure des largeurs de texte) : même réduction par rang que le
/// ruban complet (H-UI-75) : libellés retirés, puis groupe en un bouton.
#[must_use]
pub fn compact_plan(
    tab: RibbonTab,
    texts: &Texts,
    available: f32,
    measure: &mut dyn FnMut(&str, f32) -> f32,
) -> CompactPlan {
    let groups = tab.groups();
    let compact = tab.compact_groups();
    let widths: Vec<[f32; 3]> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            [Stage::Full, Stage::Compact, Stage::Folded]
                .map(|s| compact_group_plan(g, compact.get(i), s, texts, measure).width)
        })
        .collect();
    let ranks: Vec<u8> = groups.iter().map(|g| g.shrink_rank).collect();
    let stages = choose_stages_with(&ranks, &widths, available, COMPACT_SEP_W);
    let plans: Vec<CompactGroupPlan> = groups
        .iter()
        .enumerate()
        .zip(stages)
        .map(|((i, g), s)| compact_group_plan(g, compact.get(i), s, texts, measure))
        .collect();
    let count = u16::try_from(plans.len()).unwrap_or(u16::MAX);
    let width =
        plans.iter().map(|p| p.width).sum::<f32>() + COMPACT_SEP_W * f32::from(count.saturating_sub(1));
    CompactPlan { groups: plans, width }
}

/// Cache des dispositions compactes (comme [`PlanCache`]).
#[derive(Clone, Default)]
struct CompactCache(Vec<(PlanKey, Arc<CompactPlan>)>);

/// Disposition compacte de `tab` pour la largeur `available`, depuis le
/// cache de la mémoire d'egui ou calculée.
fn cached_compact(ui: &Ui, texts: &Texts, tab: RibbonTab, available: f32) -> Arc<CompactPlan> {
    let key = PlanKey {
        tab,
        language: texts.language(),
        width: width_key(available),
        ppp: ui.ctx().pixels_per_point().to_bits(),
    };
    let id = Id::unique("plannercraft.ribbon.compact_plans");
    if let Some(found) = ui.ctx().data(|data| {
        data.get_temp::<CompactCache>(id).and_then(|cache| {
            cache
                .0
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, p)| Arc::clone(p))
        })
    }) {
        return found;
    }
    let painter = ui.painter();
    let mut measure = |text: &str, size: f32| {
        painter
            .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::PLACEHOLDER)
            .size()
            .x
    };
    let fresh = Arc::new(compact_plan(tab, texts, available, &mut measure));
    ui.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<CompactCache>(id);
        cache.0.insert(0, (key, Arc::clone(&fresh)));
        cache.0.truncate(PLAN_CACHE);
    });
    fresh
}

/// Ruban compact de l'onglet `tab` dans `rect` (intérieur de la carte ;
/// lot U2, H-UI-174) : une rangée de 30 points, groupes séparés par un
/// filet, « … » par groupe pour le reste ; chevron à droite (dessiné par
/// l'appelant).
fn compact_ui(ui: &mut Ui, fc: &mut FrameCtx<'_>, rect: Rect, tab: RibbonTab) {
    if !rect.is_finite() || rect.width() <= 0.0 {
        return;
    }
    let left = rect.left() + COMPACT_CARD_PAD;
    let available = (rect.width() - 2.0 * COMPACT_CARD_PAD - CHEVRON_ROOM).max(0.0);
    let plan = cached_compact(ui, fc.texts, tab, available);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("plannercraft.ribbon.compact", tab)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let tokens = ChromeTokens::for_theme(child.ctx().theme());
    let compact = tab.compact_groups();
    let mut x = left;
    for (gi, (group, gp)) in tab.groups().iter().zip(&plan.groups).enumerate() {
        if gi > 0 {
            let sx = x.floor() + 0.5;
            child
                .painter()
                .vline(sx, rect.y_range(), Stroke::new(1.0, tokens.row_line));
            x += COMPACT_SEP_W;
        }
        let group_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(gp.width, rect.height()));
        let name = fc.texts.get(group.label);
        record(child.ctx(), |d| {
            d.groups.push((name.clone(), gp.stage, group_rect));
        });
        let gid = Id::unique(("plannercraft.ribbon.compact.group", tab, gi));
        let origin = vec2(x, rect.top());
        if gp.stage == Stage::Folded {
            let button = Rect::from_min_size(
                pos2(
                    x + COMPACT_GROUP_PAD,
                    rect.top() + (rect.height() - COMPACT_ITEM_H) / 2.0,
                ),
                vec2(FOLDED_COMPACT_W, COMPACT_ITEM_H),
            );
            compact_folded(&mut child, fc, group, button, gid);
        } else {
            if let Some(seg) = gp.segment {
                child
                    .painter()
                    .rect_filled(seg.translate(origin), 8.0, tokens.hover);
            }
            for placed in &gp.items {
                let Some(item) = group.items.get(placed.index) else {
                    continue;
                };
                let lines = [placed.text.clone()];
                draw_item(
                    &mut child,
                    fc,
                    item,
                    placed.rect.translate(origin),
                    placed.kind,
                    &lines,
                    gid.with(placed.index),
                );
            }
            if let Some(more) = gp.more {
                let (_, overflow) = compact_split(group, compact.get(gi));
                more_button(
                    &mut child,
                    fc,
                    &name,
                    &overflow,
                    more.translate(origin),
                    gid.with("more"),
                );
            }
        }
        x += gp.width;
    }
}

/// Bouton « … » d'un groupe compact (H-UI-174) : menu des éléments du
/// groupe absents de la rangée (mêmes commandes, listes en sous-menus).
fn more_button(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    group: &str,
    items: &[&'static ItemSpec],
    rect: Rect,
    id: Id,
) {
    let label = fc.texts.get_args(
        TextId::RibbonMore,
        &[("group", Arg::Str(group.to_owned().into()))],
    );
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let tokens = ChromeTokens::for_theme(ui.ctx().theme());
    let look = ItemLook::of(&tokens, true, &response, false, open);
    let painter = ui.painter().clone();
    paint_frame(&painter, &tokens, rect, look.bg, response.has_focus());
    paint_icon(
        &painter,
        Rect::from_center_size(rect.center(), Vec2::splat(SMALL_ICON)),
        Icon::Ellipsis,
        tokens.text_weak,
    );
    record(ui.ctx(), |d| {
        d.items.push(DrawnItem {
            label: label.clone(),
            rect,
            cmd: None,
            kind: Some(Kind::More),
        });
    });
    let response = response.on_hover_text(&label);
    egui::Popup::menu(&response).show(|ui| items_menu(ui, fc, items));
}

/// Groupe compact replié en un bouton (icône du groupe et flèche) : menu
/// de tous ses éléments.
fn compact_folded(ui: &mut Ui, fc: &mut FrameCtx<'_>, group: &'static GroupSpec, rect: Rect, id: Id) {
    let name = fc.texts.get(group.label);
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let tokens = ChromeTokens::for_theme(ui.ctx().theme());
    let look = ItemLook::of(&tokens, true, &response, false, open);
    let painter = ui.painter().clone();
    paint_frame(&painter, &tokens, rect, look.bg, response.has_focus());
    let icon = Rect::from_min_size(
        pos2(rect.left() + 6.0, rect.center().y - BAR_ICON / 2.0),
        Vec2::splat(BAR_ICON),
    );
    paint_icon(&painter, icon, group.icon, look.icon);
    arrow(
        &painter,
        pos2(icon.right() + 4.0, rect.center().y - 2.0),
        look.text,
    );
    record(ui.ctx(), |d| {
        d.items.push(DrawnItem {
            label: name.clone(),
            rect,
            cmd: None,
            kind: None,
        });
    });
    let response = response.on_hover_text(&name);
    let all: Vec<&'static ItemSpec> = group.items.iter().collect();
    egui::Popup::menu(&response).show(|ui| items_menu(ui, fc, &all));
}

/// Chevron de la carte du ruban (lot U2, H-UI-174) : bascule compact ⇄
/// complet, à droite, centré sur la rangée compacte (en haut du ruban
/// complet).
fn chevron(ui: &mut Ui, fc: &mut FrameCtx<'_>, inner: Rect, mode: RibbonMode) {
    let tokens = ChromeTokens::for_theme(ui.ctx().theme());
    let cy = inner.top() + (COMPACT_H - 2.0) / 2.0;
    let rect = Rect::from_center_size(
        pos2(inner.right() - COMPACT_CARD_PAD - CHEVRON / 2.0, cy),
        Vec2::splat(CHEVRON),
    );
    let full = mode == RibbonMode::Full;
    let label = fc.texts.get(if full {
        TextId::RibbonShowCompact
    } else {
        TextId::RibbonShowFull
    });
    let response = ui.interact(rect, Id::unique("plannercraft.ribbon.chevron"), Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, full, &label));
    let look = ItemLook::of(&tokens, true, &response, false, false);
    let painter = ui.painter().clone();
    paint_frame(&painter, &tokens, rect, look.bg, response.has_focus());
    // Chevron dessiné (vers le bas : afficher le complet ; vers le haut :
    // revenir au compact).
    let c = rect.center();
    let (dy0, dy1) = if full { (2.0, -2.0) } else { (-2.0, 2.0) };
    painter.add(Shape::line(
        vec![
            pos2(c.x - 4.5, c.y + dy0),
            pos2(c.x, c.y + dy1),
            pos2(c.x + 4.5, c.y + dy0),
        ],
        Stroke::new(1.6, tokens.text_weak),
    ));
    record(ui.ctx(), |d| d.chevron = Some(rect));
    let response = response.on_hover_text(&label);
    if response.clicked() {
        fc.actions.push(Action::Ribbon(RibbonOp::ToggleFull));
    }
}

/// Menu d'éléments du ruban (« … », groupe replié) : chaque élément en
/// entrée de menu (liste ⇒ sous-menu), libellé du ruban, commande
/// partagée.
fn items_menu(ui: &mut Ui, fc: &mut FrameCtx<'_>, items: &[&'static ItemSpec]) {
    let texts = fc.texts;
    for item in items {
        let label = item_label(texts, item);
        if item.size == ItemSize::Dropdown {
            let enabled = item.cmd.enabled(fc).yes;
            ui.add_enabled_ui(enabled, |ui| {
                ui.menu_button(label, |ui| dropdown_contents(ui, fc, item.cmd));
            });
        } else {
            let shortcut = item
                .binding
                .and_then(|b| shortcut_for_os(b, ui.ctx().os()).copied());
            twin_item(ui, fc, item.cmd, label, shortcut);
        }
    }
}

/// Corps de l'onglet `tab` dans `rect` : groupes disposés par [`plan`],
/// séparateurs, noms des groupes ; rien ne déborde de `rect` (découpe).
fn body_ui(ui: &mut Ui, fc: &mut FrameCtx<'_>, rect: Rect, tab: RibbonTab) {
    if !rect.is_finite() || rect.width() <= 0.0 {
        return;
    }
    record(ui.ctx(), |d| d.body = Some(rect));
    let available = (rect.width() - 2.0 * BODY_PAD).max(0.0);
    let plan = cached_plan(ui, fc.texts, tab, available);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("plannercraft.ribbon.body", tab)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let tokens = ChromeTokens::for_theme(child.ctx().theme());
    let line = Stroke::new(1.0, tokens.rule);
    let items_top = rect.top() + ITEMS_TOP;
    let mut x = rect.left() + BODY_PAD;
    for (gi, (group, gp)) in tab.groups().iter().zip(&plan.groups).enumerate() {
        if gi > 0 {
            // Séparateur court, centré entre deux groupes, au pixel près.
            let sx = (x + SEP_W / 2.0).floor() + 0.5;
            child
                .painter()
                .vline(sx, (rect.top() + SEP_INSET)..=(rect.bottom() - SEP_INSET), line);
            x += SEP_W;
        }
        let group_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(gp.width, BODY_H));
        let name = fc.texts.get(group.label);
        record(child.ctx(), |d| {
            d.groups.push((name.clone(), gp.stage, group_rect));
        });
        let gid = Id::unique(("plannercraft.ribbon.group", tab, gi));
        if gp.stage == Stage::Folded {
            let button = Rect::from_min_size(
                pos2(x + GROUP_PAD, items_top),
                vec2(gp.width - 2.0 * GROUP_PAD, ITEMS_H),
            );
            folded_group(&mut child, fc, group, button, &gp.folded, gid);
        } else {
            for placed in &gp.items {
                let Some(item) = group.items.get(placed.index) else {
                    continue;
                };
                let r = placed.rect.translate(vec2(x, items_top));
                draw_item(
                    &mut child,
                    fc,
                    item,
                    r,
                    placed.kind,
                    &placed.lines,
                    gid.with(placed.index),
                );
            }
            child.painter().text(
                pos2(group_rect.center().x, rect.bottom() - 8.0),
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(GROUP_LABEL_SIZE),
                tokens.text_weak,
            );
        }
        x += gp.width;
    }
}

/// Flèche de liste (triangle pointe en bas) dont le coin haut gauche est
/// `at`.
fn arrow(painter: &egui::Painter, at: Pos2, color: Color32) {
    let points = vec![
        at,
        pos2(at.x + ARROW_W, at.y),
        pos2(at.x + ARROW_W / 2.0, at.y + 4.0),
    ];
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Lignes d'un libellé centrées horizontalement sur `cx`, à partir de `top`.
fn centered_lines(
    painter: &egui::Painter,
    lines: &[String],
    cx: f32,
    top: f32,
    color: Color32,
    arrow_last: bool,
) {
    let count = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let galley = painter.layout_no_wrap(line.clone(), FontId::proportional(LABEL_SIZE), color);
        let with_arrow = arrow_last && i.saturating_add(1) == count;
        let total = galley.size().x + if with_arrow { 3.0 + ARROW_W } else { 0.0 };
        let y = top + LINE_H * f32::from(u8::try_from(i).unwrap_or(u8::MAX));
        let left = cx - total / 2.0;
        let size = galley.size();
        painter.galley(pos2(left, y), galley, color);
        if with_arrow {
            arrow(painter, pos2(left + size.x + 3.0, y + size.y / 2.0 - 2.0), color);
        }
    }
}

/// Infobulle d'un élément (H-UI-73) : libellé appuyé avec le raccourci de
/// `KEYMAP` du système, phrase d'aide, raison du grisé.
pub(super) fn tooltip(ui: &mut Ui, title: &str, tip: &str, reason: Option<&str>) {
    ui.label(RichText::new(title).strong());
    ui.label(tip);
    if let Some(reason) = reason {
        ui.label(RichText::new(reason).italics());
    }
}

/// Titre de l'infobulle : « Indenter (Alt+Shift+⏵) ».
pub(super) fn tooltip_title(ctx: &egui::Context, label: &str, binding: Option<Binding>) -> String {
    match binding.and_then(|b| shortcut_for_os(b, ctx.os())) {
        Some(shortcut) => format!("{label} ({})", shortcut_text(ctx, shortcut)),
        None => label.to_owned(),
    }
}

/// Dessine un élément dans `rect` et traite le clic : action de sa
/// commande (via `chrome::push_cmd`), ou liste déroulante.
fn draw_item(
    ui: &mut Ui,
    fc: &mut FrameCtx<'_>,
    item: &ItemSpec,
    rect: Rect,
    kind: Kind,
    lines: &[String],
    id: Id,
) {
    if !rect.is_finite() {
        return;
    }
    let cmd = item.cmd;
    let enabled = cmd.enabled(fc);
    let checked = item_checked(ui.ctx(), fc, cmd);
    let label = match kind {
        Kind::Percent | Kind::Quick => lines.first().cloned().unwrap_or_default(),
        _ => item_label(fc.texts, item),
    };
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(id));
    if !enabled.yes {
        child.disable();
    }
    let response = child.interact(rect, id, Sense::click());
    let role = match (checked, item.size) {
        (Some(_), _) => Role::CheckBox,
        (None, ItemSize::Dropdown) => Role::ComboBox,
        (None, _) => Role::Button,
    };
    response.widget_info(|| WidgetInfo::selected(role, enabled.yes, checked.unwrap_or(false), &label));
    let pressed = checked == Some(true) && kind != Kind::CheckBox;
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(child.ctx(), popup_id);
    let tokens = ChromeTokens::for_theme(child.ctx().theme());
    let mut look = ItemLook::of(&tokens, enabled.yes, &response, pressed, open);
    if enabled.yes {
        match kind {
            // Barre du haut, sur le fond de l'application (lot U2) : fonds
            // un cran plus soutenus, icône atténuée au repos.
            Kind::Quick => {
                look.bg = look.bg.map(|bg| {
                    if bg == tokens.hover {
                        tokens.pressed
                    } else {
                        tokens.control_active
                    }
                });
                look.icon = tokens.text_weak;
            }
            // Bouton principal du ruban compact : accent doux.
            Kind::CompactButton { primary: true, .. } => {
                look.bg = Some(look.bg.map_or(tokens.toggle_bg, |_| tokens.selection_bg));
                look.text = tokens.soft_text;
                look.icon = tokens.soft_text;
            }
            _ => {}
        }
    }
    // Peintre du parent : celui d'un `Ui` désactivé estompe les couleurs,
    // le grisé est déjà porté par `look` (opaque et lisible, H-UI-133).
    let painter = ui.painter().clone();
    paint_frame(&painter, &tokens, rect, look.bg, response.has_focus());
    paint_content(&painter, &tokens, item, rect, kind, lines, checked, look);
    record(child.ctx(), |d| {
        d.items.push(DrawnItem {
            label: label.clone(),
            rect,
            cmd: Some(cmd),
            kind: Some(kind),
        });
    });
    let texts = fc.texts;
    let title = tooltip_title(child.ctx(), &label, item.binding);
    let tip = item_tip(texts, item);
    let reason = enabled.reason.map(|r| texts.get(r));
    let response = response
        .on_hover_ui(|ui| tooltip(ui, &title, &tip, None))
        .on_disabled_hover_ui(|ui| tooltip(ui, &title, &tip, reason.as_deref()));
    if item.size == ItemSize::Dropdown {
        egui::Popup::menu(&response).show(|ui| dropdown_contents(ui, fc, cmd));
    } else if response.clicked() {
        push_cmd(fc, cmd);
    }
}

/// Couleurs d'un élément à l'image (H-UI-133) : fond (aucun au repos),
/// encre du libellé et de l'icône.
#[derive(Clone, Copy, Debug)]
struct ItemLook {
    /// Fond teinté (survol, appui, liste ouverte, bascule enfoncée).
    bg: Option<Color32>,
    /// Libellé, flèche, case.
    text: Color32,
    /// Icône (accent pour une bascule enfoncée).
    icon: Color32,
}

impl ItemLook {
    /// Couleurs selon l'état : grisé ⇒ encre `disabled` opaque (une
    /// bascule grisée garde son fond) ; appui ou liste ouverte ⇒
    /// `pressed` ; bascule enfoncée ⇒ `toggle_bg` et icône à l'accent ;
    /// survol ⇒ `hover`.
    fn of(t: &ChromeTokens, enabled: bool, response: &egui::Response, toggled: bool, open: bool) -> Self {
        if !enabled {
            return Self {
                bg: toggled.then_some(t.toggle_bg),
                text: t.disabled,
                icon: t.disabled,
            };
        }
        let down = response.is_pointer_button_down_on();
        let bg = if down || open {
            Some(t.pressed)
        } else if toggled {
            Some(t.toggle_bg)
        } else if response.hovered() {
            Some(t.hover)
        } else {
            None
        };
        Self {
            bg,
            text: t.text,
            icon: if toggled { t.accent } else { t.text },
        }
    }
}

/// Fond à coins de 6 px et contour de focus clavier (accent).
fn paint_frame(painter: &egui::Painter, t: &ChromeTokens, rect: Rect, bg: Option<Color32>, focus: bool) {
    if let Some(bg) = bg {
        painter.rect_filled(rect, CORNER, bg);
    }
    if focus {
        painter.rect_stroke(rect, CORNER, Stroke::new(1.5, t.accent), StrokeKind::Inside);
    }
}

/// Icône, libellé, flèche ou case d'un élément.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "dessin d'un élément : jetons, spécification, géométrie, état ; une branche par forme"
)]
fn paint_content(
    painter: &egui::Painter,
    tokens: &ChromeTokens,
    item: &ItemSpec,
    rect: Rect,
    kind: Kind,
    lines: &[String],
    checked: Option<bool>,
    look: ItemLook,
) {
    let color = look.text;
    let left_text = |x: f32, text: &str| {
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(LABEL_SIZE),
            color,
        )
    };
    let small_icon = |icon: Option<Icon>| {
        if let Some(icon) = icon {
            let r = Rect::from_min_size(
                pos2(rect.left() + PAD, rect.center().y - SMALL_ICON / 2.0),
                Vec2::splat(SMALL_ICON),
            );
            paint_icon(painter, r, icon, look.icon);
        }
    };
    match kind {
        Kind::Large | Kind::LargeDropdown => {
            if let Some(icon) = item.icon {
                let r = Rect::from_center_size(
                    pos2(rect.center().x, rect.top() + LARGE_ICON_TOP + LARGE_ICON / 2.0),
                    Vec2::splat(LARGE_ICON),
                );
                paint_icon(painter, r, icon, look.icon);
            }
            let top = rect.top() + LARGE_ICON_TOP + LARGE_ICON + LARGE_LABEL_GAP;
            centered_lines(
                painter,
                lines,
                rect.center().x,
                top,
                color,
                kind == Kind::LargeDropdown,
            );
        }
        Kind::Small { compact } => {
            small_icon(item.icon);
            if !compact && let Some(text) = lines.first() {
                left_text(rect.left() + PAD + SMALL_ICON + PAD, text);
            }
        }
        Kind::Dropdown { compact } => {
            small_icon(item.icon);
            let mut x = rect.left() + PAD + SMALL_ICON + 3.0;
            if !compact && let Some(text) = lines.first() {
                let r = left_text(rect.left() + PAD + SMALL_ICON + PAD, text);
                x = r.right() + 4.0;
            }
            arrow(painter, pos2(x, rect.center().y - 2.0), color);
        }
        Kind::CheckBox => {
            let r = Rect::from_min_size(
                pos2(rect.left() + PAD, rect.center().y - BOX / 2.0),
                Vec2::splat(BOX),
            );
            if checked == Some(true) {
                // Case cochée : pleine à l'accent (grisée : encre grisée),
                // coche à la couleur de la surface.
                let fill = if color == tokens.disabled {
                    tokens.disabled
                } else {
                    tokens.accent
                };
                painter.rect_filled(r, 3.0, fill);
                let points = vec![
                    pos2(r.left() + 3.0, r.center().y),
                    pos2(r.center().x - 1.0, r.bottom() - 3.5),
                    pos2(r.right() - 3.0, r.top() + 3.5),
                ];
                painter.add(Shape::line(points, Stroke::new(1.5, tokens.surface)));
            } else {
                painter.rect_stroke(r, 3.0, Stroke::new(1.0, color), StrokeKind::Inside);
            }
            if let Some(text) = lines.first() {
                left_text(r.right() + 6.0, text);
            }
        }
        Kind::Percent | Kind::CompactPercent => {
            if let Some(text) = lines.first() {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(LABEL_SIZE),
                    color,
                );
            }
        }
        Kind::Quick => {
            if let Some(icon) = item.icon {
                paint_icon(
                    painter,
                    Rect::from_center_size(rect.center(), Vec2::splat(BAR_ICON)),
                    icon,
                    look.icon,
                );
            }
        }
        Kind::CompactButton { text, .. } | Kind::CompactDropdown { text } => {
            let dropdown = matches!(kind, Kind::CompactDropdown { .. });
            let label = lines.first().filter(|_| text);
            let icon_left = if label.is_some() {
                rect.left() + COMPACT_PAD
            } else if dropdown {
                rect.left() + 6.0
            } else {
                rect.center().x - BAR_ICON / 2.0
            };
            let mut x = icon_left;
            if let Some(icon) = item.icon {
                let r = Rect::from_min_size(
                    pos2(icon_left, rect.center().y - BAR_ICON / 2.0),
                    Vec2::splat(BAR_ICON),
                );
                paint_icon(painter, r, icon, look.icon);
                x = r.right();
            }
            if let Some(text) = label {
                let r = left_text(x + COMPACT_ICON_GAP, text);
                x = r.right();
            }
            if dropdown {
                let gap = if label.is_some() { 6.0 } else { 4.0 };
                arrow(painter, pos2(x + gap, rect.center().y - 2.0), color);
            }
        }
        Kind::More => {}
    }
}

/// Groupe replié (H-UI-75, palier 2) : grand bouton à l'icône et au nom du
/// groupe, qui ouvre un menu des mêmes éléments (mêmes commandes).
fn folded_group(ui: &mut Ui, fc: &mut FrameCtx<'_>, group: &GroupSpec, rect: Rect, lines: &[String], id: Id) {
    let name = fc.texts.get(group.label);
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let tokens = ChromeTokens::for_theme(ui.ctx().theme());
    let look = ItemLook::of(&tokens, true, &response, false, open);
    let painter = ui.painter().clone();
    paint_frame(&painter, &tokens, rect, look.bg, response.has_focus());
    let icon = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + LARGE_ICON_TOP + LARGE_ICON / 2.0),
        Vec2::splat(LARGE_ICON),
    );
    paint_icon(&painter, icon, group.icon, look.icon);
    centered_lines(
        &painter,
        lines,
        rect.center().x,
        icon.bottom() + LARGE_LABEL_GAP,
        look.text,
        true,
    );
    record(ui.ctx(), |d| {
        d.items.push(DrawnItem {
            label: name.clone(),
            rect,
            cmd: None,
            kind: None,
        });
    });
    egui::Popup::menu(&response).show(|ui| group_menu(ui, fc, group));
}

/// Menu d'un groupe replié : chaque élément en entrée de menu (liste ⇒
/// sous-menu), libellé du ruban, commande partagée.
fn group_menu(ui: &mut Ui, fc: &mut FrameCtx<'_>, group: &GroupSpec) {
    let texts = fc.texts;
    for item in group.items {
        let label = item_label(texts, item);
        if item.size == ItemSize::Dropdown {
            let enabled = item.cmd.enabled(fc).yes;
            ui.add_enabled_ui(enabled, |ui| {
                ui.menu_button(label, |ui| dropdown_contents(ui, fc, item.cmd));
            });
        } else {
            let shortcut = item
                .binding
                .and_then(|b| shortcut_for_os(b, ui.ctx().os()).copied());
            twin_item(ui, fc, item.cmd, label, shortcut);
        }
    }
}

/// Texte du libellé d'un élément (tests : nom accessible attendu).
#[must_use]
pub fn label_of(texts: &Texts, item: &ItemSpec) -> String {
    item_label(texts, item)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::{LARGE_MIN_W, SMALL_H, Stage, choose_stages, plan, split_label, width_key};
    use crate::i18n::{Language, Texts};
    use crate::ribbon::RibbonTab;

    /// Mesure fictive : 6 points par caractère.
    fn measure(text: &str, _size: f32) -> f32 {
        6.0 * f32::from(u16::try_from(text.chars().count()).unwrap())
    }

    /// Règle 10 : largeur d'egui bornée avant tout calcul entier.
    #[test]
    fn ribbon_width_key_bounded() {
        assert_eq!(width_key(f32::NAN), 0);
        assert_eq!(width_key(f32::INFINITY), 0);
        assert_eq!(width_key(-3.0), 0);
        assert_eq!(width_key(799.6), 800);
        assert_eq!(width_key(1.0e9), 100_000);
    }

    /// H-UI-71 : libellé de grand bouton sur deux lignes au plus, coupé à
    /// l'espace le plus équilibré.
    #[test]
    fn h_ui_71_large_label_two_lines() {
        let mut m = measure;
        assert_eq!(split_label("Tâche", &mut m), vec!["Tâche"]);
        assert_eq!(
            split_label("Mettre à jour le projet", &mut m),
            vec!["Mettre à jour", "le projet"]
        );
        assert_eq!(split_label("Informations", &mut m), vec!["Informations"]);
    }

    /// H-UI-75 : réduction par rang croissant, un rang d'écart entre
    /// « sans libellés » et « replié » ; rien si la largeur tient.
    #[test]
    fn h_ui_75_choose_stages_by_rank() {
        let widths = [[100.0, 80.0, 50.0], [100.0, 60.0, 50.0], [100.0, 90.0, 50.0]];
        let ranks = [3, 1, 2];
        assert_eq!(choose_stages(&ranks, &widths, 1000.0), vec![Stage::Full; 3]);
        // 316 au départ (séparateurs de 8) : le rang 1 (groupe 1) passe en
        // compact ⇒ 276.
        assert_eq!(
            choose_stages(&ranks, &widths, 276.0),
            vec![Stage::Full, Stage::Compact, Stage::Full]
        );
        // 256 : rangs 1 et 2 sans libellés (276 puis 266), puis rang 1
        // replié (256) ; le rang 3 garde ses libellés.
        assert_eq!(
            choose_stages(&ranks, &widths, 256.0),
            vec![Stage::Full, Stage::Folded, Stage::Compact]
        );
        let tight = choose_stages(&ranks, &widths, 180.0);
        assert_eq!(tight[1], Stage::Folded);
        let none = choose_stages(&ranks, &widths, 0.0);
        assert_eq!(none, vec![Stage::Folded; 3]);
    }

    /// Finitions P4 (H-UI-133) : un pas qui n'étroitise pas (bouton replié
    /// plus large que le groupe compact) est sauté ; la réduction continue
    /// avec le groupe suivant.
    #[test]
    fn h_ui_133_choose_stages_skips_wider_fold() {
        // Groupe 0 : compact 40, replié 70 (plus large) ; groupe 1 : rang 2.
        let widths = [[100.0, 40.0, 70.0], [100.0, 80.0, 50.0]];
        let ranks = [1, 2];
        let stages = choose_stages(&ranks, &widths, 100.0);
        assert_eq!(stages, vec![Stage::Compact, Stage::Folded]);
        assert_eq!(
            choose_stages(&ranks, &widths, 0.0),
            vec![Stage::Compact, Stage::Folded]
        );
    }

    /// H-UI-71 : grands boutons de 76 px de haut et ≥ 52 px de large, petits
    /// de 22 px, trois par colonne ; aucun chevauchement dans un groupe.
    #[test]
    fn h_ui_71_plan_geometry() {
        let texts = Texts::new(Language::Fr);
        for tab in RibbonTab::ALL {
            let p = plan(tab, &texts, 5000.0, &mut measure);
            for g in &p.groups {
                assert_eq!(g.stage, Stage::Full);
                for (i, a) in g.items.iter().enumerate() {
                    match a.kind {
                        super::Kind::Large | super::Kind::LargeDropdown => {
                            assert!(a.rect.width() >= LARGE_MIN_W && a.rect.height() >= 76.0);
                        }
                        _ => assert!((a.rect.height() - SMALL_H).abs() < 0.01),
                    }
                    for b in g.items.iter().skip(i + 1) {
                        assert!(!a.rect.shrink(0.5).intersects(b.rect.shrink(0.5)), "{tab:?}");
                    }
                }
            }
        }
    }

    /// Mêmes entrées ⇒ même disposition (déterminisme des captures).
    #[test]
    fn ribbon_plan_deterministic() {
        let texts = Texts::new(Language::En);
        let a = plan(RibbonTab::Task, &texts, 900.0, &mut measure);
        let b = plan(RibbonTab::Task, &texts, 900.0, &mut measure);
        assert_eq!(a, b);
        assert!(a.width <= 900.0);
    }
}
