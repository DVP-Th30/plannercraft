//! Collection ordonnée d'entités indexée par UID (ADR-0008).
//!
//! Un `Vec` dans l'ordre d'affichage (l'ordre des ID), plus un index
//! UID → position. Sérialisée comme un tableau JSON ; la lecture refuse un UID
//! en double. L'UID d'un élément n'a pas de setter : l'index reste juste.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use serde::de::{Deserializer, Error as _};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::error::ModelError;
use crate::ids::EntityUid;

/// Entité qui porte un UID.
pub trait HasUid {
    /// Type de l'UID.
    type Uid: Ord + Copy;
    /// UID de l'entité.
    fn uid(&self) -> Self::Uid;
}

/// Entités dans l'ordre d'affichage, avec un index UID → position.
///
/// L'index est reconstruit en O(n log n) après [`Arena::insert`] ou
/// [`Arena::remove`] (chemin froid, un élément) ; [`Arena::splice`] et
/// [`Arena::reorder`] modifient un bloc ou tout l'ordre en une seule passe
/// (ADR-0012, D13). L'allocation des UID (monotone, jamais réutilisés) relève
/// de `plannercraft-automation` (R-151).
#[derive(Clone, PartialEq, Eq)]
pub struct Arena<K, T> {
    items: Vec<T>,
    index: BTreeMap<K, usize>,
}

impl<K: fmt::Debug, T: fmt::Debug> fmt::Debug for Arena<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.items).finish()
    }
}

impl<K: EntityUid, T: HasUid<Uid = K>> Default for Arena<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: EntityUid, T: HasUid<Uid = K>> Arena<K, T> {
    /// Collection vide.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            index: BTreeMap::new(),
        }
    }

    /// Nombre d'éléments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Vrai si la collection est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Élément d'UID `uid`.
    #[must_use]
    pub fn get(&self, uid: K) -> Option<&T> {
        self.index.get(&uid).and_then(|&p| self.items.get(p))
    }

    /// Élément modifiable d'UID `uid` (l'UID lui-même n'est pas modifiable).
    pub fn get_mut(&mut self, uid: K) -> Option<&mut T> {
        let position = *self.index.get(&uid)?;
        self.items.get_mut(position)
    }

    /// Vrai si un élément porte l'UID `uid`.
    #[must_use]
    pub fn contains(&self, uid: K) -> bool {
        self.index.contains_key(&uid)
    }

    /// Position (0 = premier) de l'élément d'UID `uid`.
    #[must_use]
    pub fn position(&self, uid: K) -> Option<usize> {
        self.index.get(&uid).copied()
    }

    /// Élément à la position donnée.
    #[must_use]
    pub fn at(&self, position: usize) -> Option<&T> {
        self.items.get(position)
    }

    /// Éléments dans l'ordre.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// Ajoute un élément à la fin.
    ///
    /// # Errors
    ///
    /// [`ModelError::DuplicateUid`] si l'UID existe déjà.
    pub fn push(&mut self, item: T) -> Result<(), ModelError> {
        let uid = item.uid();
        if self.index.contains_key(&uid) {
            return Err(duplicate(uid));
        }
        self.index.insert(uid, self.items.len());
        self.items.push(item);
        Ok(())
    }

    /// Insère un élément à la position donnée (`position == len()` ajoute à la fin).
    ///
    /// # Errors
    ///
    /// [`ModelError::DuplicateUid`] si l'UID existe déjà,
    /// [`ModelError::PositionOutOfRange`] si `position > len()`.
    pub fn insert(&mut self, position: usize, item: T) -> Result<(), ModelError> {
        let uid = item.uid();
        if self.index.contains_key(&uid) {
            return Err(duplicate(uid));
        }
        if position > self.items.len() {
            return Err(ModelError::PositionOutOfRange { position });
        }
        self.items.insert(position, item);
        self.rebuild_index();
        Ok(())
    }

    /// Retire l'élément d'UID `uid` et le rend.
    pub fn remove(&mut self, uid: K) -> Option<T> {
        let position = self.index.get(&uid).copied()?;
        if position >= self.items.len() {
            return None;
        }
        let item = self.items.remove(position);
        self.rebuild_index();
        Some(item)
    }

    /// UID des éléments, dans l'ordre d'affichage.
    pub fn uids(&self) -> impl Iterator<Item = K> + '_ {
        self.items.iter().map(HasUid::uid)
    }

    /// Remplace les éléments des positions `range` par `items` et rend les
    /// éléments retirés, dans leur ordre.
    ///
    /// Insertion d'un bloc : `range` vide (`p..p`) ; retrait : `items` vide.
    /// Une seule passe : l'index n'est mis à jour qu'une fois, à partir de
    /// `range.start` (au plus O(n log n), quel que soit le nombre d'éléments
    /// du bloc). Un refus ne modifie rien.
    ///
    /// # Errors
    ///
    /// [`ModelError::PositionOutOfRange`] si `range.start > range.end` ou
    /// `range.end > len()` (position rendue : la borne fautive) ;
    /// [`ModelError::DuplicateUid`] si deux éléments de `items` portent le même
    /// UID, ou si l'un porte l'UID d'un élément gardé (hors de `range`).
    pub fn splice(&mut self, range: Range<usize>, items: Vec<T>) -> Result<Vec<T>, ModelError> {
        let Range { start, end } = range;
        if end > self.items.len() {
            return Err(ModelError::PositionOutOfRange { position: end });
        }
        if start > end {
            return Err(ModelError::PositionOutOfRange { position: start });
        }
        let mut incoming = BTreeSet::new();
        for item in &items {
            let uid = item.uid();
            let kept = self.index.get(&uid).is_some_and(|&p| !(start..end).contains(&p));
            if kept || !incoming.insert(uid) {
                return Err(duplicate(uid));
            }
        }
        let removed: Vec<T> = self.items.splice(start..end, items).collect();
        for item in &removed {
            self.index.remove(&item.uid());
        }
        self.reindex_from(start);
        Ok(removed)
    }

    /// Réordonne la collection selon `order`, qui doit contenir chaque UID
    /// présent exactement une fois. Une seule passe (O(n log n)) ; un refus ne
    /// modifie rien.
    ///
    /// # Errors
    ///
    /// [`ModelError::NotAPermutation`] si `order` n'a pas la longueur de la
    /// collection, contient un UID absent ou répète un UID.
    pub fn reorder(&mut self, order: &[K]) -> Result<(), ModelError> {
        let len = self.items.len();
        let refuse = |uid: Option<K>| ModelError::NotAPermutation {
            entity: K::ENTITY,
            expected: len,
            given: order.len(),
            uid: uid.map(EntityUid::raw),
        };
        if order.len() != len {
            return Err(refuse(None));
        }
        // rang voulu de chaque position actuelle ; `None` = pas encore vue.
        let mut rank: Vec<Option<usize>> = vec![None; len];
        for (r, &uid) in order.iter().enumerate() {
            let slot = self
                .index
                .get(&uid)
                .and_then(|&p| rank.get_mut(p))
                .ok_or_else(|| refuse(Some(uid)))?;
            if slot.replace(r).is_some() {
                return Err(refuse(Some(uid)));
            }
        }
        // longueurs égales et aucun doublon : chaque position a son rang.
        let mut keyed: Vec<(Option<usize>, T)> =
            rank.into_iter().zip(std::mem::take(&mut self.items)).collect();
        keyed.sort_unstable_by_key(|(r, _)| *r);
        self.items = keyed.into_iter().map(|(_, item)| item).collect();
        self.reindex_from(0);
        Ok(())
    }

    /// Plus grand UID présent.
    #[must_use]
    pub fn max_uid(&self) -> Option<K> {
        self.index.last_key_value().map(|(k, _)| *k)
    }

    /// Met à jour l'index des positions `from..` (les UID retirés ont déjà
    /// quitté l'index).
    fn reindex_from(&mut self, from: usize) {
        for (position, item) in self.items.iter().enumerate().skip(from) {
            self.index.insert(item.uid(), position);
        }
    }

    fn rebuild_index(&mut self) {
        self.index = self
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| (item.uid(), position))
            .collect();
    }
}

fn duplicate<K: EntityUid>(uid: K) -> ModelError {
    ModelError::DuplicateUid {
        entity: K::ENTITY,
        uid: uid.raw(),
    }
}

impl<'a, K, T> IntoIterator for &'a Arena<K, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<K: EntityUid, T: HasUid<Uid = K>> TryFrom<Vec<T>> for Arena<K, T> {
    type Error = ModelError;
    fn try_from(items: Vec<T>) -> Result<Self, Self::Error> {
        let mut index = BTreeMap::new();
        for (position, item) in items.iter().enumerate() {
            let uid = item.uid();
            if index.insert(uid, position).is_some() {
                return Err(duplicate(uid));
            }
        }
        Ok(Self { items, index })
    }
}

impl<K, T> From<Arena<K, T>> for Vec<T> {
    fn from(arena: Arena<K, T>) -> Self {
        arena.items
    }
}

impl<K, T: Serialize> Serialize for Arena<K, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(serializer)
    }
}

impl<'de, K: EntityUid, T: HasUid<Uid = K> + Deserialize<'de>> Deserialize<'de> for Arena<K, T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let items = Vec::<T>::deserialize(deserializer)?;
        Self::try_from(items).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use proptest::prelude::*;
    use serde::{Deserialize, Serialize};

    use super::{Arena, HasUid};
    use crate::error::{EntityKind, ModelError};
    use crate::ids::{EntityUid, ResourceUid};

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Item {
        uid: ResourceUid,
        name: String,
    }

    impl HasUid for Item {
        type Uid = ResourceUid;
        fn uid(&self) -> ResourceUid {
            self.uid
        }
    }

    fn item(uid: u32, name: &str) -> Item {
        Item {
            uid: ResourceUid::new(uid),
            name: name.to_owned(),
        }
    }

    fn names(a: &Arena<ResourceUid, Item>) -> Vec<&str> {
        a.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn push_insert_remove_keep_index() {
        let mut a = Arena::new();
        assert!(a.is_empty());
        a.push(item(5, "a")).unwrap();
        a.push(item(2, "b")).unwrap();
        assert_eq!(
            a.push(item(5, "dup")),
            Err(ModelError::DuplicateUid {
                entity: EntityKind::Resource,
                uid: 5
            })
        );
        a.insert(0, item(9, "c")).unwrap();
        assert_eq!(names(&a), ["c", "a", "b"]);
        assert_eq!(a.position(ResourceUid::new(5)), Some(1));
        assert_eq!(a.position(ResourceUid::new(2)), Some(2));
        assert_eq!(a.at(0).unwrap().name, "c");
        assert_eq!(
            a.insert(4, item(10, "x")),
            Err(ModelError::PositionOutOfRange { position: 4 })
        );
        assert!(a.insert(1, item(2, "dup")).is_err());
        a.insert(3, item(10, "d")).unwrap();
        assert_eq!(a.max_uid(), Some(ResourceUid::new(10)));
        assert_eq!(a.remove(ResourceUid::new(5)).unwrap().name, "a");
        assert_eq!(a.remove(ResourceUid::new(5)), None);
        assert_eq!(names(&a), ["c", "b", "d"]);
        assert_eq!(a.position(ResourceUid::new(2)), Some(1));
        assert_eq!(a.position(ResourceUid::new(10)), Some(2));
        assert!(!a.contains(ResourceUid::new(5)));
        a.get_mut(ResourceUid::new(2)).unwrap().name = "B".to_owned();
        assert_eq!(a.get(ResourceUid::new(2)).unwrap().name, "B");
        assert_eq!(a.len(), 3);
        assert_eq!(Arena::<ResourceUid, Item>::new().max_uid(), None);
    }

    #[test]
    fn serde_is_an_array_and_refuses_duplicates() {
        let a = Arena::try_from(vec![item(3, "a"), item(1, "b")]).unwrap();
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#"[{"uid":3,"name":"a"},{"uid":1,"name":"b"}]"#);
        assert!(
            serde_json::from_str::<Arena<ResourceUid, Item>>(
                r#"[{"uid":3,"name":"a"},{"uid":3,"name":"b"}]"#
            )
            .is_err()
        );
        assert_eq!(
            Arena::try_from(vec![item(3, "a"), item(3, "b")]),
            Err(ModelError::DuplicateUid {
                entity: EntityKind::Resource,
                uid: 3
            })
        );
        let v: Vec<Item> = a.into();
        assert_eq!(v.len(), 2);
    }

    fn arena(uids: &[u32]) -> Arena<ResourceUid, Item> {
        Arena::try_from(
            uids.iter()
                .map(|&u| item(u, &format!("n{u}")))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn raw_uids(a: &Arena<ResourceUid, Item>) -> Vec<u32> {
        a.uids().map(EntityUid::raw).collect()
    }

    /// L'index est cohérent avec l'ordre (`position`, `get`, `contains`).
    fn assert_index(a: &Arena<ResourceUid, Item>) {
        for (p, it) in a.iter().enumerate() {
            assert_eq!(a.position(it.uid), Some(p));
            assert_eq!(a.get(it.uid), Some(it));
        }
        assert_eq!(a.index.len(), a.len());
    }

    #[test]
    fn arena_splice_insert_block() {
        let mut a = arena(&[1, 2, 3]);
        let removed = a.splice(1..1, vec![item(8, "x"), item(9, "y")]).unwrap();
        assert_eq!(removed, []);
        assert_eq!(raw_uids(&a), [1, 8, 9, 2, 3]);
        assert_index(&a);
        // à la fin et au début
        a.splice(5..5, vec![item(10, "z")]).unwrap();
        a.splice(0..0, vec![item(11, "w")]).unwrap();
        assert_eq!(raw_uids(&a), [11, 1, 8, 9, 2, 3, 10]);
        assert_index(&a);
        // remplacement d'un bloc par un autre de taille différente, en
        // réutilisant un UID retiré
        let removed = a.splice(2..5, vec![item(9, "y2")]).unwrap();
        assert_eq!(removed.iter().map(|i| i.uid.raw()).collect::<Vec<_>>(), [8, 9, 2]);
        assert_eq!(raw_uids(&a), [11, 1, 9, 3, 10]);
        assert_eq!(a.get(ResourceUid::new(9)).unwrap().name, "y2");
        assert!(!a.contains(ResourceUid::new(2)));
        assert_index(&a);
    }

    #[test]
    fn arena_splice_remove_block() {
        let mut a = arena(&[1, 2, 3, 4, 5]);
        let removed = a.splice(1..4, Vec::new()).unwrap();
        assert_eq!(removed.iter().map(|i| i.uid.raw()).collect::<Vec<_>>(), [2, 3, 4]);
        assert_eq!(raw_uids(&a), [1, 5]);
        assert_index(&a);
        for u in [2, 3, 4] {
            assert!(!a.contains(ResourceUid::new(u)));
        }
        let all = a.splice(0..2, Vec::new()).unwrap();
        assert_eq!(all.len(), 2);
        assert!(a.is_empty());
        assert_eq!(a.max_uid(), None);
        // déplacement d'un bloc par deux splices : retrait puis réinsertion
        let mut b = arena(&[1, 2, 3, 4, 5]);
        let block = b.splice(3..5, Vec::new()).unwrap();
        b.splice(1..1, block).unwrap();
        assert_eq!(raw_uids(&b), [1, 4, 5, 2, 3]);
        assert_index(&b);
    }

    #[test]
    fn arena_splice_duplicate_refused_untouched() {
        let original = arena(&[1, 2, 3, 4]);
        let mut a = original.clone();
        // doublon dans `items`
        assert_eq!(
            a.splice(1..2, vec![item(7, "a"), item(7, "b")]),
            Err(ModelError::DuplicateUid {
                entity: EntityKind::Resource,
                uid: 7
            })
        );
        assert_eq!(a, original);
        // doublon avec un élément gardé
        assert_eq!(
            a.splice(1..2, vec![item(4, "a")]),
            Err(ModelError::DuplicateUid {
                entity: EntityKind::Resource,
                uid: 4
            })
        );
        assert_eq!(a, original);
        // bornes
        assert_eq!(
            a.splice(2..5, Vec::new()),
            Err(ModelError::PositionOutOfRange { position: 5 })
        );
        #[allow(
            clippy::reversed_empty_ranges,
            reason = "test : plage inversée refusée volontairement"
        )]
        let reversed = 3..2;
        assert_eq!(
            a.splice(reversed, vec![item(9, "x")]),
            Err(ModelError::PositionOutOfRange { position: 3 })
        );
        assert_eq!(a, original);
        // un UID du bloc retiré peut revenir
        a.splice(1..2, vec![item(2, "b2")]).unwrap();
        assert_eq!(raw_uids(&a), [1, 2, 3, 4]);
    }

    #[test]
    fn arena_reorder_permutation() {
        let mut a = arena(&[1, 2, 3, 4]);
        let order = [3, 1, 4, 2].map(ResourceUid::new);
        a.reorder(&order).unwrap();
        assert_eq!(raw_uids(&a), [3, 1, 4, 2]);
        assert_eq!(names(&a), ["n3", "n1", "n4", "n2"]);
        assert_index(&a);
        // identité et collection vide
        a.reorder(&order).unwrap();
        assert_eq!(raw_uids(&a), [3, 1, 4, 2]);
        let mut e = Arena::<ResourceUid, Item>::new();
        e.reorder(&[]).unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn arena_reorder_not_permutation_refused() {
        let original = arena(&[1, 2, 3]);
        let mut a = original.clone();
        let refused = |given, uid| {
            Err(ModelError::NotAPermutation {
                entity: EntityKind::Resource,
                expected: 3,
                given,
                uid,
            })
        };
        let r = |v: &[u32]| v.iter().map(|&u| ResourceUid::new(u)).collect::<Vec<_>>();
        assert_eq!(a.reorder(&r(&[1, 2])), refused(2, None));
        assert_eq!(a.reorder(&r(&[1, 2, 3, 1])), refused(4, None));
        assert_eq!(a.reorder(&r(&[1, 2, 9])), refused(3, Some(9)));
        assert_eq!(a.reorder(&r(&[1, 2, 1])), refused(3, Some(1)));
        assert_eq!(a, original);
        assert_eq!(
            a.reorder(&r(&[1, 1, 1])).unwrap_err().code(),
            "model.not_a_permutation"
        );
    }

    /// Opération de splice tirée au hasard, appliquée à une arène de départ.
    fn splice_case() -> impl Strategy<Value = (Vec<u32>, usize, usize, Vec<u32>)> {
        // UID de départ distincts (1..=40), bloc entrant dans 1..=60 (doublons
        // possibles, avec lui-même ou le reste).
        (
            proptest::sample::subsequence((1..=40_u32).collect::<Vec<_>>(), 0..=12).prop_shuffle(),
            0_usize..=14,
            0_usize..=14,
            proptest::collection::vec(1..=60_u32, 0..=6),
        )
    }

    /// Splice naïf : retraits puis insertions élément par élément.
    fn naive_splice(
        a: &mut Arena<ResourceUid, Item>,
        start: usize,
        end: usize,
        incoming: Vec<Item>,
    ) -> Result<Vec<Item>, ModelError> {
        if end > a.len() {
            return Err(ModelError::PositionOutOfRange { position: end });
        }
        if start > end {
            return Err(ModelError::PositionOutOfRange { position: start });
        }
        let snapshot = a.clone();
        let doomed: Vec<ResourceUid> = (start..end).map(|p| a.at(p).unwrap().uid).collect();
        let removed: Vec<Item> = doomed.iter().map(|&u| a.remove(u).unwrap()).collect();
        for (position, it) in (start..).zip(incoming) {
            if let Err(e) = a.insert(position, it) {
                *a = snapshot;
                return Err(e);
            }
        }
        Ok(removed)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]
        /// `splice` donne le même résultat (collection, retirés, erreur) que
        /// des `remove` et `insert` successifs.
        #[test]
        fn arena_splice_matches_naive((start_uids, x, y, incoming) in splice_case()) {
            let base = arena(&start_uids);
            let (start, end) = (x.min(y), x.max(y));
            let items: Vec<Item> = incoming
                .iter()
                .enumerate()
                .map(|(k, &u)| item(u, &format!("in{k}")))
                .collect();
            let mut fast = base.clone();
            let mut slow = base.clone();
            let got = fast.splice(start..end, items.clone());
            let want = naive_splice(&mut slow, start, end, items);
            prop_assert_eq!(got.is_ok(), want.is_ok());
            if let (Err(g), Err(w)) = (&got, &want) {
                // même nature d'erreur ; l'UID signalé peut différer selon
                // l'ordre du contrôle
                prop_assert_eq!(g.code(), w.code());
                prop_assert_eq!(&fast, &base);
            } else {
                prop_assert_eq!(&got, &want);
            }
            prop_assert_eq!(&fast, &slow);
            assert_index(&fast);
        }
    }
}
