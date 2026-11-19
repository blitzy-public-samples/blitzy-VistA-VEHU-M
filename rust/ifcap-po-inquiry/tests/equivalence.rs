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
//! the corpus `load_corpus_case` and `load_corpus_run`, with
//! `corpus_transcript_exists` telling from metadata alone whether a corpus
//! transcript is present, and `read_corpus_input`, which the reader tests use
//! for the committed inputs alone. The fixtures are
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
//! an integrity check, and after those when the written `### Parity corpus`
//! block breaks its own layout (placement, parts, counts), never on a corpus
//! verdict, so a run with no captures exits 0 when the fixtures load and the
//! other harness checks pass. Exit 0 is not acceptance, which needs five
//! PASS.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ifcap_po_inquiry::corpus;
use ifcap_po_inquiry::golden::{
    CASE_IDS, CHECKS, CORPUS_META_KEYS, Case, CaseLoad, CorpusRun, Readiness, Rule,
    corpus_case_ids, corpus_dir, corpus_input_path, corpus_transcript_exists, default_dir,
    load_case, load_corpus_case, load_corpus_run, normalise, parse_corpus_meta, read_corpus_input,
    readiness, sha256_hex, split_input_lines, validate_context,
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
/// Why every non-BOUNDARY case (CP, IN and CR) is NOT COMPARED while the
/// recording run fails an integrity check.
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
    /// Whether the case folder holds a transcript capture, valid or not:
    /// the BOUNDARY map's recorded column.
    recorded: bool,
    /// The world label, for a CR case only.
    world: Option<WorldLabel>,
}

/// The verdicts of a set of corpus cases, with the recording run line and
/// the two numbers of the MATCHED line.
struct CorpusJudgement {
    /// The recording run line: `none`, the run's `meta.txt` values, or
    /// `INTEGRITY FAIL: <cause>` with the first integrity failure, the
    /// run's when the run failed, else the first case's.
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
/// once. A case that fails to load, or whose committed input differs from
/// the model's, fails its own integrity check: its cause names the file at
/// fault, and the case is NOT COMPARED unless it is BOUNDARY, while every
/// other case keeps its own verdict. So only a committed input equal to the
/// model's is run, and only for a usable capture, so with no recording none
/// runs. The run line shows the first integrity failure, of the run or
/// else of a case.
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
            Ok(case) if case.inputs == entry.inputs => Some(case),
            Ok(_) => {
                integrity.push(crate_relative(&format!(
                    "{}: committed input differs from the generated model",
                    corpus_input_path(corpus_dir, &entry.id).display()
                )));
                None
            }
            Err(cause) => {
                integrity.push(crate_relative(&cause));
                None
            }
        };
        // The committed inputs and the capture a verdict may use, or why
        // there is none: precedence rules (b), (c) and (d), in that order.
        // `case` is `None` when the case failed its own integrity check: it
        // did not load, or its committed input differs from the model's.
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
            recorded: corpus_transcript_exists(corpus_dir, &entry.id),
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
    // A failed run already shows its own cause. When the run verified, the
    // first case integrity failure replaces the run's line, so that no
    // failure leaves the line reading healthy.
    let run_line = match integrity.first() {
        Some(cause) if !run_failed => format!("{RUN_LINE}INTEGRITY FAIL: {}", one_line(cause)),
        _ => run_line,
    };
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

/// Every reason the run fails once `REPORT.md` is written, in this order:
/// each FAIL case of `results` as `<id> FAIL: <first cause>`, each failed
/// harness check of `checks` as `harness check <name> FAIL: <detail>`, then
/// each of `corpus_integrity` as `corpus integrity FAIL: <cause>`. Empty
/// when the run passes; no corpus verdict ever adds a line.
fn run_failures(
    results: &[CaseResult],
    checks: &[Check],
    corpus_integrity: &[String],
) -> Vec<String> {
    results
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
            corpus_integrity
                .iter()
                .map(|cause| format!("corpus integrity FAIL: {cause}")),
        )
        .collect()
}

/// The trimmed cells of each line of the rendered Markdown `table`, its
/// header and separator lines included, without the empty text outside
/// the outer `|`. An escaped `\|` stays inside its cell.
fn rendered_rows(table: &str) -> Vec<Vec<String>> {
    table
        .lines()
        .map(|line| {
            let cells = table_cells(line);
            let inner = cells.len().saturating_sub(2);
            cells
                .iter()
                .skip(1)
                .take(inner)
                .map(|cell| cell.trim().to_string())
                .collect()
        })
        .collect()
}

/// Cell `column` of a [`rendered_rows`] row, or empty text past its end.
fn cell_of(row: &[String], column: usize) -> String {
    row.get(column).cloned().unwrap_or_default()
}

/// Checks the `### Parity corpus` block of `corpus` as the rendered
/// `report` holds it, by rules that hold for every recording state (AAP
/// 0.4.4, 0.5.7): the block occurs once, directly after the part 3
/// definitions table and directly before the first static section, and
/// opens with its heading and the recording run line, whose `INTEGRITY
/// FAIL` agrees with the corpus failures [`run_failures`] reports. The
/// MATCHED line is the report's only line starting `MATCHED `, and no line
/// of the block begins `| PO-`. A block with no model holds only its
/// heading, an `INTEGRITY FAIL` run line and `MATCHED 0 OF 0`. A full
/// block's parts are, in order, the count table, the MATCHED line, the
/// table of every case of `golden::corpus_case_ids()` in order, then the
/// class moves, findings, dropped variants, BOUNDARY map and CR world
/// labels, each under its heading as a table or `none`. Its counts agree
/// with the per-case rows, and so do n and m of the MATCHED line. Each
/// detail fits its verdict, and with no recording run every case is
/// BOUNDARY or NOT COMPARED for want of a capture. The BOUNDARY map lists
/// the BOUNDARY cases, and the CR world labels the CR cases with the GC
/// case `corpus::CR_INPUTS` pairs each with, both in case order.
fn check_corpus_section(report: &str, corpus: &CorpusReport) {
    let block = corpus.block.as_str();
    assert_eq!(
        report.matches(block).count(),
        1,
        "the corpus block must occur exactly once in REPORT.md"
    );
    let Some(at) = report.find(block) else {
        panic!("the corpus block is not in REPORT.md");
    };
    let end = at.saturating_add(block.len());
    let part3 = report.find("\n## Part 3: Capture readiness\n");
    let definitions_end = CHECK_DEFINITIONS.last().and_then(|(name, definition)| {
        let row = format!("| `{name}` | {definition} |");
        part3.and_then(|start| {
            report
                .get(start..)
                .and_then(|rest| rest.find(&row))
                .map(|found| start.saturating_add(found).saturating_add(row.len()))
        })
    });
    let first_static = STATIC_SECTIONS
        .first()
        .and_then(|heading| report.find(&format!("\n{heading}\n")));
    match (part3, definitions_end, first_static) {
        (Some(part3), Some(definitions_end), Some(first_static)) => {
            assert!(
                part3 < definitions_end && definitions_end <= at && end < first_static,
                "the corpus block (bytes {at}..{end}) must follow part 3 (byte {part3}) and its \
                 definitions table (ending at byte {definitions_end}) and precede the first \
                 static section (byte {first_static})"
            );
            assert_eq!(
                report.get(definitions_end..at),
                Some("\n\n"),
                "the corpus block must follow the definitions table directly"
            );
            assert_eq!(
                report.get(end..first_static),
                Some("\n"),
                "the first static section must follow the corpus block directly"
            );
        }
        found => panic!(
            "part 3, its definitions table or the first static section is missing from \
             REPORT.md: {found:?}"
        ),
    }
    let opening = format!("{CORPUS_HEADING}\n\n{RUN_LINE}");
    assert!(
        block.starts_with(&opening),
        "the corpus block must open with its heading and the recording run line: {:?}",
        block.lines().take(3).collect::<Vec<_>>()
    );
    assert!(
        report.contains(&format!("\n{}\n", corpus.matched_line)),
        "REPORT.md must hold the MATCHED line at column 0: {}",
        corpus.matched_line
    );
    let matched_lines: Vec<&str> = report
        .lines()
        .filter(|line| line.starts_with("MATCHED "))
        .collect();
    assert_eq!(
        matched_lines,
        [corpus.matched_line.as_str()],
        "REPORT.md must hold exactly one line starting MATCHED"
    );
    let po_rows: Vec<&str> = block
        .lines()
        .filter(|line| line.starts_with("| PO-"))
        .collect();
    assert!(
        po_rows.is_empty(),
        "corpus block rows begin | PO-: {po_rows:?}"
    );

    let parts: Vec<&str> = block.split("\n\n").collect();
    let run_line = parts.get(1).copied().unwrap_or_default();
    let integrity_fail = format!("{RUN_LINE}INTEGRITY FAIL: ");
    assert_eq!(
        run_failures(&[], &[], &corpus.integrity).is_empty(),
        !run_line.starts_with(&integrity_fail),
        "the run line {run_line:?} must show INTEGRITY FAIL exactly when the corpus fails an \
         integrity check: {:?}",
        corpus.integrity
    );
    if let &[heading, run_line, matched] = parts.as_slice() {
        assert_eq!(heading, CORPUS_HEADING, "corpus block heading");
        assert!(
            run_line.starts_with(&integrity_fail) && !corpus.integrity.is_empty(),
            "a corpus block with no model must show its integrity failure: {run_line}"
        );
        assert_eq!(matched, matched_line(0, 0), "MATCHED line with no model");
        assert_eq!(corpus.matched_line, matched, "printed MATCHED line");
        return;
    }
    let &[
        heading,
        run_line,
        counts,
        matched,
        cases,
        moves_heading,
        moves,
        findings_heading,
        findings,
        drops_heading,
        drops,
        boundary_heading,
        boundary,
        worlds_heading,
        worlds,
    ] = parts.as_slice()
    else {
        panic!(
            "the corpus block must have 15 blank-line-separated parts, not {}: {parts:?}",
            parts.len()
        );
    };
    assert_eq!(heading, CORPUS_HEADING, "corpus block heading");
    assert!(
        run_line.starts_with(RUN_LINE) && !run_line.contains('\n'),
        "recording run line: {run_line}"
    );
    assert_eq!(matched, corpus.matched_line, "MATCHED line in the block");
    assert_eq!(
        [
            moves_heading,
            findings_heading,
            drops_heading,
            boundary_heading,
            worlds_heading
        ],
        [
            "#### Class moves",
            "#### Findings",
            "#### Dropped variants",
            "#### BOUNDARY map",
            "#### CR world labels"
        ],
        "corpus block subsection headings, in order"
    );

    assert_eq!(
        counts.lines().take(2).collect::<Vec<_>>(),
        [
            "| Class | MATCH | MISMATCH | NOT COMPARED | BOUNDARY | QUARANTINED | Total |",
            "|---|---|---|---|---|---|---|"
        ],
        "count table header"
    );
    assert_eq!(
        cases.lines().take(2).collect::<Vec<_>>(),
        [
            "| Case | Class | Verdict | First differing line |",
            "|---|---|---|---|"
        ],
        "per-case table header"
    );
    let count_table = rendered_rows(counts);
    let count_rows: Vec<&[String]> = count_table.iter().skip(2).map(Vec::as_slice).collect();
    let case_table = rendered_rows(cases);
    let case_rows: Vec<&[String]> = case_table.iter().skip(2).map(Vec::as_slice).collect();
    let ids = corpus_case_ids();
    let row_ids: Vec<String> = case_rows.iter().map(|row| cell_of(row, 0)).collect();
    assert_eq!(
        row_ids, ids,
        "per-case rows must be golden::corpus_case_ids() in order"
    );
    let classes: Vec<String> = count_rows.iter().map(|row| cell_of(row, 0)).collect();
    assert_eq!(classes, CLASS_ROWS, "count table rows");
    let mut total = 0_usize;
    for row in &count_rows {
        let class = cell_of(row, 0);
        let numbers: Vec<usize> = row
            .iter()
            .skip(1)
            .map(|cell| match cell.parse() {
                Ok(number) => number,
                Err(err) => panic!("count {cell:?} of class {class} is not a number: {err}"),
            })
            .collect();
        let Some((row_total, by_verdict)) = numbers.split_last() else {
            panic!("count row {class} is empty");
        };
        assert_eq!(
            by_verdict.len(),
            VERDICT_COLUMNS.len(),
            "count row {class} columns"
        );
        assert_eq!(
            by_verdict.iter().sum::<usize>(),
            *row_total,
            "count row {class}: verdict columns must sum to its Total"
        );
        for (column, count) in VERDICT_COLUMNS.iter().zip(by_verdict) {
            let listed = case_rows
                .iter()
                .filter(|case| cell_of(case, 1) == class && cell_of(case, 2) == *column)
                .count();
            assert_eq!(
                listed, *count,
                "count of {class} {column} must equal the per-case rows"
            );
            assert!(
                (class == CLASS_BOUNDARY) == (*column == BOUNDARY) || *count == 0,
                "{count} {class} cases are {column}: only class BOUNDARY is BOUNDARY"
            );
        }
        total = total.saturating_add(*row_total);
    }
    assert_eq!(total, ids.len(), "count table Total column");
    for case in &case_rows {
        let (id, verdict, detail) = (cell_of(case, 0), cell_of(case, 2), cell_of(case, 3));
        let allowed = match verdict.as_str() {
            MATCH | BOUNDARY | QUARANTINED => detail == NO_DETAIL,
            NOT_COMPARED => [NO_CAPTURE, RUN_INTEGRITY_FAILURE, CASE_INTEGRITY_FAILURE]
                .contains(&detail.as_str()),
            MISMATCH => {
                detail.starts_with("transcript differs at line ")
                    || detail.starts_with("rust session error: ")
            }
            _ => false,
        };
        assert!(allowed, "{id}: verdict {verdict:?} with detail {detail:?}");
        if run_line == format!("{RUN_LINE}none") {
            assert!(
                verdict == BOUNDARY || (verdict == NOT_COMPARED && detail == NO_CAPTURE),
                "{id} is {verdict} ({detail}) with no recording run"
            );
        }
    }
    let in_scope = |wanted: &[&str]| {
        case_rows
            .iter()
            .filter(|case| [CLASS_CP, CLASS_IN].contains(&cell_of(case, 1).as_str()))
            .filter(|case| wanted.contains(&cell_of(case, 2).as_str()))
            .count()
    };
    assert_eq!(
        matched,
        matched_line(in_scope(&[MATCH]), in_scope(&[MATCH, MISMATCH])),
        "MATCHED n OF m must count the CP and IN rows"
    );

    let tables: [(&str, &[&str]); 5] = [
        (moves, &["Case", "Specified class", "Final class", "Branch"]),
        (findings, &["Case", "Outcome", "Cause"]),
        (drops, &["Round", "Case", "Dimension", "Value", "Branch"]),
        (
            boundary,
            &["Case", "Input lines", "Expected branch", "Recorded"],
        ),
        (
            worlds,
            &["CR case", "GC case", "Label", "First differing line"],
        ),
    ];
    for (table, header) in tables {
        let expected_header = format!("| {} |", header.join(" | "));
        assert!(
            table == "none" || table.lines().next() == Some(expected_header.as_str()),
            "a corpus subsection must be none or a table headed {expected_header}: {table}"
        );
    }
    let boundary_ids: Vec<String> = case_rows
        .iter()
        .filter(|case| cell_of(case, 1) == CLASS_BOUNDARY)
        .map(|case| cell_of(case, 0))
        .collect();
    let mapped: Vec<String> = rendered_rows(boundary)
        .iter()
        .skip(2)
        .map(|row| {
            let recorded = cell_of(row, 3);
            assert!(
                recorded == "yes" || recorded == "no",
                "BOUNDARY map recorded column: {row:?}"
            );
            cell_of(row, 0)
        })
        .collect();
    assert_eq!(mapped, boundary_ids, "BOUNDARY map rows");
    let cr_rows: Vec<(String, String)> = case_rows
        .iter()
        .filter(|case| cell_of(case, 1) == CLASS_CR)
        .map(|case| {
            let id = cell_of(case, 0);
            let gc = corpus::CR_INPUTS
                .into_iter()
                .find(|(cr, _, _)| *cr == id)
                .map_or("", |(_, gc, _)| gc);
            (id, gc.to_string())
        })
        .collect();
    let labelled: Vec<(String, String)> = rendered_rows(worlds)
        .iter()
        .skip(2)
        .map(|row| {
            let label = cell_of(row, 2);
            assert!(
                [WORLD_INDEPENDENT, WORLD_DEPENDENT, NOT_COMPARED].contains(&label.as_str()),
                "CR world label: {row:?}"
            );
            (cell_of(row, 0), cell_of(row, 1))
        })
        .collect();
    assert_eq!(labelled, cr_rows, "CR world label rows and their GC cases");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Computes the five verdicts, prints `GC-0N <verdict>` for each, judges
/// the parity corpus, prints its `MATCHED n OF m IN-SCOPE CORPUS CASES`
/// line and writes `REPORT.md` once. A `SPEC.md` lacking a required section
/// fails the run before anything is written, and an error writing
/// `REPORT.md` fails it. The run then fails first on the [`run_failures`]
/// lines: a FAIL case, a failed harness check or a corpus integrity
/// failure, in that order, never a corpus verdict. Only once those are
/// empty is the report text written checked in memory by
/// [`check_corpus_section`]: the `### Parity corpus` block's placement in
/// part 3, its parts in order, its counts against its per-case rows, and
/// its run line against the corpus integrity failures.
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
    if let Err(err) = fs::write(crate_dir.join("REPORT.md"), &report) {
        panic!("cannot write REPORT.md: {err}");
    }

    let failures = run_failures(&results, &checks, &corpus.integrity);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    check_corpus_section(&report, &corpus);
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

    /// Writes `bytes`, which need not be UTF-8, to `relative`, creating its
    /// folder, and records it, as [`TempCorpus::write`] does for text.
    fn write_bytes(&mut self, relative: &str, bytes: &[u8]) {
        let path = self.dir.join(relative);
        let created = path.parent().map_or(Ok(()), fs::create_dir_all);
        if let Err(err) = created.and_then(|()| fs::write(&path, bytes)) {
            panic!("cannot write {}: {err}", path.display());
        }
        self.written.retain(|(listed, _)| listed != relative);
        self.written.push((relative.to_string(), sha256_hex(bytes)));
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

    /// Makes `relative` a symbolic link to `target` in `holder`, another
    /// synthetic folder (`""` naming `holder` itself), in place of any file
    /// written there. The link's own records are replaced by the digest of
    /// each file `holder` wrote at or beneath `target`, under the path the
    /// link makes it reachable by, so that [`TempCorpus::sums`] lists those
    /// bytes as verifying. Nothing is read back.
    #[cfg(unix)]
    fn link(&mut self, relative: &str, holder: &TempCorpus, target: &str) {
        let path = self.dir.join(relative);
        let to = holder.dir.join(target);
        let created = path.parent().map_or(Ok(()), fs::create_dir_all);
        let cleared = created.and_then(|()| match fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        });
        if let Err(err) = cleared.and_then(|()| std::os::unix::fs::symlink(&to, &path)) {
            panic!("cannot link {} to {}: {err}", path.display(), to.display());
        }
        self.written
            .retain(|(listed, _)| Path::new(listed).strip_prefix(relative).is_err());
        for (listed, digest) in &holder.written {
            let Ok(rest) = Path::new(listed).strip_prefix(target) else {
                continue;
            };
            let reached = if rest.as_os_str().is_empty() {
                relative.to_string()
            } else {
                format!("{relative}/{}", rest.display())
            };
            self.written.push((reached, digest.clone()));
        }
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

/// The inputs of the committed corpus case `id`: [`split_input_lines`] of
/// its `input.txt`, read through [`read_corpus_input`] alone. The committed
/// transcript and run files are never read, so the reader tests, which judge
/// synthetic folders, do not depend on any committed recording or its
/// integrity.
fn committed_inputs(id: &str) -> Vec<String> {
    match read_corpus_input(&corpus_dir(), id) {
        Ok(text) => split_input_lines(&text),
        Err(cause) => panic!("committed corpus input {id} not read: {cause}"),
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

/// A synthetic model case `id` of `class`, specified as such, with `inputs`
/// and the `expected` branch, and nothing else.
fn model_case(
    id: &str,
    class: corpus::Class,
    inputs: &[String],
    expected: Option<&str>,
) -> corpus::CaseEntry {
    corpus::CaseEntry {
        id: id.to_string(),
        class,
        specified: class,
        inputs: inputs.to_vec(),
        orders: Vec::new(),
        c_values: Vec::new(),
        item_count: None,
        tags: Vec::new(),
        expected_branch: expected.map(str::to_string),
        finding: None,
    }
}

/// Asserts that the blank-line-separated parts of the corpus `block` are
/// exactly `expected`, in order, each named by its label.
fn assert_block_parts(block: &str, expected: &[(&str, String)]) {
    let parts: Vec<&str> = block.split("\n\n").collect();
    assert_eq!(parts.len(), expected.len(), "corpus block parts: {parts:?}");
    for (part, (label, wanted)) in parts.iter().zip(expected) {
        assert_eq!(*part, wanted.as_str(), "corpus block {label}");
    }
}

/// The 1-based line `number` of `text`.
fn line_at(text: &str, number: usize) -> &str {
    match text.split('\n').nth(number.saturating_sub(1)) {
        Some(line) => line,
        None => panic!("the text has no line {number}"),
    }
}

/// The 1-based number and the text of the first line of `text` that
/// contains `fragment`.
fn line_containing<'a>(text: &'a str, fragment: &str) -> (usize, &'a str) {
    match text
        .split('\n')
        .enumerate()
        .find(|(_, line)| line.contains(fragment))
    {
        Some((index, line)) => (index.saturating_add(1), line),
        None => panic!("no line contains {fragment:?}"),
    }
}

/// `text`, which must end with `\n`, once without that `\n` and once with
/// one more: two captures that differ from it only in the final line end.
fn final_newline_variants(text: &str) -> (String, String) {
    match text.strip_suffix('\n') {
        Some(removed) => (removed.to_string(), format!("{text}\n")),
        None => panic!("the transcript must end with \\n"),
    }
}

/// `case` judged from a fresh synthetic folder `name` that records it with
/// the model's input, `capture` as its transcript and valid run files.
fn judge_recorded(name: &str, case: CorpusEntry, capture: &str) -> CorpusJudgement {
    let mut folder = TempCorpus::new(name);
    folder.input(&case.id, &case.inputs);
    folder.transcript(&case.id, capture);
    folder.run(&[(case.id.as_str(), "RECORDED")]);
    judge(&folder, &[case])
}

/// The world label of a CR case's verdict.
fn world_of(verdict: &CorpusVerdict) -> &WorldLabel {
    match &verdict.world {
        Some(world) => world,
        None => panic!("{} has no world label", verdict.id),
    }
}

/// A capture with no transcript is NOT COMPARED and a BOUNDARY case is
/// BOUNDARY; with no run files the recording run is absent, as it is for a
/// corpus folder that does not exist, an empty one and one holding only
/// `BASE.txt`, while a missing case folder or `input.txt` is named. Each
/// content rule of `input.txt` and `transcript.txt` (empty, `\r`, not
/// UTF-8, over 1 MiB, last input not caret-prefixed) rejects its own row
/// with the file's path and cause, each budget admits exactly 1 MiB, a
/// `^FOO` exit is accepted, and a transcript is present whether or not it
/// is valid. A failing input makes its case NOT COMPARED for its own
/// integrity failure, a failing transcript for the recording run's, and a
/// BOUNDARY case stays BOUNDARY either way, recorded exactly when its
/// transcript exists. Every reader refuses an id outside the series by
/// name, whatever stands at the path it would lead to. A CR case with its
/// input alone is NOT COMPARED, and so is its world label.
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

    // Absence, each asserted before the next file is written: a corpus
    // folder that does not exist, a fresh empty one, and one holding only
    // the `BASE.txt` that `load_corpus.sh` writes before any recording are
    // each no recording run. `load_corpus_case` names a missing case
    // folder, and the missing `input.txt` of a case folder that holds only
    // a transcript.
    let mut root_folder = TempCorpus::new("no-capture-root");
    let case_dir = root_folder.path().join("CP-002");
    assert_eq!(
        load_corpus_run(&root_folder.path().join("never-created"), &ids),
        Ok(CorpusRun::Absent),
        "a corpus folder that does not exist is no recording run"
    );
    assert_eq!(
        load_corpus_run(root_folder.path(), &ids),
        Ok(CorpusRun::Absent),
        "an empty corpus folder is no recording run"
    );
    assert_eq!(
        load_corpus_case(root_folder.path(), "CP-002"),
        Err(format!("{}: missing", case_dir.display())),
        "a missing case folder is named"
    );
    root_folder.write(
        "BASE.txt",
        "synthetic=written by tests/equivalence.rs, not a snapshot\n\
         snapshot_image=synthetic\nsnapshot_id=synthetic\n",
    );
    assert_eq!(
        load_corpus_run(root_folder.path(), &ids),
        Ok(CorpusRun::Absent),
        "BASE.txt alone is no recording run"
    );
    root_folder.transcript("CP-002", "SYNTHETIC CAPTURE\n");
    assert_eq!(
        load_corpus_case(root_folder.path(), "CP-002"),
        Err(format!("{}: missing", case_dir.join("input.txt").display())),
        "a case folder without input.txt is named"
    );

    // `input.txt` content rows, each in its own case folder so that no row
    // masks another, read through `load_corpus_case`: each rule rejects its
    // row with the file's path and cause, the 1 MiB budget admits exactly
    // 1_048_576 bytes, and a caret-prefixed exit other than `^` is accepted.
    // The BOUNDARY CE-17's input fails too. With no transcript there is no
    // recording run, a failing case is NOT COMPARED for its own integrity
    // failure, an accepted one for want of a capture, and CE-17 stays
    // BOUNDARY, not recorded.
    let mut input_folder = TempCorpus::new("no-capture-input");
    let long = "A".repeat(1_048_572);
    let input_at_budget = format!("\n{long}\n^\n");
    let input_over_budget = format!("\n{long}A\n^\n");
    assert_eq!(
        (input_at_budget.len(), input_over_budget.len()),
        (1_048_576, 1_048_577),
        "input budget row sizes"
    );
    let boundary_cr: String = boundary
        .iter()
        .map(|input| format!("{input}\r\n"))
        .collect();
    let input_rows = [
        ("CP-011", CLASS_CP, Vec::new(), Err("empty")),
        (
            "CP-012",
            CLASS_CP,
            b"\nCP001\r\n\n\n^\n".to_vec(),
            Err("carriage return"),
        ),
        (
            "CP-013",
            CLASS_CP,
            b"\nCP\xff01\n\n\n^\n".to_vec(),
            Err("not UTF-8"),
        ),
        (
            "CP-014",
            CLASS_CP,
            input_over_budget.into_bytes(),
            Err("larger than 1048576 bytes"),
        ),
        (
            "CP-015",
            CLASS_CP,
            b"\nCP001\n".to_vec(),
            Err("last input does not begin with ^"),
        ),
        (
            "CP-016",
            CLASS_CP,
            b"\n^FOO\n".to_vec(),
            Ok(vec![String::new(), "^FOO".to_string()]),
        ),
        (
            "CP-017",
            CLASS_CP,
            input_at_budget.into_bytes(),
            Ok(vec![String::new(), long, "^".to_string()]),
        ),
        (
            "CE-17",
            CLASS_BOUNDARY,
            boundary_cr.into_bytes(),
            Err("carriage return"),
        ),
    ];
    let mut input_entries = Vec::new();
    let mut input_verdicts = Vec::new();
    let mut input_failures = Vec::new();
    for (id, class, bytes, outcome) in input_rows {
        input_folder.write_bytes(&format!("{id}/input.txt"), &bytes);
        let path = input_folder.path().join(id).join("input.txt");
        let wanted = outcome
            .map(|lines| (id.to_string(), lines, None))
            .map_err(|cause| format!("{}: {cause}", path.display()));
        let loaded = load_corpus_case(input_folder.path(), id)
            .map(|case| (case.id, case.inputs, case.transcript));
        assert_eq!(loaded, wanted, "{id} input row");
        let (verdict, detail) = match &wanted {
            _ if class == CLASS_BOUNDARY => (BOUNDARY, NO_DETAIL),
            Ok(_) => (NOT_COMPARED, NO_CAPTURE),
            Err(_) => (NOT_COMPARED, CASE_INTEGRITY_FAILURE),
        };
        let model = match wanted {
            Ok((_, lines, _)) => lines,
            Err(cause) => {
                input_failures.push(crate_relative(&cause));
                if class == CLASS_BOUNDARY {
                    boundary.clone()
                } else {
                    cp.clone()
                }
            }
        };
        input_entries.push(entry(id, class, &model, None));
        input_verdicts.push((id, verdict, detail));
    }
    let input_ids: Vec<String> = input_entries.iter().map(|row| row.id.clone()).collect();
    assert_eq!(
        load_corpus_run(input_folder.path(), &input_ids),
        Ok(CorpusRun::Absent),
        "inputs alone, valid or not, are no recording run"
    );
    let judgement = judge(&input_folder, &input_entries);
    assert_eq!(
        judgement.integrity, input_failures,
        "each failing input is a case integrity failure"
    );
    assert_eq!(
        input_failures
            .first()
            .map(|cause| format!("{RUN_LINE}INTEGRITY FAIL: {}", one_line(cause))),
        Some(judgement.run_line.clone()),
        "the run line names the first failing input"
    );
    for (id, verdict, detail) in input_verdicts {
        let judged = verdict_of(&judgement, id);
        assert_eq!(
            (judged.verdict, judged.detail.as_str(), judged.recorded),
            (verdict, detail, false),
            "{id} verdict, detail and recorded"
        );
    }
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m with failing inputs"
    );

    // `transcript.txt` content rows next to a valid input, each in its own
    // case folder: each rule rejects its row with the file's path and cause,
    // while the transcript counts as present whether or not it is valid. A
    // transcript of exactly 1_048_576 bytes passes every content rule and
    // fails only for want of the digest list a present transcript needs.
    // Present transcripts need the run files, which are absent, so the
    // recording run fails, every CP case is NOT COMPARED for it, and the
    // BOUNDARY CE-17, whose transcript is empty, stays BOUNDARY, recorded.
    let mut transcript_folder = TempCorpus::new("no-capture-transcript");
    let transcript_root = transcript_folder.path().to_path_buf();
    let rejected = |id: &str, cause: &str| {
        format!(
            "{}: {cause}",
            transcript_root.join(id).join("transcript.txt").display()
        )
    };
    let transcript_at_budget = "A".repeat(1_048_576);
    let transcript_rows = [
        ("CP-021", CLASS_CP, Vec::new(), rejected("CP-021", "empty")),
        (
            "CP-022",
            CLASS_CP,
            b"SYNTHETIC\r\nCAPTURE\n".to_vec(),
            rejected("CP-022", "carriage return"),
        ),
        (
            "CP-023",
            CLASS_CP,
            b"SYNTHETIC \xff CAPTURE\n".to_vec(),
            rejected("CP-023", "not UTF-8"),
        ),
        (
            "CP-024",
            CLASS_CP,
            format!("{transcript_at_budget}A").into_bytes(),
            rejected("CP-024", "larger than 1048576 bytes"),
        ),
        (
            "CP-025",
            CLASS_CP,
            transcript_at_budget.into_bytes(),
            format!("{}: missing", transcript_root.join(SUMS_NAME).display()),
        ),
        (
            "CE-17",
            CLASS_BOUNDARY,
            Vec::new(),
            rejected("CE-17", "empty"),
        ),
    ];
    let run_cause = format!("{}: missing", transcript_root.join("meta.txt").display());
    let mut transcript_entries = Vec::new();
    let mut transcript_failures = vec![crate_relative(&run_cause)];
    for (id, class, bytes, cause) in transcript_rows {
        let model = if class == CLASS_BOUNDARY {
            &boundary
        } else {
            &cp
        };
        transcript_folder.input(id, model);
        transcript_folder.write_bytes(&format!("{id}/transcript.txt"), &bytes);
        assert_eq!(
            load_corpus_case(&transcript_root, id),
            Err(cause.clone()),
            "{id} transcript row"
        );
        assert!(
            corpus_transcript_exists(&transcript_root, id),
            "{id}'s transcript is present whether or not it is valid"
        );
        transcript_failures.push(crate_relative(&cause));
        transcript_entries.push(entry(id, class, model, None));
    }
    let transcript_ids: Vec<String> = transcript_entries
        .iter()
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(
        load_corpus_run(&transcript_root, &transcript_ids),
        Err(run_cause.clone()),
        "present transcripts need the run files"
    );
    let judgement = judge(&transcript_folder, &transcript_entries);
    assert_eq!(
        judgement.integrity, transcript_failures,
        "the run's failure, then each case's"
    );
    assert_eq!(
        judgement.run_line,
        format!(
            "{RUN_LINE}INTEGRITY FAIL: {}",
            one_line(&crate_relative(&run_cause))
        ),
        "the run line names the run's failure"
    );
    for row in &transcript_entries {
        let judged = verdict_of(&judgement, &row.id);
        let (verdict, detail) = if row.class == CLASS_BOUNDARY {
            (BOUNDARY, NO_DETAIL)
        } else {
            (NOT_COMPARED, RUN_INTEGRITY_FAILURE)
        };
        assert_eq!(
            (judged.verdict, judged.detail.as_str(), judged.recorded),
            (verdict, detail, true),
            "{} verdict, detail and recorded",
            row.id
        );
    }
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m with failing transcripts"
    );

    // Ids outside the series, each refused by name by every reader before
    // any path is built, although a valid input and a transcript stand
    // wherever such an id would lead inside the folder: in its own case
    // folder, and at the folder's root, where `""` and `CP-001/..` lead.
    // The known CP-001's transcript is found, so each `false` below is the
    // guard's.
    let mut id_folder = TempCorpus::new("no-capture-ids");
    for id in [
        "CP-000", "CP-201", "CP-1", "CE-27", "CR-06", "GC-01", "CP-001",
    ] {
        id_folder.input(id, &cp);
        id_folder.transcript(id, "SYNTHETIC CAPTURE\n");
    }
    let cp_text: String = cp.iter().map(|input| format!("{input}\n")).collect();
    id_folder.write("input.txt", &cp_text);
    id_folder.write("transcript.txt", "SYNTHETIC CAPTURE\n");
    assert!(
        corpus_transcript_exists(id_folder.path(), "CP-001"),
        "a known id's transcript is found"
    );
    for id in [
        "CP-000",
        "CP-201",
        "CP-1",
        "CE-27",
        "CR-06",
        "GC-01",
        "../CP-001",
        "CP-001/..",
        "",
    ] {
        let refused = format!("{id}: not a corpus case id");
        assert_eq!(
            load_corpus_case(id_folder.path(), id),
            Err(refused.clone()),
            "load_corpus_case of {id:?}"
        );
        assert_eq!(
            load_corpus_run(id_folder.path(), &[id.to_string()]),
            Err(refused),
            "load_corpus_run of {id:?}"
        );
        assert!(
            !corpus_transcript_exists(id_folder.path(), id),
            "corpus_transcript_exists of {id:?}"
        );
    }

    // A CR case with its input alone: the case and its world label are
    // NOT COMPARED for want of a capture.
    let mut replay_only = TempCorpus::new("no-capture-cr");
    let (gc, replay) = cr_case("CR-01");
    replay_only.input("CR-01", &replay);
    let judgement = judge(&replay_only, &[entry("CR-01", CLASS_CR, &replay, Some(gc))]);
    assert_eq!(judgement.run_line, "Recording run: none", "CR-01 run line");
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, NOT_COMPARED, "CR-01 verdict");
    assert_eq!(verdict.detail, NO_CAPTURE, "CR-01 detail");
    assert!(!verdict.recorded, "CR-01 is not recorded");
    let world = world_of(verdict);
    assert_eq!(world.gc, gc, "CR-01 GC case");
    assert_eq!(world.label, NOT_COMPARED, "CR-01 label");
    assert_eq!(world.detail, NO_CAPTURE, "CR-01 label detail");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );
}

/// A capture equal to the Rust transcript, with valid run files, is MATCH;
/// one recorded with an input other than the model's is NOT COMPARED.
/// MATCHED n OF m counts the CP and IN cases only, never a matching CR case
/// or a captured BOUNDARY one, and one case's integrity failure leaves
/// another case's MATCH standing. The run's `meta.txt` reads back as the
/// literal keys and values written; `parse_corpus_meta` rejects a line
/// without `=` and an unknown, repeated or missing key with its exact cause;
/// and a rewritten `meta.txt`, its digest refreshed, fails the recording run
/// on the parser alone.
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

    // The run's metadata, held to literals rather than to the key array or
    // to the values the synthetic writer shares with it.
    let keys = [
        "image_digest",
        "snapshot_id",
        "source_revision",
        "context",
        "start_command",
    ];
    assert_eq!(CORPUS_META_KEYS, keys, "the meta.txt keys, in order");
    let run_line = concat!(
        r#"Recording run: image_digest=`"synthetic"`; snapshot_id=`"synthetic"`; "#,
        r#"source_revision=`"synthetic"`; "#,
        r##"context=`"{\"DUZ\":\".5\",\"IOF\":\"#\",\"IOSL\":\"60\",\"STATION\":\"500\"}"`; "##,
        r#"start_command=`"docker exec -i vehu su - vehu -c \"<start command>\""`; "#,
        "export_sha256=754f3093c3dd04291d93795b6b30ce4eecd15bee326ce99582c4aeb3e2f6491e; ",
        "exports=IDENTICAL runs=IDENTICAL",
    );
    assert_eq!(judgement.run_line, run_line, "run line");
    let ids = ["CP-001".to_string()];
    match load_corpus_run(folder.path(), &ids) {
        Ok(CorpusRun::Recorded {
            meta,
            quarantined,
            export_sha256,
        }) => {
            for (key, value) in [
                ("image_digest", "synthetic"),
                ("snapshot_id", "synthetic"),
                ("source_revision", "synthetic"),
                (
                    "context",
                    r##"{"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}"##,
                ),
                (
                    "start_command",
                    r#"docker exec -i vehu su - vehu -c "<start command>""#,
                ),
            ] {
                assert_eq!(meta.get(key), Some(value), "meta.txt {key}");
            }
            assert_eq!(meta.get("no_such_key"), None, "a key outside meta.txt");
            assert!(quarantined.is_empty(), "quarantined: {quarantined:?}");
            assert_eq!(
                export_sha256, "754f3093c3dd04291d93795b6b30ce4eecd15bee326ce99582c4aeb3e2f6491e",
                "SHA-256 of the synthetic export"
            );
        }
        other => panic!("the valid run did not load as recorded: {other:?}"),
    }

    // CP-001 and the class IN case CE-01 each MATCH, beside a captured
    // BOUNDARY case and a CR case whose capture equals its GC capture and
    // its Rust transcript: n and m count the CP and IN cases only.
    let mut mixed = TempCorpus::new("matching-mixed");
    let edge = committed_inputs("CE-01");
    let boundary = committed_inputs("CE-17");
    let (gc, replay) = cr_case("CR-01");
    mixed.input("CP-001", &inputs);
    mixed.transcript("CP-001", &rust_transcript(&inputs));
    mixed.input("CE-01", &edge);
    mixed.transcript("CE-01", &rust_transcript(&edge));
    mixed.input("CE-17", &boundary);
    mixed.transcript("CE-17", "SYNTHETIC BOUNDARY CAPTURE\n");
    mixed.input("CR-01", &replay);
    mixed.transcript("CR-01", &gc_transcript(gc));
    mixed.run(&[
        ("CP-001", "RECORDED"),
        ("CE-01", "RECORDED"),
        ("CE-17", "RECORDED"),
        ("CR-01", "RECORDED"),
    ]);
    let judgement = judge(
        &mixed,
        &[
            entry("CP-001", CLASS_CP, &inputs, None),
            entry("CE-01", CLASS_IN, &edge, None),
            entry("CE-17", CLASS_BOUNDARY, &boundary, None),
            entry("CR-01", CLASS_CR, &replay, Some(gc)),
        ],
    );
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    for (id, wanted) in [
        ("CP-001", MATCH),
        ("CE-01", MATCH),
        ("CE-17", BOUNDARY),
        ("CR-01", MATCH),
    ] {
        let verdict = verdict_of(&judgement, id);
        assert_eq!(verdict.verdict, wanted, "{id} detail: {}", verdict.detail);
        assert_eq!(verdict.detail, NO_DETAIL, "{id} detail");
        assert!(verdict.recorded, "{id} is recorded");
    }
    assert_eq!(
        world_of(verdict_of(&judgement, "CR-01")).label,
        WORLD_INDEPENDENT,
        "CR-01 label"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (2, 2),
        "MATCHED n OF m counts CP-001 and CE-01 only"
    );

    // CP-001 recorded with CP-002's input, its capture equal to the Rust
    // transcript of that input and every digest refreshed: the case fails
    // its own integrity check, so it is never compared and stays outside m,
    // and the run line names its committed input.
    let mut changed = TempCorpus::new("matching-changed-input");
    let other = committed_inputs("CP-002");
    assert_ne!(other, inputs, "CP-002's input must differ from CP-001's");
    changed.input("CP-001", &other);
    changed.transcript("CP-001", &rust_transcript(&other));
    changed.run(&[("CP-001", "RECORDED")]);
    let named = crate_relative(
        &corpus_input_path(changed.path(), "CP-001")
            .display()
            .to_string(),
    );
    let cause = format!("{named}: committed input differs from the generated model");
    let judgement = judge(&changed, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert!(
        judgement.integrity.contains(&cause),
        "integrity: {:?}",
        judgement.integrity
    );
    assert_eq!(
        judgement.run_line,
        format!("Recording run: INTEGRITY FAIL: {}", one_line(&cause)),
        "run line"
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, NOT_COMPARED, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, CASE_INTEGRITY_FAILURE, "CP-001 detail");
    assert!(verdict.recorded, "CP-001 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // `parse_corpus_meta` alone, each row the text and either the five
    // values in `keys` order or the exact cause. Lines are numbered from 1,
    // blank and whitespace-only lines counted but ignored, keys are taken
    // as written, values are kept exactly after the first `=`, and a
    // missing key names the first one missing in key order.
    let rows: &[(&str, Result<[&str; 5], &str>)] = &[
        (
            "image_digest=\nsnapshot_id= spaced \n\n \t\nsource_revision=a=b=c\n\
             context={\"DUZ\":\".5\"}\nstart_command=docker exec -i vehu su - vehu -c \"a=b c\"\n",
            Ok([
                "",
                " spaced ",
                "a=b=c",
                "{\"DUZ\":\".5\"}",
                "docker exec -i vehu su - vehu -c \"a=b c\"",
            ]),
        ),
        (
            "start_command=s\ncontext=c\nsource_revision=r\nsnapshot_id=i\nimage_digest=d",
            Ok(["d", "i", "r", "c", "s"]),
        ),
        ("image_digest=d\nsnapshot_id\n", Err("meta line 2: no '='")),
        (
            "\n \t\nimage_digest=d\nno equals sign\n",
            Err("meta line 4: no '='"),
        ),
        (
            "Image_digest=d\n",
            Err("meta line 1: unknown key Image_digest"),
        ),
        (
            " image_digest=d\n",
            Err("meta line 1: unknown key  image_digest"),
        ),
        ("=d\n", Err("meta line 1: unknown key ")),
        (
            "image_digest=d\nsnapshot_id=i\nsource_revision=r\ncontext=c\n\
             start_command=s\nextra=x\n",
            Err("meta line 6: unknown key extra"),
        ),
        (
            "image_digest=d\n\nimage_digest=d\n",
            Err("meta line 3: repeated key image_digest"),
        ),
        (
            "image_digest=d\nsnapshot_id=i\nsource_revision=r\ncontext=c\n",
            Err("meta: missing key start_command"),
        ),
        (
            "image_digest=d\nsource_revision=r\nstart_command=s\n",
            Err("meta: missing key snapshot_id"),
        ),
        ("", Err("meta: missing key image_digest")),
    ];
    for &(text, expected) in rows {
        let parsed =
            parse_corpus_meta(text).map(|meta| keys.map(|key| meta.get(key).map(str::to_owned)));
        let wanted = expected
            .map(|values| values.map(|value| Some(value.to_owned())))
            .map_err(str::to_owned);
        assert_eq!(parsed, wanted, "parse_corpus_meta({text:?})");
    }

    // The parser wired into the run reader: a valid run whose `meta.txt` is
    // then rewritten with its digest refreshed, so that the parser and not
    // a digest rejects it. The run fails, naming `meta.txt` and the cause,
    // and CP-001 is NOT COMPARED.
    let mut rewritten = TempCorpus::new("matching-meta");
    rewritten.input("CP-001", &inputs);
    rewritten.transcript("CP-001", &rust_transcript(&inputs));
    rewritten.run(&[("CP-001", "RECORDED")]);
    assert!(
        matches!(
            load_corpus_run(rewritten.path(), &ids),
            Ok(CorpusRun::Recorded { .. })
        ),
        "the run must verify before meta.txt is rewritten"
    );
    let four_keys = concat!(
        "image_digest=synthetic\n",
        "snapshot_id=synthetic\n",
        "source_revision=synthetic\n",
        r##"context={"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}"##,
        "\n",
    );
    let start = "start_command=docker exec -i vehu su - vehu -c \"<start command>\"\n";
    for (text, reason) in [
        (four_keys.to_string(), "meta: missing key start_command"),
        (
            format!("{four_keys}{start}extra=synthetic\n"),
            "meta line 6: unknown key extra",
        ),
    ] {
        rewritten.write("meta.txt", &text);
        rewritten.sums();
        let cause = format!("{}: {reason}", rewritten.path().join("meta.txt").display());
        assert_eq!(
            load_corpus_run(rewritten.path(), &ids),
            Err(cause.clone()),
            "run reader: {reason}"
        );
        let judged = crate_relative(&cause);
        let judgement = judge(&rewritten, &[entry("CP-001", CLASS_CP, &inputs, None)]);
        assert_eq!(
            judgement.integrity,
            vec![judged.clone()],
            "integrity: {reason}"
        );
        assert_eq!(
            judgement.run_line,
            format!("Recording run: INTEGRITY FAIL: {}", one_line(&judged)),
            "run line: {reason}"
        );
        let verdict = verdict_of(&judgement, "CP-001");
        assert_eq!(verdict.verdict, NOT_COMPARED, "detail: {}", verdict.detail);
        assert_eq!(verdict.detail, RUN_INTEGRITY_FAILURE, "CP-001 detail");
        assert_eq!(
            (judgement.matched, judgement.in_scope),
            (0, 0),
            "MATCHED n OF m: {reason}"
        );
    }

    // One verified run: CP-001 healthy, and CP-002 recorded with CP-003's
    // input and that input's Rust transcript. Only CP-002 fails, its
    // failure the only one and the run line's, and CP-001 stays MATCH.
    let mut partial = TempCorpus::new("matching-case-failure");
    let second = committed_inputs("CP-002");
    let third = committed_inputs("CP-003");
    assert_ne!(third, second, "CP-003's input must differ from CP-002's");
    partial.input("CP-001", &inputs);
    partial.transcript("CP-001", &rust_transcript(&inputs));
    partial.input("CP-002", &third);
    partial.transcript("CP-002", &rust_transcript(&third));
    partial.run(&[("CP-001", "RECORDED"), ("CP-002", "RECORDED")]);
    let named = crate_relative(
        &corpus_input_path(partial.path(), "CP-002")
            .display()
            .to_string(),
    );
    let cause = format!("{named}: committed input differs from the generated model");
    let judgement = judge(
        &partial,
        &[
            entry("CP-001", CLASS_CP, &inputs, None),
            entry("CP-002", CLASS_CP, &second, None),
        ],
    );
    assert_eq!(judgement.integrity, [cause.as_str()], "integrity");
    assert_eq!(
        judgement.run_line,
        format!("Recording run: INTEGRITY FAIL: {}", one_line(&cause)),
        "run line"
    );
    let healthy = verdict_of(&judgement, "CP-001");
    assert_eq!(healthy.verdict, MATCH, "CP-001 detail: {}", healthy.detail);
    assert_eq!(healthy.detail, NO_DETAIL, "CP-001 detail");
    let failed = verdict_of(&judgement, "CP-002");
    assert_eq!(
        failed.verdict, NOT_COMPARED,
        "CP-002 detail: {}",
        failed.detail
    );
    assert_eq!(failed.detail, CASE_INTEGRITY_FAILURE, "CP-002 detail");
    assert!(failed.recorded, "CP-002 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (1, 1),
        "MATCHED n OF m"
    );
}

/// A capture that differs at line 2 is MISMATCH, its cause naming that
/// line with the capture as expected and the Rust transcript as actual.
/// So is a changed line keeping a `SPEC.md` literal, attributed to its
/// writer, an original `%YDB-E-` error line, a final `\n` removed or
/// appended, and a failing Rust session. A class IN MISMATCH counts in m,
/// and a CR one does not.
#[test]
fn corpus_reader_mismatch_first_line() {
    let mut folder = TempCorpus::new("mismatch");
    let inputs = committed_inputs("CP-001");
    let rust = rust_transcript(&inputs);
    let capture = with_line(&rust, 2, "SYNTHETIC DIFFERENT LINE");
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
    assert_eq!(
        verdict.detail,
        format!(
            "transcript differs at line 2: expected {}, actual {}, not attributed",
            quote("SYNTHETIC DIFFERENT LINE"),
            quote(line_at(&rust, 2))
        ),
        "CP-001 detail: the capture is expected, the Rust transcript actual"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 1),
        "MATCHED n OF m"
    );

    // Each capture below is CP-001's Rust transcript changed once, recorded
    // with every digest fresh. A changed line keeping the literal of
    // `ASK1^PRCHDP2` is attributed to it, although the shorter
    // `ENTER '^' TO HALT: ` of `^PRCHDP2` is in it too. An original runtime
    // error is a difference like any other, and so is the final line end.
    let literal = "END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ";
    let (literal_number, literal_line) = line_containing(&rust, literal);
    let changed_literal = format!("{literal_line} SYNTHETIC");
    let error = "%YDB-E-UNDEF, Undefined local variable: SYNTHETIC";
    let (error_number, error_line) = line_containing(&rust, "PC ORDER: ");
    let (removed, appended) = final_newline_variants(&rust);
    let lines = rust.split('\n').count();
    let subcases = [
        (
            "mismatch-literal",
            with_line(&rust, literal_number, &changed_literal),
            format!(
                "transcript differs at line {literal_number}: expected {}, actual {}, \
                 attributed to ASK1^PRCHDP2",
                quote(&changed_literal),
                quote(literal_line)
            ),
        ),
        (
            "mismatch-original-error",
            with_line(&rust, error_number, error),
            format!(
                "transcript differs at line {error_number}: expected {}, actual {}, \
                 not attributed",
                quote(error),
                quote(error_line)
            ),
        ),
        (
            "mismatch-no-final-newline",
            removed,
            format!(
                "transcript differs at line {lines}: expected {ABSENT_LINE}, actual {}, \
                 not attributed",
                quote("")
            ),
        ),
        (
            "mismatch-extra-final-newline",
            appended,
            format!(
                "transcript differs at line {}: expected {}, actual {ABSENT_LINE}, \
                 not attributed",
                lines.saturating_add(1),
                quote("")
            ),
        ),
    ];
    for (name, capture, detail) in subcases {
        let judgement = judge_recorded(name, entry("CP-001", CLASS_CP, &inputs, None), &capture);
        assert!(
            judgement.integrity.is_empty(),
            "{name} integrity: {:?}",
            judgement.integrity
        );
        let verdict = verdict_of(&judgement, "CP-001");
        assert_eq!(verdict.verdict, MISMATCH, "{name} verdict");
        assert_eq!(verdict.detail, detail, "{name} detail");
        assert_eq!(
            (judgement.matched, judgement.in_scope),
            (0, 1),
            "{name} MATCHED n OF m"
        );
    }

    // A valid capture judged on a store whose only order has a zero node
    // holding only a descendant: the Rust session fails, which is MISMATCH
    // with its error, inside m.
    let corrupt = match Store::from_zwr(
        "^PRC(442,0)=\"PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^1^1\"\n\
         ^PRC(442,1,0,1)=\"x\"\n\
         ^PRC(442,\"C\",\"CP001\",1)=\"\"\n",
    ) {
        Ok(store) => store,
        Err(err) => panic!("the corrupt store did not parse: {err}"),
    };
    let mut failing = TempCorpus::new("mismatch-rust-error");
    failing.input("CP-001", &inputs);
    failing.transcript("CP-001", &rust);
    failing.run(&[("CP-001", "RECORDED")]);
    let judgement = judge_corpus(
        failing.path(),
        &default_dir(),
        &corrupt,
        &[entry("CP-001", CLASS_CP, &inputs, None)],
        &literal_rows(SPEC),
    );
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(
        verdict.verdict, MISMATCH,
        "CP-001 verdict on the corrupt store"
    );
    assert_eq!(
        verdict.detail, "rust session error: CORRUPT STORE ^PRC(442,1,0)",
        "CP-001 detail on the corrupt store"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 1),
        "MATCHED n OF m on the corrupt store"
    );

    // CP-001 MATCH beside the class IN case CE-01 and the CR case CR-01,
    // each captured with line 2 changed: the IN MISMATCH counts in m, and
    // the CR one does not.
    let mut mixed = TempCorpus::new("mismatch-mixed");
    let edge = committed_inputs("CE-01");
    let edge_rust = rust_transcript(&edge);
    let (gc, replay) = cr_case("CR-01");
    let replay_rust = rust_transcript(&replay);
    mixed.input("CP-001", &inputs);
    mixed.transcript("CP-001", &rust);
    mixed.input("CE-01", &edge);
    mixed.transcript("CE-01", &with_line(&edge_rust, 2, "SYNTHETIC IN LINE"));
    mixed.input("CR-01", &replay);
    mixed.transcript("CR-01", &with_line(&replay_rust, 2, "SYNTHETIC CR LINE"));
    mixed.run(&[
        ("CP-001", "RECORDED"),
        ("CE-01", "RECORDED"),
        ("CR-01", "RECORDED"),
    ]);
    let judgement = judge(
        &mixed,
        &[
            entry("CP-001", CLASS_CP, &inputs, None),
            entry("CE-01", CLASS_IN, &edge, None),
            entry("CR-01", CLASS_CR, &replay, Some(gc)),
        ],
    );
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, MATCH, "CP-001 detail: {}", verdict.detail);
    for (id, changed, transcript) in [
        ("CE-01", "SYNTHETIC IN LINE", &edge_rust),
        ("CR-01", "SYNTHETIC CR LINE", &replay_rust),
    ] {
        let verdict = verdict_of(&judgement, id);
        assert_eq!(verdict.verdict, MISMATCH, "{id} verdict");
        assert_eq!(
            verdict.detail,
            format!(
                "transcript differs at line 2: expected {}, actual {}, not attributed",
                quote(changed),
                quote(line_at(transcript, 2))
            ),
            "{id} detail"
        );
    }
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (1, 2),
        "MATCHED n OF m counts CP-001 and CE-01 only"
    );
}

/// A case the run summary marks QUARANTINED is QUARANTINED and outside m.
/// A failed case or run integrity check and a missing transcript each win
/// over the quarantine, a BOUNDARY case stays BOUNDARY with or without one,
/// and a quarantined CR case keeps its world label.
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

    // A quarantined CP-001 recorded with CP-002's input, its capture that
    // input's Rust transcript and every digest fresh: the run verifies and
    // records the quarantine, but the case's own integrity failure wins.
    let mut changed = TempCorpus::new("quarantined-changed-input");
    let other = committed_inputs("CP-002");
    assert_ne!(other, inputs, "CP-002's input must differ from CP-001's");
    changed.input("CP-001", &other);
    changed.transcript("CP-001", &rust_transcript(&other));
    changed.run(&[("CP-001", "QUARANTINED")]);
    assert!(
        matches!(
            load_corpus_run(changed.path(), &ids),
            Ok(CorpusRun::Recorded { quarantined, .. }) if quarantined == ids
        ),
        "the run must record CP-001 as quarantined"
    );
    let named = crate_relative(
        &corpus_input_path(changed.path(), "CP-001")
            .display()
            .to_string(),
    );
    let cause = format!("{named}: committed input differs from the generated model");
    let judgement = judge(&changed, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert_eq!(judgement.integrity, [cause.as_str()], "integrity");
    assert_eq!(
        judgement.run_line,
        format!("Recording run: INTEGRITY FAIL: {}", one_line(&cause)),
        "run line"
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, NOT_COMPARED, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, CASE_INTEGRITY_FAILURE, "CP-001 detail");
    assert!(verdict.recorded, "CP-001 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // A quarantined CP-001 whose run has no digest list: the run's
    // integrity failure wins.
    let mut unhashed = TempCorpus::new("quarantined-no-sums");
    unhashed.input("CP-001", &inputs);
    unhashed.transcript("CP-001", &rust_transcript(&inputs));
    unhashed.run_files(&[("CP-001", "QUARANTINED")]);
    let missing = format!("{}: missing", unhashed.path().join(SUMS_NAME).display());
    let judgement = judge(&unhashed, &[entry("CP-001", CLASS_CP, &inputs, None)]);
    assert_eq!(
        judgement.integrity.first(),
        Some(&missing),
        "integrity: {:?}",
        judgement.integrity
    );
    assert_eq!(
        judgement.run_line,
        format!("Recording run: INTEGRITY FAIL: {}", one_line(&missing)),
        "run line"
    );
    let verdict = verdict_of(&judgement, "CP-001");
    assert_eq!(verdict.verdict, NOT_COMPARED, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, RUN_INTEGRITY_FAILURE, "CP-001 detail");
    assert!(verdict.recorded, "CP-001 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // One verified run: CP-001 recorded and MATCH; CP-002 quarantined with
    // no transcript, which is NOT COMPARED; CE-17 recorded and CE-18
    // quarantined, each with a usable capture and each BOUNDARY; CR-01
    // quarantined with a capture equal to its GC capture, which is
    // QUARANTINED and keeps its WORLD-INDEPENDENT label.
    let mut mixed = TempCorpus::new("quarantined-precedence");
    let absent = committed_inputs("CP-002");
    let recorded_boundary = committed_inputs("CE-17");
    let quarantined_boundary = committed_inputs("CE-18");
    let (gc, replay) = cr_case("CR-01");
    mixed.input("CP-001", &inputs);
    mixed.transcript("CP-001", &rust_transcript(&inputs));
    mixed.input("CP-002", &absent);
    mixed.input("CE-17", &recorded_boundary);
    mixed.transcript("CE-17", "SYNTHETIC BOUNDARY CAPTURE\n");
    mixed.input("CE-18", &quarantined_boundary);
    mixed.transcript("CE-18", "SYNTHETIC QUARANTINED BOUNDARY CAPTURE\n");
    mixed.input("CR-01", &replay);
    mixed.transcript("CR-01", &gc_transcript(gc));
    let statuses = [
        ("CP-001", "RECORDED"),
        ("CP-002", "QUARANTINED"),
        ("CE-17", "RECORDED"),
        ("CE-18", "QUARANTINED"),
        ("CR-01", "QUARANTINED"),
    ];
    mixed.run(&statuses);
    let mixed_ids: Vec<String> = statuses.iter().map(|(id, _)| id.to_string()).collect();
    let quarantined_ids = [
        "CP-002".to_string(),
        "CE-18".to_string(),
        "CR-01".to_string(),
    ];
    assert!(
        matches!(
            load_corpus_run(mixed.path(), &mixed_ids),
            Ok(CorpusRun::Recorded { quarantined, .. }) if quarantined == quarantined_ids
        ),
        "the run must record CP-002, CE-18 and CR-01 as quarantined"
    );
    let judgement = judge(
        &mixed,
        &[
            entry("CP-001", CLASS_CP, &inputs, None),
            entry("CP-002", CLASS_CP, &absent, None),
            entry("CE-17", CLASS_BOUNDARY, &recorded_boundary, None),
            entry("CE-18", CLASS_BOUNDARY, &quarantined_boundary, None),
            entry("CR-01", CLASS_CR, &replay, Some(gc)),
        ],
    );
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
    for (id, wanted, detail, recorded) in [
        ("CP-001", MATCH, NO_DETAIL, true),
        ("CP-002", NOT_COMPARED, NO_CAPTURE, false),
        ("CE-17", BOUNDARY, NO_DETAIL, true),
        ("CE-18", BOUNDARY, NO_DETAIL, true),
        ("CR-01", QUARANTINED, NO_DETAIL, true),
    ] {
        let verdict = verdict_of(&judgement, id);
        assert_eq!(verdict.verdict, wanted, "{id} detail: {}", verdict.detail);
        assert_eq!(verdict.detail, detail, "{id} detail");
        assert_eq!(verdict.recorded, recorded, "{id} recorded");
    }
    let world = world_of(verdict_of(&judgement, "CR-01"));
    assert_eq!(world.gc, gc, "CR-01 GC case");
    assert_eq!(world.label, WORLD_INDEPENDENT, "detail: {}", world.detail);
    assert_eq!(world.detail, NO_DETAIL, "CR-01 label detail");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (1, 1),
        "MATCHED n OF m counts CP-001 only"
    );
}

/// Transcripts without a digest list fail the recording run, which names
/// the missing file, and leave each non-BOUNDARY case NOT COMPARED. A
/// BOUNDARY case stays BOUNDARY, and its transcript, present but unhashed,
/// still shows as recorded although the case does not load. A CR case's
/// world label is NOT COMPARED too. A digest list that exists fails both
/// readers, naming the file or the 1-based line at fault, when it leaves
/// out a present transcript, lists a path outside the allowlist (through
/// `..`, absolute, starting `./`, an unknown id, another file name or a
/// bare id), holds a line that is not a digest and a path, or lists a path
/// twice. A line for a corpus case outside the run's cases fails the run
/// only, and a binary-mode line is accepted.
#[test]
fn corpus_reader_missing_sha256sums() {
    let mut folder = TempCorpus::new("missing-sums");
    let inputs = committed_inputs("CP-001");
    let boundary = committed_inputs("CE-17");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust_transcript(&inputs));
    folder.input("CE-17", &boundary);
    folder.transcript("CE-17", "SYNTHETIC BOUNDARY CAPTURE\n");
    let (gc, replay) = cr_case("CR-01");
    folder.input("CR-01", &replay);
    folder.transcript("CR-01", &gc_transcript(gc));
    folder.run_files(&[
        ("CP-001", "RECORDED"),
        ("CE-17", "RECORDED"),
        ("CR-01", "RECORDED"),
    ]);
    let ids = [
        "CP-001".to_string(),
        "CE-17".to_string(),
        "CR-01".to_string(),
    ];
    match load_corpus_run(folder.path(), &ids) {
        Err(cause) => assert!(cause.contains(SUMS_NAME), "cause: {cause}"),
        Ok(run) => panic!("a run without its digest list loaded: {run:?}"),
    }
    assert!(
        load_corpus_case(folder.path(), "CE-17").is_err(),
        "an unhashed CE-17 transcript must not load"
    );
    let judgement = judge(
        &folder,
        &[
            entry("CP-001", CLASS_CP, &inputs, None),
            entry("CE-17", CLASS_BOUNDARY, &boundary, None),
            entry("CR-01", CLASS_CR, &replay, Some(gc)),
        ],
    );
    let boundary_verdict = verdict_of(&judgement, "CE-17");
    assert_eq!(boundary_verdict.verdict, BOUNDARY, "CE-17 verdict");
    assert!(
        boundary_verdict.recorded,
        "CE-17's transcript exists, so it is recorded although it does not load"
    );
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
    // CR-01's capture equals its GC capture, yet the failed run makes the
    // case and its world label NOT COMPARED.
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, NOT_COMPARED, "CR-01 verdict");
    assert_eq!(verdict.detail, RUN_INTEGRITY_FAILURE, "CR-01 detail");
    assert!(verdict.recorded, "CR-01 is recorded");
    let world = world_of(verdict);
    assert_eq!(world.label, NOT_COMPARED, "CR-01 label");
    assert_eq!(world.detail, RUN_INTEGRITY_FAILURE, "CR-01 label detail");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // The digest list's own rules, on a valid CP-001 run whose listing is
    // then rewritten here with one change at a time. Each line names a file
    // with that file's own digest wherever the file exists, so only the
    // rule under test can reject it.
    let cp_ids = ["CP-001".to_string()];
    let rust = rust_transcript(&inputs);
    let mut listing = TempCorpus::new("missing-sums-listing");
    listing.input("CP-001", &inputs);
    listing.transcript("CP-001", &rust);
    listing.run(&[("CP-001", "RECORDED")]);
    assert!(
        matches!(
            load_corpus_run(listing.path(), &cp_ids),
            Ok(CorpusRun::Recorded { .. })
        ),
        "the valid run must load"
    );
    // The valid listing as `TempCorpus::sums` wrote it, rebuilt from the
    // digests recorded when each file was written, so no capture file is
    // read back.
    let mut written: Vec<(String, String)> = listing
        .written
        .iter()
        .filter(|(relative, _)| relative != SUMS_NAME)
        .cloned()
        .collect();
    written.sort();
    assert_eq!(
        written
            .iter()
            .map(|(relative, _)| relative.as_str())
            .collect::<Vec<_>>(),
        [
            "BASE.txt",
            "CP-001/input.txt",
            "CP-001/transcript.txt",
            "meta.txt",
            "run-summary.txt"
        ],
        "the valid listing, one line per file"
    );
    let valid: Vec<String> = written
        .iter()
        .map(|(relative, digest)| format!("{digest}  {relative}"))
        .collect();
    let digest_of = |relative: &str| {
        written
            .iter()
            .find(|(listed, _)| listed == relative)
            .map(|(_, digest)| digest.clone())
            .unwrap_or_default()
    };
    let listing_text =
        |lines: &[String]| -> String { lines.iter().map(|line| format!("{line}\n")).collect() };
    let sums_path = listing.path().join(SUMS_NAME).display().to_string();

    // A listing that leaves out the present transcript: both readers refuse
    // the transcript as unlisted.
    let unlisted: Vec<String> = valid
        .iter()
        .filter(|line| !line.ends_with("  CP-001/transcript.txt"))
        .cloned()
        .collect();
    assert_eq!(unlisted.len(), 4, "one line must be left out");
    listing.write(SUMS_NAME, &listing_text(&unlisted));
    let not_listed = format!(
        "{}: not listed in SHA256SUMS",
        listing
            .path()
            .join("CP-001")
            .join("transcript.txt")
            .display()
    );
    assert_eq!(
        load_corpus_run(listing.path(), &cp_ids),
        Err(not_listed.clone()),
        "run with its transcript unlisted"
    );
    assert_eq!(
        load_corpus_case(listing.path(), "CP-001"),
        Err(not_listed),
        "CP-001 with its transcript unlisted"
    );

    // One line after the five valid ones, so line 6, that breaks a line
    // rule: a path outside the allowlist, a line that is not a digest and a
    // path as `shasum -a 256` writes them, or a path listed a second time.
    // Both readers apply the same rules to every line.
    let meta = digest_of("meta.txt");
    let input = digest_of("CP-001/input.txt");
    let upper = meta.to_uppercase();
    assert_ne!(upper, meta, "the meta.txt digest must hold a hex letter");
    let short: String = meta.chars().skip(1).collect();
    let absolute_meta = listing.path().join("meta.txt").display().to_string();
    let not_allowed = "path not allowed";
    let not_a_line = "not a SHA-256 digest and path";
    let refused = [
        (format!("{input}  ../CP-001/input.txt"), not_allowed),
        (format!("{meta}  {absolute_meta}"), not_allowed),
        (format!("{meta}  ./meta.txt"), not_allowed),
        (format!("{input}  CP-201/input.txt"), not_allowed),
        (format!("{input}  CP-001/other.txt"), not_allowed),
        (format!("{input}  CP-001"), not_allowed),
        (format!("{short}  meta.txt"), not_a_line),
        (format!("{upper}  meta.txt"), not_a_line),
        (format!("{meta} meta.txt"), not_a_line),
        (meta.clone(), not_a_line),
        (format!("{meta}\tmeta.txt"), not_a_line),
        (format!("{meta}  meta.txt"), "path listed twice"),
    ];
    for (line, cause) in refused {
        let lines: Vec<String> = valid.iter().cloned().chain([line.clone()]).collect();
        listing.write(SUMS_NAME, &listing_text(&lines));
        let expected = format!("{sums_path}: line 6: {cause}");
        assert_eq!(
            load_corpus_run(listing.path(), &cp_ids),
            Err(expected.clone()),
            "run listing {line:?}"
        );
        assert_eq!(
            load_corpus_case(listing.path(), "CP-001"),
            Err(expected),
            "CP-001 listing {line:?}"
        );
    }

    // A line for CP-002, a corpus case outside this run's cases: the run
    // refuses it, while the case reader, which allows every corpus id and
    // compares only its own case's lines, loads CP-001.
    let other_case: Vec<String> = valid
        .iter()
        .cloned()
        .chain([format!("{input}  CP-002/input.txt")])
        .collect();
    listing.write(SUMS_NAME, &listing_text(&other_case));
    assert_eq!(
        load_corpus_run(listing.path(), &cp_ids),
        Err(format!("{sums_path}: line 6: {not_allowed}")),
        "run listing another case"
    );
    assert!(
        matches!(
            load_corpus_case(listing.path(), "CP-001"),
            Ok(case) if case.transcript.as_deref() == Some(rust.as_str())
        ),
        "CP-001 must load beside another case's line"
    );

    // The binary-mode marker of `shasum`, a space and `*`, is accepted.
    let binary: Vec<String> = valid
        .iter()
        .map(|line| match line.strip_suffix("  meta.txt") {
            Some(digest) => format!("{digest} *meta.txt"),
            None => line.clone(),
        })
        .collect();
    assert_ne!(binary, valid, "the meta.txt line must be marked binary");
    listing.write(SUMS_NAME, &listing_text(&binary));
    assert!(
        matches!(
            load_corpus_run(listing.path(), &cp_ids),
            Ok(CorpusRun::Recorded { .. })
        ),
        "a binary-mode line must load"
    );
    assert!(
        matches!(
            load_corpus_case(listing.path(), "CP-001"),
            Ok(case) if case.transcript.as_deref() == Some(rust.as_str())
        ),
        "CP-001 must load beside a binary-mode line"
    );
}

/// A transcript changed by one byte after the digest list was written
/// fails the recording run, which names that transcript.
///
/// The folder first holds a verified run of every class: CP-001 MATCH,
/// CP-002 MISMATCH at a line holding `|` and a backtick, CP-003
/// QUARANTINED, the IN case CE-01 with no capture, the BOUNDARY case CE-17
/// with one, and CR-01 equal to its GC capture. Rendered by
/// `corpus_block` with a synthetic model holding one class move, one
/// finding and one dropped variant, every table of the `### Parity corpus`
/// block is exact, row for row: the run line shows the `meta.txt` values,
/// the export digest and `exports=IDENTICAL runs=IDENTICAL`, the MISMATCH
/// cell escapes `|` and widens its code-span fence, and [`run_failures`]
/// reports nothing. After the edit, the rendered run line reads
/// `INTEGRITY FAIL: <cause>`, every non-BOUNDARY case and the CR world
/// label are NOT COMPARED for the run's failure, the BOUNDARY case stays
/// BOUNDARY and recorded, and [`run_failures`] lists a GC FAIL, then a
/// failed harness check, then each corpus integrity failure.
///
/// Each other listed file, the input, `meta.txt`, `run-summary.txt` and
/// `BASE.txt`, replaced with valid text under its stale digest, fails the
/// run too, which leaves CP-001 NOT COMPARED. The case reader binds the
/// text it returns to the digest list on every call, so a case file
/// replaced or deleted after the run verified fails it, naming that file.
#[test]
fn corpus_reader_edited_transcript() {
    let mut folder = TempCorpus::new("edited");
    let inputs = committed_inputs("CP-001");
    let rust = rust_transcript(&inputs);
    let differing = committed_inputs("CP-002");
    let paging = committed_inputs("CP-003");
    let edge = committed_inputs("CE-01");
    let boundary = committed_inputs("CE-17");
    let (gc, replay) = cr_case("CR-01");
    folder.input("CP-001", &inputs);
    folder.transcript("CP-001", &rust);
    folder.input("CP-002", &differing);
    folder.transcript(
        "CP-002",
        &with_line(&rust_transcript(&differing), 2, "A|B `C`"),
    );
    folder.input("CP-003", &paging);
    folder.transcript("CP-003", &rust_transcript(&paging));
    folder.input("CE-01", &edge);
    folder.input("CE-17", &boundary);
    folder.transcript("CE-17", "SYNTHETIC BOUNDARY CAPTURE\n");
    folder.input("CR-01", &replay);
    folder.transcript("CR-01", &gc_transcript(gc));
    folder.run(&[
        ("CP-001", "RECORDED"),
        ("CP-002", "RECORDED"),
        ("CP-003", "QUARANTINED"),
        ("CE-01", "RECORDED"),
        ("CE-17", "RECORDED"),
        ("CR-01", "RECORDED"),
    ]);
    let model = corpus::Corpus {
        cases: vec![
            model_case("CP-001", corpus::Class::Cp, &inputs, None),
            model_case("CP-002", corpus::Class::Cp, &differing, None),
            model_case("CP-003", corpus::Class::Cp, &paging, None),
            model_case("CE-01", corpus::Class::In, &edge, None),
            model_case(
                "CE-17",
                corpus::Class::Boundary,
                &boundary,
                Some("PO-09.b6"),
            ),
            model_case("CR-01", corpus::Class::Cr, &replay, None),
        ],
        records: Vec::new(),
        drops: vec![corpus::DroppedVariant {
            round: 1,
            case: "CP-002".to_string(),
            dimension: "item.cost".to_string(),
            value: "3dp".to_string(),
            branch: "PO-30.b2".to_string(),
        }],
        moves: vec![corpus::ClassMove {
            case: "CE-04".to_string(),
            from: corpus::Class::In,
            to: corpus::Class::Boundary,
            branch: "PO-09.b11".to_string(),
        }],
        findings: vec![corpus::Finding {
            case: "CP-003".to_string(),
            branch: "PO-30.b1".to_string(),
            cause: "synthetic | finding".to_string(),
        }],
        rounds: 2,
        largest_item_count: 8,
        worst_y: 44,
    };
    let entries = corpus_entries(&model);
    let ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
    assert!(
        matches!(
            load_corpus_run(folder.path(), &ids),
            Ok(CorpusRun::Recorded { .. })
        ),
        "the run must verify before the edit"
    );
    let judgement = judge(&folder, &entries);
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    assert!(
        run_failures(&[], &[], &judgement.integrity).is_empty(),
        "a verified run adds no failure"
    );
    let export = format!("export_sha256={}", sha256_hex(b"synthetic export"));
    let run_line = [
        r#"Recording run: image_digest=`"synthetic"`"#,
        r#"snapshot_id=`"synthetic"`"#,
        r#"source_revision=`"synthetic"`"#,
        r##"context=`"{\"DUZ\":\".5\",\"IOF\":\"#\",\"IOSL\":\"60\",\"STATION\":\"500\"}"`"##,
        r#"start_command=`"docker exec -i vehu su - vehu -c \"<start command>\""`"#,
        export.as_str(),
        "exports=IDENTICAL runs=IDENTICAL",
    ]
    .join("; ");
    let moves = [
        "| Case | Specified class | Final class | Branch |",
        "|---|---|---|---|",
        "| CE-04 | IN | BOUNDARY | PO-09.b11 |",
    ]
    .join("\n");
    let findings = [
        "| Case | Outcome | Cause |",
        "|---|---|---|",
        r"| CP-003 | Implemented(PO-30.b1) | synthetic \| finding |",
    ]
    .join("\n");
    let drops = [
        "| Round | Case | Dimension | Value | Branch |",
        "|---|---|---|---|---|",
        "| 1 | CP-002 | item.cost | 3dp | PO-30.b2 |",
    ]
    .join("\n");
    let boundary_map = [
        "| Case | Input lines | Expected branch | Recorded |",
        "|---|---|---|---|",
        r#"| CE-17 | `""`, `""`, `"CP002"`, `""`, `""`, `"^"`, `"^"`, `"^"` | PO-09.b6 | yes |"#,
    ]
    .join("\n");
    // The MISMATCH cell: `|` escaped, and a two-backtick fence around the
    // quoted line, which holds one backtick on each side of `C`.
    let cases = [
        "| Case | Class | Verdict | First differing line |",
        "|---|---|---|---|",
        "| CP-001 | CP | MATCH | — |",
        r#"| CP-002 | CP | MISMATCH | transcript differs at line 2: expected ``"A\|B `C`"``, actual `""`, not attributed |"#,
        "| CP-003 | CP | QUARANTINED | — |",
        "| CE-01 | IN | NOT COMPARED | no capture |",
        "| CE-17 | BOUNDARY | BOUNDARY | — |",
        "| CR-01 | CR | MATCH | — |",
    ]
    .join("\n");
    let matched = matched_line(judgement.matched, judgement.in_scope);
    assert_eq!(
        matched, "MATCHED 1 OF 2 IN-SCOPE CORPUS CASES",
        "n counts CP-001 alone, and m CP-001 and CP-002"
    );
    assert_block_parts(
        &corpus_block(&model, &judgement, &matched),
        &[
            ("heading", CORPUS_HEADING.to_string()),
            ("run line", run_line),
            (
                "count table",
                [
                    "| Class | MATCH | MISMATCH | NOT COMPARED | BOUNDARY | QUARANTINED | Total |",
                    "|---|---|---|---|---|---|---|",
                    "| CP | 1 | 1 | 0 | 0 | 1 | 3 |",
                    "| IN | 0 | 0 | 1 | 0 | 0 | 1 |",
                    "| BOUNDARY | 0 | 0 | 0 | 1 | 0 | 1 |",
                    "| CR | 1 | 0 | 0 | 0 | 0 | 1 |",
                ]
                .join("\n"),
            ),
            ("MATCHED line", matched.clone()),
            ("per-case table", cases),
            ("class moves heading", "#### Class moves".to_string()),
            ("class moves", moves.clone()),
            ("findings heading", "#### Findings".to_string()),
            ("findings", findings.clone()),
            (
                "dropped variants heading",
                "#### Dropped variants".to_string(),
            ),
            ("dropped variants", drops.clone()),
            ("BOUNDARY map heading", "#### BOUNDARY map".to_string()),
            ("BOUNDARY map", boundary_map.clone()),
            (
                "CR world labels heading",
                "#### CR world labels".to_string(),
            ),
            (
                "CR world labels",
                [
                    "| CR case | GC case | Label | First differing line |",
                    "|---|---|---|---|",
                    "| CR-01 | GC-01 | WORLD-INDEPENDENT | — |",
                ]
                .join("\n"),
            ),
        ],
    );

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
    let cause = match load_corpus_run(folder.path(), &ids) {
        Err(cause) => cause,
        Ok(run) => panic!("an edited transcript passed its digest list: {run:?}"),
    };
    assert!(cause.contains(&named), "cause: {cause}");
    assert_eq!(
        cause,
        format!(
            "{}: SHA-256 differs from {SUMS_NAME}",
            folder.path().join(&named).display()
        ),
        "the run's cause names the edited transcript"
    );
    let judgement = judge(&folder, &entries);
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
    // The run's failure, then CP-001's own: its load binds the edited
    // transcript to the same listing, which fails with the same cause.
    let cause = crate_relative(&cause);
    assert_eq!(
        judgement.integrity,
        [cause.clone(), cause.clone()],
        "integrity failures, the run's first"
    );
    let corpus_failure = format!("corpus integrity FAIL: {cause}");
    assert_eq!(
        run_failures(&[], &[], &judgement.integrity),
        [corpus_failure.clone(), corpus_failure.clone()],
        "a failed run adds one failure per integrity cause"
    );
    let failed = format!("{RUN_LINE}INTEGRITY FAIL: {}", one_line(&cause));
    let not_compared = matched_line(judgement.matched, judgement.in_scope);
    assert_eq!(
        not_compared,
        matched_line(0, 0),
        "a failed run compares nothing"
    );
    assert_block_parts(
        &corpus_block(&model, &judgement, &not_compared),
        &[
            ("heading", CORPUS_HEADING.to_string()),
            ("run line", failed),
            (
                "count table",
                [
                    "| Class | MATCH | MISMATCH | NOT COMPARED | BOUNDARY | QUARANTINED | Total |",
                    "|---|---|---|---|---|---|---|",
                    "| CP | 0 | 0 | 3 | 0 | 0 | 3 |",
                    "| IN | 0 | 0 | 1 | 0 | 0 | 1 |",
                    "| BOUNDARY | 0 | 0 | 0 | 1 | 0 | 1 |",
                    "| CR | 0 | 0 | 1 | 0 | 0 | 1 |",
                ]
                .join("\n"),
            ),
            ("MATCHED line", not_compared.clone()),
            (
                "per-case table",
                [
                    "| Case | Class | Verdict | First differing line |",
                    "|---|---|---|---|",
                    "| CP-001 | CP | NOT COMPARED | recording run integrity failure |",
                    "| CP-002 | CP | NOT COMPARED | recording run integrity failure |",
                    "| CP-003 | CP | NOT COMPARED | recording run integrity failure |",
                    "| CE-01 | IN | NOT COMPARED | recording run integrity failure |",
                    "| CE-17 | BOUNDARY | BOUNDARY | — |",
                    "| CR-01 | CR | NOT COMPARED | recording run integrity failure |",
                ]
                .join("\n"),
            ),
            ("class moves heading", "#### Class moves".to_string()),
            ("class moves", moves),
            ("findings heading", "#### Findings".to_string()),
            ("findings", findings),
            (
                "dropped variants heading",
                "#### Dropped variants".to_string(),
            ),
            ("dropped variants", drops),
            ("BOUNDARY map heading", "#### BOUNDARY map".to_string()),
            ("BOUNDARY map", boundary_map),
            (
                "CR world labels heading",
                "#### CR world labels".to_string(),
            ),
            (
                "CR world labels",
                [
                    "| CR case | GC case | Label | First differing line |",
                    "|---|---|---|---|",
                    "| CR-01 | GC-01 | NOT COMPARED | recording run integrity failure |",
                ]
                .join("\n"),
            ),
        ],
    );

    // The failure order of `equivalence_report`: a FAIL case with its first
    // cause, then a failed harness check, then each corpus failure. A PASS
    // case and a passed check add nothing.
    let gc_result = |id: &'static str, verdict: &'static str, causes: &[&str]| CaseResult {
        id,
        verdict,
        readiness: "READY".to_string(),
        rules: Rules::Listed(Vec::new()),
        image_digest: None,
        snapshot_id: None,
        causes: causes.iter().map(|cause| cause.to_string()).collect(),
        store_changed: Some(false),
    };
    let results = [
        gc_result("GC-01", PASS, &[]),
        gc_result(
            "GC-02",
            FAIL,
            &["synthetic first cause", "synthetic second cause"],
        ),
    ];
    let checks = [
        Check {
            name: "fixtures loaded",
            passed: true,
            detail: "synthetic nodes".to_string(),
        },
        Check {
            name: "store equal to its pre-call clone",
            passed: false,
            detail: "synthetic change".to_string(),
        },
    ];
    assert_eq!(
        run_failures(&results, &checks, &judgement.integrity),
        [
            "GC-02 FAIL: synthetic first cause".to_string(),
            "harness check store equal to its pre-call clone FAIL: synthetic change".to_string(),
            corpus_failure.clone(),
            corpus_failure,
        ],
        "failures: GC FAIL, then failed harness checks, then corpus integrity"
    );

    // Each folder below records CP-001 alone, so its run is read for that
    // case only.
    let ids = ["CP-001".to_string()];

    // Every other listed file replaced after a valid run loaded, in a fresh
    // folder each, its stale digest kept and its new text still valid: the
    // run fails and names that file, as the case reader does for the case's
    // own input, and CP-001 is NOT COMPARED for the failed run.
    let other = committed_inputs("CP-002");
    assert_ne!(other, inputs, "CP-002's input must differ from CP-001's");
    let other_input: String = other.iter().map(|input| format!("{input}\n")).collect();
    let changed_meta = format!(
        "image_digest=synthetic\nsnapshot_id=synthetic-changed\nsource_revision=synthetic\n\
         context={SYNTHETIC_CONTEXT}\nstart_command={SYNTHETIC_START}\n"
    );
    let changed_summary = format!(
        "exports=IDENTICAL\nexport_sha256={}\nruns=IDENTICAL\nCP-001\tRECORDED\t0\t1\n",
        sha256_hex(b"synthetic export")
    );
    let appended_base = "synthetic=written by tests/equivalence.rs, not a snapshot\n\
                         snapshot_image=synthetic\nsnapshot_id=synthetic\nappended=synthetic\n";
    let tampered = [
        ("edited-input", "CP-001/input.txt", other_input.as_str()),
        ("edited-meta", "meta.txt", changed_meta.as_str()),
        (
            "edited-summary",
            "run-summary.txt",
            changed_summary.as_str(),
        ),
        ("edited-base", "BASE.txt", appended_base),
    ];
    for (name, relative, text) in tampered {
        let mut run = TempCorpus::new(name);
        run.input("CP-001", &inputs);
        run.transcript("CP-001", &rust);
        run.run(&[("CP-001", "RECORDED")]);
        assert!(
            matches!(
                load_corpus_run(run.path(), &ids),
                Ok(CorpusRun::Recorded { .. })
            ),
            "{name}: the valid run must load"
        );
        run.write(relative, text);
        let cause = format!(
            "{}: SHA-256 differs from SHA256SUMS",
            run.path().join(relative).display()
        );
        assert_eq!(
            load_corpus_run(run.path(), &ids),
            Err(cause.clone()),
            "{name}: run"
        );
        if relative == "CP-001/input.txt" {
            assert_eq!(
                load_corpus_case(run.path(), "CP-001"),
                Err(cause.clone()),
                "{name}: CP-001"
            );
        }
        let judgement = judge(&run, &[entry("CP-001", CLASS_CP, &inputs, None)]);
        let cause = crate_relative(&cause);
        assert_eq!(
            judgement.run_line,
            format!("Recording run: INTEGRITY FAIL: {}", one_line(&cause)),
            "{name}: run line"
        );
        assert_eq!(
            judgement.integrity.first(),
            Some(&cause),
            "{name}: integrity"
        );
        let verdict = verdict_of(&judgement, "CP-001");
        assert_eq!(verdict.verdict, NOT_COMPARED, "{name}: CP-001 verdict");
        assert_eq!(
            verdict.detail, RUN_INTEGRITY_FAILURE,
            "{name}: CP-001 detail"
        );
    }

    // A case file replaced after the run loaded, its stale digest kept: the
    // case reader binds the very text it returns to the digest list on every
    // call, so it fails although the run verified, and loads again once the
    // text is restored. A listed transcript then deleted fails both readers.
    let mut reload = TempCorpus::new("edited-reload");
    reload.input("CP-001", &inputs);
    reload.transcript("CP-001", &rust);
    reload.run(&[("CP-001", "RECORDED")]);
    let transcript_path = reload.path().join("CP-001").join("transcript.txt");
    let input_path = reload.path().join("CP-001").join("input.txt");
    let loads = |folder: &TempCorpus| {
        matches!(
            load_corpus_case(folder.path(), "CP-001"),
            Ok(case) if case.inputs == inputs && case.transcript.as_deref() == Some(rust.as_str())
        )
    };
    assert!(
        matches!(
            load_corpus_run(reload.path(), &ids),
            Ok(CorpusRun::Recorded { .. })
        ),
        "the valid run must load"
    );
    assert!(loads(&reload), "CP-001 must load from the verified run");
    reload.transcript("CP-001", &with_line(&rust, 2, "SYNTHETIC REPLACED LINE"));
    assert_eq!(
        load_corpus_case(reload.path(), "CP-001"),
        Err(format!(
            "{}: SHA-256 differs from SHA256SUMS",
            transcript_path.display()
        )),
        "CP-001 with its transcript replaced after the run loaded"
    );
    reload.transcript("CP-001", &rust);
    assert!(
        loads(&reload),
        "CP-001 must load with its transcript restored"
    );
    reload.input("CP-001", &other);
    assert_eq!(
        load_corpus_case(reload.path(), "CP-001"),
        Err(format!(
            "{}: SHA-256 differs from SHA256SUMS",
            input_path.display()
        )),
        "CP-001 with its input replaced after the run loaded"
    );
    reload.input("CP-001", &inputs);
    assert!(loads(&reload), "CP-001 must load with its input restored");
    if let Err(err) = fs::remove_file(&transcript_path) {
        panic!("cannot remove {}: {err}", transcript_path.display());
    }
    let missing = format!(
        "{}: listed in SHA256SUMS but missing",
        transcript_path.display()
    );
    assert_eq!(
        load_corpus_case(reload.path(), "CP-001"),
        Err(missing.clone()),
        "CP-001 with its listed transcript deleted"
    );
    assert_eq!(
        load_corpus_run(reload.path(), &ids),
        Err(missing),
        "run with a listed transcript deleted"
    );
}

/// A CR capture equal to its GC capture is WORLD-INDEPENDENT, unless it was
/// recorded with an input other than the model's, which makes the case and
/// its label NOT COMPARED. The case is MATCH and outside n and m, and
/// without the GC capture its label is NOT COMPARED while its verdict stands.
/// On Unix, the same capture reached through a symbolic link (at the
/// transcript, the input, the case folder, `meta.txt` or the corpus folder
/// itself, its target outside the folder under test) is refused as
/// `<path>: symbolic link`, so the case and its label are NOT COMPARED, and
/// a dangling transcript link counts as present.
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
    // CR-01's Rust transcript on world B equals the GC-01 capture, so the
    // case is MATCH, and a CR MATCH stays outside n and m.
    assert_eq!(
        rust_transcript(&inputs),
        gc_transcript(gc),
        "CR-01's Rust transcript on world B must equal the GC-01 capture"
    );
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, MATCH, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, NO_DETAIL, "CR-01 detail");
    assert!(verdict.recorded, "CR-01 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // The same folder judged against a GC folder that does not exist: the
    // label is NOT COMPARED for want of the GC capture, while the case
    // keeps its own verdict.
    let no_gc = folder.path().join("no-gc");
    let judgement = judge_corpus(
        folder.path(),
        &no_gc,
        test_world_b(),
        &[entry("CR-01", CLASS_CR, &inputs, Some(gc))],
        &literal_rows(SPEC),
    );
    assert!(
        judgement.integrity.is_empty(),
        "integrity: {:?}",
        judgement.integrity
    );
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, MATCH, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, NO_DETAIL, "CR-01 detail");
    let world = world_of(verdict);
    assert_eq!(world.gc, gc, "GC case");
    assert_eq!(world.label, NOT_COMPARED, "label without the GC capture");
    assert_eq!(
        world.detail, "GC-01 capture not loaded",
        "label detail without the GC capture"
    );
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // The same capture recorded with CR-02's input and refreshed digests:
    // the case fails its own integrity check, so its verdict and its world
    // label are NOT COMPARED, not WORLD-INDEPENDENT.
    let mut changed = TempCorpus::new("world-independent-changed-input");
    let (_, other) = cr_case("CR-02");
    assert_ne!(other, inputs, "CR-02's input must differ from CR-01's");
    changed.input("CR-01", &other);
    changed.transcript("CR-01", &gc_transcript(gc));
    changed.run(&[("CR-01", "RECORDED")]);
    let judgement = judge(&changed, &[entry("CR-01", CLASS_CR, &inputs, Some(gc))]);
    assert!(
        !judgement.integrity.is_empty(),
        "a changed CR input must fail integrity"
    );
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, NOT_COMPARED, "detail: {}", verdict.detail);
    assert_eq!(verdict.detail, CASE_INTEGRITY_FAILURE, "CR-01 detail");
    match &verdict.world {
        Some(world) => {
            assert_eq!(world.label, NOT_COMPARED, "detail: {}", world.detail);
            assert_eq!(world.detail, CASE_INTEGRITY_FAILURE, "CR-01 label detail");
        }
        None => panic!("CR-01 has no world label"),
    }

    // The same capture reached through a symbolic link, whose target lives
    // in a separate synthetic folder outside the corpus folder under test:
    // no link is followed, so each is `<path>: symbolic link` even where
    // the bytes behind it would verify, and a dangling one is a recording
    // to verify, never an absent one. Unix only, where a test can link.
    #[cfg(unix)]
    {
        let capture = gc_transcript(gc);
        let ids = ["CR-01".to_string()];
        let linked = |root: &Path, relative: &str| {
            format!("{}: symbolic link", root.join(relative).display())
        };

        // (a) The transcript links to an outside file holding the GC-01
        // capture bytes, listed with their digest.
        let mut target = TempCorpus::new("world-independent-link-transcript-target");
        target.transcript("CR-01", &capture);
        let mut folder = TempCorpus::new("world-independent-link-transcript");
        folder.input("CR-01", &inputs);
        folder.link("CR-01/transcript.txt", &target, "CR-01/transcript.txt");
        folder.run(&[("CR-01", "RECORDED")]);
        let cause = linked(folder.path(), "CR-01/transcript.txt");
        assert_eq!(
            load_corpus_run(folder.path(), &ids),
            Err(cause.clone()),
            "(a) run with a linked transcript"
        );
        assert_eq!(
            load_corpus_case(folder.path(), "CR-01"),
            Err(cause.clone()),
            "(a) case with a linked transcript"
        );
        assert!(
            corpus_transcript_exists(folder.path(), "CR-01"),
            "(a) a linked transcript is present"
        );
        let judgement = judge(&folder, &[entry("CR-01", CLASS_CR, &inputs, Some(gc))]);
        assert!(
            judgement.integrity.contains(&cause),
            "(a) integrity: {:?}",
            judgement.integrity
        );
        assert_eq!(
            judgement.run_line,
            format!("Recording run: INTEGRITY FAIL: {cause}"),
            "(a) run line"
        );
        let verdict = verdict_of(&judgement, "CR-01");
        assert_eq!(
            verdict.verdict, NOT_COMPARED,
            "(a) detail: {}",
            verdict.detail
        );
        assert_eq!(verdict.detail, RUN_INTEGRITY_FAILURE, "(a) CR-01 detail");
        match &verdict.world {
            Some(world) => assert_eq!(world.label, NOT_COMPARED, "(a) {}", world.detail),
            None => panic!("(a) CR-01 has no world label"),
        }

        // (b) The input links to an outside file holding the right input.
        let mut target = TempCorpus::new("world-independent-link-input-target");
        target.input("CR-01", &inputs);
        let mut folder = TempCorpus::new("world-independent-link-input");
        folder.link("CR-01/input.txt", &target, "CR-01/input.txt");
        folder.transcript("CR-01", &capture);
        folder.run(&[("CR-01", "RECORDED")]);
        let cause = linked(folder.path(), "CR-01/input.txt");
        assert_eq!(
            load_corpus_run(folder.path(), &ids),
            Err(cause.clone()),
            "(b) run with a linked input"
        );
        assert_eq!(
            load_corpus_case(folder.path(), "CR-01"),
            Err(cause),
            "(b) case with a linked input"
        );

        // (c) The case folder links to an outside folder holding a valid
        // input and transcript.
        let mut target = TempCorpus::new("world-independent-link-folder-target");
        target.input("CR-01", &inputs);
        target.transcript("CR-01", &capture);
        let mut folder = TempCorpus::new("world-independent-link-folder");
        folder.link("CR-01", &target, "CR-01");
        folder.run(&[("CR-01", "RECORDED")]);
        let cause = linked(folder.path(), "CR-01");
        assert_eq!(
            load_corpus_run(folder.path(), &ids),
            Err(cause.clone()),
            "(c) run with a linked case folder"
        );
        assert_eq!(
            load_corpus_case(folder.path(), "CR-01"),
            Err(cause),
            "(c) case with a linked case folder"
        );
        assert!(
            corpus_transcript_exists(folder.path(), "CR-01"),
            "(c) a linked case folder is not confirmed to lack a transcript"
        );

        // (d) `meta.txt` links to an outside valid one. Only the run reads
        // it: a case reads its own files and the digest list alone.
        let mut target = TempCorpus::new("world-independent-link-meta-target");
        target.run_files(&[("CR-01", "RECORDED")]);
        let mut folder = TempCorpus::new("world-independent-link-meta");
        folder.input("CR-01", &inputs);
        folder.transcript("CR-01", &capture);
        folder.run_files(&[("CR-01", "RECORDED")]);
        folder.link("meta.txt", &target, "meta.txt");
        folder.sums();
        assert_eq!(
            load_corpus_run(folder.path(), &ids),
            Err(linked(folder.path(), "meta.txt")),
            "(d) run with a linked meta.txt"
        );
        assert_eq!(
            load_corpus_case(folder.path(), "CR-01").map(|case| case.transcript),
            Ok(Some(capture.clone())),
            "(d) the case reads no meta.txt"
        );

        // (e) The corpus folder itself is a link, made in an outside
        // folder, to an otherwise healthy corpus folder; a trailing
        // separator, through which a path's metadata is its target's,
        // hides nothing.
        let mut healthy = TempCorpus::new("world-independent-link-root-target");
        healthy.input("CR-01", &inputs);
        healthy.transcript("CR-01", &capture);
        healthy.run(&[("CR-01", "RECORDED")]);
        assert!(
            matches!(
                load_corpus_run(healthy.path(), &ids),
                Ok(CorpusRun::Recorded { .. })
            ),
            "(e) the linked-to corpus folder verifies on its own"
        );
        let mut holder = TempCorpus::new("world-independent-link-root");
        holder.link("corpus", &healthy, "");
        let root = holder.path().join("corpus");
        let slashed = root.join("");
        let cause = format!("{}: symbolic link", root.display());
        for given in [&root, &slashed] {
            assert_eq!(
                load_corpus_run(given, &ids),
                Err(cause.clone()),
                "(e) run from {}",
                given.display()
            );
            assert_eq!(
                load_corpus_case(given, "CR-01"),
                Err(cause.clone()),
                "(e) case from {}",
                given.display()
            );
            assert!(
                corpus_transcript_exists(given, "CR-01"),
                "(e) a linked corpus folder is not confirmed to lack a transcript: {}",
                given.display()
            );
        }

        // (f) A dangling transcript link beside the input alone: present,
        // so the run is not absent and needs its run files, and the case
        // fails on the link rather than loading unrecorded.
        let target = TempCorpus::new("world-independent-link-dangling-target");
        let mut folder = TempCorpus::new("world-independent-link-dangling");
        folder.input("CR-01", &inputs);
        folder.link("CR-01/transcript.txt", &target, "CR-01/transcript.txt");
        assert!(
            corpus_transcript_exists(folder.path(), "CR-01"),
            "(f) a dangling transcript link is present"
        );
        assert_eq!(
            load_corpus_run(folder.path(), &ids),
            Err(format!(
                "{}: missing",
                folder.path().join("meta.txt").display()
            )),
            "(f) a dangling transcript link is a recording, not Ok(Absent)"
        );
        assert_eq!(
            load_corpus_case(folder.path(), "CR-01"),
            Err(linked(folder.path(), "CR-01/transcript.txt")),
            "(f) case with a dangling transcript link"
        );
    }
}

/// A CR capture that differs from its GC capture is WORLD-DEPENDENT, its
/// detail naming the first differing line with the GC capture as expected
/// and the CR capture as actual, also when they differ only by a final
/// `\n` removed or appended. The case is MISMATCH, its sides the other way
/// round, and outside n and m.
#[test]
fn corpus_cr_world_dependent() {
    let mut folder = TempCorpus::new("world-dependent");
    let (gc, inputs) = cr_case("CR-01");
    let gc_capture = gc_transcript(gc);
    let capture = with_line(&gc_capture, 2, "SYNTHETIC WORLD B LINE");
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
            assert_eq!(
                world.detail,
                format!(
                    "transcript differs at line 2: expected {}, actual {}, not attributed",
                    quote(line_at(&gc_capture, 2)),
                    quote("SYNTHETIC WORLD B LINE")
                ),
                "CR-01 label detail: the GC capture is expected, the CR capture actual"
            );
        }
        None => panic!("CR-01 has no world label"),
    }
    // CR-01's Rust transcript on world B equals the GC-01 capture, so the
    // case verdict is MISMATCH at the same line with the sides the other way
    // round, and a CR case stays outside n and m.
    assert_eq!(
        rust_transcript(&inputs),
        gc_capture,
        "CR-01's Rust transcript on world B must equal the GC-01 capture"
    );
    let verdict = verdict_of(&judgement, "CR-01");
    assert_eq!(verdict.verdict, MISMATCH, "CR-01 verdict");
    assert_eq!(
        verdict.detail,
        format!(
            "transcript differs at line 2: expected {}, actual {}, not attributed",
            quote("SYNTHETIC WORLD B LINE"),
            quote(line_at(&gc_capture, 2))
        ),
        "CR-01 detail: the capture is expected, the Rust transcript actual"
    );
    assert!(verdict.recorded, "CR-01 is recorded");
    assert_eq!(
        (judgement.matched, judgement.in_scope),
        (0, 0),
        "MATCHED n OF m"
    );

    // The GC capture with its final `\n` removed, then with one appended,
    // each recorded with fresh digests: WORLD-DEPENDENT and MISMATCH at the
    // line past the shorter text, one side absent and the other empty.
    let (removed, appended) = final_newline_variants(&gc_capture);
    let lines = gc_capture.split('\n').count();
    let extra = lines.saturating_add(1);
    let empty = quote("");
    let differs = |line: usize, expected: &str, actual: &str| {
        format!(
            "transcript differs at line {line}: expected {expected}, actual {actual}, \
             not attributed"
        )
    };
    // Each: the folder, the capture, the label detail with the GC capture
    // expected, and the verdict detail with the capture expected.
    let subcases = [
        (
            "world-dependent-no-final-newline",
            removed,
            differs(lines, &empty, ABSENT_LINE),
            differs(lines, ABSENT_LINE, &empty),
        ),
        (
            "world-dependent-extra-final-newline",
            appended,
            differs(extra, ABSENT_LINE, &empty),
            differs(extra, &empty, ABSENT_LINE),
        ),
    ];
    for (name, capture, world_detail, detail) in subcases {
        let judgement = judge_recorded(name, entry("CR-01", CLASS_CR, &inputs, Some(gc)), &capture);
        assert!(
            judgement.integrity.is_empty(),
            "{name} integrity: {:?}",
            judgement.integrity
        );
        let verdict = verdict_of(&judgement, "CR-01");
        let world = world_of(verdict);
        assert_eq!(world.label, WORLD_DEPENDENT, "{name} label");
        assert_eq!(world.detail, world_detail, "{name} label detail");
        assert_eq!(verdict.verdict, MISMATCH, "{name} verdict");
        assert_eq!(verdict.detail, detail, "{name} detail");
        assert_eq!(
            (judgement.matched, judgement.in_scope),
            (0, 0),
            "{name} MATCHED n OF m"
        );
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
