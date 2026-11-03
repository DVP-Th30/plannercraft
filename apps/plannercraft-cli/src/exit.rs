//! Codes de sortie (C-MCP-08).
//!
//! 0 succès ; 1 commande refusée (erreur métier du moteur ou du registre) ;
//! 2 usage (arguments, commande indisponible) ; 3 fichier
//! (lecture, écriture, MSPDI ou CSV inexploitable).

/// Succès.
pub const SUCCESS: u8 = 0;
/// Commande refusée (erreur métier).
pub const REFUSED: u8 = 1;
/// Usage (arguments, commande indisponible).
pub const USAGE: u8 = 2;
/// Fichier (lecture, écriture, MSPDI illisible).
pub const FILE: u8 = 3;

/// Code de sortie d'une erreur d'outil, d'après son code stable : `file.*`
/// (lecture, écriture, taille, MSPDI inexploitable, fichier tenu par
/// l'application) et `csv.error.*` (CSV illisible) sont des erreurs de
/// fichier ; tout autre refus est une erreur métier.
#[must_use]
pub fn for_code(code: &str) -> u8 {
    if code.starts_with("file.") || code.starts_with("csv.error.") {
        FILE
    } else {
        REFUSED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_by_error_family() {
        assert_eq!(for_code("file.read"), FILE);
        assert_eq!(for_code("file.invalid"), FILE);
        assert_eq!(for_code("csv.error.empty"), FILE);
        assert_eq!(for_code("task.not_found"), REFUSED);
        assert_eq!(for_code("batch.would_create_errors"), REFUSED);
        assert_eq!(for_code("param.invalid"), REFUSED);
        assert_eq!(SUCCESS, 0);
        assert_eq!(USAGE, 2);
    }
}
