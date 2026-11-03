//! Rapport du corpus : `corpus-report.md` et `corpus-report.json`
//! (ADR-0009 § 9.6).
//!
//! Déterministe : aucun horodatage, ordre du manifeste puis des UID,
//! `BTreeMap` partout. **Aucun nom de tâche ni texte du fichier** : UID, ID,
//! champs, valeurs et codes seulement (les fichiers ne sont pas
//! redistribuables, le rapport est publié en artefact de CI).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use plannercraft_core::TaskUid;
use serde::Serialize;

use crate::corpus::FileRun;
use crate::corpus::compare::{Case, CaseKind, Field};
use crate::corpus::roundtrip::{self, RoundTrip};
use crate::corpus::verdict::{AbsentReason, FileStatus, Verdict};

/// Nombre de divergences détaillées par fichier dans le rapport Markdown
/// (toutes figurent dans le JSON).
pub const MARKDOWN_DIVERGENCES: usize = 10;

/// Rapport complet.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// Planchers en vigueur.
    pub floors: Floors,
    /// Totaux sur les fichiers comparés.
    pub totals: Totals,
    /// Un rapport par fichier, dans l'ordre du manifeste.
    pub files: Vec<FileReport>,
    /// Pour chaque règle, les cas identiques qui l'exercent (base du statut
    /// « vérifié contre MS Project »).
    pub rules_verified: BTreeMap<&'static str, Vec<CaseRef>>,
    /// Verdict.
    pub verdict: Verdict,
    /// Aller-retour MSPDI des fichiers présents (P25).
    pub round_trips: Vec<RoundTrip>,
}

/// Planchers.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Floors {
    /// `plannercraft_core::parity::CORPUS_FLOOR`.
    pub corpus: u32,
    /// `plannercraft_core::parity::ORACLE_FLOOR`.
    pub oracle: u32,
    /// `plannercraft_core::parity::FLOOR`.
    pub total: u32,
}

/// Totaux.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Totals {
    /// Fichiers au manifeste.
    pub files: u32,
    /// Fichiers lus et comparés.
    pub compared: u32,
    /// Cas.
    pub cases: u32,
    /// Cas identiques.
    pub identical: u32,
}

/// Référence d'un cas.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CaseRef {
    /// Fichier.
    pub file: String,
    /// UID.
    pub uid: u32,
}

/// Comptes d'une nature de cas.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct KindCount {
    /// Cas.
    pub cases: u32,
    /// Identiques.
    pub identical: u32,
}

/// Signature d'un enregistrement par MS Project (ADR-0009 § 9.1) :
/// `SaveVersion` égal à celui du manifeste, tâche UID 0, `EarlyStart`,
/// `LateFinish`, `TotalSlack` et `FreeSlack` sur toutes les tâches non vides,
/// sections d'interface présentes.
#[derive(Clone, Debug, Serialize)]
pub struct Signature {
    /// Sections d'interface présentes.
    pub ui_sections: Vec<String>,
    /// Critères manquants (`save_version`, `project_summary`,
    /// `computed_fields`, `ui_sections`) ; vide pour une signature complète.
    pub missing: Vec<&'static str>,
}

impl Signature {
    /// Les quatre critères réunis.
    #[must_use]
    pub fn msproject(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Rapport d'un fichier.
#[derive(Clone, Debug, Serialize)]
pub struct FileReport {
    /// Nom du manifeste.
    pub name: String,
    /// Licence du manifeste.
    pub license: String,
    /// Note du manifeste (contenu du fichier, divergences connues et leur
    /// jalon).
    pub note: String,
    /// Producteur déclaré au manifeste.
    pub producer: &'static str,
    /// État (absent et raison, erreur de lecture, comparé).
    pub status: FileStatus,
    /// `SaveVersion` lu.
    pub save_version: Option<i64>,
    /// Signature MS Project constatée.
    pub signature: Option<Signature>,
    /// Diagnostics d'import, comptés par code.
    pub import_diagnostics: BTreeMap<String, u32>,
    /// Diagnostics du moteur, comptés par code.
    pub engine_diagnostics: BTreeMap<String, u32>,
    /// Cas.
    pub cases: u32,
    /// Identiques.
    pub identical: u32,
    /// Différents.
    pub different: u32,
    /// Plancher.
    pub floor: u32,
    /// Identiques − plancher.
    pub gap: i64,
    /// Identiques > plancher.
    pub raisable: bool,
    /// Ventilation feuilles / récapitulatives / UID 0.
    pub by_kind: BTreeMap<&'static str, KindCount>,
    /// Exclus par raison.
    pub excluded: BTreeMap<String, u32>,
    /// Nombre de comparaisons par champ (garantie que le plancher n'est pas
    /// tenu à vide).
    pub compared_by_field: BTreeMap<&'static str, u32>,
    /// Comparaisons incomplètes (voir `FileComparison::incomplete_count`).
    pub incomplete: u32,
    /// Nombre de divergences par champ.
    pub divergences_by_field: BTreeMap<&'static str, u32>,
    /// Cas différents par cause probable (un cas compte pour chacune de ses
    /// causes).
    pub divergent_cases_by_cause: BTreeMap<&'static str, u32>,
    /// Cas différents sans aucune cause probable.
    pub divergent_cases_without_cause: u32,
    /// Toutes les divergences, par UID puis champ.
    pub divergences: Vec<DivergenceRow>,
}

/// Une divergence.
#[derive(Clone, Debug, Serialize)]
pub struct DivergenceRow {
    /// UID.
    pub uid: u32,
    /// ID.
    pub id: Option<u32>,
    /// Nature du cas.
    pub kind: CaseKind,
    /// Champ.
    pub field: Field,
    /// Valeur MS Project.
    pub msproject: String,
    /// Valeur PlannerCraft.
    pub plannercraft: String,
    /// Écart en minutes (PlannerCraft − MS Project).
    pub delta_minutes: Option<i64>,
    /// Causes probables du cas.
    pub causes: Vec<&'static str>,
}

fn bump<K: Ord>(map: &mut BTreeMap<K, u32>, key: K) {
    let n = map.entry(key).or_insert(0);
    *n = n.saturating_add(1);
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Construit le rapport.
#[must_use]
pub fn build(runs: &[FileRun], floors: Floors, verdict: Verdict) -> Report {
    let mut totals = Totals {
        files: to_u32(runs.len()),
        ..Totals::default()
    };
    let mut rules_verified: BTreeMap<&'static str, Vec<CaseRef>> = BTreeMap::new();
    let mut files = Vec::with_capacity(runs.len());
    for run in runs {
        let file = file_report(run);
        if matches!(file.status, FileStatus::Compared { .. }) {
            totals.compared = totals.compared.saturating_add(1);
            totals.cases = totals.cases.saturating_add(file.cases);
            totals.identical = totals.identical.saturating_add(file.identical);
        }
        if let Some(cmp) = &run.comparison {
            for case in cmp.cases.iter().filter(|c| c.identical()) {
                for rule in &case.rules {
                    rules_verified.entry(rule).or_default().push(CaseRef {
                        file: run.entry.name.clone(),
                        uid: case.uid,
                    });
                }
            }
        }
        files.push(file);
    }
    Report {
        floors,
        totals,
        files,
        rules_verified,
        verdict,
        round_trips: Vec::new(),
    }
}

fn signature(run: &FileRun) -> Option<Signature> {
    let doc = run.document.as_ref()?;
    let save_version_matches = doc.source.save_version == Some(run.entry.save_version);
    let project_summary = doc.recorded.tasks.contains_key(&TaskUid::PROJECT_SUMMARY);
    let computed_fields_complete = doc.project.tasks.iter().filter(|t| !t.is_null).all(|t| {
        doc.recorded.tasks.get(&t.uid()).is_some_and(|r| {
            r.early_start.is_some()
                && r.late_finish.is_some()
                && r.total_slack.is_some()
                && r.free_slack.is_some()
        })
    });
    let ui_sections = doc.source.ui_sections.clone();
    let mut missing = Vec::new();
    for (ok, name) in [
        (save_version_matches, "save_version"),
        (project_summary, "project_summary"),
        (computed_fields_complete, "computed_fields"),
        (!ui_sections.is_empty(), "ui_sections"),
    ] {
        if !ok {
            missing.push(name);
        }
    }
    Some(Signature { ui_sections, missing })
}

fn file_report(run: &FileRun) -> FileReport {
    let mut import_diagnostics = BTreeMap::new();
    if let Some(doc) = &run.document {
        for d in &doc.diagnostics {
            bump(&mut import_diagnostics, d.code.code());
        }
    }
    let mut engine_diagnostics = BTreeMap::new();
    for code in &run.engine_diagnostics {
        bump(&mut engine_diagnostics, code.clone());
    }
    let mut by_kind: BTreeMap<&'static str, KindCount> = BTreeMap::new();
    let mut excluded = BTreeMap::new();
    let mut compared_by_field = BTreeMap::new();
    let mut incomplete = 0;
    let mut divergences_by_field = BTreeMap::new();
    let mut divergent_cases_by_cause = BTreeMap::new();
    let mut divergent_cases_without_cause = 0_u32;
    let mut divergences = Vec::new();
    let (mut cases, mut identical) = (0_u32, 0_u32);
    if let Some(cmp) = &run.comparison {
        cases = cmp.case_count();
        identical = cmp.identical_count();
        for (reason, n) in &cmp.excluded {
            excluded.insert(reason.key(), *n);
        }
        for (field, n) in cmp.compared_by_field() {
            compared_by_field.insert(field.as_str(), n);
        }
        incomplete = cmp.incomplete_count();
        for case in &cmp.cases {
            let k = by_kind.entry(case.kind.as_str()).or_default();
            k.cases = k.cases.saturating_add(1);
            if case.identical() {
                k.identical = k.identical.saturating_add(1);
                continue;
            }
            if case.causes.is_empty() {
                divergent_cases_without_cause = divergent_cases_without_cause.saturating_add(1);
            }
            for cause in &case.causes {
                bump(&mut divergent_cases_by_cause, cause.as_str());
            }
            for d in &case.divergences {
                bump(&mut divergences_by_field, d.field.as_str());
                divergences.push(row(case, d));
            }
        }
    }
    let floor = run.entry.floor;
    FileReport {
        name: run.entry.name.clone(),
        license: run.entry.license.clone(),
        note: run.entry.note.clone(),
        producer: run.entry.producer.as_str(),
        status: run.status.clone(),
        save_version: run.document.as_ref().and_then(|d| d.source.save_version),
        signature: signature(run),
        import_diagnostics,
        engine_diagnostics,
        cases,
        identical,
        different: cases.saturating_sub(identical),
        floor,
        gap: i64::from(identical).saturating_sub(i64::from(floor)),
        raisable: run.comparison.is_some() && identical > floor,
        by_kind,
        excluded,
        compared_by_field,
        incomplete,
        divergences_by_field,
        divergent_cases_by_cause,
        divergent_cases_without_cause,
        divergences,
    }
}

fn row(case: &Case, d: &crate::corpus::compare::Divergence) -> DivergenceRow {
    DivergenceRow {
        uid: case.uid,
        id: case.id,
        kind: case.kind,
        field: d.field,
        msproject: d.recorded.clone(),
        plannercraft: d.computed.clone(),
        delta_minutes: d.delta_minutes,
        causes: case.causes.iter().map(|c| c.as_str()).collect(),
    }
}

fn status_text(status: &FileStatus) -> String {
    match status {
        FileStatus::Absent(AbsentReason::NotDownloaded) => "absent (non téléchargé)".to_owned(),
        FileStatus::Absent(AbsentReason::HashMismatch { .. }) => "absent (empreinte différente)".to_owned(),
        FileStatus::ReadError(e) => format!("erreur de lecture : {e}"),
        FileStatus::Compared { .. } => "lu et comparé".to_owned(),
    }
}

fn counts(map: &BTreeMap<impl AsRef<str> + Ord, u32>) -> String {
    if map.is_empty() {
        return "aucun".to_owned();
    }
    map.iter()
        .map(|(k, v)| format!("`{}` {v}", k.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Rapport Markdown.
#[must_use]
pub fn markdown(report: &Report) -> String {
    let mut s = String::new();
    let t = &report.totals;
    // L'écriture dans une `String` n'échoue pas : résultats ignorés.
    let _ = writeln!(s, "# Rapport du corpus public\n");
    let _ = writeln!(
        s,
        "Parité : **{} / {} cas identiques** sur {} fichier(s) comparé(s) ({} au manifeste). Planchers : `CORPUS_FLOOR` {}, `ORACLE_FLOOR` {}, `FLOOR` {}.\n",
        t.identical,
        t.cases,
        t.compared,
        t.files,
        report.floors.corpus,
        report.floors.oracle,
        report.floors.total
    );
    let v = &report.verdict;
    let _ = writeln!(
        s,
        "Verdict : **{}**.",
        if v.passed() { "succès" } else { "échec" }
    );
    for f in &v.failures {
        let _ = writeln!(s, "- échec : {f}");
    }
    for w in &v.warnings {
        let _ = writeln!(s, "- avertissement : {w}");
    }
    for r in &v.raisable {
        let _ = writeln!(s, "- {r}");
    }
    let _ = writeln!(
        s,
        "\n| Fichier | État | Cas | Identiques | Différents | Plancher | Écart |\n|---|---|---|---|---|---|---|"
    );
    for f in &report.files {
        let _ = writeln!(
            s,
            "| `{}` | {} | {} | {} | {} | {} | {:+} |",
            f.name,
            status_text(&f.status),
            f.cases,
            f.identical,
            f.different,
            f.floor,
            f.gap
        );
    }
    for f in &report.files {
        file_markdown(&mut s, f);
    }
    let _ = writeln!(
        s,
        "\n## Règles exercées par des cas identiques\n\n| Règle | Cas identiques | Exemples (fichier, UID) |\n|---|---|---|"
    );
    for (rule, refs) in &report.rules_verified {
        let examples = refs
            .iter()
            .take(3)
            .map(|r| format!("`{}` {}", r.file, r.uid))
            .collect::<Vec<_>>()
            .join(" ; ");
        let _ = writeln!(s, "| {rule} | {} | {examples} |", refs.len());
    }
    s
}

fn file_markdown(s: &mut String, f: &FileReport) {
    let _ = writeln!(s, "\n## `{}`\n", f.name);
    let _ = writeln!(
        s,
        "- État : {} ; producteur déclaré `{}`, `SaveVersion` {} ; licence `{}`.",
        status_text(&f.status),
        f.producer,
        f.save_version
            .map_or_else(|| "absent".to_owned(), |v| v.to_string()),
        f.license
    );
    let _ = writeln!(s, "- Note du manifeste : {}", f.note);
    if let Some(sig) = &f.signature {
        let _ = writeln!(
            s,
            "- Signature MS Project : {} ; sections d'interface : {}.",
            if sig.msproject() {
                "complète".to_owned()
            } else {
                format!("incomplète (manque : {})", sig.missing.join(", "))
            },
            if sig.ui_sections.is_empty() {
                "aucune".to_owned()
            } else {
                sig.ui_sections.join(", ")
            }
        );
    }
    if !matches!(f.status, FileStatus::Compared { .. }) {
        return;
    }
    let _ = writeln!(
        s,
        "- Cas : {} ; identiques {} ; différents {} ; plancher {} ; écart {:+}{}.",
        f.cases,
        f.identical,
        f.different,
        f.floor,
        f.gap,
        if f.raisable { " (plancher relevable)" } else { "" }
    );
    let kinds = f
        .by_kind
        .iter()
        .map(|(k, c)| format!("{k} {}/{}", c.identical, c.cases))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(s, "- Par nature (identiques/cas) : {kinds}.");
    let _ = writeln!(s, "- Exclus : {}.", counts(&f.excluded));
    let _ = writeln!(
        s,
        "- Comparaisons par champ : {} ; incomplètes : {}.",
        counts(&f.compared_by_field),
        f.incomplete
    );
    let _ = writeln!(s, "- Diagnostics d'import : {}.", counts(&f.import_diagnostics));
    let _ = writeln!(s, "- Diagnostics du moteur : {}.", counts(&f.engine_diagnostics));
    let _ = writeln!(
        s,
        "- Divergences par champ : {}.",
        counts(&f.divergences_by_field)
    );
    let _ = writeln!(
        s,
        "- Cas différents par cause probable : {} ; sans cause : {}.",
        counts(&f.divergent_cases_by_cause),
        f.divergent_cases_without_cause
    );
    if f.divergences.is_empty() {
        return;
    }
    let _ = writeln!(
        s,
        "\n| UID | ID | Nature | Champ | MS Project | PlannerCraft | Écart (min) | Causes probables |\n|---|---|---|---|---|---|---|---|"
    );
    for d in f.divergences.iter().take(MARKDOWN_DIVERGENCES) {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            d.uid,
            d.id.map_or_else(|| "–".to_owned(), |i| i.to_string()),
            d.kind.as_str(),
            d.field.as_str(),
            d.msproject,
            d.plannercraft,
            d.delta_minutes
                .map_or_else(|| "–".to_owned(), |m| format!("{m:+}")),
            if d.causes.is_empty() {
                "–".to_owned()
            } else {
                d.causes.join(", ")
            }
        );
    }
    if f.divergences.len() > MARKDOWN_DIVERGENCES {
        let _ = writeln!(
            s,
            "\n{} divergence(s) de plus dans `corpus-report.json`.",
            f.divergences.len().saturating_sub(MARKDOWN_DIVERGENCES)
        );
    }
}

/// Rapport Markdown complet : parité puis aller-retour MSPDI.
#[must_use]
pub fn full_markdown(report: &Report) -> String {
    let mut s = markdown(report);
    if !report.round_trips.is_empty() {
        s.push_str(&roundtrip::markdown(&report.round_trips));
    }
    s
}

/// Résumé court pour la sortie standard.
#[must_use]
pub fn summary(report: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "corpus : {} / {} cas identiques ({} fichier(s) comparé(s) sur {}), FLOOR {}",
        report.totals.identical,
        report.totals.cases,
        report.totals.compared,
        report.totals.files,
        report.floors.total
    );
    for f in &report.files {
        let _ = writeln!(
            s,
            "  {} : {} — {} / {} identiques, plancher {}",
            f.name,
            status_text(&f.status),
            f.identical,
            f.cases,
            f.floor
        );
    }
    s
}

/// Écrit `corpus-report.md` et `corpus-report.json` dans `dir`.
///
/// # Errors
///
/// Création du dossier, sérialisation ou écriture impossible.
pub fn write(dir: &Path, report: &Report) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("corpus-report.json"), format!("{json}\n")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("corpus-report.md"), full_markdown(report)).map_err(|e| e.to_string())
}
