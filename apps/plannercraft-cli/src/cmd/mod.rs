//! Sous-commandes (lot L9) et aides communes.
//!
//! Chaque sous-commande ouvre un [`Workspace`] et passe par
//! [`Workspace::call`] avec les ids du registre : les sorties `--json` sont
//! exactement les vues JSON du MCP (ADR-0012 D1). Les sorties texte sont
//! faites des mêmes valeurs (dates et durées du moteur, règle 3).
//!
//! Aucune écriture sur la sortie standard ne peut planter (tube fermé :
//! `plannercraft tasks plan.xml | head`) : tout passe par [`out`] et [`err_line`].

pub mod apply;
pub mod explain;
pub mod export_pdf;
pub mod import_csv;
pub mod import_pdf;
pub mod mcp;
pub mod render;
pub mod resources;
pub mod style;
pub mod summary;
pub mod tasks;
pub mod usage;

use std::io::Write as _;
use std::process::ExitCode;

use plannercraft_automation::text::Language;
use plannercraft_automation::{ToolError, Workspace};
use plannercraft_core::{Moment, PRODUCT_NAME};
use serde_json::{Value, json};

use crate::exit;

/// Contexte commun : « maintenant » et langue résolus une fois (`now.rs`).
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// « Maintenant » injecté.
    pub now: Moment,
    /// Langue des messages et des textes (durées).
    pub lang: Language,
    /// `--lang` donné explicitement (le MCP a sa propre langue par défaut).
    pub lang_explicit: bool,
}

impl Ctx {
    /// Espace de travail vide à ce contexte.
    #[must_use]
    pub const fn workspace(&self) -> Workspace {
        Workspace::new(self.now, self.lang)
    }

    /// Texte selon la langue.
    #[must_use]
    pub const fn tr(&self, fr: &'static str, en: &'static str) -> &'static str {
        match self.lang {
            Language::Fr => fr,
            Language::En => en,
        }
    }
}

/// Écrit `text` sur la sortie standard. Échec (tube fermé) : `Err` avec
/// le code de sortie « fichier ».
pub fn out(text: &str) -> Result<(), ExitCode> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|_| ExitCode::from(exit::FILE))
}

/// Message sur la sortie d'erreur, préfixé du nom du produit
/// (`docs/rename.md`).
pub fn warn(text: &str) {
    err_line(&format!("{PRODUCT_NAME} : {text}"));
}

/// Écrit une ligne sur la sortie d'erreur, sans jamais planter.
pub fn err_line(text: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{text}");
}

/// JSON indenté (clés triées : objets `serde_json::Value`) suivi d'une fin
/// de ligne.
pub fn out_json(v: &Value) -> Result<(), ExitCode> {
    let text = serde_json::to_string_pretty(v).map_err(|e| {
        warn(&format!("sérialisation impossible : {e}"));
        ExitCode::from(exit::REFUSED)
    })?;
    out(&text)?;
    out("\n")
}

/// Signale une erreur d'outil : message sur la sortie d'erreur, et en
/// `--json` l'erreur structurée `{"error": …}` (forme du MCP) sur la sortie
/// standard. Rend le code de sortie de sa famille ([`exit::for_code`]).
pub fn report(e: &ToolError, json: bool) -> ExitCode {
    let code = exit::for_code(e.code);
    warn(&format!("{} : {}", e.code, e.message));
    for d in &e.diagnostics {
        err_line(&format!("  {} : {d}", d.code()));
    }
    if json {
        let value = serde_json::to_value(e).unwrap_or(Value::Null);
        if out_json(&json!({ "error": value })).is_err() {
            return ExitCode::from(exit::FILE);
        }
    }
    ExitCode::from(code)
}

/// Ouvre le fichier MSPDI `plan` dans `ws`.
///
/// # Errors
///
/// L'erreur d'outil (`file.read`, `file.invalid`…).
pub fn open(ws: &mut Workspace, plan: &str) -> Result<Value, ToolError> {
    ws.call("project.open", json!({ "path": plan }))
}

/// Résultat d'une sous-commande : `Ok(())` succès, sinon le code de sortie
/// déjà signalé.
pub type Run = Result<(), ExitCode>;

/// Code de sortie d'un [`Run`].
pub fn finish(r: Run) -> ExitCode {
    match r {
        Ok(()) => ExitCode::from(exit::SUCCESS),
        Err(code) => code,
    }
}

/// Champ textuel d'une vue (`""` si absent).
#[must_use]
pub fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Texte d'une durée de vue (`{"minutes","unit","text"}`), `""` si absente.
#[must_use]
pub fn duration_text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key)
        .and_then(|d| d.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Entier d'une vue (0 si absent).
#[must_use]
pub fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}
