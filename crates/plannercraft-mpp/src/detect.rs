//! Détection du format et de la disposition (R-281, R-282).
//!
//! Observé (`docs/formats/mpp.md` § 2) : à la racine du conteneur, un
//! dossier nommé d'espaces suivis d'un numéro porte les données du projet
//! (« `   114` » MPP14, « `   112` » MPP12, « `   19` » et « `   29` »
//! MPP9, « `   1` » et « `   2` » MPP8) ; le flux `Props14` de la racine
//! porte la version de l'application qui a enregistré le fichier
//! (« 14,0,7011,1000 », « 16,0,10730,20102 ») ; la disposition des
//! enregistrements de tâches, de liens et de calendriers dépend de son
//! numéro majeur (14 : Project 2010 ; 15 et plus : Project 2013 et suivants).

use serde::Serialize;

use crate::MppError;
use crate::cfb::{Cfb, EntryKind};

/// Format du fichier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    /// MPP14 : Project 2010 à 2021, Microsoft 365.
    Mpp14,
    /// MPP12 : Project 2007.
    Mpp12,
    /// MPP9 : Project 2000 à 2003.
    Mpp9,
    /// MPP8 : Project 98.
    Mpp8,
}

impl FileFormat {
    /// Nom court.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mpp14 => "MPP14",
            Self::Mpp12 => "MPP12",
            Self::Mpp9 => "MPP9",
            Self::Mpp8 => "MPP8",
        }
    }

    /// Versions de MS Project qui écrivent ce format.
    #[must_use]
    pub const fn versions(self) -> &'static str {
        match self {
            Self::Mpp14 => "Project 2010 à 2021, Microsoft 365",
            Self::Mpp12 => "Project 2007",
            Self::Mpp9 => "Project 2000 à 2003",
            Self::Mpp8 => "Project 98",
        }
    }
}

/// Disposition des enregistrements d'un MPP14.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Enregistré par Project 2010 (version 14).
    Project2010,
    /// Enregistré par Project 2013 et suivants (version 15 et plus).
    Project2013,
}

/// Résultat de la détection.
#[derive(Clone, Debug)]
pub struct Found {
    /// Format.
    pub format: FileFormat,
    /// Disposition.
    pub layout: Layout,
    /// Version de l'application (« 16,0,10730,20102 »).
    pub saved_by: String,
    /// Index du dossier des données (« `   114` »).
    pub data_dir: usize,
}

/// Format d'après les dossiers de la racine, `None` si aucun n'est connu.
#[must_use]
pub fn format_of(cfb: &Cfb<'_>) -> Option<(FileFormat, usize)> {
    let mut best: Option<(FileFormat, usize)> = None;
    for i in cfb.children(0) {
        let Some(e) = cfb.entry(i) else { continue };
        if e.kind != EntryKind::Storage {
            continue;
        }
        let f = match e.name.trim_start_matches(' ') {
            "114" => FileFormat::Mpp14,
            "112" => FileFormat::Mpp12,
            "19" | "29" => FileFormat::Mpp9,
            "1" | "2" => FileFormat::Mpp8,
            _ => continue,
        };
        // Le plus récent l'emporte (un MPP14 garde-t-il d'anciens dossiers ?
        // inconnu : jamais observé).
        if best.is_none_or(|(b, _)| rank(f) > rank(b)) {
            best = Some((f, i));
        }
    }
    best
}

const fn rank(f: FileFormat) -> u8 {
    match f {
        FileFormat::Mpp8 => 0,
        FileFormat::Mpp9 => 1,
        FileFormat::Mpp12 => 2,
        FileFormat::Mpp14 => 3,
    }
}

/// Première version « NN,0,NNNN,NNNN » du texte UTF-16 de `Props14`.
#[must_use]
pub fn saved_by(props14: &[u8]) -> Option<(u32, String)> {
    let text = crate::bytes::utf16_z_all(props14, 4096);
    let mut previous_digit = false;
    for (start, c) in text.char_indices() {
        let after_digit = previous_digit;
        previous_digit = c.is_ascii_digit();
        if after_digit || !c.is_ascii_digit() {
            continue;
        }
        let Some(rest) = text.get(start..) else { continue };
        let candidate: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .collect();
        let parts: Vec<&str> = candidate.split(',').collect();
        if let [major, minor, build, rev] = parts.as_slice()
            && minor == &"0"
            && !major.is_empty()
            && !build.is_empty()
            && !rev.is_empty()
            && major.len() <= 3
            && let Ok(m) = major.parse::<u32>()
        {
            return Some((m, candidate));
        }
    }
    None
}

/// Format, disposition, version.
///
/// # Errors
///
/// [`MppError::NotProject`] sans dossier connu, [`MppError::Unsupported`]
/// pour un format reconnu autre que MPP14, [`MppError::UnknownLayout`] sans
/// version lisible.
pub fn detect(cfb: &Cfb<'_>, max_stream: usize) -> Result<Found, MppError> {
    let (format, data_dir) = format_of(cfb).ok_or(MppError::NotProject)?;
    if format != FileFormat::Mpp14 {
        return Err(MppError::Unsupported(format));
    }
    let props14 = cfb
        .child(0, "Props14")
        .and_then(|i| cfb.read_stream(i, max_stream.min(1 << 20)).ok())
        .ok_or(MppError::UnknownLayout)?;
    let (major, saved_by) = saved_by(&props14).ok_or(MppError::UnknownLayout)?;
    let layout = match major {
        14 => Layout::Project2010,
        15..=99 => Layout::Project2013,
        _ => return Err(MppError::UnknownLayout),
    };
    Ok(Found {
        format,
        layout,
        saved_by,
        data_dir,
    })
}
