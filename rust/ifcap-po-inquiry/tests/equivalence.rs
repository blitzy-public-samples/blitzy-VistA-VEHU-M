//! Equivalence harness for OPTION `PRCH DISPLAY PO` (`EN3^PRCHRPT`): judges
//! each golden case GC-01 to GC-05 against [`inquire`] and writes `REPORT.md`
//! in full.
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
//! Capture files are read only through `golden`: `load_case`, which also
//! evaluates readiness, and `readiness` alone for an unreadable case. The
//! fixtures are loaded only through `store::load_fixtures`. `SPEC.md` is
//! compiled in: the report copies its static sections, and its literals table
//! only labels a differing line. Neither golden nor specification text ever
//! derives a rule.
//!
//! `equivalence_report` is the only writer of `REPORT.md`, and the harness
//! writes nothing else. A `SPEC.md` lacking a required section fails it before
//! anything is written, and an error writing `REPORT.md` fails it. Once the
//! report is written, it fails only when a case is FAIL or a harness check
//! fails, so a run with no captures exits 0 when the fixtures load and the
//! other harness checks pass. Exit 0 is not acceptance, which needs five PASS.

use std::fs;
use std::path::Path;

use ifcap_po_inquiry::golden::{
    CASE_IDS, CHECKS, Case, CaseLoad, Readiness, Rule, default_dir, load_case, normalise,
    readiness, validate_context,
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
/// each literal, used only to label the first differing line of a FAIL.
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

/// The first line, 1-based, where two different normalised transcripts
/// differ, with both texts and the attribution of the expected line:
/// `attributed to <TAG^ROUTINE>` or `not attributed`.
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

/// The whole of `REPORT.md`: header, parts 1 to 3, then the static sections.
fn render(spec: &str, checks: &[Check], results: &[CaseResult]) -> String {
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

/// Computes the five verdicts, prints `GC-0N <verdict>` for each and writes
/// `REPORT.md`. A `SPEC.md` lacking a required section fails the run before
/// anything is written, and an error writing `REPORT.md` fails it. Once the
/// report is written, the run fails only when a case is FAIL or a harness
/// check fails.
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
    let checks = harness_checks(&fixtures, &results);

    if let Err(err) = fs::write(crate_dir.join("REPORT.md"), render(SPEC, &checks, &results)) {
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
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every `SPEC.md` section the report includes exists as a heading line.
#[test]
fn spec_has_report_sections() {
    let missing = missing_headings(SPEC);
    assert!(missing.is_empty(), "{}", missing_message(&missing));
}
