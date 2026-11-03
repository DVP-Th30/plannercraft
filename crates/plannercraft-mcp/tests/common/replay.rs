//! Harnais de rejeu des scénarios MCP (lot L18), partagé par
//! `crates/plannercraft-mcp/tests/replay.rs` (en mémoire, HTTP) et
//! `apps/plannercraft-cli/tests/mcp_replay.rs` (vrai binaire en stdio).
//!
//! Un scénario (`tests/mcp/scenarios/<nom>.jsonl`) a exactement la forme du
//! journal `--journal` du serveur (`plannercraft_automation::journal::Entry`) :
//! pour chaque appel, une ligne de requête (`params`, sans `result`) puis une
//! ligne de résultat de même `seq` (`params` à `null`, `result` =
//! `{"ok": …}` ou `{"error": …}`). Le jeton `$TMP` des chaînes désigne le
//! dossier temporaire du rejeu : il est remplacé avant l'appel et remis à sa
//! place dans le journal produit, qui doit alors être égal au scénario
//! octet pour octet (déterminisme, vérification 4 du cahier).
//!
//! `PLANNERCRAFT_BLESS=1` réécrit le scénario (résultats compris) depuis le
//! journal produit, et `tests/mcp/expected/<nom>.xml` depuis le fichier
//! enregistré : à relire avant de versionner.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    reason = "harnais de test partagé : un échec doit s'arrêter net, chaque cible n'en utilise qu'une partie"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use plannercraft_automation::journal::{Entry, parse_line};
use serde_json::{Value, json};

/// Jeton du dossier temporaire dans les scénarios.
pub const TMP_TOKEN: &str = "$TMP";

/// « Maintenant » figé des scénarios (`--now`).
pub const NOW: &str = "2026-10-19T08:00";

/// « Maintenant » figé du scénario de suivi J6 (`tracking_j6`, lot L10) :
/// le jour du début du projet, pour que la date d'état par défaut tombe au
/// début du chantier.
pub const NOW_TRACKING: &str = "2026-11-02T08:00";

/// « Maintenant » de l'essai réel n° 3 avec Claude (« Nous sommes le
/// 13/11/2026 », `docs/features/mcp.md` § 6).
pub const NOW_CLAUDE_RUN_3: &str = "2026-11-13T17:00";

/// « Maintenant » du scénario `name` (`--now` du rejeu).
pub fn now_of(name: &str) -> &'static str {
    match name {
        "tracking_j6" => NOW_TRACKING,
        "claude_run_3" => NOW_CLAUDE_RUN_3,
        _ => NOW,
    }
}

/// Racine du dépôt (les deux crates qui incluent ce fichier sont à deux
/// niveaux de la racine : `crates/plannercraft-mcp`, `apps/plannercraft-cli`).
pub fn repo_root(manifest_dir: &str) -> PathBuf {
    Path::new(manifest_dir).join("..").join("..")
}

/// Chemin du scénario `name`.
pub fn scenario_path(root: &Path, name: &str) -> PathBuf {
    root.join("tests/mcp/scenarios").join(format!("{name}.jsonl"))
}

/// Chemin du MSPDI attendu du scénario `name`.
pub fn expected_xml_path(root: &Path, name: &str) -> PathBuf {
    root.join("tests/mcp/expected").join(format!("{name}.xml"))
}

/// Vrai si `PLANNERCRAFT_BLESS=1` (régénération des attendus).
pub fn blessing() -> bool {
    std::env::var("PLANNERCRAFT_BLESS").is_ok_and(|v| v == "1")
}

/// Un appel du scénario.
#[derive(Clone, Debug)]
pub struct Call {
    /// Nom de l'outil MCP.
    pub tool: String,
    /// Paramètres (avec le jeton `$TMP`).
    pub params: Value,
    /// Résultat attendu (`{"ok": …}` / `{"error": …}`), absent avant
    /// `PLANNERCRAFT_BLESS=1`.
    pub expected: Option<Value>,
}

/// Scénario chargé.
#[derive(Clone, Debug)]
pub struct Scenario {
    /// Nom (`build_30`).
    pub name: String,
    /// Texte du fichier, fins de ligne `\r` retirées.
    pub text: String,
    /// Appels, dans l'ordre.
    pub calls: Vec<Call>,
}

impl Scenario {
    /// Charge `tests/mcp/scenarios/<name>.jsonl`.
    pub fn load(root: &Path, name: &str) -> Self {
        let path = scenario_path(root, name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} : {e}", path.display()))
            .replace('\r', "");
        let mut calls: Vec<(u64, Call)> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Entry {
                seq,
                tool,
                params,
                result,
                ..
            } = parse_line(line).unwrap_or_else(|e| panic!("{name}.jsonl:{} : {e}", n + 1));
            match result {
                None => calls.push((
                    seq,
                    Call {
                        tool,
                        params,
                        expected: None,
                    },
                )),
                Some(r) => {
                    let Some((s, call)) = calls.last_mut() else {
                        panic!("{name}.jsonl:{} : résultat sans requête", n + 1)
                    };
                    assert_eq!(*s, seq, "{name}.jsonl:{} : seq du résultat", n + 1);
                    assert_eq!(call.tool, tool, "{name}.jsonl:{} : outil du résultat", n + 1);
                    call.expected = Some(r);
                }
            }
        }
        Self {
            name: name.to_owned(),
            text,
            calls: calls.into_iter().map(|(_, c)| c).collect(),
        }
    }
}

/// Forme échappée JSON d'une chaîne (sans les guillemets).
fn json_escaped(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap();
    quoted[1..quoted.len() - 1].to_owned()
}

/// Remplace `$TMP` par `tmp` dans toutes les chaînes de `v` ; les `/` qui
/// suivent le jeton dans la même chaîne deviennent le séparateur natif.
///
/// Sous Windows, `tmp` est canonique, donc verbatim (`\\?\C:\…`) : dans ce
/// préfixe, `/` n'est pas un séparateur (`$TMP/build_30.xml` désignerait un
/// dossier inexistant, os error 3). Le dossier reste verbatim : c'est la
/// forme que renvoie un serveur confiné (`--root`, `files::confine`).
pub fn substitute(v: &Value, tmp: &Path) -> Value {
    substitute_with(v, &tmp.display().to_string(), std::path::MAIN_SEPARATOR)
}

fn substitute_with(v: &Value, dir: &str, sep: char) -> Value {
    match v {
        Value::String(s) => Value::String(match s.split_once(TMP_TOKEN) {
            Some((head, tail)) => format!("{head}{dir}{}", tail.replace('/', &sep.to_string())),
            None => s.clone(),
        }),
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute_with(x, dir, sep)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), substitute_with(x, dir, sep)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Remet le jeton `$TMP` à la place du dossier `tmp` dans un texte JSON
/// (journal produit) ; dans la suite du chemin, jusqu'à la fin de la chaîne
/// JSON, le séparateur natif redevient `/` (inverse de [`substitute`]).
pub fn unsubstitute(text: &str, tmp: &Path) -> String {
    unsubstitute_with(text, &tmp.display().to_string(), std::path::MAIN_SEPARATOR)
}

fn unsubstitute_with(text: &str, dir: &str, sep: char) -> String {
    let text = text.replace('\r', "").replace(&json_escaped(dir), TMP_TOKEN);
    if sep == '/' {
        return text;
    }
    let sep = json_escaped(&sep.to_string());
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(i) = rest.find(TMP_TOKEN) {
        let (before, after) = rest.split_at(i + TMP_TOKEN.len());
        out.push_str(before);
        // Fin de la chaîne JSON : premier `"` non échappé.
        let mut end = after.len();
        let mut chars = after.char_indices();
        while let Some((j, c)) = chars.next() {
            if c == '"' {
                end = j;
                break;
            }
            if c == '\\' {
                chars.next();
            }
        }
        out.push_str(&after[..end].replace(&sep, "/"));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Résultat d'un appel tel que le journal l'écrit.
pub fn logged(is_error: Option<bool>, structured: Option<Value>) -> Value {
    match (structured, is_error) {
        (Some(v), Some(true)) => json!({ "error": v }),
        (Some(v), _) => json!({ "ok": v }),
        (None, _) => json!({ "ok": null }),
    }
}

/// Résultat ramené à la forme du scénario (jeton `$TMP`).
pub fn normalized(result: &Value, tmp: &Path) -> Value {
    let text = serde_json::to_string(result).unwrap();
    serde_json::from_str(&unsubstitute(&text, tmp)).unwrap()
}

/// Compare les résultats obtenus aux attendus, appel par appel.
pub fn assert_results(scenario: &Scenario, got: &[Value]) {
    assert_eq!(
        got.len(),
        scenario.calls.len(),
        "{} : nombre d'appels",
        scenario.name
    );
    for (i, (call, got)) in scenario.calls.iter().zip(got).enumerate() {
        let Some(expected) = &call.expected else {
            panic!(
                "{} : appel {} ({}) sans résultat attendu : régénérer avec PLANNERCRAFT_BLESS=1",
                scenario.name,
                i + 1,
                call.tool
            )
        };
        assert_eq!(
            got,
            expected,
            "{} : appel {} ({}) : résultat différent de l'attendu",
            scenario.name,
            i + 1,
            call.tool
        );
    }
}

/// Compare le journal produit (jeton `$TMP` remis) au scénario, ligne à
/// ligne puis octet pour octet.
pub fn assert_journal(scenario: &Scenario, produced: &str, tmp: &Path) {
    let produced = unsubstitute(produced, tmp);
    for (n, (a, b)) in produced.lines().zip(scenario.text.lines()).enumerate() {
        assert_eq!(a, b, "{} : ligne {} du journal produit", scenario.name, n + 1);
    }
    assert_eq!(produced, scenario.text, "{} : journal produit", scenario.name);
}

/// `PLANNERCRAFT_BLESS=1` : réécrit le scénario depuis le journal produit et
/// l'attendu MSPDI depuis le fichier enregistré.
pub fn bless(root: &Path, scenario: &Scenario, produced: &str, tmp: &Path, saved_xml: Option<&Path>) {
    let path = scenario_path(root, &scenario.name);
    std::fs::write(&path, unsubstitute(produced, tmp)).unwrap();
    eprintln!("PLANNERCRAFT_BLESS : {} réécrit", path.display());
    if let Some(xml) = saved_xml {
        let dest = expected_xml_path(root, &scenario.name);
        std::fs::copy(xml, &dest).unwrap();
        eprintln!("PLANNERCRAFT_BLESS : {} réécrit", dest.display());
    }
}

/// Dossier temporaire propre à un test (retiré à la fin du test).
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plannercraft-replay-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Chemin canonique : celui que renvoie un serveur confiné (`--root`).
        Self(std::fs::canonicalize(&dir).unwrap_or(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forme Windows simulée : dossier verbatim, séparateur `\`.
    #[test]
    fn substitute_round_trip_windows_verbatim() {
        let dir = r"\\?\C:\Users\RUNNER\Temp\plannercraft-replay-stdio-1-0";
        let params = json!({ "path": "$TMP/build_30.xml", "n": 1, "list": ["$TMP/a/b.xml", "x/y"] });
        let got = substitute_with(&params, dir, '\\');
        assert_eq!(got["path"], format!(r"{dir}\build_30.xml"));
        assert_eq!(got["list"][0], format!(r"{dir}\a\b.xml"));
        assert_eq!(got["list"][1], "x/y");
        let text = serde_json::to_string(&got).unwrap();
        assert_eq!(
            unsubstitute_with(&text, dir, '\\'),
            serde_json::to_string(&params).unwrap()
        );
    }

    /// Forme Unix : rien ne change hors du jeton.
    #[test]
    fn substitute_round_trip_unix() {
        let dir = "/tmp/plannercraft-replay-stdio-1-0";
        let params = json!({ "path": "$TMP/build_30.xml" });
        let got = substitute_with(&params, dir, '/');
        assert_eq!(got["path"], "/tmp/plannercraft-replay-stdio-1-0/build_30.xml");
        let text = serde_json::to_string(&got).unwrap();
        assert_eq!(
            unsubstitute_with(&text, dir, '/'),
            serde_json::to_string(&params).unwrap()
        );
    }
}
