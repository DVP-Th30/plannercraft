//! Prédécesseurs en texte (R-142) : « 3FD+2j;5DD », « 3FS+2d,5SS ».

use plannercraft_core::{Lag, LinkType, Project, Task, TaskUid};

use super::duration::{format_lag, parse_lag};
use super::lex::{Cursor, bounded_chars, fold, is_space};
use super::{Language, TextContext, TextError};

/// Référence à un prédécesseur lue dans le texte : numéro (ID, position
/// dans la liste des tâches), type de lien, décalage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PredecessorRef {
    /// ID du prédécesseur (ou UID pour la colonne « N° unique des
    /// prédécesseurs » d'un CSV).
    pub id: u32,
    /// Type de lien (fin à début si absent).
    pub kind: LinkType,
    /// Décalage (nul si absent).
    pub lag: Lag,
}

/// R-142 : type de lien d'un code (casse ignorée) : FS/FD, SS/DD, FF, SF/DF.
fn link_type(code: &str) -> Option<LinkType> {
    Some(match code {
        "fs" | "fd" => LinkType::FinishToStart,
        "ss" | "dd" => LinkType::StartToStart,
        "ff" => LinkType::FinishToFinish,
        "sf" | "df" => LinkType::StartToFinish,
        _ => return None,
    })
}

/// R-142 : code d'un type de lien dans la langue (français FD/DD/FF/DF,
/// anglais FS/SS/FF/SF).
#[must_use]
pub const fn link_code(kind: LinkType, language: Language) -> &'static str {
    match (kind, language) {
        (LinkType::FinishToStart, Language::Fr) => "FD",
        (LinkType::FinishToStart, Language::En) => "FS",
        (LinkType::StartToStart, Language::Fr) => "DD",
        (LinkType::StartToStart, Language::En) => "SS",
        (LinkType::FinishToFinish, _) => "FF",
        (LinkType::StartToFinish, Language::Fr) => "DF",
        (LinkType::StartToFinish, Language::En) => "SF",
    }
}

/// R-142 : lit un élément `id [type] [décalage]`, espaces permises.
fn parse_element(element: &[char], ctx: &TextContext) -> Result<PredecessorRef, TextError> {
    // Chemin de fichier avant le numéro : prédécesseur d'un autre projet (S38).
    if element.iter().any(|c| matches!(c, '\\' | '/')) {
        return Err(TextError::ExternalPredecessor);
    }
    let mut cur = Cursor::new(element);
    cur.skip_spaces();
    let id = cur.integer()?;
    cur.skip_spaces();
    let mut code = Vec::new();
    while let Some(c) = cur.peek().filter(|c| c.is_alphabetic()) {
        code.push(c);
        cur.bump();
    }
    let kind = if code.is_empty() {
        LinkType::FinishToStart
    } else {
        link_type(&fold(code.iter().copied())).ok_or_else(|| TextError::BadLinkType {
            code: code.iter().collect(),
        })?
    };
    cur.skip_spaces();
    let rest: String = cur.rest().iter().collect();
    let lag = if rest.is_empty() {
        Lag::ZERO
    } else {
        parse_lag(&rest, ctx)?
    };
    Ok(PredecessorRef { id, kind, lag })
}

/// R-142 : vrai si `c` sépare deux éléments d'une liste : le séparateur de
/// liste du contexte (`;` en français, `,` en anglais, S38) et, en anglais,
/// aussi `;` (sans ambiguïté avec le point décimal anglais). Le français
/// garde `;` seul : la virgule y est la marque décimale.
fn is_list_separator(c: char, ctx: &TextContext) -> bool {
    c == ctx.list_separator || (ctx.language == Language::En && c == ';')
}

/// Élément d'une liste de prédécesseurs : texte (espaces de bord retirées)
/// et lecture.
pub(crate) type Element = (String, Result<PredecessorRef, TextError>);

/// R-142 : éléments d'une liste de prédécesseurs, dans l'ordre, chacun avec
/// son texte (espaces de bord retirées) et sa lecture ; éléments vides
/// sautés.
///
/// # Errors
///
/// [`TextError::TooLong`] au-delà de [`super::MAX_INPUT_CHARS`] caractères.
pub(crate) fn predecessor_elements(s: &str, ctx: &TextContext) -> Result<Vec<Element>, TextError> {
    let chars = bounded_chars(s)?;
    Ok(chars
        .split(|c| is_list_separator(*c, ctx))
        .filter(|element| !element.iter().all(|c| is_space(*c)))
        .map(|element| {
            let text: String = element.iter().collect();
            (
                text.trim_matches(is_space).to_owned(),
                parse_element(element, ctx),
            )
        })
        .collect())
}

/// R-142 : lit une liste de prédécesseurs séparée par le séparateur de liste
/// du contexte (`;` en français ; `,` en anglais, où `;` est aussi admis,
/// S38). Élément : `id [type] [décalage]` ; type FS/FD, SS/DD, FF, SF/DF
/// (casse ignorée), absent → fin à début ; décalage signé de R-140 ou
/// pourcentage (`%`, `e%`). Un élément vide est ignoré ; un élément
/// illisible est écarté et son erreur rendue, les autres sont gardés (dans
/// l'ordre).
///
/// Exemples : « 3FD+2j;5DD » → (3, FS, +960), (5, SS, 0) ; « 4FF-1j » →
/// FF −480 ; « 3 FS + 50% » → FS +5 000 ; « C:\a.mpp\3FF » →
/// `text.external_predecessor` ; « 3XY » → `text.bad_link_type` ; en
/// anglais « 3;5 » et « 3,5 » → deux liens.
#[must_use]
pub fn parse_predecessors(s: &str, ctx: &TextContext) -> (Vec<PredecessorRef>, Vec<TextError>) {
    let mut refs = Vec::new();
    let mut errors = Vec::new();
    match predecessor_elements(s, ctx) {
        Ok(elements) => {
            for (_, r) in elements {
                match r {
                    Ok(r) => refs.push(r),
                    Err(e) => errors.push(e),
                }
            }
        }
        Err(e) => errors.push(e),
    }
    (refs, errors)
}

/// R-142 : écrit une référence : FS sans décalage → « 3 » ; sinon l'ID, le
/// code de la langue puis, si le décalage n'est pas le décalage nul par
/// défaut, le décalage signé de R-141 (« 7DD+1j », « 4FF », « 3FD+0h »).
#[must_use]
pub fn format_predecessor(r: &PredecessorRef, ctx: &TextContext) -> String {
    let mut s = r.id.to_string();
    let zero = r.lag == Lag::ZERO;
    if r.kind == LinkType::FinishToStart && zero {
        return s;
    }
    s.push_str(link_code(r.kind, ctx.language));
    if !zero {
        s.push_str(&format_lag(&r.lag, ctx));
    }
    s
}

/// R-142 : écrit les prédécesseurs d'une tâche, ID = position dans le
/// projet (`Project::task_id`), séparés par le séparateur de liste du
/// contexte. Un prédécesseur sans ID (UID absent du projet) est omis et
/// rendu dans le second élément.
#[must_use]
pub fn format_predecessors(task: &Task, project: &Project, ctx: &TextContext) -> (String, Vec<TaskUid>) {
    let mut parts = Vec::new();
    let mut missing = Vec::new();
    for link in &task.predecessors {
        match project.task_id(link.predecessor) {
            Some(id) => parts.push(format_predecessor(
                &PredecessorRef {
                    id: id.get(),
                    kind: link.kind,
                    lag: link.lag,
                },
                ctx,
            )),
            None => missing.push(link.predecessor),
        }
    }
    (parts.join(&ctx.list_separator.to_string()), missing)
}
