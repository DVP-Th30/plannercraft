//! Tables d'une entité (`TBkndTask`, `TBkndCons`, `TBkndCal`…) et jeu de
//! propriétés `Props`, telles qu'observées sur le corpus
//! (`docs/formats/mpp.md` § 3 à § 5). Toute longueur et tout décalage lus
//! sont bornés par les octets du flux.

use std::collections::BTreeMap;

use crate::bytes::{le_u16, le_u32};

/// En-tête de `VarMeta` (octets).
const VAR_META_HEADER: usize = 24;
/// Entrée de `VarMeta`.
const VAR_META_ENTRY: usize = 12;
/// En-tête de `FixedMeta`.
const FIXED_META_HEADER: usize = 16;
/// En-tête de `Props`.
const PROPS_HEADER: usize = 16;

/// Valeurs variables d'une entité : UID → (champ → octets). Les octets sont
/// empruntés à `Var2Data` : plusieurs entrées qui désignent la même valeur
/// ne la recopient pas (aucune amplification d'allocation, règle 10).
pub type VarValues<'a> = BTreeMap<u32, BTreeMap<u32, &'a [u8]>>;

/// Lit `VarMeta` et `Var2Data` : chaque entrée (UID, décalage, champ)
/// désigne dans `Var2Data` une valeur préfixée de sa longueur (`u32`).
/// Une entrée hors des octets est ignorée ; `max_entries` borne le nombre
/// d'entrées lues.
#[must_use]
pub fn var_values<'a>(meta: &[u8], data: &'a [u8], max_entries: usize) -> VarValues<'a> {
    let mut out: VarValues = BTreeMap::new();
    let count = le_u32(meta, 8).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let room = meta.len().saturating_sub(VAR_META_HEADER) / VAR_META_ENTRY;
    for k in 0..count.min(room).min(max_entries) {
        let base = VAR_META_HEADER.saturating_add(k.saturating_mul(VAR_META_ENTRY));
        let (Some(uid), Some(off), Some(field)) = (
            le_u32(meta, base),
            le_u32(meta, base.saturating_add(4)),
            le_u32(meta, base.saturating_add(8)),
        ) else {
            continue;
        };
        let Ok(off) = usize::try_from(off) else { continue };
        let Some(len) = le_u32(data, off).and_then(|l| usize::try_from(l).ok()) else {
            continue;
        };
        let Some(value) = off
            .checked_add(4)
            .and_then(|s| Some((s, s.checked_add(len)?)))
            .and_then(|(s, e)| data.get(s..e))
        else {
            continue;
        };
        out.entry(uid).or_default().insert(field, value);
    }
    out
}

/// Élément de `FixedMeta` et l'enregistrement qu'il désigne.
#[derive(Clone, Copy, Debug)]
pub struct FixedItem<'a> {
    /// Octets de l'élément de méta (indicateurs à l'octet 0, décalage à
    /// l'octet 4).
    pub meta: &'a [u8],
    /// Indicateurs (`u16` à l'octet 0) : bit 1 = enregistrement supprimé,
    /// bit 2 = enregistrement court (16 octets : ligne vide ou réservé).
    pub flags: u16,
    /// Octets de l'enregistrement, de son décalage à la fin de `FixedData`
    /// (l'appelant en prend la longueur de sa disposition).
    pub data: &'a [u8],
}

impl FixedItem<'_> {
    /// Enregistrement supprimé (bit 1 des indicateurs).
    #[must_use]
    pub const fn deleted(&self) -> bool {
        self.flags & 0x2 != 0
    }

    /// Enregistrement court (bit 2 des indicateurs).
    #[must_use]
    pub const fn short(&self) -> bool {
        self.flags & 0x4 != 0
    }
}

/// Lit `FixedMeta` : en-tête de 16 octets (nombre d'éléments à l'octet 8),
/// puis des éléments de taille constante `(longueur − 16) / nombre`.
/// `max_items` borne le nombre d'éléments rendus.
#[must_use]
pub fn fixed_items<'a>(meta: &'a [u8], data: &'a [u8], max_items: usize) -> Vec<FixedItem<'a>> {
    let mut out = Vec::new();
    let count = le_u32(meta, 8).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    if count == 0 {
        return out;
    }
    let item_len = meta
        .len()
        .saturating_sub(FIXED_META_HEADER)
        .checked_div(count)
        .unwrap_or(0);
    if item_len < 8 {
        return out;
    }
    for k in 0..count.min(max_items) {
        let start = FIXED_META_HEADER.saturating_add(k.saturating_mul(item_len));
        let Some(item) = start.checked_add(item_len).and_then(|end| meta.get(start..end)) else {
            break;
        };
        let (Some(flags), Some(off)) = (le_u16(item, 0), le_u32(item, 4)) else {
            continue;
        };
        let Some(record) = usize::try_from(off).ok().and_then(|o| data.get(o..)) else {
            continue;
        };
        out.push(FixedItem {
            meta: item,
            flags,
            data: record,
        });
    }
    out
}

/// Lit `Props` : en-tête de 16 octets, puis des entrées
/// `[u32 taille][u32 clé][u32 ?][taille octets]`. La première clé gagne.
#[must_use]
pub fn props(data: &[u8], max_entries: usize) -> BTreeMap<u32, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut off = PROPS_HEADER;
    for _ in 0..max_entries {
        let (Some(size), Some(key)) = (le_u32(data, off), le_u32(data, off.saturating_add(4))) else {
            break;
        };
        let Ok(size) = usize::try_from(size) else { break };
        let Some(start) = off.checked_add(12) else { break };
        let Some(end) = start.checked_add(size) else { break };
        let Some(value) = data.get(start..end) else { break };
        out.entry(key).or_insert_with(|| value.to_vec());
        off = end;
    }
    out
}
