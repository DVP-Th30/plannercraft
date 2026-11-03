//! Modèle de la feuille des ressources (J7, lot L10) : lignes, textes
//! pré-formatés et saisie, sans egui.
//!
//! Une ligne par ressource du projet, dans l'ordre du modèle (la ressource
//! cachée d'UID 0 des fichiers MSPDI n'est pas montrée, comme l'ID 0 de
//! `plannercraft_core::resource_id`). Les textes sont formatés **une fois** par
//! génération du document et par langue ([`ResourceRows::build`]) : aucune
//! image ne refait ce travail. Aucun calcul (règle 3) : la surutilisation
//! vient du moteur (`Schedule::resources`, R-186), les taux sont affichés
//! dans leur unité d'origine (H-UI-24) et la saisie est lue par
//! `plannercraft-automation` (unités : `UnitsJson::to_units`) puis appliquée par
//! le registre (`resource.add`, `resource.update`, `resource.delete`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use plannercraft_automation::command::{ResourceAdd, ResourceDelete, ResourceRef, ResourceUpdate};
use plannercraft_automation::json::UnitsJson;
use plannercraft_automation::text::{self, TextContext};
use plannercraft_automation::{Batch, Command, Patch, Session};
use plannercraft_core::{CalendarUid, Money, Project, Rate, RateUnit, ResourceKind, ResourceUid, Units};

use crate::app::OpenDocument;
use crate::column::CellAlign;
use crate::edit::text_language;
use crate::grid::{CellStyle, Sheet};
use crate::i18n::{Arg, Language, TextId, Texts};
use crate::nav::RowRef;
use crate::theme::Palette;

/// Indicateur de surutilisation (H-UI-23), le même dans les deux langues.
pub const OVERALLOCATED_MARK: &str = "!";

/// Colonne de la feuille des ressources (ordre d'affichage).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceColumn {
    /// ID (position dans le projet).
    Id,
    /// Indicateur de surutilisation (H-UI-23).
    Overallocated,
    /// Nom.
    #[default]
    Name,
    /// Type (travail, matériel, coût).
    Kind,
    /// Étiquette d'unité d'une ressource matérielle.
    MaterialLabel,
    /// Initiales.
    Initials,
    /// Groupe.
    Group,
    /// Capacité maximale.
    MaxUnits,
    /// Taux standard (H-UI-24).
    StandardRate,
    /// Taux des heures supplémentaires (H-UI-24).
    OvertimeRate,
    /// Coût par utilisation.
    CostPerUse,
    /// Calendrier de la ressource (choisi dans une liste).
    Calendar,
}

impl ResourceColumn {
    /// Toutes les colonnes, dans l'ordre d'affichage.
    pub const ALL: [Self; 12] = [
        Self::Id,
        Self::Overallocated,
        Self::Name,
        Self::Kind,
        Self::MaterialLabel,
        Self::Initials,
        Self::Group,
        Self::MaxUnits,
        Self::StandardRate,
        Self::OvertimeRate,
        Self::CostPerUse,
        Self::Calendar,
    ];

    /// Nombre de colonnes.
    pub const COUNT: usize = Self::ALL.len();

    /// Rang dans [`ResourceColumn::ALL`].
    #[must_use]
    pub const fn ordinal(self) -> usize {
        match self {
            Self::Id => 0,
            Self::Overallocated => 1,
            Self::Name => 2,
            Self::Kind => 3,
            Self::MaterialLabel => 4,
            Self::Initials => 5,
            Self::Group => 6,
            Self::MaxUnits => 7,
            Self::StandardRate => 8,
            Self::OvertimeRate => 9,
            Self::CostPerUse => 10,
            Self::Calendar => 11,
        }
    }

    /// Largeur, en points logiques (fixe en J7 : 1 136 au total).
    #[must_use]
    pub const fn width(self) -> u16 {
        match self {
            Self::Id => 40,
            Self::Overallocated => 36,
            Self::Name => 200,
            Self::Kind | Self::MaxUnits | Self::CostPerUse => 90,
            Self::MaterialLabel => 80,
            Self::Initials => 70,
            Self::Group | Self::StandardRate | Self::OvertimeRate => 100,
            Self::Calendar => 140,
        }
    }

    /// Alignement : nombres à droite, indicateur centré, le reste à gauche.
    #[must_use]
    pub const fn align(self) -> CellAlign {
        match self {
            Self::Id | Self::MaxUnits | Self::StandardRate | Self::OvertimeRate | Self::CostPerUse => {
                CellAlign::Right
            }
            Self::Overallocated => CellAlign::Center,
            Self::Name | Self::Kind | Self::MaterialLabel | Self::Initials | Self::Group | Self::Calendar => {
                CellAlign::Left
            }
        }
    }

    /// Colonne saisissable : nom, étiquette, initiales, groupe, capacité,
    /// calendrier (taux et type en lecture seule en J7).
    #[must_use]
    pub const fn editable(self) -> bool {
        matches!(
            self,
            Self::Name | Self::MaterialLabel | Self::Initials | Self::Group | Self::MaxUnits | Self::Calendar
        )
    }

    /// Titre traduit.
    #[must_use]
    pub const fn title(self) -> TextId {
        match self {
            Self::Id => TextId::ColId,
            Self::Overallocated => TextId::ResColIndicator,
            Self::Name => TextId::ResColName,
            Self::Kind => TextId::ResColKind,
            Self::MaterialLabel => TextId::ResColMaterialLabel,
            Self::Initials => TextId::ResColInitials,
            Self::Group => TextId::ResColGroup,
            Self::MaxUnits => TextId::ResColMaxUnits,
            Self::StandardRate => TextId::ResColStandardRate,
            Self::OvertimeRate => TextId::ResColOvertimeRate,
            Self::CostPerUse => TextId::ResColCostPerUse,
            Self::Calendar => TextId::ResColCalendar,
        }
    }
}

/// Colonnes de la feuille et leurs largeurs (toutes visibles en J7).
pub const COLUMNS: [(ResourceColumn, u16); ResourceColumn::COUNT] = [
    (ResourceColumn::Id, ResourceColumn::Id.width()),
    (
        ResourceColumn::Overallocated,
        ResourceColumn::Overallocated.width(),
    ),
    (ResourceColumn::Name, ResourceColumn::Name.width()),
    (ResourceColumn::Kind, ResourceColumn::Kind.width()),
    (
        ResourceColumn::MaterialLabel,
        ResourceColumn::MaterialLabel.width(),
    ),
    (ResourceColumn::Initials, ResourceColumn::Initials.width()),
    (ResourceColumn::Group, ResourceColumn::Group.width()),
    (ResourceColumn::MaxUnits, ResourceColumn::MaxUnits.width()),
    (ResourceColumn::StandardRate, ResourceColumn::StandardRate.width()),
    (ResourceColumn::OvertimeRate, ResourceColumn::OvertimeRate.width()),
    (ResourceColumn::CostPerUse, ResourceColumn::CostPerUse.width()),
    (ResourceColumn::Calendar, ResourceColumn::Calendar.width()),
];

/// Une ligne de la feuille.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceRow {
    /// UID de la ressource.
    pub uid: ResourceUid,
    /// ID affiché.
    pub id: u64,
    /// Surutilisée selon le moteur (R-186).
    pub overallocated: bool,
}

/// Lignes et textes de la feuille des ressources d'un document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceRows {
    /// Lignes, dans l'ordre du projet.
    rows: Vec<ResourceRow>,
    /// `rows.len() × ResourceColumn::COUNT` textes, ligne par ligne.
    texts: Vec<String>,
    /// Indice de ligne de chaque ressource.
    index: BTreeMap<ResourceUid, usize>,
    /// Clé du document (ouverture, génération) et langue des textes.
    key: Option<((u64, u64), Language)>,
}

/// Montant en centièmes écrit avec deux décimales (« 120,00 », « -3,50 »),
/// séparateur décimal de la langue (H-UI-24 : aucun calcul, un découpage
/// des centièmes).
#[must_use]
pub fn money_text(m: Money, sep: char) -> String {
    let v = m.get();
    let abs = v.unsigned_abs();
    let units = abs.checked_div(100).unwrap_or(0);
    let cents = abs.checked_rem(100).unwrap_or(0);
    let mut out = String::new();
    if v < 0 {
        out.push('-');
    }
    let _ = write!(out, "{units}{sep}{cents:02}");
    out
}

/// Suffixe traduit d'une unité de taux (« /h », « /j ») ; aucun pour une
/// unité de matériau.
fn rate_suffix(per: RateUnit, texts: &Texts) -> String {
    let id = match per {
        RateUnit::Minute => TextId::RateMinute,
        RateUnit::Hour => TextId::RateHour,
        RateUnit::Day => TextId::RateDay,
        RateUnit::Week => TextId::RateWeek,
        RateUnit::Month => TextId::RateMonth,
        RateUnit::Year => TextId::RateYear,
        RateUnit::MaterialUnit => return String::new(),
    };
    texts.get(id)
}

/// Taux dans l'unité de son format d'origine, deux décimales (H-UI-24) :
/// 120 par jour ⇒ « 120,00/j ».
#[must_use]
pub fn rate_text(rate: &Rate, texts: &Texts) -> String {
    let mut out = money_text(rate.amount, texts.format_labels().decimal_sep);
    out.push_str(&rate_suffix(rate.per, texts));
    out
}

/// Capacité en pourcentage (10 000 ⇒ « 100 % », 1 250 ⇒ « 12,5 % »),
/// zéros finaux retirés, suffixe de pourcentage de la langue.
fn units_text(u: Units, texts: &Texts) -> String {
    let labels = texts.format_labels();
    let mut out = String::new();
    match plannercraft_core::Decimal::new(u.get(), 2) {
        Ok(d) => text::write_decimal(&mut out, d, labels.decimal_sep),
        Err(_) => out.push_str(&labels.unknown),
    }
    out.push_str(&labels.percent_suffix);
    out
}

/// Nom du type d'une ressource.
fn kind_text(kind: &ResourceKind, texts: &Texts) -> String {
    texts.get(match kind {
        ResourceKind::Work => TextId::ResKindWork,
        ResourceKind::Material { .. } => TextId::ResKindMaterial,
        ResourceKind::Cost => TextId::ResKindCost,
    })
}

impl ResourceRows {
    /// Construit les lignes d'un document dans la langue de `texts`.
    #[must_use]
    pub fn build(doc: &OpenDocument, texts: &Texts) -> Self {
        let project = doc.project();
        let schedule = doc.schedule();
        let hidden = ResourceUid::new(0);
        let mut out = Self {
            key: Some((doc.cache_key(), texts.language())),
            ..Self::default()
        };
        for (position, resource) in project.resources.iter().enumerate() {
            let uid = resource.uid();
            if uid == hidden {
                continue;
            }
            let overallocated = schedule
                .resources
                .get(&uid)
                .is_some_and(|load| load.overallocated);
            let row = ResourceRow {
                uid,
                id: plannercraft_core::resource_id(&project.resources, position),
                overallocated,
            };
            let work = resource.kind == ResourceKind::Work;
            let cost = resource.kind == ResourceKind::Cost;
            for column in ResourceColumn::ALL {
                let text = match column {
                    ResourceColumn::Id => row.id.to_string(),
                    ResourceColumn::Overallocated if overallocated => OVERALLOCATED_MARK.to_owned(),
                    ResourceColumn::Name => resource.name.clone(),
                    ResourceColumn::Kind => kind_text(&resource.kind, texts),
                    ResourceColumn::MaterialLabel => match &resource.kind {
                        ResourceKind::Material { label } => label.clone(),
                        _ => String::new(),
                    },
                    ResourceColumn::Initials => resource.initials.clone(),
                    ResourceColumn::Group => resource.group.clone(),
                    ResourceColumn::MaxUnits if work => units_text(resource.max_units, texts),
                    ResourceColumn::StandardRate if !cost => rate_text(&resource.standard_rate, texts),
                    ResourceColumn::OvertimeRate if work => rate_text(&resource.overtime_rate, texts),
                    ResourceColumn::CostPerUse if !cost => {
                        money_text(resource.cost_per_use, texts.format_labels().decimal_sep)
                    }
                    ResourceColumn::Calendar => calendar_name(project, resource.calendar),
                    ResourceColumn::Overallocated
                    | ResourceColumn::MaxUnits
                    | ResourceColumn::StandardRate
                    | ResourceColumn::OvertimeRate
                    | ResourceColumn::CostPerUse => String::new(),
                };
                out.texts.push(text);
            }
            out.index.insert(uid, out.rows.len());
            out.rows.push(row);
        }
        out
    }

    /// Vrai si les lignes ont été construites pour cet état du document et
    /// cette langue.
    #[must_use]
    pub fn is_current(&self, doc: &OpenDocument, language: Language) -> bool {
        self.key == Some((doc.cache_key(), language))
    }

    /// Nombre de lignes (ligne vide non comprise).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Aucune ressource.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Ligne d'indice `i`.
    #[must_use]
    pub fn row(&self, i: usize) -> Option<&ResourceRow> {
        self.rows.get(i)
    }

    /// Texte d'une cellule ; vide hors bornes.
    #[must_use]
    pub fn text(&self, i: usize, c: ResourceColumn) -> &str {
        i.checked_mul(ResourceColumn::COUNT)
            .and_then(|base| base.checked_add(c.ordinal()))
            .and_then(|k| self.texts.get(k))
            .map_or("", String::as_str)
    }

    /// Indice de la ligne d'une ressource.
    #[must_use]
    pub fn index_of(&self, uid: ResourceUid) -> Option<usize> {
        self.index.get(&uid).copied()
    }
}

/// Nom du calendrier `uid` (vide sans calendrier, « ? » s'il est absent du
/// projet).
fn calendar_name(project: &Project, uid: Option<CalendarUid>) -> String {
    match uid {
        None => String::new(),
        Some(uid) => project
            .calendars
            .get(uid)
            .map_or_else(|| "?".to_owned(), |calendar| calendar.name.clone()),
    }
}

/// Calendriers proposés par la liste de la colonne Calendrier : UID et nom,
/// dans l'ordre du projet.
#[must_use]
pub fn calendar_choices(project: &Project) -> Vec<(CalendarUid, String)> {
    project
        .calendars
        .iter()
        .map(|calendar| (calendar.uid(), calendar.name.clone()))
        .collect()
}

/// Lot d'une seule commande, dans la langue `lang`.
fn single(op: Command, lang: text::Language) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: Some(lang),
        ops: vec![op],
        label: None,
    }
}

/// `resource.update` vide sur `uid`.
const fn update(uid: ResourceUid) -> ResourceUpdate {
    ResourceUpdate {
        uid: ResourceRef::Uid(uid),
        name: Patch::Keep,
        kind: Patch::Keep,
        material_label: Patch::Keep,
        max_units: Patch::Keep,
        calendar: Patch::Keep,
        initials: Patch::Keep,
        group: Patch::Keep,
        notes: Patch::Keep,
        inactive: Patch::Keep,
        standard_rate: Patch::Keep,
        overtime_rate: Patch::Keep,
        cost_per_use: Patch::Keep,
    }
}

/// `resource.delete` de `uid` ; `cascade` retire aussi ses affectations
/// (sans `cascade`, le registre refuse une ressource affectée :
/// `resource.has_assignments`).
#[must_use]
pub fn delete_batch(uid: ResourceUid, cascade: bool) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: vec![Command::ResourceDelete(ResourceDelete {
            uid: ResourceRef::Uid(uid),
            cascade,
        })],
        label: None,
    }
}

/// La feuille des ressources vue comme une grille ([`Sheet`]).
pub struct ResourceSheet<'a> {
    /// Lignes et textes.
    rows: &'a ResourceRows,
    /// Messages (refus d'une saisie).
    texts: &'a Texts,
}

impl<'a> ResourceSheet<'a> {
    /// Grille des lignes `rows`.
    #[must_use]
    pub const fn new(rows: &'a ResourceRows, texts: &'a Texts) -> Self {
        Self { rows, texts }
    }

    /// Lignes de la grille.
    #[must_use]
    pub const fn rows(&self) -> &'a ResourceRows {
        self.rows
    }

    /// Refus d'une commande du registre en langage d'utilisateur
    /// (H-UI-105, [`crate::error_text::user_message`]).
    fn refusal(&self, error: &plannercraft_automation::CommandError, project: &Project) -> String {
        crate::error_text::user_message(
            error,
            Some(project),
            self.texts,
            text_language(self.texts.language()),
        )
        .0
    }
}

/// Capacité saisie (« 50 », « 50 % ») en `UnitsJson` texte avec `%`, comme
/// la colonne % achevé de la table (le registre exige le `%`).
fn units_json(text: &str) -> UnitsJson {
    let trimmed = text.trim();
    let text = if trimmed.ends_with('%') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}%")
    };
    UnitsJson {
        value: None,
        text: Some(text),
    }
}

impl Sheet for ResourceSheet<'_> {
    type Key = ResourceUid;
    type Col = ResourceColumn;

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn key(&self, row: usize) -> Option<ResourceUid> {
        self.rows.row(row).map(|row| row.uid)
    }

    fn index_of(&self, key: ResourceUid) -> Option<usize> {
        self.rows.index_of(key)
    }

    fn columns(&self) -> &[(ResourceColumn, u16)] {
        &COLUMNS
    }

    fn title(&self, col: ResourceColumn) -> TextId {
        col.title()
    }

    fn align(&self, col: ResourceColumn) -> CellAlign {
        col.align()
    }

    /// Ressource surutilisée : toute la ligne en rouge (H-UI-23).
    fn style(&self, row: usize, _: ResourceColumn, palette: &Palette) -> CellStyle {
        let over = self.rows.row(row).is_some_and(|row| row.overallocated);
        CellStyle::plain(if over { palette.critical } else { palette.text })
    }

    fn text(&self, row: usize, col: ResourceColumn) -> &str {
        self.rows.text(row, col)
    }

    fn editable(&self, col: ResourceColumn) -> bool {
        col.editable()
    }

    /// Ligne vide : seul le nom crée une ressource (`resource.add`, nom
    /// vide : aucun lot) ; sinon `resource.update` du champ. Capacité lue
    /// par `UnitsJson::to_units` (refus sous la cellule) ; calendrier
    /// désigné par son nom exact (sans tenir compte de la casse), vide :
    /// aucun calendrier propre.
    fn commit(
        &self,
        row: RowRef<ResourceUid>,
        col: ResourceColumn,
        text: &str,
        s: &Session,
        lang: text::Language,
    ) -> Result<Batch, String> {
        let project = s.project();
        let empty = Batch {
            format_version: Batch::FORMAT_VERSION,
            lang: Some(lang),
            ops: Vec::new(),
            label: None,
        };
        let Some(uid) = row.item() else {
            if col != ResourceColumn::Name {
                return Err(self.texts.get(TextId::ResNameFirst));
            }
            let name = text.trim();
            if name.is_empty() {
                return Ok(empty);
            }
            return Ok(single(
                Command::ResourceAdd(ResourceAdd {
                    name: name.to_owned(),
                    label: None,
                    kind: None,
                    material_label: None,
                    max_units: None,
                    calendar: None,
                    initials: None,
                    group: None,
                }),
                lang,
            ));
        };
        let mut op = update(uid);
        match col {
            ResourceColumn::Name => op.name = Patch::Set(text.to_owned()),
            ResourceColumn::MaterialLabel => op.material_label = Patch::Set(text.to_owned()),
            ResourceColumn::Initials => op.initials = Patch::Set(text.to_owned()),
            ResourceColumn::Group => op.group = Patch::Set(text.to_owned()),
            ResourceColumn::MaxUnits => {
                let units = units_json(text);
                let ctx = TextContext::for_project(project, lang);
                units.to_units(&ctx).map_err(|e| self.refusal(&e, project))?;
                op.max_units = Patch::Set(units);
            }
            ResourceColumn::Calendar => {
                let wanted = text.trim();
                op.calendar = if wanted.is_empty() {
                    Patch::Clear
                } else {
                    let found = project
                        .calendars
                        .iter()
                        .find(|calendar| calendar.name.trim().to_lowercase() == wanted.to_lowercase());
                    match found {
                        Some(calendar) => Patch::Set(calendar.uid()),
                        None => {
                            return Err(self
                                .texts
                                .get_args(TextId::ResUnknownCalendar, &[("name", Arg::Str(wanted.into()))]));
                        }
                    }
                };
            }
            ResourceColumn::Id
            | ResourceColumn::Overallocated
            | ResourceColumn::Kind
            | ResourceColumn::StandardRate
            | ResourceColumn::OvertimeRate
            | ResourceColumn::CostPerUse => return Err(self.texts.get(TextId::EditErrorNotEditable)),
        }
        Ok(single(Command::ResourceUpdate(op), lang))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::{Money, Rate, RateUnit};

    use super::{COLUMNS, ResourceColumn, money_text, rate_text};
    use crate::i18n::{Language, Texts};

    #[test]
    fn resource_columns_in_order() {
        for (i, column) in ResourceColumn::ALL.iter().enumerate() {
            assert_eq!(column.ordinal(), i, "{column:?}");
            assert_eq!(COLUMNS[i], (*column, column.width()));
        }
    }

    #[test]
    fn resource_money_two_decimals() {
        assert_eq!(money_text(Money::new(12_000).unwrap(), ','), "120,00");
        assert_eq!(money_text(Money::new(415).unwrap(), '.'), "4.15");
        assert_eq!(money_text(Money::new(-350).unwrap(), ','), "-3,50");
        assert_eq!(money_text(Money::ZERO, ','), "0,00");
        assert_eq!(
            money_text(Money::new(Money::LIMIT).unwrap(), ','),
            "999999999999,99"
        );
    }

    #[test]
    fn resource_rate_suffix_by_unit() {
        let fr = Texts::new(Language::Fr);
        let rate = |amount, per| Rate {
            amount: Money::new(amount).unwrap(),
            per,
        };
        assert_eq!(rate_text(&rate(12_000, RateUnit::Day), &fr), "120,00/j");
        assert_eq!(rate_text(&rate(4_150, RateUnit::Hour), &fr), "41,50/h");
        assert_eq!(rate_text(&rate(500, RateUnit::MaterialUnit), &fr), "5,00");
        let en = Texts::new(Language::En);
        assert_eq!(rate_text(&rate(12_000, RateUnit::Day), &en), "120.00/day");
    }
}
