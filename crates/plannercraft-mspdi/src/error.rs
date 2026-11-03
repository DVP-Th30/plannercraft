//! Erreurs de lecture d'un document MSPDI et positions dans l'entrée.
//!
//! [`MspdiError`] n'est rendue que si le document est structurellement
//! inexploitable (vide, trop grand, mal formé, trop profond, autre racine) ;
//! tout le reste devient un diagnostic d'import (`crate::diag`).

use std::cell::OnceCell;
use std::fmt;

use plannercraft_core::DiagArg;

/// Position dans l'entrée : octet (depuis 0), ligne et colonne (depuis 1,
/// colonne comptée en caractères). Calculée à la construction de l'erreur ou
/// du diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    /// Décalage en octets depuis le début de l'entrée (après le BOM éventuel).
    pub byte: u64,
    /// Ligne, depuis 1.
    pub line: u32,
    /// Colonne en caractères, depuis 1.
    pub column: u32,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ligne {}, colonne {} (octet {})",
            self.line, self.column, self.byte
        )
    }
}

/// Erreur de lecture : document inexploitable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MspdiError {
    /// Entrée vide.
    #[error("document vide")]
    Empty,
    /// Entrée plus grande que la limite.
    #[error("document trop grand : {size} octets (limite {max})")]
    TooLarge {
        /// Taille de l'entrée.
        size: u64,
        /// Limite.
        max: u64,
    },
    /// Encodage UTF-16 (BOM) : non pris en charge, MS Project écrit en UTF-8.
    #[error("document en UTF-16 : seul l'UTF-8 est pris en charge")]
    Utf16NotSupported,
    /// Octets qui ne sont pas de l'UTF-8.
    #[error("UTF-8 invalide à la {at}")]
    InvalidUtf8 {
        /// Premier octet invalide.
        at: Location,
    },
    /// XML mal formé.
    #[error("XML mal formé à la {at} : {message}")]
    Xml {
        /// Position de l'erreur.
        at: Location,
        /// Message de l'analyseur.
        message: String,
    },
    /// Fin de l'entrée avec des éléments encore ouverts (fichier tronqué).
    /// Sans élément ouvert, pas de parenthèse vide (N10).
    #[error("fin inattendue du document à la {at}{}", open_elements(open))]
    UnexpectedEof {
        /// Fin de l'entrée.
        at: Location,
        /// Chemin des éléments ouverts, de la racine au plus profond.
        open: Vec<String>,
    },
    /// Déclaration DOCTYPE refusée (entités, expansion).
    #[error("déclaration DOCTYPE refusée à la {at}")]
    DoctypeNotAllowed {
        /// Position de la déclaration.
        at: Location,
    },
    /// Imbrication plus profonde que la limite.
    #[error("imbrication trop profonde à la {at} (limite {max} niveaux)")]
    TooDeep {
        /// Élément qui dépasse la limite.
        at: Location,
        /// Limite.
        max: u16,
    },
    /// Plus d'éléments que la limite.
    #[error("trop d'éléments (limite {max})")]
    TooManyElements {
        /// Limite.
        max: u64,
    },
    /// Plus d'éléments retenus (construits en mémoire) que la limite.
    #[error("trop d'éléments retenus à la {at} (limite {max})")]
    TooManyRetained {
        /// Élément qui dépasse la limite.
        at: Location,
        /// Limite.
        max: u64,
    },
    /// Texte d'un élément plus long que la limite.
    #[error("texte trop long à la {at} (limite {max} octets)")]
    TextTooLong {
        /// Texte en cause.
        at: Location,
        /// Limite.
        max: usize,
    },
    /// La racine n'est pas `Project` : ce n'est pas un document MSPDI.
    #[error("pas un document MSPDI : racine « {root} » au lieu de « Project »")]
    NotMspdi {
        /// Nom local de la racine trouvée.
        root: String,
    },
}

impl MspdiError {
    /// Code stable (`mspdi.error.too_deep`…).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "mspdi.error.empty",
            Self::TooLarge { .. } => "mspdi.error.too_large",
            Self::Utf16NotSupported => "mspdi.error.utf16_not_supported",
            Self::InvalidUtf8 { .. } => "mspdi.error.invalid_utf8",
            Self::Xml { .. } => "mspdi.error.xml",
            Self::UnexpectedEof { .. } => "mspdi.error.unexpected_eof",
            Self::DoctypeNotAllowed { .. } => "mspdi.error.doctype_not_allowed",
            Self::TooDeep { .. } => "mspdi.error.too_deep",
            Self::TooManyElements { .. } => "mspdi.error.too_many_elements",
            Self::TooManyRetained { .. } => "mspdi.error.too_many_retained",
            Self::TextTooLong { .. } => "mspdi.error.text_too_long",
            Self::NotMspdi { .. } => "mspdi.error.not_mspdi",
        }
    }
}

impl Location {
    /// Position en argument neutre de diagnostic.
    #[must_use]
    pub const fn arg(&self) -> DiagArg {
        DiagArg::Position {
            line: self.line,
            column: self.column,
            byte: self.byte,
        }
    }
}

/// Traduction par code (N9, J8 lot L14) : arguments neutres, en plus du
/// `Display` français qui ne change pas.
impl MspdiError {
    /// Arguments nommés : `at` (position), `size`, `max`, `message`
    /// (message de l'analyseur XML, non traduit), `open` (éléments ouverts,
    /// « a/b ») et `depth` (leur nombre), `root`.
    #[must_use]
    pub fn args(&self) -> Vec<(&'static str, DiagArg)> {
        match self {
            Self::Empty | Self::Utf16NotSupported => Vec::new(),
            Self::TooLarge { size, max } => {
                vec![("size", DiagArg::count64(*size)), ("max", DiagArg::count64(*max))]
            }
            Self::InvalidUtf8 { at } | Self::DoctypeNotAllowed { at } => vec![("at", at.arg())],
            Self::Xml { at, message } => vec![("at", at.arg()), ("message", DiagArg::Text(message.clone()))],
            Self::UnexpectedEof { at, open } => vec![
                ("at", at.arg()),
                ("open", DiagArg::Text(open.join("/"))),
                ("depth", DiagArg::count(open.len())),
            ],
            Self::TooDeep { at, max } => vec![("at", at.arg()), ("max", DiagArg::Int(i64::from(*max)))],
            Self::TooManyElements { max } => vec![("max", DiagArg::count64(*max))],
            Self::TooManyRetained { at, max } => vec![("at", at.arg()), ("max", DiagArg::count64(*max))],
            Self::TextTooLong { at, max } => vec![("at", at.arg()), ("max", DiagArg::count(*max))],
            Self::NotMspdi { root } => vec![("root", DiagArg::Text(root.clone()))],
        }
    }

    /// Un exemple de chaque variante, dans l'ordre de déclaration, pour les
    /// contrôles de couverture des traductions.
    #[must_use]
    pub fn examples() -> Vec<Self> {
        let at = Location {
            byte: 120,
            line: 4,
            column: 9,
        };
        vec![
            Self::Empty,
            Self::TooLarge { size: 300, max: 200 },
            Self::Utf16NotSupported,
            Self::InvalidUtf8 { at },
            Self::Xml {
                at,
                message: "x".to_owned(),
            },
            Self::UnexpectedEof {
                at,
                open: vec!["Project".to_owned(), "Tasks".to_owned()],
            },
            Self::DoctypeNotAllowed { at },
            Self::TooDeep { at, max: 64 },
            Self::TooManyElements { max: 1_000 },
            Self::TooManyRetained { at, max: 1_000 },
            Self::TextTooLong { at, max: 1_000 },
            Self::NotMspdi {
                root: "html".to_owned(),
            },
        ]
    }
}

/// Suffixe « (éléments ouverts : a/b) » du message de fin inattendue ; vide
/// si aucun élément n'est ouvert (N10, `docs/ui-needs.md`).
fn open_elements(open: &[String]) -> String {
    if open.is_empty() {
        String::new()
    } else {
        format!(" (éléments ouverts : {})", open.join("/"))
    }
}

/// Taille d'un bloc de l'index des caractères.
const BLOCK: usize = 4_096;

/// Vrai pour un octet qui commence un caractère UTF-8 (pas une suite).
const fn starts_char(b: u8) -> bool {
    b & 0xC0 != 0x80
}

/// Index d'un texte : débuts de ligne et nombre de caractères avant chaque
/// bloc de 4 Kio. Construit en un parcours, au premier besoin seulement (une
/// lecture sans diagnostic ne le paie pas) ; `locate` coûte ensuite une
/// dichotomie et le parcours d'au plus deux blocs, même sur un fichier d'une
/// seule ligne.
pub(crate) struct LineIndex<'a> {
    text: &'a str,
    index: OnceCell<Index>,
}

/// Index construit.
struct Index {
    /// Octet de début de chaque ligne ; le premier vaut 0.
    starts: Vec<usize>,
    /// Caractères dans `[0, i × BLOCK)`.
    blocks: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Index paresseux du texte.
    pub(crate) const fn new(text: &'a str) -> Self {
        Self {
            text,
            index: OnceCell::new(),
        }
    }

    /// Un parcours de l'entrée.
    fn build(&self) -> Index {
        let bytes = self.text.as_bytes();
        let mut starts = vec![0];
        let mut blocks = Vec::with_capacity((bytes.len() / BLOCK).saturating_add(1));
        let mut chars: usize = 0;
        for (index, chunk) in bytes.chunks(BLOCK).enumerate() {
            blocks.push(chars);
            chars = chars.saturating_add(chunk.iter().filter(|b| starts_char(**b)).count());
            let base = index.saturating_mul(BLOCK);
            starts.extend(
                chunk
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b == b'\n')
                    .map(|(i, _)| base.saturating_add(i).saturating_add(1)),
            );
        }
        if blocks.is_empty() {
            blocks.push(0);
        }
        Index { starts, blocks }
    }

    /// Caractères commencés dans `[0, offset)` (`offset` ≤ longueur).
    fn chars_before(&self, index: &Index, offset: usize) -> usize {
        let block = offset / BLOCK;
        let base = index.blocks.get(block).copied().unwrap_or(0);
        let from = block.saturating_mul(BLOCK);
        let tail = self
            .text
            .as_bytes()
            .get(from..offset)
            .map_or(0, |s| s.iter().filter(|b| starts_char(**b)).count());
        base.saturating_add(tail)
    }

    /// Position de l'octet `byte` (borné à la longueur du texte).
    pub(crate) fn locate(&self, byte: u64) -> Location {
        let len = self.text.len();
        let offset = usize::try_from(byte).map_or(len, |b| b.min(len));
        let index = self.index.get_or_init(|| self.build());
        let line_index = index.starts.partition_point(|s| *s <= offset).saturating_sub(1);
        let start = index.starts.get(line_index).copied().unwrap_or(0);
        let column = self
            .chars_before(index, offset)
            .saturating_sub(self.chars_before(index, start));
        Location {
            byte: u64::try_from(offset).unwrap_or(u64::MAX),
            line: u32::try_from(line_index.saturating_add(1)).unwrap_or(u32::MAX),
            column: u32::try_from(column.saturating_add(1)).unwrap_or(u32::MAX),
        }
    }
}

/// Position d'un octet invalide dans une entrée qui n'est pas de l'UTF-8 :
/// le préfixe `bytes[..valid_up_to]` est valide.
pub(crate) fn locate_in_bytes(bytes: &[u8], valid_up_to: usize) -> Location {
    let prefix = bytes
        .get(..valid_up_to)
        .and_then(|p| std::str::from_utf8(p).ok())
        .unwrap_or("");
    LineIndex::new(prefix).locate(u64::try_from(prefix.len()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::{DiagArg, LineIndex, Location, MspdiError, locate_in_bytes};

    #[test]
    fn line_index() {
        let text = "ab\ncdé\n\nf";
        let idx = LineIndex::new(text);
        assert_eq!(
            idx.locate(0),
            Location {
                byte: 0,
                line: 1,
                column: 1
            }
        );
        assert_eq!(idx.locate(2).column, 3);
        assert_eq!(
            idx.locate(3),
            Location {
                byte: 3,
                line: 2,
                column: 1
            }
        );
        // « é » compte pour une colonne : l'octet 7 (après « cdé ») est en colonne 4.
        assert_eq!(idx.locate(7).column, 4);
        assert_eq!(idx.locate(8).line, 3);
        assert_eq!(idx.locate(9).line, 4);
        // Au-delà de la fin : bornée.
        assert_eq!(idx.locate(1_000).byte, 10);
        // Au milieu d'un caractère : colonne en octets, sans panique.
        assert_eq!(idx.locate(6).line, 2);
        assert_eq!(LineIndex::new("").locate(5).byte, 0);
    }

    #[test]
    fn line_index_across_blocks() {
        // Une seule ligne de 10 000 « é » (2 octets chacun) puis « x » : la
        // colonne compte les caractères, au-delà de plusieurs blocs.
        let text = format!("{}x\n{}", "é".repeat(10_000), "a".repeat(5_000));
        let idx = LineIndex::new(&text);
        let loc = idx.locate(20_000);
        assert_eq!((loc.line, loc.column), (1, 10_001));
        let loc = idx.locate(20_002 + 4_500);
        assert_eq!((loc.line, loc.column), (2, 4_501));
    }

    #[test]
    fn invalid_utf8_location() {
        let bytes = b"<a>\n  \xff</a>";
        let loc = locate_in_bytes(bytes, 6);
        assert_eq!(
            loc,
            Location {
                byte: 6,
                line: 2,
                column: 3
            }
        );
    }

    #[test]
    fn codes_and_messages() {
        let e = MspdiError::UnexpectedEof {
            at: Location {
                byte: 10,
                line: 2,
                column: 3,
            },
            open: vec!["Project".to_owned(), "Tasks".to_owned()],
        };
        assert_eq!(e.code(), "mspdi.error.unexpected_eof");
        assert_eq!(
            e.to_string(),
            "fin inattendue du document à la ligne 2, colonne 3 (octet 10) (éléments ouverts : Project/Tasks)"
        );
        assert_eq!(MspdiError::Empty.code(), "mspdi.error.empty");
        let e = MspdiError::TooManyRetained {
            at: Location {
                byte: 10,
                line: 2,
                column: 3,
            },
            max: 3,
        };
        assert_eq!(e.code(), "mspdi.error.too_many_retained");
        assert!(e.to_string().contains("limite 3"), "{e}");
    }

    /// N10 : sans élément ouvert, le message n'a pas de parenthèse vide.
    #[test]
    fn n10_eof_message_no_empty_parens() {
        let e = MspdiError::UnexpectedEof {
            at: Location {
                byte: 0,
                line: 1,
                column: 1,
            },
            open: Vec::new(),
        };
        let text = e.to_string();
        assert_eq!(
            text,
            "fin inattendue du document à la ligne 1, colonne 1 (octet 0)"
        );
        assert!(!text.contains("()") && !text.contains(": )"), "{text}");
    }

    /// Un exemple par variante, codes distincts ; fin inattendue : éléments
    /// ouverts et leur nombre (N9).
    #[test]
    fn n9_examples_cover_every_variant() {
        let examples = MspdiError::examples();
        let codes: std::collections::BTreeSet<&str> = examples.iter().map(MspdiError::code).collect();
        assert_eq!(codes.len(), 12);
        assert_eq!(examples.len(), 12);
        assert_eq!(
            examples.get(5).map(MspdiError::args),
            Some(vec![
                (
                    "at",
                    DiagArg::Position {
                        line: 4,
                        column: 9,
                        byte: 120
                    }
                ),
                ("open", DiagArg::Text("Project/Tasks".to_owned())),
                ("depth", DiagArg::Int(2)),
            ])
        );
    }
}
