//! Palette de commandes (interface adoucie, ADR-0026 lot U3, H-UI-177 à
//! H-UI-183) : liste des commandes, correspondance floue et état de la
//! fenêtre. Le dessin est dans `view::palette`.
//!
//! **Source unique.** La liste est dérivée des tables existantes, jamais
//! écrite à la main : barre d'accès rapide ([`QUICK_ACCESS`]), éléments de
//! chaque onglet de [`RIBBON`] dans l'ordre (une [`RibbonCmd`] par
//! élément ; les listes déroulantes sont dépliées : vues, décaler,
//! colonnes, et les entrées de `chrome::list_choices` pour l'échelle de
//! temps, le thème, la langue, le tri, le mode plan, le filtre, le
//! regroupement et les colonnes affichées), menus contextuels
//! ([`CONTEXT_MENU`], [`TIMESCALE_MENU`]), section « remplir, effacer, tout
//! sélectionner » du menu Édition (`chrome::EDIT_FILL_SELECT`), puis
//! entrées propres aux menus (`chrome::MenuCmd`). Chaque entrée exécute exactement ce que pousse le
//! ruban ou le menu (`chrome::push_cmd`, ou l'action de l'entrée de liste) ;
//! activation et raison du grisé sont celles du ruban
//! ([`RibbonCmd::enabled`]). Hors de la palette : couleurs du texte et du
//! surlignage (des valeurs, pas des commandes) et fichiers récents (des
//! documents).
//!
//! **Correspondance floue maison** (aucune bibliothèque : voir ADR-0026
//! D11) : texte replié (accents et casse, `fold_text` de R-251), puis par
//! ordre de rang : préfixe, sous-chaîne en début de mot, sous-chaîne,
//! sous-séquence par débuts de mots, chaque mot de la requête ; à défaut le
//! même calcul sur « origine libellé ». Classement stable : rang, puis
//! ordre de la liste.

use plannercraft_automation::filter::fold_text;

use crate::action::Action;
use crate::i18n::TextId;
use crate::keymap::{Binding, Shortcut, UiBinding, shortcut_for_os};
use crate::ribbon::Enabled;
use crate::ribbon::icon_data::Icon;
use crate::ribbon::spec::{CONTEXT_MENU, QUICK_ACCESS, RIBBON, RibbonCmd, TIMESCALE_MENU};
use crate::view::FrameCtx;
use crate::view::chrome::{
    Choice, EDIT_FILL_SELECT, MenuCmd, VIEW_CHOICES, item_checked, list_choices, twin_text,
};

/// Ouverture et fermeture de la palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteOp {
    /// Ouvre la palette (Ctrl/Cmd+K, clic sur le champ de la barre du haut),
    /// saisie vide.
    Open,
    /// Referme la palette et rend le focus à la vue.
    Close,
}

/// Ce que fait une entrée.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    /// Commande du ruban : `chrome::push_cmd` (action et style du Gantt,
    /// comme le ruban et le menu).
    Cmd(RibbonCmd),
    /// Entrée de liste ou de menu : son action.
    Do(Action),
}

/// Entrée de la palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Exécution.
    pub run: Run,
    /// Libellé (celui du jumeau de menu pour une commande du ruban).
    pub label: String,
    /// Onglet du ruban ou menu d'origine (« Affichage › Échelle de temps »
    /// pour une entrée de liste).
    pub origin: String,
    /// Raccourci affiché à droite.
    pub shortcut: Option<Shortcut>,
    /// Activation et raison du grisé.
    pub enabled: Enabled,
    /// Icône de l'élément du ruban, s'il en a une.
    pub icon: Option<Icon>,
    /// État d'une case ou d'un choix exclusif (`None` : commande).
    pub on: Option<bool>,
}

/// Séparateur des niveaux d'une origine.
const ORIGIN_SEP: &str = " › ";

/// Liaison dont le raccourci est affiché : celle du ruban, ou celle que le
/// menu affiche pour son jumeau (Assistant IA : Ctrl/Cmd+J, H-UI-162 ; le
/// ruban ne l'affiche pas).
const fn palette_binding(cmd: RibbonCmd) -> Option<Binding> {
    match cmd {
        RibbonCmd::Assistant => Some(Binding::Ui(UiBinding::ToggleAssistant)),
        _ => cmd.binding(),
    }
}

/// Entrées de la palette pour l'image (ordre de la liste : accès rapide,
/// onglets du ruban, menus contextuels, entrées propres aux menus ; chaque
/// commande une fois, à sa première place).
#[must_use]
pub fn entries(ctx: &egui::Context, fc: &FrameCtx<'_>) -> Vec<Entry> {
    let texts = fc.texts;
    let os = ctx.os();
    let path = |ids: &[TextId]| {
        ids.iter()
            .map(|id| texts.get(*id))
            .collect::<Vec<_>>()
            .join(ORIGIN_SEP)
    };
    let mut out = Vec::new();
    let mut seen: Vec<RibbonCmd> = Vec::new();
    let mut add_cmd = |out: &mut Vec<Entry>, cmd: RibbonCmd, icon: Option<Icon>, origin: String| {
        if seen.contains(&cmd) {
            return;
        }
        seen.push(cmd);
        let on = if cmd.menu_is_check() || cmd.context_is_check() {
            item_checked(ctx, fc, cmd)
        } else if VIEW_CHOICES.contains(&cmd) {
            Some(cmd.view_selected(fc))
        } else {
            None
        };
        let shortcut = palette_binding(cmd)
            .and_then(|binding| shortcut_for_os(binding, os).copied())
            .or_else(|| cmd.shortcut(os));
        out.push(Entry {
            run: Run::Cmd(cmd),
            label: twin_text(fc, cmd),
            origin,
            shortcut,
            enabled: cmd.enabled(fc),
            icon,
            on,
        });
    };
    for item in QUICK_ACCESS {
        add_cmd(&mut out, item.cmd, item.icon, path(item.cmd.menu_path()));
    }
    for (tab, groups) in RIBBON {
        let tab_label = texts.get(tab.label());
        for item in groups.iter().flat_map(|group| group.items.iter()) {
            let list = || {
                format!(
                    "{tab_label}{ORIGIN_SEP}{}",
                    crate::view::ribbon::label_of(texts, item)
                )
            };
            match item.cmd {
                RibbonCmd::ViewMenu => {
                    for cmd in VIEW_CHOICES {
                        add_cmd(&mut out, cmd, None, list());
                    }
                }
                RibbonCmd::ShiftMenu => {
                    for step in crate::action::ShiftStep::ALL {
                        add_cmd(&mut out, RibbonCmd::Shift(step), None, list());
                    }
                    add_cmd(&mut out, RibbonCmd::ShiftCustom, None, list());
                }
                RibbonCmd::ColumnsMenu => {
                    for cmd in [RibbonCmd::InsertColumn, RibbonCmd::FieldSettings] {
                        add_cmd(&mut out, cmd, None, list());
                    }
                    push_choices(&mut out, &list(), list_choices(fc, item.cmd), os);
                }
                // Couleurs : des valeurs, hors de la palette (ADR-0026 D10).
                RibbonCmd::TextColorMenu | RibbonCmd::HighlightMenu => {}
                cmd if cmd.action(fc).is_none() => {
                    push_choices(&mut out, &list(), list_choices(fc, cmd), os);
                }
                cmd => add_cmd(&mut out, cmd, item.icon, tab_label.clone()),
            }
        }
    }
    for cmd in CONTEXT_MENU
        .iter()
        .flat_map(|group| group.iter())
        .chain(TIMESCALE_MENU)
        .chain(&EDIT_FILL_SELECT)
    {
        add_cmd(&mut out, *cmd, None, path(cmd.menu_path()));
    }
    for cmd in MenuCmd::ALL.into_iter().filter(|cmd| cmd.available()) {
        let choice = cmd.choice(fc);
        out.push(choice_entry(choice, path(cmd.menu_path()), None, os));
    }
    out
}

/// Ajoute les entrées d'une liste déroulante du ruban.
fn push_choices(out: &mut Vec<Entry>, origin: &str, choices: Vec<Choice>, os: egui::os::OperatingSystem) {
    for choice in choices {
        out.push(choice_entry(choice, origin.to_owned(), None, os));
    }
}

/// Entrée d'une entrée de liste ou de menu.
fn choice_entry(choice: Choice, origin: String, icon: Option<Icon>, os: egui::os::OperatingSystem) -> Entry {
    use crate::view::chrome::ChoiceLook;
    let on = match choice.look {
        ChoiceLook::Button => None,
        ChoiceLook::Exclusive | ChoiceLook::Selectable | ChoiceLook::Check => Some(choice.on),
    };
    let enabled = if choice.enabled.yes || choice.enabled.reason.is_some() {
        choice.enabled
    } else {
        Enabled::no(TextId::PaletteUnavailable)
    };
    Entry {
        run: Run::Do(choice.action),
        label: choice.label,
        origin,
        shortcut: choice
            .binding
            .and_then(|binding| shortcut_for_os(binding, os).copied()),
        enabled,
        icon,
        on,
    }
}

// --- Correspondance floue (H-UI-178).

/// Rang d'une correspondance exacte en tête du texte.
const PREFIX: u32 = 0;
/// Rang d'une sous-chaîne en début de mot (plus la position).
const WORD_START: u32 = 100;
/// Rang d'une sous-chaîne au milieu d'un mot (plus la position).
const INSIDE: u32 = 200;
/// Rang d'une sous-séquence par débuts de mots (plus 10 par morceau au-delà
/// du premier, plus les caractères sautés, plus la position du premier).
const SUBSEQUENCE: u32 = 1_000;
/// Rang de mots trouvés séparément, dans n'importe quel ordre (plus la
/// somme de leurs rangs).
const WORDS: u32 = 5_000;
/// Pénalité d'une correspondance trouvée seulement avec l'origine.
const VIA_ORIGIN: u32 = 10_000;

/// Caractères du texte replié (accents retirés, minuscules).
fn folded(text: &str) -> Vec<char> {
    fold_text(text).chars().collect()
}

/// Conversion saturée d'une position.
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Rang de `needle` dans `hay` (déjà repliés), `None` sans correspondance ;
/// plus petit = meilleur.
fn rank_chars(needle: &[char], hay: &[char]) -> Option<u32> {
    if needle.is_empty() || hay.starts_with(needle) {
        return Some(PREFIX);
    }
    // Sous-chaîne : la première en début de mot, sinon la première.
    let n = needle.len();
    let mut inside: Option<u32> = None;
    for (start, window) in hay.windows(n).enumerate() {
        if window != needle {
            continue;
        }
        if word_start(hay, start) {
            return Some(WORD_START.saturating_add(to_u32(start)));
        }
        inside.get_or_insert(INSIDE.saturating_add(to_u32(start)));
    }
    if inside.is_some() {
        return inside;
    }
    // Sous-séquence par débuts de mots (« litac » : « LIer les TÂChes ») :
    // chaque morceau commence au début d'un mot et s'y prolonge tant que
    // les caractères suivent ; gloutonne, au plus tôt, déterministe. Une
    // lettre isolée au milieu d'un mot ne compte pas (bruit : « lier »
    // dans « réappLIquER »).
    let mut at = 0usize;
    let mut from = 0usize;
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    let mut segments = 0u32;
    while let Some(c) = needle.get(at) {
        let start = (from..hay.len()).find(|&i| hay.get(i) == Some(c) && word_start(hay, i))?;
        first.get_or_insert(start);
        segments = segments.saturating_add(1);
        let mut i = start;
        while let (Some(h), Some(c)) = (hay.get(i), needle.get(at)) {
            if h != c {
                break;
            }
            last = i;
            i = i.saturating_add(1);
            at = at.saturating_add(1);
        }
        from = i;
    }
    let first = first.unwrap_or(0);
    let span = last.saturating_sub(first).saturating_add(1);
    let skipped = span.saturating_sub(n);
    Some(
        SUBSEQUENCE
            .saturating_add(segments.saturating_sub(1).saturating_mul(10))
            .saturating_add(to_u32(skipped))
            .saturating_add(to_u32(first)),
    )
}

/// Le caractère d'indice `i` commence-t-il un mot (début du texte ou
/// précédé d'un caractère non alphanumérique) ?
fn word_start(hay: &[char], i: usize) -> bool {
    i.checked_sub(1)
        .and_then(|before| hay.get(before))
        .is_none_or(|c| !c.is_alphanumeric())
}

/// Rang de la requête `query` dans `text` (accents et casse ignorés) :
/// requête entière d'abord, sinon chaque mot séparément ; `None` sans
/// correspondance. Requête vide : tout correspond, rang 0.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<u32> {
    let words: Vec<Vec<char>> = fold_text(query)
        .split_whitespace()
        .map(|word| word.chars().collect())
        .collect();
    if words.is_empty() {
        return Some(PREFIX);
    }
    let hay = folded(text);
    let phrase: Vec<char> = words.join(&' ');
    if let Some(rank) = rank_chars(&phrase, &hay) {
        return Some(rank);
    }
    if words.len() < 2 {
        return None;
    }
    let mut total = WORDS;
    for word in &words {
        total = total.saturating_add(rank_chars(word, &hay)?);
    }
    Some(total)
}

/// Indices des entrées qui correspondent à `query`, du meilleur rang au
/// moins bon ; à rang égal, ordre de la liste (classement stable).
#[must_use]
pub fn rank(entries: &[Entry], query: &str) -> Vec<usize> {
    let mut ranked: Vec<(u32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let own = score(query, &entry.label);
            let with_origin = score(query, &format!("{} {}", entry.origin, entry.label))
                .map(|rank| rank.saturating_add(VIA_ORIGIN));
            own.into_iter().chain(with_origin).min().map(|rank| (rank, i))
        })
        .collect();
    ranked.sort_unstable();
    ranked.into_iter().map(|(_, i)| i).collect()
}

// --- État de la fenêtre.

/// Touches lues avant le dessin pendant que la palette est ouverte
/// (`App::palette_keys`), servies par `view::palette::show`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaletteKeys {
    /// Déplacement de la ligne choisie (↓ positif, ↑ négatif).
    pub moves: i32,
    /// Entrée : exécuter la ligne choisie.
    pub run: bool,
    /// Tab : demander à l'assistant.
    pub ask: bool,
}

/// Ligne affichée à la dernière image (relevé pour les tests et l'aide).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    /// Libellé.
    pub label: String,
    /// Origine (vide pour la ligne de l'assistant).
    pub origin: String,
    /// Raccourci affiché.
    pub shortcut: Option<String>,
    /// Exécutable.
    pub enabled: bool,
    /// Raison du grisé, traduite.
    pub reason: Option<String>,
    /// État d'une case ou d'un choix.
    pub on: Option<bool>,
    /// Ligne « Demander à l'assistant ».
    pub ask: bool,
}

/// État de la palette (jamais sérialisé : rien n'est retenu d'une
/// ouverture à l'autre).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteState {
    /// Fenêtre ouverte.
    pub open: bool,
    /// Saisie.
    pub query: String,
    /// Ligne choisie (indice dans les lignes affichées).
    pub selected: usize,
    /// Saisie de l'image précédente (la ligne choisie revient en tête quand
    /// elle change).
    pub(crate) last_query: String,
    /// Touches de l'image.
    pub(crate) keys: PaletteKeys,
    /// Rendre le focus à la vue à la prochaine image (fermeture).
    pub(crate) refocus: bool,
    /// Faire défiler la liste jusqu'à la ligne choisie.
    pub(crate) scroll: bool,
    /// Lignes affichées à la dernière image.
    pub listed: Vec<Listed>,
}

impl PaletteState {
    /// Ouvre la palette, saisie vide, première ligne choisie.
    pub fn open(&mut self) {
        *self = Self {
            open: true,
            ..Self::default()
        };
    }

    /// Referme la palette ; le focus revient à la vue à l'image suivante.
    pub fn close(&mut self) {
        let was_open = self.open;
        *self = Self::default();
        self.refocus = was_open;
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::{Entry, Run, rank, score};
    use crate::action::Action;
    use crate::ribbon::Enabled;

    fn entry(label: &str, origin: &str) -> Entry {
        Entry {
            run: Run::Do(Action::About),
            label: label.to_owned(),
            origin: origin.to_owned(),
            shortcut: None,
            enabled: Enabled::YES,
            icon: None,
            on: None,
        }
    }

    fn ranked<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a str> {
        rank(entries, query)
            .into_iter()
            .map(|i| entries[i].label.as_str())
            .collect()
    }

    /// H-UI-178 : accents et casse ignorés dans les deux sens.
    #[test]
    fn h_ui_178_accents_and_case() {
        assert_eq!(
            score("taches", "Lier les tâches"),
            score("TÂCHES", "lier les taches")
        );
        assert!(score("echelle", "Échelle de temps").is_some());
        assert!(score("ÉCHELLE", "echelle de temps").is_some());
        assert_eq!(score("lier", "LIER les tâches"), Some(0));
    }

    /// H-UI-178 : préfixe, début de mot, milieu de mot, sous-séquence,
    /// mots dans le désordre, aucune correspondance.
    #[test]
    fn h_ui_178_ranks_by_kind() {
        let prefix = score("lier", "Lier les tâches").unwrap();
        let word = score("tâch", "Lier les tâches").unwrap();
        let inside = score("ches", "Lier les tâches").unwrap();
        let subsequence = score("litac", "Lier les tâches").unwrap();
        let words = score("tâches lier", "Lier les tâches").unwrap();
        assert!(prefix < word && word < inside && inside < subsequence && subsequence < words);
        assert_eq!(score("zz", "Lier les tâches"), None);
        // Lettres au milieu des mots : pas une correspondance (bruit).
        assert_eq!(score("lrtch", "Lier les tâches"), None);
        assert_eq!(score("lier", "Réappliquer le filtre"), None);
        assert_eq!(score("", "Lier"), Some(0));
        assert_eq!(score("   ", "Lier"), Some(0));
    }

    /// H-UI-178 : une sous-séquence serrée passe avant une sous-séquence
    /// étalée.
    #[test]
    fn h_ui_178_tight_subsequence_first() {
        let tight = score("zav", "Zoom avant").unwrap();
        let loose = score("zav", "Zoom sur la tâche, avancer").unwrap();
        assert!(tight < loose, "{tight} {loose}");
    }

    /// H-UI-178 : classement stable (ordre de la liste à rang égal),
    /// correspondance par l'origine en dernier, filtrage.
    #[test]
    fn h_ui_178_stable_ranking() {
        let entries = [
            entry("Supprimer les liens", "Tâche"),
            entry("Lier les tâches", "Tâche"),
            entry("Lier à la chronologie", "Affichage"),
            entry("Copier", "Tâche"),
            entry("Mois relatifs", "Format"),
        ];
        assert_eq!(
            ranked(&entries, "lier"),
            ["Lier les tâches", "Lier à la chronologie"]
        );
        // « lie » : préfixes d'abord, puis le début de mot de « liens ».
        assert_eq!(
            ranked(&entries, "lie"),
            ["Lier les tâches", "Lier à la chronologie", "Supprimer les liens"]
        );
        // Origine : « format » ne figure que dans l'origine.
        assert_eq!(ranked(&entries, "format"), ["Mois relatifs"]);
        // Requête vide : tout, dans l'ordre de la liste.
        assert_eq!(ranked(&entries, "").len(), entries.len());
        assert_eq!(ranked(&entries, "")[0], "Supprimer les liens");
        // Déterminisme : deux classements identiques.
        assert_eq!(rank(&entries, "li"), rank(&entries, "li"));
    }
}
