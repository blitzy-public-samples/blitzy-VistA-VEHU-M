//! Equivalence harness for OPTION `PRCH DISPLAY PO` (`EN3^PRCHRPT`): judges
//! each golden case GC-01 to GC-05 against [`inquire`], judges each case of
//! the parity corpus, and writes `REPORT.md` in full.
//!
//! The verdicts (AAP 0.7.2):
//!
//! - NOT COMPARED: the case folder is absent, or it lacks the input or the
//!   transcript capture file;
//! - PASS: the case is READY (all six readiness checks pass), its recorded
//!   context is accepted, the normalised transcripts are equal, the return
//!   values are equal and the store equals its pre-call clone;
//! - FAIL: any other recorded case, an unreadable capture included. A case
//!   that is recorded but not READY is FAIL (conflict K12).
//!
//! The GC cases are judged on world A, the fixtures alone. The parity corpus
//! (`SPEC.md` "Parity corpus") is judged on world B, the fixtures plus
//! `corpus.zwr`, with raw transcripts and no normalisation. Its verdicts
//! (AAP 0.5.7), the first that applies winning:
//!
//! - BOUNDARY: the case is class BOUNDARY, outside equivalence;
//! - NOT COMPARED: the recording run or the case fails an integrity check,
//!   or the case has no transcript capture;
//! - QUARANTINED: the run summary marks the case as showing a paging prompt;
//! - MATCH: the capture equals the Rust transcript byte for byte;
//! - MISMATCH: any other capture, with its first differing line.
//!
//! A CR case also gets a world label: WORLD-INDEPENDENT when its capture
//! equals the raw capture of its GC case, WORLD-DEPENDENT when it differs,
//! and NOT COMPARED without a usable capture of either.
//!
//! Capture files are read only through `golden`: `load_case`, which also
//! evaluates readiness, and `readiness` alone for an unreadable case, and for
//! the corpus `load_corpus_case` and `load_corpus_run`. The fixtures are
//! loaded only through `store::load_fixtures`, and `corpus.zwr` with
//! `fs::read_to_string`. `SPEC.md` is compiled in: the report copies its
//! static sections, its literals table only labels a differing line, and the
//! corpus model takes its branch inventory. Neither golden nor specification
//! text ever derives a rule.
//!
//! `equivalence_report` is the only writer of `REPORT.md`, and the harness
//! writes nothing else, except that the corpus reader tests write synthetic
//! corpus folders under the system temp dir, each removed by its test. A
//! `SPEC.md` lacking a required section fails it before anything is written,
//! and an error writing `REPORT.md` fails it. Once the report is written, it
//! fails only when a case is FAIL, a harness check fails or the corpus fails
//! an integrity check, never on a corpus verdict, so a run with no captures
//! exits 0 when the fixtures load and the other harness checks pass. Exit 0
//! is not acceptance, which needs five PASS.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ifcap_po_inquiry::corpus;
use ifcap_po_inquiry::golden::{
    CASE_IDS, CHECKS, CORPUS_META_KEYS, Case, CaseLoad, CorpusRun, Readiness, Rule,
    corpus_case_ids, corpus_dir, default_dir, load_case, load_corpus_case, load_corpus_run,
    normalise, readiness, sha256_hex, validate_context,
};
use ifcap_po_inquiry::store::{Store, load_fixtures};
use ifcap_po_inquiry::{InquiryError, inquire};

/// The specification the report's static parts come from, read at compile
/// time so the report never depends on the working directory.
const SPEC: &str = include_str!("../SPEC.md");

/// The crate folder: the fixtures and `REPORT.md` are found under it, and
/// report text names every path below it crate-relatively.
const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

const PASS: &str = "PASS";
const FAIL: &str = "FAIL";
const NOT_COMPARED: &str = "NOT COMPARED";

/// The `SPEC.md` section whose body is part 1 of the report.
const RUN_EVIDENCE: &str = "## Run evidence";
/// The `SPEC.md` section whose first sentence heads the report.
const SCOPE_STATEMENT: &str = "## Scope statement";

/// The `SPEC.md` sections the report copies verbatim after part 3, heading
/// included, in report order (AAP 0.3.6).
const STATIC_SECTIONS: [&str; 9] = [
    "## Stubs",
    "## Flagged hard-MUMPS features",
    "## Unexercised branches",
    "## Normalisation",
    "## Places the original looks wrong",
    "## Existing items outside the layout",
    "## Open items",
    SCOPE_STATEMENT,
    "## Acceptance record",
];

/// The `SPEC.md` subsection whose table names the `TAG^ROUTINE` that writes
/// each literal, used only to label the first differing line of a FAIL, of a
/// corpus MISMATCH or of a WORLD-DEPENDENT label.
const LITERALS: &str = "### Literals the five cases can print";
const NOT_ATTRIBUTED: &str = "not attributed";

const ABSENT_LINE: &str = "`<absent>`";

const TITLE: &str = "# REPORT: PRCH DISPLAY PO (EN3^PRCHRPT) parity";
const WRITTEN_BY: &str = "This file is written in full by `tests/equivalence.rs` on every \
                          `cargo test` and is never edited by hand. Exit 0 of `cargo test` is \
                          not acceptance, which needs five PASS.";

/// Each `golden::CHECKS` name, in that order, with the definition its
/// implementation in `src/golden.rs` applies. `equivalence_report` fails the
/// readiness harness check when the names differ from `CHECKS`.
const CHECK_DEFINITIONS: [(&str, &str); 6] = [
    (
        "seven_files_present",
        "Each of the seven capture files (`input.txt`, `transcript.txt`, `returned.txt`, \
         `db-before.zwr`, `db-after.zwr`, `meta.txt`, `normalisation.txt`) exists as a regular \
         file in the case folder.",
    ),
    (
        "meta_keys_present",
        "`meta.txt` is UTF-8, parses as `key=value` lines and holds all eight keys \
         `image_digest`, `source_revision`, `snapshot_id`, `record_ids`, `entry_point`, \
         `context`, `setup` and `docker_exec_args`.",
    ),
    (
        "input_ends_with_exit",
        "`input.txt` is UTF-8 and its last line is the exit input `^`.",
    ),
    (
        "normalisation_parses",
        "`normalisation.txt` is UTF-8 and every non-blank line is a non-empty open delimiter, \
         one tab and a non-empty close delimiter (an empty file holds no rules).",
    ),
    (
        "db_before_equals_after",
        "`db-before.zwr` and `db-after.zwr` are both readable and byte-identical.",
    ),
    (
        "no_carriage_return",
        "Every capture file that is present is readable and contains no carriage return.",
    ),
];

// ---------------------------------------------------------------------------
// SPEC.md slicing
// ---------------------------------------------------------------------------

/// The lines of `spec`, each paired with whether it can be a heading. Lines
/// of a fenced code block, its fence lines included, cannot, so command
/// output quoted in `SPEC.md` never starts or ends a section.
fn spec_lines(spec: &str) -> Vec<(&str, bool)> {
    let mut open: Option<(char, usize)> = None;
    spec.split('\n')
        .map(|line| match (open, fence_marker(line)) {
            (None, Some((mark, run, _))) => {
                open = Some((mark, run));
                (line, false)
            }
            (Some((mark, run)), Some((found, length, bare)))
                if found == mark && length >= run && bare =>
            {
                open = None;
                (line, false)
            }
            (Some(_), _) => (line, false),
            (None, None) => (line, true),
        })
        .collect()
}

/// For a code-fence line (at most three spaces, then three or more backticks
/// or tildes): the fence character, the length of its run and whether only
/// whitespace follows the run, which a closing fence needs.
fn fence_marker(line: &str) -> Option<(char, usize, bool)> {
    let body = line.trim_start_matches(' ');
    if line.len() - body.len() > 3 {
        return None;
    }
    let mark = body.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = body.chars().take_while(|c| *c == mark).count();
    if run < 3 {
        return None;
    }
    // The fence characters are ASCII, so `run` is also a byte offset.
    let rest = body.get(run..).unwrap_or_default();
    Some((mark, run, rest.trim().is_empty()))
}

/// The text of `spec` from the heading line exactly equal to `heading`
/// through the line before the next heading line `ends` accepts, or to the
/// end, without trailing blank lines; `None` when no heading line equals
/// `heading`.
fn slice(spec: &str, heading: &str, ends: impl Fn(&str) -> bool) -> Option<String> {
    let lines = spec_lines(spec);
    let start = lines
        .iter()
        .position(|(line, heading_line)| *heading_line && *line == heading)?;
    let mut taken: Vec<&str> = lines
        .iter()
        .skip(start)
        .enumerate()
        .take_while(|(index, (line, heading_line))| *index == 0 || !(*heading_line && ends(line)))
        .map(|(_, (line, _))| *line)
        .collect();
    while taken.last().is_some_and(|line| line.trim().is_empty()) {
        taken.pop();
    }
    Some(taken.join("\n"))
}

/// The `## ` section `heading` of `spec`, heading line included.
fn section(spec: &str, heading: &str) -> Option<String> {
    slice(spec, heading, |line| line.starts_with("## "))
}

/// The `## ` section `heading` of `spec` without its heading line and
/// without leading blank lines.
fn section_body(spec: &str, heading: &str) -> Option<String> {
    let text = section(spec, heading)?;
    let body: Vec<&str> = text
        .split('\n')
        .skip(1)
        .skip_while(|line| line.trim().is_empty())
        .collect();
    Some(body.join("\n"))
}

/// The required headings `spec` lacks as heading lines, in report order.
fn missing_headings(spec: &str) -> Vec<&'static str> {
    let lines = spec_lines(spec);
    std::iter::once(RUN_EVIDENCE)
        .chain(STATIC_SECTIONS)
        .filter(|heading| {
            !lines
                .iter()
                .any(|(line, heading_line)| *heading_line && line == heading)
        })
        .collect()
}

fn missing_message(missing: &[&str]) -> String {
    missing
        .iter()
        .map(|heading| format!("MISSING SECTION {heading} IN SPEC.md"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The report's header sentence: the first sentence of the first non-blank
/// line of the scope statement, through the first `. `, or the whole line.
fn scope_sentence(spec: &str) -> Option<String> {
    let body = section_body(spec, SCOPE_STATEMENT)?;
    let line = body
        .split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(match line.split_once(". ") {
        Some((first, _)) => format!("{first}."),
        None => line.to_string(),
    })
}

/// One row of the `SPEC.md` literals table: the literal fragments it lists
/// and the `TAG^ROUTINE` text of the tags that write them.
struct Literal {
    fragments: Vec<String>,
    writer: String,
}

/// The rows of the literals table, header and separator rows skipped. A
/// missing subsection gives no rows, so every expected line is `not
/// attributed`.
fn literal_rows(spec: &str) -> Vec<Literal> {
    let Some(table) = slice(spec, LITERALS, |line| {
        line.starts_with("## ") || line.starts_with("### ")
    }) else {
        return Vec::new();
    };
    table
        .split('\n')
        .filter(|line| line.starts_with('|'))
        .skip(1)
        .filter_map(|line| {
            let cells = table_cells(line);
            let literal = cells.get(1)?;
            let writer = cells.get(2)?.replace('`', "").trim().to_string();
            let separator = literal.trim().chars().all(|c| c == '-' || c == ':');
            let fragments = fragments(literal);
            (!separator && !writer.is_empty() && !fragments.is_empty())
                .then_some(Literal { fragments, writer })
        })
        .collect()
}

/// The cells of a table row, split on each `|` not preceded by `\`. The
/// text before the leading `|` is cell 0.
fn table_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if c == '|' && !escaped {
            cells.push(String::new());
        } else if let Some(cell) = cells.last_mut() {
            cell.push(c);
        }
        escaped = c == '\\';
    }
    cells
}

/// The backtick-quoted fragments of a literal cell, as they appear on one
/// transcript line: `\|` is `|` and `<BEL>` is `$C(7)`, as `SPEC.md` writes
/// them, and each `\n` marks a line end, which no line contains. A fragment
/// shorter than two characters after trimming labels nothing and is dropped.
fn fragments(cell: &str) -> Vec<String> {
    let parts: Vec<&str> = cell.split('`').collect();
    parts
        .iter()
        .take(parts.len().saturating_sub(1))
        .skip(1)
        .step_by(2)
        .map(|part| {
            part.replace("\\|", "|")
                .replace("<BEL>", "\u{7}")
                .replace("\\n", "")
        })
        .filter(|fragment| fragment.trim().chars().count() >= 2)
        .collect()
}

/// The attribution of the expected line: `attributed to <TAG^ROUTINE>`,
/// naming the writer of the row whose longest fragment contained in the
/// line is the longest of all rows, the earliest row on a tie, or `not
/// attributed` when no row's fragment is in the line or the line is absent.
///
/// The longest match decides rather than the first row that matches,
/// because a short literal of an earlier row can sit inside a later row's
/// longer one: `ENTER '^' TO HALT: ` of `^PRCHDP2` is part of
/// `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ` of `ASK1^PRCHDP2`.
fn attribute(rows: &[Literal], expected: Option<&str>) -> String {
    expected
        .and_then(|line| {
            // `max_by_key` keeps the last of equal keys, so walking the rows
            // backwards keeps the earliest row on a tie.
            rows.iter()
                .rev()
                .filter_map(|row| {
                    row.fragments
                        .iter()
                        .filter(|fragment| line.contains(fragment.as_str()))
                        .map(|fragment| fragment.chars().count())
                        .max()
                        .map(|length| (length, row))
                })
                .max_by_key(|(length, _)| *length)
        })
        .map_or_else(
            || NOT_ATTRIBUTED.to_string(),
            |(_, row)| format!("attributed to {}", row.writer),
        )
}

// ---------------------------------------------------------------------------
// Report text
// ---------------------------------------------------------------------------

/// `text` with every path under the crate folder made crate-relative, so the
/// report does not depend on where the crate is checked out.
fn crate_relative(text: &str) -> String {
    let prefix = format!("{CRATE_DIR}{}", std::path::MAIN_SEPARATOR);
    text.replace(&prefix, "")
}

fn escape_control(c: char, out: &mut String) {
    match c {
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        // Every control character is below U+00A0, so two digits suffice.
        c if c.is_control() => out.push_str(&format!("\\x{:02X}", u32::from(c))),
        c => out.push(c),
    }
}

/// `text` on one line, its control characters shown as escapes.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        escape_control(c, &mut out);
    }
    out
}

/// `text` in double quotes inside a Markdown code span, with `\`, `"` and
/// control characters escaped, so empty text, edge spaces and the original's
/// BEL and line ends all stay visible on one line.
fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len().saturating_add(2));
    quoted.push('"');
    for c in text.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            c => escape_control(c, &mut quoted),
        }
    }
    quoted.push('"');
    // A fence longer than every backtick run inside keeps the span whole.
    let longest = quoted
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat(longest.saturating_add(1));
    format!("{fence}{quoted}{fence}")
}

fn shown(value: Option<&str>) -> String {
    value.map_or_else(|| "None".to_string(), quote)
}

fn readiness_text(readiness: &Readiness) -> String {
    match readiness {
        Readiness::Absent => "ABSENT".to_string(),
        Readiness::Ready => "READY".to_string(),
        Readiness::Incomplete(failed) => format!("INCOMPLETE: {}", failed.join(", ")),
    }
}

// ---------------------------------------------------------------------------
// Verdicts
// ---------------------------------------------------------------------------

/// What part 2 shows for a case's normalisation rules.
enum Rules {
    NoCapture,
    Unreadable,
    Listed(Vec<Rule>),
}

/// One case's verdict and everything the report shows for it.
struct CaseResult {
    id: &'static str,
    verdict: &'static str,
    readiness: String,
    rules: Rules,
    image_digest: Option<String>,
    snapshot_id: Option<String>,
    /// Every reason the case is not PASS, in evaluation order; empty unless
    /// the verdict is FAIL.
    causes: Vec<String>,
    /// Whether the store differed from its pre-call clone after the case's
    /// session; `None` when no session ran.
    store_changed: Option<bool>,
}

fn not_compared(id: &'static str, readiness: &Readiness) -> CaseResult {
    CaseResult {
        id,
        verdict: NOT_COMPARED,
        readiness: readiness_text(readiness),
        rules: Rules::NoCapture,
        image_digest: None,
        snapshot_id: None,
        causes: Vec::new(),
        store_changed: None,
    }
}

/// The verdict of case `id` in the golden folder `golden`, over the loaded
/// fixtures or the reason they did not load.
fn evaluate(
    id: &'static str,
    golden: &Path,
    fixtures: &Result<Store, String>,
    literals: &[Literal],
) -> CaseResult {
    match load_case(golden, id) {
        CaseLoad::Absent => not_compared(id, &Readiness::Absent),
        CaseLoad::NotRecorded(failed) => not_compared(id, &Readiness::Incomplete(failed)),
        CaseLoad::Unreadable(cause) => CaseResult {
            id,
            verdict: FAIL,
            readiness: readiness_text(&readiness(&golden.join(id))),
            rules: Rules::Unreadable,
            image_digest: None,
            snapshot_id: None,
            causes: vec![format!(
                "unreadable capture file: {}",
                crate_relative(&cause)
            )],
            store_changed: None,
        },
        CaseLoad::Recorded(case) => compare(id, &case, fixtures, literals),
    }
}

/// The verdict of a recorded case: PASS only when no cause is found.
fn compare(
    id: &'static str,
    case: &Case,
    fixtures: &Result<Store, String>,
    literals: &[Literal],
) -> CaseResult {
    let mut causes = Vec::new();
    match &case.readiness {
        Readiness::Ready => {}
        Readiness::Incomplete(failed) => {
            causes.push(format!("failed readiness checks: {}", failed.join(", ")));
        }
        Readiness::Absent => causes.push("failed readiness checks: case folder absent".to_string()),
    }
    match case.meta.as_ref().map(|meta| meta.context()) {
        None => causes.push("meta.txt missing or unparsable".to_string()),
        Some(Err(cause)) => causes.push(format!("context unparsable: {cause}")),
        Some(Ok(context)) => {
            if let Err(mismatch) = validate_context(&context) {
                causes.push(format!("context mismatch: {mismatch}"));
            }
        }
    }
    let mut store_changed = None;
    match fixtures {
        Err(cause) => causes.push(format!("fixtures not loaded: {cause}")),
        Ok(store) => {
            let before = store.clone();
            match inquire(store, &case.inputs) {
                Err(err @ InquiryError::CorruptStore(_)) => {
                    causes.push(format!("CorruptStore: {err}"));
                }
                Ok(out) => {
                    let expected = normalise(&case.transcript, &case.rules);
                    let actual = normalise(&out.transcript, &case.rules);
                    if expected != actual {
                        causes.push(transcript_cause(&expected, &actual, literals));
                    }
                    if case.returned != out.returned {
                        causes.push(format!(
                            "returned differs: expected {}, actual {}",
                            shown(case.returned.as_deref()),
                            shown(out.returned.as_deref())
                        ));
                    }
                }
            }
            let changed = *store != before;
            if changed {
                causes.push("store changed during the session".to_string());
            }
            store_changed = Some(changed);
        }
    }
    let meta = |key: &str| {
        case.meta
            .as_ref()
            .and_then(|meta| meta.get(key))
            .map(str::to_string)
    };
    CaseResult {
        id,
        verdict: if causes.is_empty() { PASS } else { FAIL },
        readiness: readiness_text(&case.readiness),
        rules: Rules::Listed(case.rules.clone()),
        image_digest: meta("image_digest"),
        snapshot_id: meta("snapshot_id"),
        causes,
        store_changed,
    }
}

/// The first line, 1-based, where two different transcripts differ, with
/// both texts and the attribution of the expected line: `attributed to
/// <TAG^ROUTINE>` or `not attributed`. A GC case passes its normalised
/// transcripts, and the corpus its raw ones.
fn transcript_cause(expected: &str, actual: &str, literals: &[Literal]) -> String {
    let wanted: Vec<&str> = expected.split('\n').collect();
    let got: Vec<&str> = actual.split('\n').collect();
    let longest = wanted.len().max(got.len());
    let index = (0..longest)
        .find(|&index| wanted.get(index) != got.get(index))
        .unwrap_or(longest);
    let line = |lines: &[&str]| {
        lines
            .get(index)
            .map_or_else(|| ABSENT_LINE.to_string(), |text| quote(text))
    };
    format!(
        "transcript differs at line {}: expected {}, actual {}, {}",
        index.saturating_add(1),
        line(&wanted),
        line(&got),
        attribute(literals, wanted.get(index).copied())
    )
}

// ---------------------------------------------------------------------------
// Parity corpus
// ---------------------------------------------------------------------------

const MATCH: &str = "MATCH";
const MISMATCH: &str = "MISMATCH";
const BOUNDARY: &str = "BOUNDARY";
const QUARANTINED: &str = "QUARANTINED";
const WORLD_INDEPENDENT: &str = "WORLD-INDEPENDENT";
const WORLD_DEPENDENT: &str = "WORLD-DEPENDENT";

/// Why a case with no transcript capture is NOT COMPARED.
const NO_CAPTURE: &str = "no capture";
/// Why every case is NOT COMPARED while the recording run fails an
/// integrity check.
const RUN_INTEGRITY_FAILURE: &str = "recording run integrity failure";
/// Why a case that fails an integrity check of its own is NOT COMPARED.
const CASE_INTEGRITY_FAILURE: &str = "integrity failure";
/// The detail of a verdict or label that has no differing line to show.
const NO_DETAIL: &str = "—";

/// The corpus class labels, as `corpus::Class::label` writes them.
const CLASS_CP: &str = "CP";
const CLASS_IN: &str = "IN";
const CLASS_BOUNDARY: &str = "BOUNDARY";
const CLASS_CR: &str = "CR";
/// The class labels in the row order of the count table.
const CLASS_ROWS: [&str; 4] = [CLASS_CP, CLASS_IN, CLASS_BOUNDARY, CLASS_CR];

/// The verdict columns of the count table, before its total.
const VERDICT_COLUMNS: [&str; 5] = [MATCH, MISMATCH, NOT_COMPARED, BOUNDARY, QUARANTINED];

/// The heading of the corpus block, a subsection of part 3 (conflict K18).
const CORPUS_HEADING: &str = "### Parity corpus";
/// The start of the block's recording run line.
const RUN_LINE: &str = "Recording run: ";

/// One corpus case to judge: its id, its final class label, the inputs the
/// model generated for it and, for a CR case, the GC case it replays.
struct CorpusEntry {
    id: String,
    class: &'static str,
    inputs: Vec<String>,
    gc: Option<&'static str>,
}

/// The world label of a CR case.
struct WorldLabel {
    /// The GC case whose capture the CR capture is compared with.
    gc: String,
    label: &'static str,
    /// The first differing line of a WORLD-DEPENDENT label, why nothing was
    /// compared, or [`NO_DETAIL`].
    detail: String,
}

/// One corpus case's verdict and everything the report shows for it.
struct CorpusVerdict {
    id: String,
    class: &'static str,
    verdict: &'static str,
    /// The first differing line of a MISMATCH, why a case is NOT COMPARED,
    /// or [`NO_DETAIL`].
    detail: String,
    /// Whether the case loaded with a transcript capture.
    recorded: bool,
    /// The world label, for a CR case only.
    world: Option<WorldLabel>,
}

/// The verdicts of a set of corpus cases, with the recording run line and
/// the two numbers of the MATCHED line.
struct CorpusJudgement {
    run_line: String,
    verdicts: Vec<CorpusVerdict>,
    /// Every integrity failure, of the recording run and of each case, in
    /// the order found.
    integrity: Vec<String>,
    /// n of the MATCHED line: the CP and IN cases that are MATCH.
    matched: usize,
    /// m of the MATCHED line: the CP and IN cases that are MATCH or
    /// MISMATCH, which are those with a usable capture that are not
    /// quarantined.
    in_scope: usize,
}

/// Judges `entries` from the corpus folder `corpus_dir` on `world_b`, and
/// labels each CR case against its GC capture in `gc_dir`, by the
/// precedence of the module doc (AAP 0.5.7). The recording run is verified
/// once. A case that fails to load, and a committed input that differs from
/// the model's, is an integrity failure; the committed input is the one
/// run. A Rust session runs only for a usable capture, so with no recording
/// none runs.
fn judge_corpus(
    corpus_dir: &Path,
    gc_dir: &Path,
    world_b: &Store,
    entries: &[CorpusEntry],
    literals: &[Literal],
) -> CorpusJudgement {
    let ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
    let mut integrity = Vec::new();
    let (run_line, run_failed, quarantined) = match load_corpus_run(corpus_dir, &ids) {
        Ok(CorpusRun::Absent) => (format!("{RUN_LINE}none"), false, Vec::new()),
        Ok(CorpusRun::Recorded {
            meta,
            quarantined,
            export_sha256,
        }) => {
            let values: Vec<String> = CORPUS_META_KEYS
                .iter()
                .map(|key| format!("{key}={}", quote(meta.get(key).unwrap_or_default())))
                .collect();
            let line = format!(
                "{RUN_LINE}{}; export_sha256={export_sha256}; exports=IDENTICAL runs=IDENTICAL",
                values.join("; ")
            );
            (line, false, quarantined)
        }
        Err(cause) => {
            let cause = crate_relative(&cause);
            let line = format!("{RUN_LINE}INTEGRITY FAIL: {}", one_line(&cause));
            integrity.push(cause);
            (line, true, Vec::new())
        }
    };
    let mut verdicts = Vec::with_capacity(entries.len());
    for entry in entries {
        let case = match load_corpus_case(corpus_dir, &entry.id) {
            Ok(case) => {
                if case.inputs != entry.inputs {
                    integrity.push(format!(
                        "{}: committed input differs from the generated model",
                        entry.id
                    ));
                }
                Some(case)
            }
            Err(cause) => {
                integrity.push(crate_relative(&cause));
                None
            }
        };
        // The committed inputs and the capture a verdict may use, or why
        // there is none: precedence rules (b), (c) and (d), in that order.
        let usable: Result<(&[String], &str), &'static str> = match &case {
            _ if run_failed => Err(RUN_INTEGRITY_FAILURE),
            None => Err(CASE_INTEGRITY_FAILURE),
            Some(case) => case
                .transcript
                .as_deref()
                .map(|capture| (case.inputs.as_slice(), capture))
                .ok_or(NO_CAPTURE),
        };
        let (verdict, detail) = match usable {
            _ if entry.class == CLASS_BOUNDARY => (BOUNDARY, NO_DETAIL.to_string()),
            Err(reason) => (NOT_COMPARED, reason.to_string()),
            Ok(_) if quarantined.contains(&entry.id) => (QUARANTINED, NO_DETAIL.to_string()),
            Ok((inputs, capture)) => session_verdict(world_b, inputs, capture, literals),
        };
        let capture = usable.map(|(_, capture)| capture);
        verdicts.push(CorpusVerdict {
            id: entry.id.clone(),
            class: entry.class,
            verdict,
            detail,
            recorded: case.as_ref().is_some_and(|case| case.transcript.is_some()),
            world: entry
                .gc
                .map(|gc| world_label(gc_dir, gc, capture, literals)),
        });
    }
    let counted = |wanted: &[&str]| {
        verdicts
            .iter()
            .filter(|verdict| verdict.class == CLASS_CP || verdict.class == CLASS_IN)
            .filter(|verdict| wanted.contains(&verdict.verdict))
            .count()
    };
    let matched = counted(&[MATCH]);
    let in_scope = counted(&[MATCH, MISMATCH]);
    CorpusJudgement {
        run_line,
        verdicts,
        integrity,
        matched,
        in_scope,
    }
}

/// The verdict of a usable `capture`: the Rust session on `world_b` with
/// the committed `inputs`, compared with it byte for byte. A Rust session
/// error is a MISMATCH, and so is a capture that shows a runtime error of
/// the original.
fn session_verdict(
    world_b: &Store,
    inputs: &[String],
    capture: &str,
    literals: &[Literal],
) -> (&'static str, String) {
    let run = corpus::run_case(world_b, inputs);
    if let Some(error) = run.error {
        (MISMATCH, format!("rust session error: {error}"))
    } else if run.transcript == capture {
        (MATCH, NO_DETAIL.to_string())
    } else {
        (
            MISMATCH,
            transcript_cause(capture, &run.transcript, literals),
        )
    }
}

/// The world label of a CR case replaying GC case `gc`: its usable
/// `capture` compared byte for byte with the raw capture of `gc` in
/// `gc_dir`, or NOT COMPARED with the reason one of them is not usable.
/// Quarantine does not change it.
fn world_label(
    gc_dir: &Path,
    gc: &str,
    capture: Result<&str, &'static str>,
    literals: &[Literal],
) -> WorldLabel {
    let (label, detail) = match capture {
        Err(reason) => (NOT_COMPARED, reason.to_string()),
        Ok(capture) => match load_case(gc_dir, gc) {
            CaseLoad::Recorded(case) if case.transcript == capture => {
                (WORLD_INDEPENDENT, NO_DETAIL.to_string())
            }
            CaseLoad::Recorded(case) => (
                WORLD_DEPENDENT,
                transcript_cause(&case.transcript, capture, literals),
            ),
            CaseLoad::Absent | CaseLoad::NotRecorded(_) | CaseLoad::Unreadable(_) => {
                (NOT_COMPARED, format!("{gc} capture not loaded"))
            }
        },
    };
    WorldLabel {
        gc: gc.to_string(),
        label,
        detail,
    }
}

/// The corpus part of the report: the `### Parity corpus` block of part 3,
/// the MATCHED line the run prints, and every integrity failure, each of
/// which fails the run once the report is written.
struct CorpusReport {
    block: String,
    matched_line: String,
    integrity: Vec<String>,
}

/// World B: `world_a` plus the committed `corpus.zwr`, which is a fixture,
/// not a capture. Each error is crate-relative text naming the file.
fn corpus_world(world_a: &Store) -> Result<Store, String> {
    let path = corpus::corpus_zwr_path();
    let text = fs::read_to_string(&path)
        .map_err(|err| crate_relative(&format!("{}: {err}", path.display())))?;
    corpus::world_b(world_a, &text)
        .map_err(|err| crate_relative(&format!("{}: {err}", path.display())))
}

/// The corpus model generated from `world_a` and `SPEC.md`, and world B, or
/// every reason either is missing. A model whose case ids are not
/// `golden::corpus_case_ids()` in order is missing too.
fn corpus_model(world_a: &Store) -> Result<(corpus::Corpus, Store), Vec<String>> {
    let ids = corpus_case_ids();
    let model = corpus::generate(world_a, SPEC)
        .map_err(|err| {
            format!(
                "corpus model not generated: {}",
                crate_relative(&err.to_string())
            )
        })
        .and_then(|model| {
            if model.cases.iter().map(|case| &case.id).eq(ids.iter()) {
                Ok(model)
            } else {
                Err("corpus model case ids differ from golden::corpus_case_ids()".to_string())
            }
        });
    match (model, corpus_world(world_a)) {
        (Ok(model), Ok(world_b)) => Ok((model, world_b)),
        (model, world_b) => Err(model.err().into_iter().chain(world_b.err()).collect()),
    }
}

/// One entry per model case, in case order, with its final class and, for
/// a CR case, the GC case `corpus::CR_INPUTS` pairs it with.
fn corpus_entries(model: &corpus::Corpus) -> Vec<CorpusEntry> {
    model
        .cases
        .iter()
        .map(|case| CorpusEntry {
            id: case.id.clone(),
            class: case.class.label(),
            inputs: case.inputs.clone(),
            gc: corpus::CR_INPUTS
                .into_iter()
                .find(|(cr, _, _)| *cr == case.id)
                .map(|(_, gc, _)| gc),
        })
        .collect()
}

fn matched_line(matched: usize, in_scope: usize) -> String {
    format!("MATCHED {matched} OF {in_scope} IN-SCOPE CORPUS CASES")
}

/// The corpus verdicts over the loaded fixtures, or the reason they did not
/// load, judged from the committed corpus folder against the GC captures.
/// When the model or world B is missing, the block holds only its heading,
/// the first cause as the recording run line and `MATCHED 0 OF 0`.
fn corpus_results(world_a: &Result<Store, String>, literals: &[Literal]) -> CorpusReport {
    let built = match world_a {
        Err(cause) => Err(vec![format!("fixtures not loaded: {cause}")]),
        Ok(store) => corpus_model(store),
    };
    match built {
        Err(integrity) => {
            let matched_line = matched_line(0, 0);
            let first = integrity.first().map_or("", String::as_str);
            let block = [
                CORPUS_HEADING.to_string(),
                format!("{RUN_LINE}INTEGRITY FAIL: {}", one_line(first)),
                matched_line.clone(),
            ]
            .join("\n\n");
            CorpusReport {
                block,
                matched_line,
                integrity,
            }
        }
        Ok((model, world_b)) => {
            let entries = corpus_entries(&model);
            let judgement =
                judge_corpus(&corpus_dir(), &default_dir(), &world_b, &entries, literals);
            let matched_line = matched_line(judgement.matched, judgement.in_scope);
            CorpusReport {
                block: corpus_block(&model, &judgement, &matched_line),
                matched_line,
                integrity: judgement.integrity,
            }
        }
    }
}

/// `text` as one Markdown table cell: on one line, with every `|` escaped,
/// so no cell breaks its row.
fn cell(text: &str) -> String {
    one_line(text).replace('|', "\\|")
}

/// A Markdown table of the `header` cells and `rows`, each row cell through
/// [`cell`].
fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut lines = vec![
        format!("| {} |", header.join(" | ")),
        format!("|{}", "---|".repeat(header.len())),
    ];
    lines.extend(rows.iter().map(|row| {
        let cells: Vec<String> = row.iter().map(|text| cell(text)).collect();
        format!("| {} |", cells.join(" | "))
    }));
    lines.join("\n")
}

/// [`table`], or `none` when there are no rows.
fn table_or_none(header: &[&str], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        "none".to_string()
    } else {
        table(header, rows)
    }
}

/// The `### Parity corpus` block of part 3 (AAP 0.5.7): the recording run
/// line, the count table, the MATCHED line at column 0, the table of every
/// case, then the class moves, findings and dropped variants of `model`,
/// the BOUNDARY map and the CR world labels. The first column of every
/// table is a class, a case id or a round, so no row begins `| PO-`.
fn corpus_block(model: &corpus::Corpus, judgement: &CorpusJudgement, matched_line: &str) -> String {
    let verdicts = &judgement.verdicts;
    let mut count_header = vec!["Class"];
    count_header.extend(VERDICT_COLUMNS);
    count_header.push("Total");
    let counts: Vec<Vec<String>> = CLASS_ROWS
        .iter()
        .map(|class| {
            let of_class: Vec<&CorpusVerdict> = verdicts
                .iter()
                .filter(|verdict| verdict.class == *class)
                .collect();
            std::iter::once(class.to_string())
                .chain(VERDICT_COLUMNS.iter().map(|column| {
                    of_class
                        .iter()
                        .filter(|verdict| verdict.verdict == *column)
                        .count()
                        .to_string()
                }))
                .chain(std::iter::once(of_class.len().to_string()))
                .collect()
        })
        .collect();
    let cases: Vec<Vec<String>> = verdicts
        .iter()
        .map(|verdict| {
            vec![
                verdict.id.clone(),
                verdict.class.to_string(),
                verdict.verdict.to_string(),
                verdict.detail.clone(),
            ]
        })
        .collect();
    let moves: Vec<Vec<String>> = model
        .moves
        .iter()
        .map(|change| {
            vec![
                change.case.clone(),
                change.from.label().to_string(),
                change.to.label().to_string(),
                change.branch.clone(),
            ]
        })
        .collect();
    let findings: Vec<Vec<String>> = model
        .findings
        .iter()
        .map(|finding| {
            vec![
                finding.case.clone(),
                format!("Implemented({})", finding.branch),
                finding.cause.clone(),
            ]
        })
        .collect();
    let drops: Vec<Vec<String>> = model
        .drops
        .iter()
        .map(|dropped| {
            vec![
                dropped.round.to_string(),
                dropped.case.clone(),
                dropped.dimension.clone(),
                dropped.value.clone(),
                dropped.branch.clone(),
            ]
        })
        .collect();
    // The verdicts are in model case order, as `corpus_entries` built them.
    let boundary: Vec<Vec<String>> = model
        .cases
        .iter()
        .zip(verdicts)
        .filter(|(case, _)| case.class.label() == CLASS_BOUNDARY)
        .map(|(case, verdict)| {
            let inputs: Vec<String> = case.inputs.iter().map(|input| quote(input)).collect();
            vec![
                case.id.clone(),
                inputs.join(", "),
                case.expected_branch
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                if verdict.recorded { "yes" } else { "no" }.to_string(),
            ]
        })
        .collect();
    let worlds: Vec<Vec<String>> = verdicts
        .iter()
        .filter_map(|verdict| {
            verdict.world.as_ref().map(|world| {
                vec![
                    verdict.id.clone(),
                    world.gc.clone(),
                    world.label.to_string(),
                    world.detail.clone(),
                ]
            })
        })
        .collect();
    [
        CORPUS_HEADING.to_string(),
        judgement.run_line.clone(),
        table(&count_header, &counts),
        matched_line.to_string(),
        table(
            &["Case", "Class", "Verdict", "First differing line"],
            &cases,
        ),
        "#### Class moves".to_string(),
        table_or_none(
            &["Case", "Specified class", "Final class", "Branch"],
            &moves,
        ),
        "#### Findings".to_string(),
        table_or_none(&["Case", "Outcome", "Cause"], &findings),
        "#### Dropped variants".to_string(),
        table_or_none(&["Round", "Case", "Dimension", "Value", "Branch"], &drops),
        "#### BOUNDARY map".to_string(),
        table_or_none(
            &["Case", "Input lines", "Expected branch", "Recorded"],
            &boundary,
        ),
        "#### CR world labels".to_string(),
        table_or_none(
            &["CR case", "GC case", "Label", "First differing line"],
            &worlds,
        ),
    ]
    .join("\n\n")
}

// ---------------------------------------------------------------------------
// Harness checks and the report
// ---------------------------------------------------------------------------

/// One check the harness performed in this run, for part 1.
struct Check {
    name: &'static str,
    passed: bool,
    detail: String,
}

fn harness_checks(fixtures: &Result<Store, String>, results: &[CaseResult]) -> Vec<Check> {
    let names_match = CHECK_DEFINITIONS
        .iter()
        .map(|(name, _)| *name)
        .eq(CHECKS.iter().copied());
    let sessions = results
        .iter()
        .filter(|result| result.store_changed.is_some())
        .count();
    let changed: Vec<&str> = results
        .iter()
        .filter(|result| result.store_changed == Some(true))
        .map(|result| result.id)
        .collect();
    let required = STATIC_SECTIONS.len().saturating_add(1);
    vec![
        Check {
            name: "fixtures loaded",
            passed: fixtures.is_ok(),
            detail: match fixtures {
                Ok(store) => format!("{} nodes", store.len()),
                Err(cause) => cause.clone(),
            },
        },
        Check {
            name: "per-case readiness evaluated",
            passed: names_match && results.len() == CASE_IDS.len(),
            detail: if names_match {
                format!("{} of {} cases", results.len(), CASE_IDS.len())
            } else {
                "the check definitions differ from golden::CHECKS".to_string()
            },
        },
        Check {
            name: "store equal to its pre-call clone",
            passed: changed.is_empty(),
            detail: if changed.is_empty() {
                format!("sessions run: {sessions}")
            } else {
                format!(
                    "sessions run: {sessions}; changed by {}",
                    changed.join(", ")
                )
            },
        },
        Check {
            name: "SPEC.md sections found",
            passed: true,
            detail: format!("{required} of {required}"),
        },
    ]
}

fn case_block(result: &CaseResult) -> String {
    let mut lines = vec![
        format!("### {}", result.id),
        String::new(),
        format!("- Verdict: {}", result.verdict),
    ];
    match &result.rules {
        Rules::NoCapture => lines.push("- Normalisation rules: none (no capture)".to_string()),
        Rules::Unreadable => {
            lines.push("- Normalisation rules: none (capture unreadable)".to_string());
        }
        Rules::Listed(rules) if rules.is_empty() => {
            lines.push("- Normalisation rules: none".to_string());
        }
        Rules::Listed(rules) => {
            lines.push("- Normalisation rules:".to_string());
            lines.extend(
                rules
                    .iter()
                    .map(|rule| format!("  - {} → {}", quote(&rule.open), quote(&rule.close))),
            );
        }
    }
    if let Some(digest) = &result.image_digest {
        lines.push(format!("- image_digest: {}", quote(digest)));
    }
    if let Some(snapshot) = &result.snapshot_id {
        lines.push(format!("- snapshot_id: {}", quote(snapshot)));
    }
    if !result.causes.is_empty() {
        lines.push("- Causes:".to_string());
        lines.extend(
            result
                .causes
                .iter()
                .map(|cause| format!("  - {}", one_line(cause))),
        );
    }
    lines.join("\n")
}

/// The whole of `REPORT.md`: header, parts 1 to 3, part 3 ending with the
/// `corpus` block (`### Parity corpus`), then the static sections.
fn render(spec: &str, checks: &[Check], results: &[CaseResult], corpus: &str) -> String {
    let mut blocks = vec![TITLE.to_string()];
    blocks.extend(scope_sentence(spec));
    blocks.push(WRITTEN_BY.to_string());

    blocks.push("## Part 1: Run evidence".to_string());
    blocks.extend(section_body(spec, RUN_EVIDENCE).filter(|body| !body.is_empty()));
    let mut harness = vec!["### Harness checks in this run".to_string(), String::new()];
    harness.extend(checks.iter().map(|check| {
        format!(
            "- {}: {} — {}",
            check.name,
            if check.passed { PASS } else { FAIL },
            one_line(&check.detail)
        )
    }));
    blocks.push(harness.join("\n"));

    blocks.push("## Part 2: Case results".to_string());
    blocks.extend(results.iter().map(case_block));

    blocks.push("## Part 3: Capture readiness".to_string());
    let mut readiness = vec!["| Case | Readiness |".to_string(), "|---|---|".to_string()];
    readiness.extend(
        results
            .iter()
            .map(|result| format!("| {} | {} |", result.id, result.readiness)),
    );
    blocks.push(readiness.join("\n"));
    let mut definitions = vec![
        "| Check | Definition |".to_string(),
        "|---|---|".to_string(),
    ];
    definitions.extend(
        CHECK_DEFINITIONS
            .iter()
            .map(|(name, definition)| format!("| `{name}` | {definition} |")),
    );
    blocks.push(definitions.join("\n"));
    blocks.push(corpus.to_string());

    blocks.extend(
        STATIC_SECTIONS
            .iter()
            .filter_map(|heading| section(spec, heading)),
    );
    let mut report = blocks.join("\n\n");
    report.push('\n');
    report
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Computes the five verdicts, prints `GC-0N <verdict>` for each, judges
/// the parity corpus, prints its `MATCHED n OF m IN-SCOPE CORPUS CASES`
/// line and writes `REPORT.md`. A `SPEC.md` lacking a required section fails
/// the run before anything is written, and an error writing `REPORT.md`
/// fails it. Once the report is written, the run fails only when a case is
/// FAIL, a harness check fails or the corpus fails an integrity check, never
/// on a corpus verdict.
#[test]
fn equivalence_report() {
    let missing = missing_headings(SPEC);
    assert!(missing.is_empty(), "{}", missing_message(&missing));

    let crate_dir = Path::new(CRATE_DIR);
    let fixtures = load_fixtures(&crate_dir.join("tests").join("fixtures").join("ifcap"))
        .map_err(|err| crate_relative(&err.to_string()));
    let golden = default_dir();
    let literals = literal_rows(SPEC);
    let results: Vec<CaseResult> = CASE_IDS
        .into_iter()
        .map(|id| evaluate(id, &golden, &fixtures, &literals))
        .collect();
    for result in &results {
        println!("{} {}", result.id, result.verdict);
    }
    let corpus = corpus_results(&fixtures, &literals);
    println!("{}", corpus.matched_line);
    let checks = harness_checks(&fixtures, &results);

    let report = render(SPEC, &checks, &results, &corpus.block);
    if let Err(err) = fs::write(crate_dir.join("REPORT.md"), report) {
        panic!("cannot write REPORT.md: {err}");
    }

    let failures: Vec<String> = results
        .iter()
        .filter(|result| result.verdict == FAIL)
        .map(|result| {
            format!(
                "{} FAIL: {}",
                result.id,
                result.causes.first().map_or("", |cause| cause.as_str())
            )
        })
        .chain(
            checks
                .iter()
                .filter(|check| !check.passed)
                .map(|check| format!("harness check {} FAIL: {}", check.name, check.detail)),
        )
        .chain(
            corpus
                .integrity
                .iter()
                .map(|cause| format!("corpus integrity FAIL: {cause}")),
        )
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every `SPEC.md` section the report includes exists as a heading line.
#[test]
fn spec_has_report_sections() {
    let missing = missing_headings(SPEC);
    assert!(missing.is_empty(), "{}", missing_message(&missing));
}

// ---------------------------------------------------------------------------
// Corpus reader tests: synthetic corpus folders under the system temp dir
// ---------------------------------------------------------------------------

/// The `context` a synthetic run records: the fixed O-01 answer that
/// `record_corpus.sh` writes.
const SYNTHETIC_CONTEXT: &str = r##"{"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}"##;

/// The `start_command` a synthetic run records, in the form
/// `record_corpus.sh` writes with the default container name.
const SYNTHETIC_START: &str = "docker exec -i vehu su - vehu -c \"<start command>\"";

/// The digest list's name, which the listing never includes.
const SUMS_NAME: &str = "SHA256SUMS";

/// A synthetic corpus folder `ifcap-corpus-<pid>-<name>` under the system
/// temp dir, removed when dropped. It records each file written to it with
/// its SHA-256, so its digest list is built from the bytes written and no
/// capture file is ever read back here.
struct TempCorpus {
    dir: PathBuf,
    /// Each file written, relative to `dir`, with the SHA-256 of its last
    /// content.
    written: Vec<(String, String)>,
}

impl TempCorpus {
    /// An empty folder for the test `name`, a leftover of an earlier run
    /// removed first.
    fn new(name: &str) -> TempCorpus {
        let dir = std::env::temp_dir().join(format!("ifcap-corpus-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        if let Err(err) = fs::create_dir_all(&dir) {
            panic!("cannot create {}: {err}", dir.display());
        }
        TempCorpus {
            dir,
            written: Vec::new(),
        }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    /// Writes `text` to `relative`, creating its folder, and records it.
    fn write(&mut self, relative: &str, text: &str) {
        let path = self.dir.join(relative);
        let created = path.parent().map_or(Ok(()), fs::create_dir_all);
        if let Err(err) = created.and_then(|()| fs::write(&path, text)) {
            panic!("cannot write {}: {err}", path.display());
        }
        self.written.retain(|(listed, _)| listed != relative);
        self.written
            .push((relative.to_string(), sha256_hex(text.as_bytes())));
    }

    /// Writes case `id`'s input file: each of `inputs` followed by `\n`.
    fn input(&mut self, id: &str, inputs: &[String]) {
        let text: String = inputs.iter().map(|input| format!("{input}\n")).collect();
        self.write(&format!("{id}/input.txt"), &text);
    }

    /// Writes case `id`'s transcript capture.
    fn transcript(&mut self, id: &str, text: &str) {
        self.write(&format!("{id}/transcript.txt"), text);
    }

    /// Writes the run files `record_corpus.sh` writes before its digest
    /// list: a synthetic `BASE.txt`, `meta.txt` with each of the five keys,
    /// and `run-summary.txt` with one line per `(id, status)` of `cases`.
    fn run_files(&mut self, cases: &[(&str, &str)]) {
        self.write(
            "BASE.txt",
            "synthetic=written by tests/equivalence.rs, not a snapshot\n\
             snapshot_image=synthetic\nsnapshot_id=synthetic\n",
        );
        let meta: String = CORPUS_META_KEYS
            .iter()
            .map(|key| {
                let value = match *key {
                    "context" => SYNTHETIC_CONTEXT,
                    "start_command" => SYNTHETIC_START,
                    _ => "synthetic",
                };
                format!("{key}={value}\n")
            })
            .collect();
        self.write("meta.txt", &meta);
        let identity = format!(
            "exports=IDENTICAL\nexport_sha256={}\nruns=IDENTICAL\n",
            sha256_hex(b"synthetic export")
        );
        let lines: String = cases
            .iter()
            .map(|(id, status)| format!("{id}\t{status}\t0\t0\n"))
            .collect();
        self.write("run-summary.txt", &format!("{identity}{lines}"));
    }

    /// Writes the digest list last, as `record_corpus.sh` does: every file
    /// written so far, in byte order of its path, as `<hex>  <path>`.
    fn sums(&mut self) {
        let mut listed: Vec<(String, String)> = self
            .written
            .iter()
            .filter(|(relative, _)| relative != SUMS_NAME)
            .cloned()
            .collect();
        listed.sort();
        let text: String = listed
            .iter()
            .map(|(relative, digest)| format!("{digest}  {relative}\n"))
            .collect();
        self.write(SUMS_NAME, &text);
    }

    /// The run files and then the digest list.
    fn run(&mut self, cases: &[(&str, &str)]) {
        self.run_files(cases);
        self.sums();
    }
}

impl Drop for TempCorpus {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// World B for the reader tests, built once: the world A fixtures plus the
/// committed `corpus.zwr`.
fn test_world_b() -> &'static Store {
    static WORLD_B: OnceLock<Result<Store, String>> = OnceLock::new();
    let built = WORLD_B.get_or_init(|| {
        load_fixtures(&corpus::world_a_dir())
            .map_err(|err| crate_relative(&err.to_string()))
            .and_then(|world_a| corpus_world(&world_a))
    });
    match built {
        Ok(store) => store,
        Err(cause) => panic!("world B not built: {cause}"),
    }
}

/// The inputs of the committed corpus case `id`.
fn committed_inputs(id: &str) -> Vec<String> {
    match load_corpus_case(&corpus_dir(), id) {
        Ok(case) => case.inputs,
        Err(cause) => panic!("committed corpus case {id} not loaded: {cause}"),
    }
}

/// The Rust transcript of `inputs` on world B, which must not be an error.
fn rust_transcript(inputs: &[String]) -> String {
    let run = corpus::run_case(test_world_b(), inputs);
    assert_eq!(run.error, None, "the Rust session failed");
    run.transcript
}

/// CR case `cr`'s GC case and inputs, from `corpus::CR_INPUTS`.
fn cr_case(cr: &str) -> (&'static str, Vec<String>) {
    match corpus::CR_INPUTS.into_iter().find(|(id, _, _)| *id == cr) {
        Some((_, gc, inputs)) => (gc, inputs.iter().map(|input| input.to_string()).collect()),
        None => panic!("{cr} is not in corpus::CR_INPUTS"),
    }
}

/// The raw transcript capture of GC case `gc`.
fn gc_transcript(gc: &str) -> String {
    match load_case(&default_dir(), gc) {
        CaseLoad::Recorded(case) => case.transcript,
        other => panic!("{gc} is not recorded: {other:?}"),
    }
}

fn entry(
    id: &str,
    class: &'static str,
    inputs: &[String],
    gc: Option<&'static str>,
) -> CorpusEntry {
    CorpusEntry {
        id: id.to_string(),
        class,
        inputs: inputs.to_vec(),
        gc,
    }
}

/// `entries` judged from the synthetic folder on world B, the CR cases
/// against the committed GC captures.
fn judge(folder: &TempCorpus, entries: &[CorpusEntry]) -> CorpusJudgement {
    judge_corpus(
        folder.path(),
        &default_dir(),
        test_world_b(),
        entries,
        &literal_rows(SPEC),
    )
}

fn verdict_of<'a>(judgement: &'a CorpusJudgement, id: &str) -> &'a CorpusVerdict {
    match judgement.verdicts.iter().find(|verdict| verdict.id == id) {
        Some(verdict) => verdict,
        None => panic!("no verdict for {id}"),
    }
}

/// `text` with its 1-based line `number` replaced by `line`; the result
/// must differ from `text`.
fn with_line(text: &str, number: usize, line: &str) -> String {
    let changed = text
        .split('\n')
        .enumerate()
        .map(|(index, old)| {
            if index.saturating_add(1) == number {
                line
            } else {
                old
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(changed, text, "line {number} was not replaced");
    changed
}

/// A capture with no transcript is NOT COMPARED and a BOUNDARY case is
/// BOUNDARY; with no run files the recording run is absent.
#[test]
fn corpus_reader_no_capture_not_compared() {
    let mut folder = TempCorpus::new("no-capture");
    let cp = committed_inputs("CP-001");
    let boundary = committed_inputs("CE-17");
    folder.input("CP-001", &cp);
    folder.input("CE-17", &boundary);
    let ids = ["CP-001".to_string(), "CE-17".to_string()];
    assert_eq!(
        load_corpus_run(folder.path(), &ids),
        Ok(CorpusRun::Absent),
        "inputs alone are no recording run"
    );
    let judgement = judge(
        &folder,
        &[
            entry("CP-001", CLASS_CP, &cp, None),
            entry("CE-17", CLASS_BOUNDARY, &boundary, None),
        ],
    );
    assert_eq!(judgement.run_line, "Recording run: none", "run line");
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let cp_verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(cp_verdict.verdict, NOT_COMPARED, "CP-001 verdict");
    assert_eq!(cp_verdict.detail, NO_CAPTURE, "CP-001 detail");
    assert!(!cp_verdict.recorded, "CP-001 is not recorded");
    assert_eq!(
        verdict_of(&judgement, "CE-17").verdict,
        BOUNDARY,
        "CE-17 verdict"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );
}

/// A capture equal to the Rust transcript, with valid run files, is MATCH.
#[test]
fn corpus_reader_matching_capture() {
    let mut folder = TempCorpus::new("matching");
    let inputs = committed_inputs("CP-001");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust_transcript(&inputs));
    folder.run(&[("CP-001", "RECORDED")]);
    let judgement = judge(&folder, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    assert!(
        judgement
            .run_line
            .starts_with("Recording run: image_digest="),
        "run line: {}",
        judgement.run_line
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, MATCH, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, NO_DETAIL, "MATCH detail");
    assert!(verdict.recorded, "CP-001 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (1, 1),
        "MATCHED n OF m"
    );
}

/// A capture that differs at line 2 is MISMATCH, its cause naming that
/// line.
#[test]
fn corpus_reader_mismatch_first_line() {
    let mut folder = TempCorpus::new("mismatch");
    let inputs = committed_inputs("CP-001");
    let capture = with_line(&rust_transcript(&inputs), 2, "SYNTHETIC DIFFERENT LINE");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &capture);
    folder.run(&[("CP-001", "RECORDED")]);
    let judgement = judge(&folder, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, MISMATCH, "CP-001 verdict");
    assert!(
        verdict.detail.contains("transcript differs at line 2")
            && verdict.detail.contains("SYNTHETIC DIFFERENT LINE"),
        "detail: {}",
        verdict.detail
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 1),
        "MATCHED n OF m"
    );
}

/// A case the run summary marks QUARANTINED is QUARANTINED and outside m.
#[test]
fn corpus_reader_quarantined() {
    let mut folder = TempCorpus::new("quarantined");
    let inputs = committed_inputs("CP-001");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust_transcript(&inputs));
    folder.run(&[("CP-001", "QUARANTINED")]);
    let ids = ["CP-001".to_string()];
    assert!(
        matches!(
            load_corpus_run(folder.path(), &ids),
            Ok(CorpusRun::Recorded { quarantined, .. }) if quarantined == ids
        ),
        "the run must record CP-001 as quarantined"
    );
    let judgement = judge(&folder, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    assert_eq!(
        verdict_of(&judgement, "CP-001").verdict,
        QUARANTINED,
        "CP-001 verdict"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );
}

/// Transcripts without a digest list fail the recording run, which names
/// the missing file, and leave every case NOT COMPARED.
#[test]
fn corpus_reader_missing_sha256sums() {
    let mut folder = TempCorpus::new("missing-sums");
    let inputs = committed_inputs("CP-001");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust_transcript(&inputs));
    folder.run_files(&[("CP-001", "RECORDED")]);
    let ids = ["CP-001".to_string()];
    match load_corpus_run(folder.path(), &ids) {
        Err(cause) => assert!(cause.contains(SUMS_NAME), "cause: {cause}"),
        Ok(run) => panic!("a run without its digest list loaded: {run:?}"),
    }
    let judgement = judge(&folder, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement
            .integrity
            .iter()
            .any(|cause| cause.contains(SUMS_NAME)),
        "integrity: {:?}",
        judgement.integrity
    );
    assert!(
        judgement
            .run_line
            .starts_with("Recording run: INTEGRITY FAIL:"),
        "run line: {}",
        judgement.run_line
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, NOT_COMPARED, "CP-001 verdict");
    assert_eq!(verdict.detail, RUN_INTEGRITY_FAILURE, "CP-001 detail");
}

/// A transcript changed by one byte after the digest list was written
/// fails the recording run, which names that transcript.
#[test]
fn corpus_reader_edited_transcript() {
    let mut folder = TempCorpus::new("edited");
    let inputs = committed_inputs("CP-001");
    let rust = rust_transcript(&inputs);
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust);
    folder.run(&[("CP-001", "RECORDED")]);
    let mut replaced = false;
    let edited: String = rust
        .chars()
        .map(|c| match c {
            'A'..='Z' if !replaced => {
                replaced = true;
                if c == 'X' { 'Y' } else { 'X' }
            }
            c => c,
        })
        .collect();
    assert!(
        replaced && edited.len() == rust.len(),
        "one byte must change"
    );
    folder.transcript("CP-001", &edited);
    let named = Path::new("CP-001")
        .join("transcript.txt")
        .display()
        .to_string();
    let ids = ["CP-001".to_string()];
    match load_corpus_run(folder.path(), &ids) {
        Err(cause) => assert!(cause.contains(&named), "cause: {cause}"),
        Ok(run) => panic!("an edited transcript passed its digest list: {run:?}"),
    }
    let judgement = judge(&folder, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement
            .integrity
            .iter()
            .any(|cause| cause.contains(&named)),
        "integrity: {:?}",
        judgement.integrity
    );
    assert_eq!(
        verdict_of(&judgement, "CP-001").verdict,
        NOT_COMPARED,
        "CP-001 verdict"
    );
}

/// A CR capture equal to its GC capture is WORLD-INDEPENDENT.
#[test]
fn corpus_cr_world_independent() {
    let mut folder = TempCorpus::new("world-independent");
    let (gc, inputs) = cr_case("CR-01");
    folder.input("CR-01", &inputs);
    folder.transcript("CR-01", &gc_transcript(gc));
    folder.run(&[("CR-01", "RECORDED")]);
    let judgement = judge(&folder, &[entry("CR-01", CLASS_CR, &inputs, Some(gc))]);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    match &verdict_of(&judgement, "CR-01").world {
        Some(world) => {
            assert_eq!(world.gc, gc, "GC case");
            assert_eq!(world.label, WORLD_INDEPENDENT, "detail: {}", world.detail);
            assert_eq!(world.detail, NO_DETAIL, "WORLD-INDEPENDENT detail");
        }
        None => panic!("CR-01 has no world label"),
    }
}

/// A CR capture that differs from its GC capture is WORLD-DEPENDENT, its
/// detail naming the first differing line.
#[test]
fn corpus_cr_world_dependent() {
    let mut folder = TempCorpus::new("world-dependent");
    let (gc, inputs) = cr_case("CR-01");
    let capture = with_line(&gc_transcript(gc), 2, "SYNTHETIC WORLD B LINE");
    folder.input("CR-01", &inputs);
    folder.transcript("CR-01", &capture);
    folder.run(&[("CR-01", "RECORDED")]);
    let judgement = judge(&folder, &[entry("CR-01", CLASS_CR, &inputs, Some(gc))]);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    match &verdict_of(&judgement, "CR-01").world {
        Some(world) => {
            assert_eq!(world.label, WORLD_DEPENDENT, "CR-01 label");
            assert!(
                world.detail.contains("transcript differs at line 2")
                    && world.detail.contains("SYNTHETIC WORLD B LINE"),
                "detail: {}",
                world.detail
            );
        }
        None => panic!("CR-01 has no world label"),
    }
}

/// Each CR case replays exactly the inputs of the GC case it names.
#[test]
fn cr_inputs_equal_gc_inputs() {
    for (index, (cr, gc, inputs)) in corpus::CR_INPUTS.into_iter().enumerate() {
        let number = index.saturating_add(1);
        assert_eq!(cr, format!("CR-{number:02}"), "CR case {number}");
        assert_eq!(gc, format!("GC-{number:02}"), "GC case of {cr}");
        match load_case(&default_dir(), gc) {
            CaseLoad::Recorded(case) => assert_eq!(
                case.inputs,
                inputs
                    .iter()
                    .map(|input| input.to_string())
                    .collect::<Vec<_>>(),
                "{cr} inputs differ from the inputs of {gc}"
            ),
            other => panic!("{gc} is not recorded: {other:?}"),
        }
    }
}

/// `sha256_hex` gives the FIPS 180-2 example digests, the two-block padding
/// case of 56 bytes, 15,625 full blocks, and 55 bytes, the largest message
/// that pads into one block.
#[test]
fn sha256_known_answers() {
    let million = "a".repeat(1_000_000);
    let fifty_five = "a".repeat(55);
    let vectors: [(&str, &str); 6] = [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopq\
             rlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
        (
            &million,
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        ),
        (
            &fifty_five,
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
        ),
    ];
    for (message, digest) in vectors {
        assert_eq!(
            sha256_hex(message.as_bytes()),
            digest,
            "SHA-256 of a {}-byte message",
            message.len()
        );
    }
}
