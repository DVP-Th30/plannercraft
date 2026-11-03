//! Catalogue des champs de tâche proposés comme colonnes (seconde passe
//! UX, ADR-0019 D7, H-UI-114, H-UI-117).
//!
//! Contrat posé par le lot L0 : types, catégories ([`category`]),
//! [`catalog`] réduit aux 14 colonnes d'avant la passe et [`search`]
//! (sous-chaîne sans accents ni casse, titre et alias). Le lot L6 étend le
//! catalogue à tous les champs de [`Column::FIELDS`] (sélecteur, insertion),
//! L7 fixe leur disponibilité finale ([`availability`]) et les libellés des
//! champs à choix ([`field_labels`]), L9 ajoute les champs personnalisés
//! (H-UI-118 : Texte1-30, Nombre1-20, Date1-10, Indicateur1-20 dans la
//! catégorie Personnalisés, titre « Lot (Texte1) » quand le champ a un
//! alias, champs définis par le fichier — formule, table de choix — absents
//! du sélecteur, [`CustomCatalog`]).
//! Aucune indexation pouvant paniquer (règle 10).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::Project;
use plannercraft_core::model::custom::CustomFieldId;

use crate::column::{CellAlign, Column, ColumnState};
use crate::format::{FieldLabels, FormatLabels};
use crate::i18n::{Language, TextId, Texts};

/// Catégorie du sélecteur de champs (H-UI-117).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldCategory {
    /// Général.
    General,
    /// Planification.
    Scheduling,
    /// Avancement.
    Progress,
    /// Ligne de base.
    Baseline,
    /// Écarts.
    Variance,
    /// Marges.
    Slack,
    /// Ressources.
    Resources,
    /// Coûts (grisés : non calculés dans cette version).
    Costs,
    /// Champs personnalisés.
    Custom,
}

impl FieldCategory {
    /// Toutes les catégories, dans l'ordre du sélecteur.
    pub const ALL: [Self; 9] = [
        Self::General,
        Self::Scheduling,
        Self::Progress,
        Self::Baseline,
        Self::Variance,
        Self::Slack,
        Self::Resources,
        Self::Costs,
        Self::Custom,
    ];

    /// Titre traduit de la catégorie.
    #[must_use]
    pub const fn title(self) -> TextId {
        match self {
            Self::General => TextId::FieldCatGeneral,
            Self::Scheduling => TextId::FieldCatScheduling,
            Self::Progress => TextId::FieldCatProgress,
            Self::Baseline => TextId::FieldCatBaseline,
            Self::Variance => TextId::FieldCatVariance,
            Self::Slack => TextId::FieldCatSlack,
            Self::Resources => TextId::FieldCatResources,
            Self::Costs => TextId::FieldCatCosts,
            Self::Custom => TextId::FieldCatCustom,
        }
    }
}

/// Disponibilité d'un champ dans le sélecteur (H-UI-114, H-UI-117).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Affichable et modifiable (le registre a la commande).
    Ready,
    /// Affichable, en lecture seule (valeur calculée ou sans commande).
    ReadOnly,
    /// Grisé, avec sa raison (coûts : non calculés dans cette version).
    Unavailable(TextId),
}

/// Raison des coûts grisés (H-UI-117).
pub const COSTS_REASON: TextId = TextId::FieldCostsNotComputed;

/// Entrée du catalogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldDesc {
    /// Colonne du champ.
    pub column: Column,
    /// Titre traduit.
    pub title: TextId,
    /// Catégorie.
    pub category: FieldCategory,
    /// Alignement des cellules.
    pub align: CellAlign,
    /// Disponibilité.
    pub availability: Availability,
}

/// Entrée d'une colonne : titre, catégorie et alignement de [`Column`].
const fn desc(column: Column, availability: Availability) -> FieldDesc {
    FieldDesc {
        column,
        title: column.title(),
        category: category(column),
        align: column.align(),
        availability,
    }
}

/// Disponibilité finale d'un champ (lot L7, H-UI-117) : modifiable si le
/// registre a la commande qui l'écrit (`edit::editable`), grisé pour les
/// coûts ([`COSTS_REASON`]), en lecture seule sinon (valeur calculée par
/// le moteur, ou champ que le registre n'écrit pas : la priorité, sans
/// opération `task.update priority`, écart signalé dans ADR-0019).
#[must_use]
pub const fn availability(column: Column) -> Availability {
    match column {
        Column::Cost
        | Column::FixedCost
        | Column::ActualCost
        | Column::RemainingCost
        | Column::BaselineCost
        | Column::CostVariance => Availability::Unavailable(COSTS_REASON),
        _ if crate::edit::editable(column) => Availability::Ready,
        _ => Availability::ReadOnly,
    }
}

/// Catalogue complet des champs de tâche (H-UI-117) : les 14 colonnes
/// d'avant la passe ([`Column::ALL`]) puis les champs de
/// [`Column::FIELDS`], dans l'ordre de l'énumération. Les champs
/// personnalisés (lot L9) sont à part ([`custom_catalog`]) : leur titre
/// dépend du numéro et de l'alias du document.
static CATALOG: std::sync::LazyLock<Vec<FieldDesc>> = std::sync::LazyLock::new(|| {
    Column::ALL
        .into_iter()
        .chain(Column::FIELDS)
        .map(|column| desc(column, availability(column)))
        .collect()
});

/// Champs personnalisés de tâche (H-UI-118, lot L9) : les 80 champs de
/// [`CustomFieldId::all`], par type puis par numéro, catégorie
/// Personnalisés, modifiables.
static CUSTOM_CATALOG: std::sync::LazyLock<Vec<FieldDesc>> = std::sync::LazyLock::new(|| {
    CustomFieldId::all()
        .map(Column::Custom)
        .map(|column| desc(column, availability(column)))
        .collect()
});

/// Champs proposés par le sélecteur, dans l'ordre du catalogue (sans les
/// champs personnalisés : [`custom_catalog`]).
#[must_use]
pub fn catalog() -> &'static [FieldDesc] {
    CATALOG.as_slice()
}

/// Champs personnalisés (H-UI-118), par type puis par numéro.
#[must_use]
pub fn custom_catalog() -> &'static [FieldDesc] {
    CUSTOM_CATALOG.as_slice()
}

/// Entrée du catalogue d'une colonne, champ personnalisé compris.
#[must_use]
pub fn describe(column: Column) -> Option<&'static FieldDesc> {
    match column {
        Column::Custom(_) => custom_catalog().iter().find(|d| d.column == column),
        _ => catalog().iter().find(|d| d.column == column),
    }
}

/// Champs personnalisés d'un document vus par le sélecteur et les titres
/// (H-UI-118) : alias posés (`Project.custom_fields`) et champs définis par
/// le fichier (formule, table de choix : R-265), que le sélecteur ne
/// propose pas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomCatalog {
    /// Alias des champs renommés.
    pub aliases: BTreeMap<CustomFieldId, String>,
    /// Champs définis par le fichier : absents du sélecteur.
    pub hidden: BTreeSet<CustomFieldId>,
}

impl CustomCatalog {
    /// Alias et champs étrangers du projet `project`.
    #[must_use]
    pub fn of(project: &Project) -> Self {
        Self {
            aliases: aliases(project),
            hidden: plannercraft_mspdi::custom::foreign_fields(project),
        }
    }
}

/// Alias des champs personnalisés renommés de `project` (alias vides
/// ignorés).
#[must_use]
pub fn aliases(project: &Project) -> BTreeMap<CustomFieldId, String> {
    project
        .custom_fields
        .iter()
        .filter_map(|(id, def)| {
            def.alias
                .as_ref()
                .filter(|a| !a.is_empty())
                .map(|a| (*id, a.clone()))
        })
        .collect()
}

/// Titre d'un champ personnalisé (H-UI-118) : son nom (« Texte1 »), ou
/// « Lot (Texte1) » s'il a l'alias « Lot ».
#[must_use]
pub fn custom_title(id: CustomFieldId, texts: &Texts, alias: Option<&str>) -> String {
    let name = Column::Custom(id).label(texts);
    match alias.filter(|a| !a.is_empty()) {
        Some(alias) => texts.get_args(
            TextId::CustomFieldsAliasTitle,
            &[
                ("alias", crate::i18n::Arg::Str(alias.to_owned().into())),
                ("column", crate::i18n::Arg::Str(name.into())),
            ],
        ),
        None => name,
    }
}

/// Titre par défaut d'une colonne (H-UI-118) : celui du champ
/// ([`Column::label`]), avec l'alias du document pour un champ
/// personnalisé renommé (`project`, `None` sans document).
#[must_use]
pub fn column_label(column: Column, texts: &Texts, project: Option<&Project>) -> String {
    match column {
        Column::Custom(id) => {
            let alias = project
                .and_then(|p| p.custom_fields.get(&id))
                .and_then(|def| def.alias.as_deref());
            custom_title(id, texts, alias)
        }
        _ => column.label(texts),
    }
}

/// Titre affiché d'une colonne (en-tête de la table) : celui de
/// « Paramètres de champ » s'il est posé (H-UI-115), sinon le titre par
/// défaut, qui suit l'alias à chaque image ([`column_label`]).
#[must_use]
pub fn display_title(state: &ColumnState, texts: &Texts, project: Option<&Project>) -> String {
    state
        .title
        .clone()
        .unwrap_or_else(|| column_label(state.column, texts, project))
}

/// Catégorie d'une colonne (H-UI-117).
#[must_use]
pub const fn category(column: Column) -> FieldCategory {
    match column {
        Column::Id
        | Column::Name
        | Column::UniqueId
        | Column::Wbs
        | Column::OutlineLevel
        | Column::Summary
        | Column::Milestone
        | Column::Notes
        | Column::Created
        | Column::Priority
        | Column::Active => FieldCategory::General,
        Column::Duration
        | Column::Start
        | Column::Finish
        | Column::Predecessors
        | Column::Critical
        | Column::Mode
        | Column::Successors
        | Column::ConstraintType
        | Column::ConstraintDate
        | Column::Deadline
        | Column::TaskType
        | Column::EffortDriven
        | Column::Work
        | Column::EarlyStart
        | Column::EarlyFinish
        | Column::LateStart
        | Column::LateFinish => FieldCategory::Scheduling,
        Column::TotalSlack | Column::FreeSlack => FieldCategory::Slack,
        Column::PercentComplete
        | Column::PercentWorkComplete
        | Column::ActualStart
        | Column::ActualFinish
        | Column::ActualDuration
        | Column::RemainingDuration
        | Column::ActualWork
        | Column::RemainingWork => FieldCategory::Progress,
        Column::ResourceNames => FieldCategory::Resources,
        Column::BaselineStart
        | Column::BaselineFinish
        | Column::BaselineDuration
        | Column::BaselineWork
        | Column::Baseline1Start
        | Column::Baseline1Finish
        | Column::Baseline1Duration
        | Column::Baseline1Work
        | Column::Baseline2Start
        | Column::Baseline2Finish
        | Column::Baseline2Duration
        | Column::Baseline2Work
        | Column::Baseline3Start
        | Column::Baseline3Finish
        | Column::Baseline3Duration
        | Column::Baseline3Work
        | Column::Baseline4Start
        | Column::Baseline4Finish
        | Column::Baseline4Duration
        | Column::Baseline4Work
        | Column::Baseline5Start
        | Column::Baseline5Finish
        | Column::Baseline5Duration
        | Column::Baseline5Work
        | Column::Baseline6Start
        | Column::Baseline6Finish
        | Column::Baseline6Duration
        | Column::Baseline6Work
        | Column::Baseline7Start
        | Column::Baseline7Finish
        | Column::Baseline7Duration
        | Column::Baseline7Work
        | Column::Baseline8Start
        | Column::Baseline8Finish
        | Column::Baseline8Duration
        | Column::Baseline8Work
        | Column::Baseline9Start
        | Column::Baseline9Finish
        | Column::Baseline9Duration
        | Column::Baseline9Work
        | Column::Baseline10Start
        | Column::Baseline10Finish
        | Column::Baseline10Duration
        | Column::Baseline10Work => FieldCategory::Baseline,
        Column::FinishVariance | Column::StartVariance | Column::DurationVariance | Column::WorkVariance => {
            FieldCategory::Variance
        }
        Column::Cost
        | Column::FixedCost
        | Column::ActualCost
        | Column::RemainingCost
        | Column::BaselineCost
        | Column::CostVariance => FieldCategory::Costs,
        Column::Custom(_) => FieldCategory::Custom,
    }
}

/// Texte replié pour la recherche : minuscules, sans accents latins
/// courants (« Durée » ⇒ « duree », « Œuvre » ⇒ « oeuvre »).
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' | 'ì' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ÿ' | 'ý' => out.push('y'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            other => out.push(other),
        }
    }
    out
}

thread_local! {
    /// Textes de la dernière langue cherchée : une recherche par frappe ne
    /// relit pas les fichiers fluent.
    static TEXTS: RefCell<Option<Texts>> = const { RefCell::new(None) };
}

/// Champs dont le titre (langue `lang`) ou l'alias contient `query`
/// (sous-chaîne, sans accents ni casse ; vide : tout le catalogue), dans
/// l'ordre du catalogue, puis les champs personnalisés (H-UI-118 : nom ou
/// alias de `aliases`), dans l'ordre de leurs identifiants. Aucun champ
/// masqué : voir [`search_in`].
#[must_use]
pub fn search(query: &str, lang: Language, aliases: &BTreeMap<CustomFieldId, String>) -> Vec<Column> {
    let custom = CustomCatalog {
        aliases: aliases.clone(),
        hidden: BTreeSet::new(),
    };
    search_in(query, lang, &custom)
}

/// [`search`] sur les champs personnalisés d'un document : alias de
/// `custom` cherchés avec le nom, champs `custom.hidden` (formule, table de
/// choix) absents (H-UI-118).
#[must_use]
pub fn search_in(query: &str, lang: Language, custom: &CustomCatalog) -> Vec<Column> {
    let needle = fold(query.trim());
    TEXTS.with(|cell| {
        let mut cache = cell.borrow_mut();
        let texts = match cache.take() {
            Some(texts) if texts.language() == lang => texts,
            _ => Texts::new(lang),
        };
        let mut found: Vec<Column> = catalog()
            .iter()
            .filter(|d| fold(&texts.get(d.title)).contains(&needle))
            .map(|d| d.column)
            .collect();
        for id in CustomFieldId::all().filter(|id| !custom.hidden.contains(id)) {
            let title = custom_title(id, &texts, custom.aliases.get(&id).map(String::as_str));
            if fold(&title).contains(&needle) {
                found.push(Column::Custom(id));
            }
        }
        *cache = Some(texts);
        found
    })
}

/// Libellés des champs à choix d'une langue (lot L7), lus dans les
/// textes de la fenêtre « Informations sur la tâche ».
#[must_use]
pub fn field_labels(texts: &Texts) -> FieldLabels {
    FieldLabels {
        asap: texts.get(TextId::TaskInfoConstraintAsap),
        alap: texts.get(TextId::TaskInfoConstraintAlap),
        mso: texts.get(TextId::TaskInfoConstraintMso),
        mfo: texts.get(TextId::TaskInfoConstraintMfo),
        snet: texts.get(TextId::TaskInfoConstraintSnet),
        snlt: texts.get(TextId::TaskInfoConstraintSnlt),
        fnet: texts.get(TextId::TaskInfoConstraintFnet),
        fnlt: texts.get(TextId::TaskInfoConstraintFnlt),
        fixed_units: texts.get(TextId::TaskInfoTypeFixedUnits),
        fixed_duration: texts.get(TextId::TaskInfoTypeFixedDuration),
        fixed_work: texts.get(TextId::TaskInfoTypeFixedWork),
    }
}

thread_local! {
    /// Libellés des choix et de l'affichage de la dernière langue lue par
    /// la saisie ([`with_choice_labels`]) : une validation ne relit pas les
    /// fichiers fluent.
    static CHOICES: RefCell<Option<(Language, FormatLabels, FieldLabels)>> = const { RefCell::new(None) };
}

/// Appelle `f` avec les libellés d'affichage et des champs à choix de la
/// langue `lang` (lecture d'un choix saisi, `edit::commit_edit`), gardés
/// d'un appel à l'autre.
pub fn with_choice_labels<R>(lang: Language, f: impl FnOnce(&FormatLabels, &FieldLabels) -> R) -> R {
    CHOICES.with(|cell| {
        let mut cache = cell.borrow_mut();
        let fresh = match cache.take() {
            Some(entry) if entry.0 == lang => entry,
            _ => {
                let texts = Texts::new(lang);
                (lang, texts.format_labels().clone(), field_labels(&texts))
            }
        };
        let out = f(&fresh.1, &fresh.2);
        *cache = Some(fresh);
        out
    })
}

/// Textes posés par le lot L7 : refus d'un choix non reconnu, champ
/// calculé.
pub const L7_TEXTS: [TextId; 2] = [TextId::EditErrorBadChoice, TextId::EditErrorComputed];

/// Fenêtre de colonne ouverte (H-UI-114, H-UI-115, lot L6) : sélecteur de
/// champ (insertion au rang `at`) ou, si `settings` est posé, « Paramètres
/// de champ » de la colonne de rang `at`. Une seule à la fois, hébergée par
/// `ViewTransient::picker` (contrat L0) : la fenêtre « Paramètres de champ »
/// n'a pas de `DialogKind` (préférence d'affichage, aucun brouillon du
/// document), elle partage donc l'emplacement du sélecteur.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldPicker {
    /// Rang d'insertion dans `UiState.columns` (sélecteur) ou rang de la
    /// colonne réglée (`settings`).
    pub at: usize,
    /// Texte de recherche.
    pub query: String,
    /// Rang de l'entrée en surbrillance parmi les entrées choisissables
    /// de la liste filtrée ([`choices`]).
    pub highlighted: usize,
    /// Focus à donner à la zone de saisie à la prochaine image (ouverture).
    pub focus_pending: bool,
    /// « Paramètres de champ » ouverts (H-UI-115) à la place du sélecteur.
    pub settings: Option<Box<crate::dialog::field_settings::FieldSettingsDraft>>,
}

impl FieldPicker {
    /// Sélecteur ouvert pour une insertion au rang `at`, recherche vide,
    /// première entrée en surbrillance, focus à la recherche.
    #[must_use]
    pub fn insert_at(at: usize) -> Self {
        Self {
            at,
            focus_pending: true,
            ..Self::default()
        }
    }
}

/// Entrées choisissables du sélecteur pour la recherche `query` : champs
/// trouvés ([`search`]) qui ne sont pas grisés, dans l'ordre d'affichage
/// (par catégorie, puis ordre du catalogue). C'est la liste que ↑/↓
/// parcourent et qu'Entrée insère (H-UI-114).
#[must_use]
pub fn choices(query: &str, lang: Language, aliases: &BTreeMap<CustomFieldId, String>) -> Vec<Column> {
    ordered(&search(query, lang, aliases))
}

/// [`choices`] sur les champs personnalisés d'un document
/// ([`search_in`]).
#[must_use]
pub fn choices_in(query: &str, lang: Language, custom: &CustomCatalog) -> Vec<Column> {
    ordered(&search_in(query, lang, custom))
}

/// Rang, dans `list` (= [`choices_in`] pour `query`), de l'entrée à mettre
/// en surbrillance quand la recherche change (H-UI-114, relecture finale
/// ux2, C6) : la **meilleure** correspondance, et non la première de
/// l'ordre du catalogue. Classement : titre ou alias égal à la recherche,
/// puis qui commence par elle, puis qui la contient en début de mot, puis
/// simple sous-chaîne ; à égalité, la première de la liste. Recherche
/// vide : 0. La liste reste affichée par catégorie (↑/↓ la parcourent dans
/// cet ordre).
#[must_use]
pub fn best_choice(query: &str, lang: Language, custom: &CustomCatalog, list: &[Column]) -> usize {
    let needle = fold(query.trim());
    if needle.is_empty() {
        return 0;
    }
    TEXTS.with(|cell| {
        let mut cache = cell.borrow_mut();
        let texts = match cache.take() {
            Some(texts) if texts.language() == lang => texts,
            _ => Texts::new(lang),
        };
        let best = list
            .iter()
            .enumerate()
            .filter_map(|(i, column)| {
                let rank = match column {
                    Column::Custom(id) => {
                        let alias = custom.aliases.get(id).map(String::as_str);
                        let title = custom_title(*id, &texts, alias);
                        let by_title = match_rank(&fold(&title), &needle);
                        let by_alias = alias.and_then(|a| match_rank(&fold(a), &needle));
                        by_title.into_iter().chain(by_alias).min()
                    }
                    _ => describe(*column).and_then(|d| match_rank(&fold(&texts.get(d.title)), &needle)),
                };
                rank.map(|rank| (rank, i))
            })
            .min()
            .map_or(0, |(_, i)| i);
        *cache = Some(texts);
        best
    })
}

/// Qualité d'une correspondance de `needle` dans `title` (textes repliés,
/// [`fold`]) : 0 égal, 1 début du titre, 2 début d'un mot, 3 ailleurs ;
/// `None` : absent.
fn match_rank(title: &str, needle: &str) -> Option<u8> {
    if title == needle {
        return Some(0);
    }
    if title.starts_with(needle) {
        return Some(1);
    }
    let mut rank = None;
    for (at, _) in title.match_indices(needle) {
        let word_start = title
            .get(..at)
            .and_then(|head| head.chars().next_back())
            .is_none_or(|c| !c.is_alphanumeric());
        let r = if word_start { 2 } else { 3 };
        rank = Some(rank.map_or(r, |old: u8| old.min(r)));
    }
    rank
}

/// Champs trouvés `found` non grisés, par catégorie puis dans leur ordre.
fn ordered(found: &[Column]) -> Vec<Column> {
    let mut out = Vec::with_capacity(found.len());
    for cat in FieldCategory::ALL {
        out.extend(found.iter().copied().filter(|c| {
            category(*c) == cat
                && !matches!(
                    describe(*c).map(|d| d.availability),
                    Some(Availability::Unavailable(_))
                )
        }));
    }
    out
}

/// Textes posés par le lot L0 pour l'insertion de colonnes, servis par le
/// lot L6 (menu d'en-tête, sélecteur, « Ajouter une nouvelle colonne »,
/// « Paramètres de champ »).
pub const L6_TEXTS: [TextId; 16] = [
    TextId::MenuInsertColumn,
    TextId::MenuHideColumn,
    TextId::MenuFieldSettings,
    TextId::AddNewColumn,
    TextId::FieldPickerSearch,
    TextId::FieldPickerEmpty,
    TextId::FieldPickerShown,
    TextId::FieldSettingsTitle,
    TextId::FieldSettingsField,
    TextId::FieldSettingsColumnTitle,
    TextId::FieldSettingsAlignTitle,
    TextId::FieldSettingsAlignData,
    TextId::FieldSettingsWidth,
    TextId::AlignLeft,
    TextId::AlignCenter,
    TextId::AlignRight,
];

/// Textes posés par le lot L0 pour les colonnes, servis par L6 : refus
/// (64 colonnes, déplacer l'ID), sous-menu « Affichage du tableau ».
pub const L6_MORE_TEXTS: [TextId; 3] = [
    TextId::ColumnsMaxReached,
    TextId::ColumnIdStaysFirst,
    TextId::MenuTableDisplay,
];

/// Textes posés par le lot L0 pour les champs personnalisés, servis par
/// le lot L9 (fenêtre Projet → Champs personnalisés…, types, titre avec
/// alias, libellé du lot d'annulation).
pub const L9_TEXTS: [TextId; 10] = [
    TextId::MenuCustomFields,
    TextId::CustomFieldsTitle,
    TextId::CustomFieldsRename,
    TextId::CustomFieldsAliasTitle,
    TextId::CustomFieldsBatchLabel,
    TextId::CustomFieldsAliasTooLong,
    TextId::CustomTypeText,
    TextId::CustomTypeNumber,
    TextId::CustomTypeDate,
    TextId::CustomTypeFlag,
];

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use plannercraft_core::model::custom::{CustomFieldId, CustomKind};

    use super::{FieldCategory, catalog, category, fold, search};
    use crate::column::Column;
    use crate::i18n::{Language, Texts};

    /// H-UI-117 : chaque colonne au plus une fois ; chaque titre (de tous
    /// les champs, catégories et textes posés) traduit en français et en
    /// anglais ; le catalogue (lot L6) couvre les 14 colonnes et les champs
    /// de [`Column::FIELDS`].
    #[test]
    fn field_catalog_unique() {
        let columns: BTreeSet<Column> = catalog().iter().map(|d| d.column).collect();
        assert_eq!(columns.len(), catalog().len());
        assert_eq!(columns, Column::ALL.into_iter().chain(Column::FIELDS).collect());
        let mut ids: Vec<crate::i18n::TextId> = Column::ALL
            .into_iter()
            .chain(Column::FIELDS)
            .map(Column::title)
            .chain(FieldCategory::ALL.map(FieldCategory::title))
            .collect();
        ids.extend(super::L6_TEXTS);
        ids.extend(super::L6_MORE_TEXTS);
        ids.extend(super::L9_TEXTS);
        ids.extend(super::L7_TEXTS);
        ids.push(super::COSTS_REASON);
        for lang in Language::ALL {
            let texts = Texts::new(lang);
            for id in &ids {
                let text = texts.get(*id);
                assert!(!text.is_empty() && text != id.key(), "{lang:?} {id:?}");
            }
        }
        // Titres distincts (une langue suffit à le montrer).
        let fr = Texts::new(Language::Fr);
        let titles: BTreeSet<String> = Column::ALL
            .into_iter()
            .chain(Column::FIELDS)
            .map(|c| fr.get(c.title()))
            .collect();
        assert_eq!(titles.len(), Column::COUNT + Column::FIELDS.len());
        assert_eq!(category(Column::FreeSlack), FieldCategory::Slack);
        assert_eq!(category(Column::Cost), FieldCategory::Costs);
        let custom = Column::Custom(CustomFieldId::new(CustomKind::Flag, 3).unwrap());
        assert_eq!(category(custom), FieldCategory::Custom);
        assert_eq!(custom.label(&fr), "Indicateur3");
        assert_eq!(custom.label(&Texts::new(Language::En)), "Flag3");
    }

    /// H-UI-114 : « duree » trouve « Durée », sans casse ; recherche vide
    /// ⇒ tout ; alias de champ personnalisé trouvé.
    #[test]
    fn field_search_accents() {
        let none = BTreeMap::new();
        assert_eq!(fold("Durée ŒUVRE"), "duree oeuvre");
        let first = |q: &str, lang| search(q, lang, &none).first().copied();
        assert_eq!(first("duree", Language::Fr), Some(Column::Duration));
        assert_eq!(first("DURÉE", Language::Fr), Some(Column::Duration));
        assert_eq!(first("duration", Language::En), Some(Column::Duration));
        assert!(search("duree", Language::Fr, &none).contains(&Column::RemainingDuration));
        assert_eq!(
            search("", Language::Fr, &none).len(),
            catalog().len() + super::custom_catalog().len()
        );
        let found = search("fin de référence", Language::Fr, &none);
        assert_eq!(found.first(), Some(&Column::BaselineFinish));
        assert!(found.contains(&Column::Baseline10Finish));
        assert_eq!(search("zzz", Language::Fr, &none).len(), 0);
        let lot = CustomFieldId::new(CustomKind::Text, 1).unwrap();
        let aliases = BTreeMap::from([(lot, "Lot à bâtir".to_owned())]);
        assert_eq!(search("batir", Language::Fr, &aliases), [Column::Custom(lot)]);
    }

    /// H-UI-118 : titre d'un champ personnalisé : nom, ou « alias (nom) » ;
    /// le titre de « Paramètres de champ » l'emporte ; autre colonne : son
    /// titre.
    #[test]
    fn h_ui_118_custom_titles() {
        use plannercraft_core::model::custom::CustomFieldDef;

        let fr = Texts::new(Language::Fr);
        let en = Texts::new(Language::En);
        let lot = CustomFieldId::new(CustomKind::Text, 1).unwrap();
        assert_eq!(super::custom_title(lot, &fr, None), "Texte1");
        assert_eq!(super::custom_title(lot, &fr, Some("")), "Texte1");
        assert_eq!(super::custom_title(lot, &fr, Some("Lot")), "Lot (Texte1)");
        assert_eq!(super::custom_title(lot, &en, Some("Lot")), "Lot (Text1)");
        let now: plannercraft_core::Moment = "2026-10-19T08:00".parse().unwrap();
        let mut project = plannercraft_core::Project::new(plannercraft_core::ProjectInit {
            name: "P".to_owned(),
            anchor: plannercraft_core::ScheduleAnchor::FromStart(now),
            current_date: now,
            calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
        });
        project.custom_fields.insert(
            lot,
            CustomFieldDef {
                alias: Some("Lot".to_owned()),
            },
        );
        let column = Column::Custom(lot);
        assert_eq!(super::column_label(column, &fr, Some(&project)), "Lot (Texte1)");
        assert_eq!(super::column_label(column, &fr, None), "Texte1");
        assert_eq!(
            super::column_label(Column::Duration, &fr, Some(&project)),
            "Durée"
        );
        let mut state = crate::column::ColumnState::default_for(column);
        assert_eq!(super::display_title(&state, &fr, Some(&project)), "Lot (Texte1)");
        state.title = Some("Lots".to_owned());
        assert_eq!(super::display_title(&state, &fr, Some(&project)), "Lots");
        assert_eq!(
            super::aliases(&project).get(&lot).map(String::as_str),
            Some("Lot")
        );
        assert_eq!(super::custom_catalog().len(), 80);
        assert!(super::describe(column).is_some());
    }
}
