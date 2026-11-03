//! Ressource (ADR-0008, § 5.5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::arena::HasUid;
use crate::ids::{BaselineNumber, CalendarUid, Guid, ResourceUid};
use crate::model::Extras;
use crate::model::baseline::ResourceBaseline;
use crate::quantity::{Money, Rate, Units};

/// Type de ressource.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceKind {
    /// Ressource travail (personne, machine).
    #[default]
    Work,
    /// Ressource matérielle, avec son étiquette d'unité (« m³ »).
    Material {
        /// Étiquette d'unité.
        label: String,
    },
    /// Ressource coût.
    Cost,
}

/// Ressource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    uid: ResourceUid,
    /// GUID conservé tel quel.
    pub guid: Option<Guid>,
    /// Nom.
    pub name: String,
    /// Initiales.
    pub initials: String,
    /// Groupe.
    pub group: String,
    /// Type.
    pub kind: ResourceKind,
    /// Capacité maximale (1/10 000 ; 10 000 = 100 % par défaut, S5).
    pub max_units: Units,
    /// Calendrier propre (dérivé d'un calendrier de base en général).
    pub calendar: Option<CalendarUid>,
    /// Taux standard.
    pub standard_rate: Rate,
    /// Taux des heures supplémentaires.
    pub overtime_rate: Rate,
    /// Coût par utilisation.
    pub cost_per_use: Money,
    /// Ressource générique.
    pub is_generic: bool,
    /// Ressource inactive.
    pub is_inactive: bool,
    /// Notes.
    pub notes: String,
    /// Lignes de base 0 à 10.
    pub baselines: BTreeMap<BaselineNumber, ResourceBaseline>,
    /// Données hors modèle.
    pub extras: Extras,
}

impl HasUid for Resource {
    type Uid = ResourceUid;
    fn uid(&self) -> ResourceUid {
        self.uid
    }
}

impl Resource {
    /// Ressource travail à 100 %, sans calendrier propre ni coût.
    #[must_use]
    pub fn new(uid: ResourceUid, name: String) -> Self {
        Self {
            uid,
            guid: None,
            name,
            initials: String::new(),
            group: String::new(),
            kind: ResourceKind::Work,
            max_units: Units::FULL,
            calendar: None,
            standard_rate: Rate::default(),
            overtime_rate: Rate::default(),
            cost_per_use: Money::ZERO,
            is_generic: false,
            is_inactive: false,
            notes: String::new(),
            baselines: BTreeMap::new(),
            extras: Extras::new(),
        }
    }

    /// UID.
    #[must_use]
    pub const fn uid(&self) -> ResourceUid {
        self.uid
    }
}

/// ID (colonne « ID » de MS Project) de la ressource en position `pos` de
/// `resources` : la ressource UID 0 des fichiers MS Project (cachée, O34) a
/// l'ID 0 et n'est pas comptée ; les autres sont numérotées depuis 1 dans
/// l'ordre de l'arène. Un projet sans UID 0 (créé par PlannerCraft) garde donc
/// ID = position + 1, et un fichier relu garde les ID de MS Project.
#[must_use]
pub fn resource_id(resources: &crate::arena::Arena<ResourceUid, Resource>, pos: usize) -> u64 {
    let hidden = ResourceUid::new(0);
    let rank = u64::try_from(pos).unwrap_or(u64::MAX);
    match resources.position(hidden) {
        Some(h) if h == pos => 0,
        Some(h) if h < pos => rank,
        _ => rank.saturating_add(1),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Resource, resource_id};
    use crate::arena::Arena;
    use crate::ids::ResourceUid;

    fn arena(uids: &[u32]) -> Arena<ResourceUid, Resource> {
        let mut a = Arena::new();
        for &u in uids {
            a.push(Resource::new(ResourceUid::new(u), format!("R{u}")))
                .unwrap();
        }
        a
    }

    #[test]
    fn resource_id_skips_hidden_uid_0() {
        // Fichier MS Project : UID 0 / ID 0, puis UID 1 / ID 1, UID 2 / ID 2.
        let file = arena(&[0, 1, 2]);
        let ids: Vec<u64> = (0..3).map(|p| resource_id(&file, p)).collect();
        assert_eq!(ids, [0, 1, 2]);
        // Projet PlannerCraft, sans UID 0 : position + 1.
        let own = arena(&[1, 2]);
        assert_eq!((resource_id(&own, 0), resource_id(&own, 1)), (1, 2));
    }
}
