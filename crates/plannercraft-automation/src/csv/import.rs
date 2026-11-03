//! Import d'un CSV des tâches (R-144, R-145).

use std::collections::{BTreeMap, BTreeSet};

use ::csv::{ReaderBuilder, StringRecord};
use plannercraft_core::{
    BuiltinCalendarNames, Constraint, Duration, Link, Minutes, Moment, OutlineLevel, Percent, Project,
    ProjectInit, Task, TaskUid, schedule,
};

use super::decode::{decode, detect_separator, preamble};
use super::header::{Field, HeaderLanguage, recognize};
use super::{
    CsvCode, CsvColumn, CsvDates, CsvDiagnostic, CsvError, CsvImport, CsvImportOptions, CsvLimits, CsvLocale,
    Detected,
};
use crate::text::lex::{Cursor, bounded_chars, fold};
use crate::text::{
    DateOrder, DateRole, Language, TextContext, TextError, parse_date, parse_duration, predecessor_elements,
};

/// Longueur maximale (en caractères) d'une valeur citée dans un diagnostic.
const MAX_VALUE_CHARS: usize = 200;

/// Colonnes retenues : indice de chaque colonne lue, intitulés du fichier.
struct Columns {
    index: BTreeMap<CsvColumn, usize>,
    headers: Vec<String>,
}

impl Columns {
    fn header(&self, column: CsvColumn) -> Option<String> {
        let i = *self.index.get(&column)?;
        self.headers.get(i).map(|h| h.trim().to_owned())
    }
}

/// Ligne de données : numéro de ligne physique et cellules des seules
/// colonnes reconnues (règle 10 : 200 000 lignes × 256 colonnes gardées en
/// entier coûteraient ~1,2 Go), par colonne.
struct Row {
    line: u64,
    cells: BTreeMap<CsvColumn, String>,
}

impl Row {
    /// Cellule d'une colonne telle quelle, `None` si absente.
    fn cell(&self, column: CsvColumn) -> Option<&str> {
        self.cells.get(&column).map(String::as_str)
    }

    /// Cellule brute d'une colonne, `None` si absente ou vide (espaces).
    fn raw<'a>(&'a self, _columns: &Columns, column: CsvColumn) -> Option<&'a str> {
        self.cell(column).filter(|c| !c.trim().is_empty())
    }

    /// Cellule de texte (nom, notes, WBS) telle quelle, espaces comprises ;
    /// `None` si absente ou vide.
    fn text<'a>(&'a self, _columns: &Columns, column: CsvColumn) -> Option<&'a str> {
        self.cell(column).filter(|c| !c.is_empty())
    }

    /// Cellule d'une colonne, espaces de bord retirées.
    fn value<'a>(&'a self, columns: &Columns, column: CsvColumn) -> Option<&'a str> {
        self.raw(columns, column).map(str::trim)
    }
}

/// Nombre maximal de diagnostics gardés (règle 10, comme l'import MSPDI) ;
/// au-delà, seul leur nombre est gardé (`csv.truncated`).
pub const MAX_DIAGNOSTICS: usize = 10_000;

/// Nombre maximal de diagnostics gardés par code : une avalanche d'un code
/// n'en masque pas d'autres.
pub const MAX_DIAGNOSTICS_PER_CODE: u32 = 1_000;

/// Diagnostics accumulés, bornés ; chacun avec le rang de sa colonne dans
/// l'en-tête (0 : aucune), calculé une fois à l'insertion, pour le tri.
#[derive(Default)]
struct Diags {
    kept: Vec<(usize, CsvDiagnostic)>,
    /// Rang (1 = première) de la première colonne de chaque intitulé
    /// (espaces de bord retirées).
    positions: BTreeMap<String, usize>,
    per_code: BTreeMap<&'static str, u32>,
    dropped: u64,
}

impl Diags {
    /// Fixe les intitulés de l'en-tête (avant tout diagnostic de colonne).
    fn set_headers(&mut self, headers: &[String]) {
        for (i, h) in headers.iter().enumerate() {
            self.positions
                .entry(h.trim().to_owned())
                .or_insert_with(|| i.saturating_add(1));
        }
    }

    fn push(&mut self, line: u64, column: Option<String>, code: CsvCode, value: Option<&str>) {
        self.push_more(line, column, code, value, 0);
    }

    /// Diagnostic qui en résume `others` autres de la même cellule.
    fn push_more(
        &mut self,
        line: u64,
        column: Option<String>,
        code: CsvCode,
        value: Option<&str>,
        others: u32,
    ) {
        let count = self.per_code.entry(code.code()).or_insert(0);
        if self.kept.len() >= MAX_DIAGNOSTICS || *count >= MAX_DIAGNOSTICS_PER_CODE {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        *count = count.saturating_add(1);
        let position = column
            .as_deref()
            .and_then(|c| self.positions.get(c).copied())
            .unwrap_or(0);
        self.kept.push((
            position,
            CsvDiagnostic {
                line,
                column,
                code,
                value: value.map(|v| v.chars().take(MAX_VALUE_CHARS).collect()),
                others,
            },
        ));
    }

    /// Diagnostics triés par ligne puis par colonne (tri stable), suivis d'un
    /// `csv.truncated` s'il y en a eu de supprimés.
    fn finish(self) -> Vec<CsvDiagnostic> {
        let mut kept = self.kept;
        kept.sort_by_key(|(position, d)| (d.line, *position));
        let mut out: Vec<CsvDiagnostic> = kept.into_iter().map(|(_, d)| d).collect();
        if self.dropped > 0 {
            out.push(CsvDiagnostic {
                line: 0,
                column: None,
                code: CsvCode::Truncated {
                    dropped: self.dropped,
                },
                value: None,
                others: 0,
            });
        }
        out
    }
}

/// Vrai si toutes les cellules sont vides (espaces).
fn blank(record: &StringRecord) -> bool {
    record.iter().all(|c| c.trim().is_empty())
}

/// Numéros de ligne physiques : la crate `csv` ne compte pas les sauts de
/// ligne à l'intérieur d'un champ entre guillemets ; on les compte depuis la
/// position en octets de chaque enregistrement (croissante : coût linéaire).
struct Lines<'a> {
    text: &'a [u8],
    offset: usize,
    line: u64,
}

impl<'a> Lines<'a> {
    /// Compteur sur `text`, dont le premier octet est sur la ligne physique
    /// `line` du fichier.
    const fn new(text: &'a [u8], line: u64) -> Self {
        Self {
            text,
            offset: 0,
            line,
        }
    }

    /// Ligne physique (1 = première) de l'octet `byte` : `\n`, `\r\n` et
    /// `\r` seul terminent une ligne.
    fn at(&mut self, byte: u64) -> u64 {
        let target = usize::try_from(byte).unwrap_or(usize::MAX).min(self.text.len());
        if target > self.offset {
            let span = self.text.get(self.offset..target).unwrap_or(&[]);
            let mut previous = self.offset.checked_sub(1).and_then(|i| self.text.get(i)).copied();
            for b in span {
                if *b == b'\r' || (*b == b'\n' && previous != Some(b'\r')) {
                    self.line = self.line.saturating_add(1);
                }
                previous = Some(*b);
            }
            self.offset = target;
        }
        self.line
    }

    /// Ligne physique du début d'un enregistrement. La crate `csv` place
    /// parfois ce début sur une fin de ligne déjà lue (après `\r\n`, ou
    /// avant des lignes vides sautées) : on avance jusqu'au premier octet
    /// qui n'est ni `\r` ni `\n` (un enregistrement n'en commence jamais
    /// par un).
    fn of(&mut self, record: &StringRecord) -> u64 {
        let Some(p) = record.position() else { return 0 };
        let mut byte = usize::try_from(p.byte()).unwrap_or(usize::MAX);
        while matches!(self.text.get(byte), Some(b'\r' | b'\n')) {
            byte = byte.saturating_add(1);
        }
        self.at(u64::try_from(byte).unwrap_or(u64::MAX))
    }
}

/// R-145 : garde de formule à l'import, réciproque de l'export (R-146) : un
/// texte qui commence par `'` suivi de `=`, `+`, `-`, `@`, tabulation, retour
/// chariot ou `'` perd son premier `'`.
fn unguard(s: &str) -> &str {
    let mut chars = s.chars();
    if chars.next() == Some('\'') && matches!(chars.next(), Some('=' | '+' | '-' | '@' | '\t' | '\r' | '\''))
    {
        return s.get(1..).unwrap_or(s);
    }
    s
}

/// Texte d'une cellule : caractères de contrôle (sauf tabulation et fins de
/// ligne) remplacés par U+FFFD (`csv.invalid_char`), garde de formule retirée.
fn clean_text(raw: &str, line: u64, column: Option<String>, diags: &mut Diags) -> String {
    let mut replaced = false;
    let text: String = unguard(raw)
        .chars()
        .map(|c| {
            if c.is_control() && !matches!(c, '\t' | '\n' | '\r') {
                replaced = true;
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect();
    if replaced {
        diags.push(line, column, CsvCode::InvalidChar, Some(&text));
    }
    text
}

/// Entier positif d'une cellule (UID, ID).
fn positive(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().filter(|v| *v > 0)
}

/// R-145 : jalon Oui/Non, Yes/No, Vrai/Faux, True/False, 1/0.
fn boolean(s: &str) -> Option<bool> {
    match fold(s.trim().chars()).as_str() {
        "oui" | "yes" | "vrai" | "true" | "1" => Some(true),
        "non" | "no" | "faux" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// % achevé : nombre (virgule ou point), `%` final facultatif.
fn percent(s: &str) -> Option<Percent> {
    let chars = bounded_chars(s).ok()?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    let value = cur.decimal().ok()?;
    cur.skip_spaces();
    if cur.peek() == Some('%') {
        cur.bump();
        cur.skip_spaces();
    }
    if !cur.at_end() {
        return None;
    }
    Percent::from_percent(value).ok()
}

/// Ce que la passe des lignes garde pour les prédécesseurs et les dates.
struct Placed {
    row: usize,
    uid: TaskUid,
    start: Option<Moment>,
    finish: Option<Moment>,
}

/// R-144 : en-tête (premier enregistrement non vide) et colonnes retenues ;
/// rend aussi le nombre d'en-têtes propres au français et à l'anglais.
fn read_header(
    reader: &mut ::csv::Reader<&[u8]>,
    record: &mut StringRecord,
    lines: &mut Lines<'_>,
    limits: CsvLimits,
    diags: &mut Diags,
) -> Result<(Columns, u32, u32), CsvError> {
    let (header_line, headers) = loop {
        match reader.read_record(record) {
            Ok(true) if blank(record) => {}
            Ok(true) => {
                break (
                    lines.of(record),
                    record.iter().map(str::to_owned).collect::<Vec<_>>(),
                );
            }
            Ok(false) | Err(_) => return Err(CsvError::NoHeader),
        }
    };
    if headers.len() > usize::from(limits.max_columns) {
        return Err(CsvError::TooManyColumns);
    }
    let mut columns = Columns {
        index: BTreeMap::new(),
        headers,
    };
    diags.set_headers(&columns.headers);
    let (mut fr, mut en) = (0_u32, 0_u32);
    for (i, h) in columns.headers.iter().enumerate() {
        let name = Some(h.trim().to_owned());
        let Some((field, language)) = recognize(h) else {
            if !h.trim().is_empty() {
                diags.push(header_line, name, CsvCode::UnknownColumn, None);
            }
            continue;
        };
        match language {
            HeaderLanguage::Fr => fr = fr.saturating_add(1),
            HeaderLanguage::En => en = en.saturating_add(1),
            HeaderLanguage::Both => {}
        }
        match field {
            Field::Column(c) if columns.index.contains_key(&c) => {
                diags.push(header_line, name, CsvCode::DuplicateColumn, None);
            }
            Field::Column(c) => {
                columns.index.insert(c, i);
            }
            Field::Unsupported => diags.push(header_line, name, CsvCode::UnsupportedColumn, None),
        }
    }
    if !columns.index.contains_key(&CsvColumn::Name) && !columns.index.contains_key(&CsvColumn::Duration) {
        return Err(CsvError::NoKnownColumn);
    }
    Ok((columns, fr, en))
}

/// R-144 : locale à la majorité des en-têtes propres à une langue ; à
/// égalité, `,` → anglais, `;` et tabulation → français.
const fn locale_of(fr: u32, en: u32, separator: u8) -> CsvLocale {
    if fr > en || (fr == en && separator != b',') {
        CsvLocale::Fr
    } else {
        CsvLocale::En
    }
}

/// R-144 : lignes de données ; lignes vides sautées, bornes vérifiées
/// enregistrement par enregistrement, champ trop long ignoré.
fn read_rows(
    reader: &mut ::csv::Reader<&[u8]>,
    record: &mut StringRecord,
    lines: &mut Lines<'_>,
    columns: &Columns,
    limits: CsvLimits,
    diags: &mut Diags,
) -> Result<Vec<Row>, CsvError> {
    let mut rows = Vec::new();
    loop {
        match reader.read_record(record) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                let line = e.position().map_or(0, |p| lines.at(p.byte()));
                diags.push(line, None, CsvCode::BadValue, None);
                break;
            }
        }
        if blank(record) {
            continue;
        }
        if record.len() > usize::from(limits.max_columns) {
            return Err(CsvError::TooManyColumns);
        }
        if u32::try_from(rows.len()).map_or(true, |n| n >= limits.max_rows) {
            return Err(CsvError::TooManyRows);
        }
        let line = lines.of(record);
        for (i, cell) in record.iter().enumerate() {
            if cell.len() > limits.max_field_bytes {
                let column = columns.headers.get(i).map(|h| h.trim().to_owned());
                diags.push(line, column, CsvCode::BadValue, Some(cell));
            }
        }
        let cells = columns
            .index
            .iter()
            .filter_map(|(c, i)| {
                let cell = record.get(*i)?;
                (cell.len() <= limits.max_field_bytes).then(|| (*c, cell.to_owned()))
            })
            .collect();
        rows.push(Row { line, cells });
    }
    Ok(rows)
}

/// R-144, R-145 : importe un CSV des tâches.
///
/// # Errors
///
/// [`CsvError`] seulement pour le fichier entier : trop grand, vide, sans
/// en-tête, sans colonne Nom ni Durée, trop de lignes ou de colonnes,
/// octets invalides dans l'encodage imposé. Une valeur illisible est un
/// diagnostic de sa ligne.
pub fn import(bytes: &[u8], options: &CsvImportOptions) -> Result<CsvImport, CsvError> {
    let limits = options.limits;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limits.max_bytes {
        return Err(CsvError::TooLarge);
    }
    let decoded = decode(bytes, options.encoding)?;
    if decoded.text.trim().is_empty() {
        return Err(CsvError::Empty);
    }
    let mut diags = Diags::default();
    if decoded.malformed {
        diags.push(0, None, CsvCode::InvalidChar, None);
    }
    // Ligne Excel `sep=X` en tête : fixe le séparateur (sauf séparateur
    // imposé) et n'est pas lue.
    let pre = preamble(&decoded.text);
    let body = decoded.text.get(pre.body_start..).unwrap_or("");
    let separator = options
        .separator
        .or(pre.separator)
        .unwrap_or_else(|| detect_separator(body));
    let mut reader = ReaderBuilder::new()
        .delimiter(separator)
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes());
    let mut record = StringRecord::new();
    let mut lines = Lines::new(body.as_bytes(), pre.body_line);
    let (columns, fr, en) = read_header(&mut reader, &mut record, &mut lines, limits, &mut diags)?;
    let locale = options.locale.unwrap_or(locale_of(fr, en, separator));
    let rows = read_rows(&mut reader, &mut record, &mut lines, &columns, limits, &mut diags)?;
    let build = Build {
        options,
        columns: &columns,
        rows: &rows,
        language: match locale {
            CsvLocale::Fr => Language::Fr,
            CsvLocale::En => Language::En,
        },
    };
    let project = build.project(&mut diags);
    // Ordre des lignes, puis des colonnes du fichier (tri stable).
    let diagnostics = diags.finish();
    Ok(CsvImport {
        project,
        diagnostics,
        detected: Detected {
            separator,
            locale,
            encoding: decoded.encoding,
        },
    })
}

/// Construction du projet depuis les lignes (R-145).
struct Build<'a> {
    options: &'a CsvImportOptions,
    columns: &'a Columns,
    rows: &'a [Row],
    language: Language,
}

impl Build<'_> {
    fn header(&self, column: CsvColumn) -> Option<String> {
        self.columns.header(column)
    }

    /// Lignes retenues : une ligne sans aucune cellule reconnue non vide est
    /// sautée ; une ligne dont seuls le N° ou le N° unique sont remplis est
    /// une ligne vide (`IsNull`).
    fn kept(&self) -> Vec<(usize, bool)> {
        let mut out = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            let filled = |c: &CsvColumn| row.raw(self.columns, *c).is_some();
            let any = self.columns.index.keys().any(filled);
            if !any {
                continue;
            }
            let null = !self
                .columns
                .index
                .keys()
                .filter(|c| !matches!(c, CsvColumn::Id | CsvColumn::UniqueId))
                .any(filled);
            out.push((i, null));
        }
        out
    }

    /// R-145 : UID depuis le N° unique s'il est valide (> 0) et unique (le
    /// premier gagne), sinon le plus petit entier libre au-delà du plus grand
    /// UID retenu, dans l'ordre des lignes.
    fn uids(&self, kept: &[(usize, bool)], diags: &mut Diags) -> Vec<TaskUid> {
        let mut wanted = Vec::with_capacity(kept.len());
        let mut used = BTreeSet::new();
        for (i, _) in kept {
            let Some(row) = self.rows.get(*i) else {
                wanted.push(None);
                continue;
            };
            let v = row.value(self.columns, CsvColumn::UniqueId);
            let uid = v.and_then(positive).filter(|u| used.insert(*u));
            if v.is_some() && uid.is_none() {
                diags.push(row.line, self.header(CsvColumn::UniqueId), CsvCode::BadValue, v);
            }
            wanted.push(uid);
        }
        let mut next = used.last().copied().unwrap_or(0);
        wanted
            .into_iter()
            .map(|w| {
                let uid = w.unwrap_or_else(|| {
                    loop {
                        next = next.checked_add(1).unwrap_or(1);
                        if used.insert(next) {
                            break next;
                        }
                    }
                });
                TaskUid::new(uid)
            })
            .collect()
    }

    /// Correspondance ID → indice dans `kept` : valeurs de la colonne N° si
    /// elle existe (valides et uniques, la première gagne), sinon la position
    /// (1 = première tâche).
    fn ids(&self, kept: &[(usize, bool)], diags: &mut Diags) -> BTreeMap<u32, usize> {
        let mut map = BTreeMap::new();
        let by_column = self.columns.index.contains_key(&CsvColumn::Id);
        for (k, (i, _)) in kept.iter().enumerate() {
            if by_column {
                let Some(row) = self.rows.get(*i) else { continue };
                let Some(v) = row.value(self.columns, CsvColumn::Id) else {
                    continue;
                };
                match positive(v) {
                    Some(id) if !map.contains_key(&id) => {
                        map.insert(id, k);
                    }
                    _ => diags.push(row.line, self.header(CsvColumn::Id), CsvCode::BadValue, Some(v)),
                }
            } else if let Some(id) = u32::try_from(k).ok().and_then(|k| k.checked_add(1)) {
                map.insert(id, k);
            }
        }
        map
    }

    /// R-145 : niveaux hiérarchiques. Absent → 1 ; 0 ou illisible → 1
    /// (`csv.bad_value`) ; saut de plus d'un cran → précédent + 1
    /// (`csv.outline_jump`). Les lignes vides ne comptent pas.
    fn levels(&self, kept: &[(usize, bool)], diags: &mut Diags) -> Vec<u16> {
        let mut out = Vec::with_capacity(kept.len());
        let mut previous: u16 = 0;
        for (i, null) in kept {
            let Some(row) = self.rows.get(*i) else {
                out.push(1);
                continue;
            };
            if *null {
                out.push(1);
                continue;
            }
            let header = || self.header(CsvColumn::OutlineLevel);
            let mut level = match row.value(self.columns, CsvColumn::OutlineLevel) {
                None => 1,
                Some(v) => match v.parse::<u16>() {
                    Ok(l) if l > 0 => l,
                    _ => {
                        diags.push(row.line, header(), CsvCode::BadValue, Some(v));
                        1
                    }
                },
            };
            let max = previous.saturating_add(1);
            if level > max {
                diags.push(row.line, header(), CsvCode::OutlineJump, Some(&level.to_string()));
                level = max;
            }
            previous = level;
            out.push(level);
        }
        out
    }

    fn project(&self, diags: &mut Diags) -> Project {
        let opts = self.options;
        let mut project = Project::new(ProjectInit {
            name: String::new(),
            anchor: opts.anchor,
            current_date: opts.current_date,
            calendar_names: BuiltinCalendarNames::default(),
        });
        let o = project.options;
        let ctx = TextContext::for_options(&o, self.language);
        let kept = self.kept();
        let uids = self.uids(&kept, diags);
        let ids = self.ids(&kept, diags);
        let levels = self.levels(&kept, diags);
        let mut placed = Vec::with_capacity(kept.len());

        for (k, (i, null)) in kept.iter().enumerate() {
            let (Some(row), Some(uid), Some(level)) =
                (self.rows.get(*i), uids.get(k).copied(), levels.get(k).copied())
            else {
                continue;
            };
            let mut task = Task::new(uid, String::new(), &o);
            if *null {
                task.is_null = true;
                if project.tasks.push(task).is_ok() {
                    placed.push(Placed {
                        row: *i,
                        uid,
                        start: None,
                        finish: None,
                    });
                }
                continue;
            }
            // R-052 : récapitulative si la tâche suivante (lignes vides
            // exceptées) est plus profonde.
            let summary = kept
                .iter()
                .zip(&levels)
                .skip(k.saturating_add(1))
                .find(|((_, n), _)| !*n)
                .is_some_and(|(_, l)| *l > level);
            task.outline_level = OutlineLevel::new(level).unwrap_or(OutlineLevel::TOP);
            let (start, finish) = self.fill(&mut task, row, summary, &ctx, diags);
            if project.tasks.push(task).is_ok() {
                placed.push(Placed {
                    row: *i,
                    uid,
                    start,
                    finish,
                });
            }
        }

        self.links(&mut project, &placed, &ids, &ctx, diags);
        if self.options.locale.is_none() {
            self.ambiguous_dates(&placed, &ctx, diags);
        }
        if opts.dates == CsvDates::Ignore {
            self.compare_dates(&project, &placed, diags);
        }
        project
    }

    /// R-145 : valeurs d'une ligne de tâche (nom, notes, WBS, jalon, durée,
    /// % achevé, contrainte) ; rend les dates lues (Début, Fin).
    fn fill(
        &self,
        task: &mut Task,
        row: &Row,
        summary: bool,
        ctx: &TextContext,
        diags: &mut Diags,
    ) -> (Option<Moment>, Option<Moment>) {
        let line = row.line;
        let col = |c| self.header(c);
        task.constraint = Constraint::default_for(&self.options.anchor);
        if let Some(v) = row.text(self.columns, CsvColumn::Name) {
            task.name = clean_text(v, line, col(CsvColumn::Name), diags);
        }
        if let Some(v) = row.text(self.columns, CsvColumn::Notes) {
            task.notes = clean_text(v, line, col(CsvColumn::Notes), diags);
        }
        if let Some(v) = row.text(self.columns, CsvColumn::Wbs) {
            task.wbs = Some(clean_text(v, line, col(CsvColumn::Wbs), diags));
        }
        if let Some(v) = row.value(self.columns, CsvColumn::Milestone) {
            match boolean(v) {
                Some(b) => task.milestone = b,
                None => diags.push(line, col(CsvColumn::Milestone), CsvCode::BadValue, Some(v)),
            }
        }
        match row.value(self.columns, CsvColumn::Duration) {
            Some(v) if summary => {
                // Durée calculée (R-054) : seules l'unité, la base et
                // l'estimation lues sont gardées, sur une durée nulle (comme
                // R-071 pour MSPDI).
                if let Ok(d) = parse_duration(v, ctx)
                    && let Ok(zero) = Duration::new(Minutes::ZERO, d.unit(), d.basis(), d.is_estimated())
                {
                    task.duration = zero;
                }
                diags.push(
                    line,
                    col(CsvColumn::Duration),
                    CsvCode::SummaryDurationIgnored,
                    Some(v),
                );
            }
            Some(v) => match parse_duration(v, ctx) {
                Ok(d) => task.duration = d,
                Err(_) => diags.push(line, col(CsvColumn::Duration), CsvCode::BadValue, Some(v)),
            },
            None if task.milestone && !summary => {
                task.duration = Duration::zero(ctx.default_duration_unit);
            }
            None => {}
        }
        if let Some(v) = row.value(self.columns, CsvColumn::PercentComplete) {
            match percent(v) {
                Some(p) => task.progress.percent_complete = p,
                None => diags.push(line, col(CsvColumn::PercentComplete), CsvCode::BadValue, Some(v)),
            }
        }
        let mut date = |c: CsvColumn, role| {
            let v = row.value(self.columns, c)?;
            parse_date(v, role, ctx)
                .map_err(|_| diags.push(line, self.header(c), CsvCode::BadValue, Some(v)))
                .ok()
        };
        let start = date(CsvColumn::Start, DateRole::Start);
        let finish = date(CsvColumn::Finish, DateRole::Finish);
        if self.options.dates == CsvDates::Constrain {
            // S30 : une date saisie à la main devient une contrainte.
            if let Some(date) = start {
                task.constraint = Constraint::StartNoEarlierThan { date };
            } else if let Some(date) = finish {
                task.constraint = Constraint::FinishNoEarlierThan { date };
            }
        }
        (start, finish)
    }

    /// R-145 : prédécesseurs par ID (colonne Prédécesseurs) ou, si la
    /// cellule est vide, par UID (N° unique des prédécesseurs) — seulement
    /// si la colonne N° unique existe : sans elle, les UID sont réattribués
    /// 1..n et les liens seraient faux, la colonne est ignorée
    /// (`csv.unique_predecessors_without_uid`, une fois). Inconnu, sur soi,
    /// en double, externe ou illisible : lien ignoré ; un seul diagnostic
    /// par cellule (premier élément fautif, nombre des autres).
    fn links(
        &self,
        project: &mut Project,
        placed: &[Placed],
        ids: &BTreeMap<u32, usize>,
        ctx: &TextContext,
        diags: &mut Diags,
    ) {
        let uid_of_kept: Vec<TaskUid> = placed.iter().map(|p| p.uid).collect();
        let has_uid_column = self.columns.index.contains_key(&CsvColumn::UniqueId);
        let mut warned = false;
        for p in placed {
            let Some(row) = self.rows.get(p.row) else { continue };
            let (column, by_uid) = if row.value(self.columns, CsvColumn::Predecessors).is_some() {
                (CsvColumn::Predecessors, false)
            } else {
                (CsvColumn::UniquePredecessors, true)
            };
            let Some(v) = row.value(self.columns, column) else {
                continue;
            };
            let header = self.header(column);
            if by_uid && !has_uid_column {
                if !warned {
                    diags.push(row.line, header, CsvCode::UniquePredecessorsWithoutUid, None);
                    warned = true;
                }
                continue;
            }
            // Premier élément fautif (code, texte) et nombre d'éléments fautifs.
            let mut first: Option<(CsvCode, String)> = None;
            let mut faults: u32 = 0;
            let mut fault = |code: CsvCode, text: &str| {
                faults = faults.saturating_add(1);
                if first.is_none() {
                    first = Some((code, text.to_owned()));
                }
            };
            let mut links: Vec<Link> = Vec::new();
            let elements = predecessor_elements(v, ctx).unwrap_or_else(|_| {
                fault(CsvCode::BadValue, v);
                Vec::new()
            });
            for (text, parsed) in elements {
                let r = match parsed {
                    Ok(r) => r,
                    Err(TextError::ExternalPredecessor) => {
                        fault(CsvCode::ExternalPredecessor, &text);
                        continue;
                    }
                    Err(_) => {
                        fault(CsvCode::BadValue, &text);
                        continue;
                    }
                };
                let target = if by_uid {
                    Some(TaskUid::new(r.id)).filter(|u| project.tasks.contains(*u))
                } else {
                    ids.get(&r.id).and_then(|k| uid_of_kept.get(*k)).copied()
                };
                let Some(pred) = target else {
                    fault(CsvCode::UnknownPredecessor, &r.id.to_string());
                    continue;
                };
                if pred == p.uid {
                    fault(CsvCode::SelfPredecessor, &r.id.to_string());
                } else if links.iter().any(|l| l.predecessor == pred) {
                    fault(CsvCode::DuplicatePredecessor, &r.id.to_string());
                } else {
                    links.push(Link {
                        predecessor: pred,
                        kind: r.kind,
                        lag: r.lag,
                    });
                }
            }
            if let Some((code, text)) = first {
                diags.push_more(row.line, header, code, Some(&text), faults.saturating_sub(1));
            }
            if let Some(task) = project.tasks.get_mut(p.uid) {
                task.predecessors = links;
            }
        }
    }

    /// R-144 (amendée en J4) : locale déduite des en-têtes (ou du séparateur
    /// à égalité), jamais des valeurs. Pour chaque colonne de dates, une
    /// valeur lue dans l'ordre retenu que l'ordre inverse ne lit pas (jour
    /// au-delà de 12) lève l'ambiguïté ; sinon, si une valeur se lit autrement dans
    /// l'ordre inverse (« 03/04/2026 »), `csv.date_ambiguous` une fois, à la
    /// première d'entre elles. Une date ISO ou identique dans les deux
    /// ordres (« 03/03/2026 ») ne compte ni pour ni contre.
    fn ambiguous_dates(&self, placed: &[Placed], ctx: &TextContext, diags: &mut Diags) {
        let other = TextContext {
            date_order: match ctx.date_order {
                DateOrder::DayMonthYear => DateOrder::MonthDayYear,
                DateOrder::MonthDayYear => DateOrder::DayMonthYear,
            },
            ..*ctx
        };
        for (column, role) in [
            (CsvColumn::Start, DateRole::Start),
            (CsvColumn::Finish, DateRole::Finish),
        ] {
            let mut first: Option<(u64, &str)> = None;
            let mut resolved = false;
            for p in placed {
                let read = match role {
                    DateRole::Start => p.start,
                    DateRole::Finish => p.finish,
                };
                let (Some(read), Some(row)) = (read, self.rows.get(p.row)) else {
                    continue;
                };
                let Some(v) = row.value(self.columns, column) else {
                    continue;
                };
                match parse_date(v, role, &other) {
                    Err(_) => {
                        resolved = true;
                        break;
                    }
                    Ok(m) if m != read && first.is_none() => first = Some((row.line, v)),
                    Ok(_) => {}
                }
            }
            if let (false, Some((line, v))) = (resolved, first) {
                diags.push(line, self.header(column), CsvCode::DateAmbiguous, Some(v));
            }
        }
    }

    /// R-145 : dates ignorées : chaque date lue est comparée à la date
    /// planifiée par le moteur ; un écart est une information.
    fn compare_dates(&self, project: &Project, placed: &[Placed], diags: &mut Diags) {
        if placed.iter().all(|p| p.start.is_none() && p.finish.is_none()) {
            return;
        }
        let computed = schedule(project);
        for p in placed {
            let Some(row) = self.rows.get(p.row) else { continue };
            let s = computed.scheduled(p.uid);
            for (date, column, value) in [
                (p.start, CsvColumn::Start, s.map(|s| s.start)),
                (p.finish, CsvColumn::Finish, s.map(|s| s.finish)),
            ] {
                if let Some(csv) = date
                    && value != Some(csv)
                {
                    diags.push(
                        row.line,
                        self.header(column),
                        CsvCode::DateMismatch { csv, computed: value },
                        row.value(self.columns, column),
                    );
                }
            }
        }
    }
}
