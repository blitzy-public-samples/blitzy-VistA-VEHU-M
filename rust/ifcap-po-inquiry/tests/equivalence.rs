//! Equivalence harness: replays the golden captures GC-01 to GC-05 of OPTION
//! `PRCH DISPLAY PO` (`EN3^PRCHRPT`) against [`inquire`] and writes the
//! comparison section of `REPORT.md`.
//!
//! Each per-case test calls [`evaluate_case`], which reads the case's capture
//! files under `tests/golden/ifcap/GC-0N/` and the fixtures under
//! `tests/fixtures/ifcap/`, and writes nothing. A case is NOT COMPARED while
//! any required file is missing, FAIL when a present capture is malformed,
//! inconsistent, baseline-mismatched or differs, BLOCKED when a tool a check
//! needs is unavailable, and PASS only when every check succeeds. A per-case
//! test fails only on FAIL, so a run with no captures exits 0. Exit 0 never
//! means equivalence is accepted.
//!
//! [`comparison_report`] is the only writer of `REPORT.md`, and it writes only
//! between the comparison markers. Every capture-format and loader rule comes
//! from the crate's shared helpers; nothing here re-implements one.
//!
//! A finding is held twice: as its console cause, which names only crate
//! files, keys, branch IDs and positions, and as its full detail, which can
//! quote capture values, store records and absolute paths. The console and
//! the assertion messages show causes only; the details go to `REPORT.md`.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use ifcap_po_inquiry::bridge::{
    Meta, normalise, parse_meta, parse_normalisation, split_input_lines, validate_context,
};
use ifcap_po_inquiry::store::load_fixtures;
use ifcap_po_inquiry::{InquiryError, STUB_CONTEXT, Store, inquire};

// ---------------------------------------------------------------------------
// Test data
// ---------------------------------------------------------------------------

/// Path of the capture directories, relative to the crate folder.
const GOLDEN_REL: &str = "tests/golden/ifcap";
/// Path of the fixture directory, relative to the crate folder.
const FIXTURES_REL: &str = "tests/fixtures/ifcap";

/// The seven files every `GC-0N/` directory holds.
const CAPTURE_FILES: [&str; 7] = [
    "input.txt",
    "transcript.txt",
    "returned.txt",
    "db-before.zwr",
    "db-after.zwr",
    "meta.txt",
    "normalisation.txt",
];

/// The byte copies of the repository exports.
const REPO_FIXTURES: [&str; 11] = [
    "411.zwr",
    "411.2.zwr",
    "420.5.zwr",
    "420.8.zwr",
    "440.zwr",
    "442.3.zwr",
    "442.4.zwr",
    "442.5.zwr",
    "442.8.zwr",
    "5.zwr",
    "49.zwr",
];

/// The capture owner's runtime export, which holds the five orders.
const RUNTIME_EXPORT: &str = "cases.zwr";
/// One line per fixture: file, origin, `snapshot_id`, SHA-256.
const MANIFEST: &str = "MANIFEST.txt";

/// Contract ID to `TAG^ROUTINE`, for naming the routine of a branch ID.
const CONTRACTS: [(&str, &str); 41] = [
    ("PO-01", "EN3^PRCHRPT"),
    ("PO-02", "ST^PRCHRPT"),
    ("PO-03", "^PRCFSITE"),
    ("PO-04", "DUZ^PRCFSITE"),
    ("PO-05", "^DIC (file 411)"),
    ("PO-06", "EN30^PRCHRPT"),
    ("PO-07", "PO^PRCHRPT"),
    ("PO-08", "EN3^PRCHPAT"),
    ("PO-09", "IX^DIC"),
    ("PO-10", "^PRCHDP1"),
    ("PO-11", "FTYP^PRCHFPNT"),
    ("PO-12", "^%ZIS"),
    ("PO-13", "FOB^PRCHDP1"),
    ("PO-14", "DIWP^PRCUTL"),
    ("PO-15", "^DIWP"),
    ("PO-16", "%XY^%RCR"),
    ("PO-17", "TY^PRCHDP1"),
    ("PO-18", "DIS^PRCHDP1"),
    ("PO-19", "DT^PRCHDP1"),
    ("PO-20", "CON^PRCHDP1"),
    ("PO-21", "APP^PRCHDP1"),
    ("PO-22", "DAT^PRCHDP1"),
    ("PO-23", "EST^PRCHDP1"),
    ("PO-24", "TOT^PRCHDP1"),
    ("PO-25", "^PRCHDP2"),
    ("PO-26", "HDR^PRCHDP2"),
    ("PO-27", "ITEM^PRCHDP2"),
    ("PO-28", "PIPECK^PRCHDP2"),
    ("PO-29", "EDISTAT^PRCHUTL"),
    ("PO-30", "ASK^PRCHDP2"),
    ("PO-31", "COM^PRCHDP2"),
    ("PO-32", "PT^PRCHDP2"),
    ("PO-33", "AMENDS^PRCHDP6"),
    ("PO-34", "ASK2^PRCHDP2"),
    ("PO-35", "EN^PRCHDP4"),
    ("PO-36", "EN2^PRCHDP4"),
    ("PO-37", "EXDELV^PRCHDP4"),
    ("PO-38", "ASK1^PRCHDP2"),
    ("PO-39", "Q^PRCHDP2"),
    ("PO-40", "K^PRCHRPT"),
    ("PO-41", "^%DT"),
];

// The predicted paths are SPEC.md's "Capture mapping" text, which lists the
// contracts in call order (^PRCFSITE runs ^%DT before ^DIC) and supersedes the
// plan's ordering of the same contracts. The notes name the first BLOCKED
// branch each kind of case reaches while its open item stands.
const PREDICTED_FOUND: &str =
    "PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b8), PO-10 to PO-40, then PO-09 (b3)";
const NOTE_FOUND: &str = "BLOCKED at PO-12.b1 while O-01 is open";
const PREDICTED_MISS: &str = "PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b14), then PO-09 (b3)";
const NOTE_MISS: &str = "BLOCKED at PO-09.b14 while O-03 is open";

// One golden case is the tuple (ID, test name, description, predicted
// contracts and branches, note on the first BLOCKED branch it reaches).
const GC_01: (&str, &str, &str, &str, &str) = (
    "GC-01",
    "gc_01_standard_order",
    "existing order with one line (2 × $125.50)",
    PREDICTED_FOUND,
    NOTE_FOUND,
);
const GC_02: (&str, &str, &str, &str, &str) = (
    "GC-02",
    "gc_02_multiple_lines_and_decimals",
    "three lines (3 × $19.95, 2 × $125.50, 1 × $0.99)",
    PREDICTED_FOUND,
    NOTE_FOUND,
);
const GC_03: (&str, &str, &str, &str, &str) = (
    "GC-03",
    "gc_03_missing_order",
    "syntactically valid identifier absent from the fixture database",
    PREDICTED_MISS,
    NOTE_MISS,
);
const GC_04: (&str, &str, &str, &str, &str) = (
    "GC-04",
    "gc_04_invalid_input",
    "`INVALID!` at the order-selection prompt, then `^`",
    PREDICTED_MISS,
    NOTE_MISS,
);
const GC_05: (&str, &str, &str, &str, &str) = (
    "GC-05",
    "gc_05_cancelled_order",
    "existing cancelled order with retained line-item history",
    PREDICTED_FOUND,
    NOTE_FOUND,
);

const CASES: [(&str, &str, &str, &str, &str); 5] = [GC_01, GC_02, GC_03, GC_04, GC_05];

/// The report the harness writes, relative to the crate folder.
const REPORT: &str = "REPORT.md";
const BEGIN_MARKER: &str = "<!-- COMPARISON BEGIN -->";
const END_MARKER: &str = "<!-- COMPARISON END -->";

/// The `SPEC.md` sections the report copies verbatim, in report order.
const SPEC_SECTIONS: [&str; 4] = [
    "## Stubs",
    "## Flagged hard-MUMPS features",
    "## Unexercised and BLOCKED branches",
    "## Scope statement",
];

/// The text `normalise` puts in place of each replaced span.
const NORM_TOKEN: &str = "<NORM>";

/// The longest excerpt, in characters, shown for a differing line.
const EXCERPT_CHARS: usize = 160;

/// The number of report fields the steps after readiness fill.
const STEP_COUNT: usize = 15;
/// The label of the context field, whose value always shows `STUB_CONTEXT`.
const CONTEXT_FIELD: &str = "Context";
/// The labels of the report fields the steps after readiness fill, in report
/// order. Each field holds one item, or one per hash row or normalisation
/// rule.
const STEP_FIELDS: [&str; STEP_COUNT] = [
    "Inputs",
    "Fixture hashes",
    "Capture hashes",
    "Hash summary",
    "Meta",
    CONTEXT_FIELD,
    "Normalisation rules",
    "Expected returned",
    "Actual returned",
    "Returned comparison",
    "Transcript comparison",
    "Baseline (db-before.zwr against the fixture store)",
    "Snapshot identity",
    "Original side (db-before.zwr against db-after.zwr)",
    "Rust side",
];

// ---------------------------------------------------------------------------
// Outcome model
// ---------------------------------------------------------------------------

// The case outcomes (AAP 0.8.3). The four outcomes are never merged.
const PASS: &str = "PASS";
const FAIL: &str = "FAIL";
const NOT_COMPARED: &str = "NOT COMPARED";
/// A tool a check needs, such as `sha256sum`, is unavailable.
const BLOCKED: &str = "BLOCKED";

/// Records `(cause, detail)` in `findings`, in step order. Findings can quote
/// capture text, so control bytes are made visible to keep each part on one
/// line.
fn record(findings: &mut Vec<(String, String)>, (cause, detail): (String, String)) {
    findings.push((one_line(&cause), one_line(&detail)));
}

/// Records a FAIL finding and returns its detail as a code span, for the
/// step's field.
fn failed(failures: &mut Vec<(String, String)>, (cause, detail): (String, String)) -> String {
    let detail = one_line(&detail);
    let shown = code(&detail);
    failures.push((one_line(&cause), detail));
    shown
}

/// The value of `result`, or `None` after recording its error as FAIL.
fn take<T>(failures: &mut Vec<(String, String)>, result: Result<T, (String, String)>) -> Option<T> {
    result.map_err(|finding| record(failures, finding)).ok()
}

/// A finding that quotes no capture content, so its console cause is the
/// whole finding.
fn plain(finding: String) -> (String, String) {
    (finding.clone(), finding)
}

/// The findings that decide `status`: the FAIL findings for FAIL, the
/// unavailable-tool findings for BLOCKED, and none otherwise.
fn deciding<'a>(
    status: &str,
    failures: &'a [(String, String)],
    blocked: &'a [(String, String)],
) -> &'a [(String, String)] {
    match status {
        FAIL => failures,
        BLOCKED => blocked,
        _ => &[],
    }
}

/// The case's first difference, for the report: the missing files for NOT
/// COMPARED, the first deciding finding's detail for FAIL or BLOCKED, and
/// `none` for PASS.
fn first_difference(status: &str, missing: &[String], findings: &[(String, String)]) -> String {
    match status {
        NOT_COMPARED => format!("missing: {}", missing.join(", ")),
        PASS => "none".to_string(),
        _ => findings
            .first()
            .map(|(_, detail)| detail.clone())
            .unwrap_or_default(),
    }
}

/// The case's one-line console summary: ID, test name and status, then the
/// missing files for NOT COMPARED, `none` for PASS, and for FAIL or BLOCKED
/// the first deciding finding's cause and its place among the findings.
/// Details stay in the `REPORT.md` comparison section.
fn console_summary(
    id: &str,
    test: &str,
    status: &str,
    missing: &[String],
    findings: &[(String, String)],
) -> String {
    let pointer = "details in the REPORT.md comparison section";
    let outcome = match (status, findings.first()) {
        (NOT_COMPARED, _) => format!("missing: {}", missing.join(", ")),
        (PASS, _) => "none".to_string(),
        (_, Some((cause, _))) => format!("{cause} (finding 1 of {}; {pointer})", findings.len()),
        (_, None) => format!("({pointer})"),
    };
    format!("{id} {test}: {status} {outcome}")
}

/// The console cause of a `validate_context` finding: it names the key only
/// when the key is a `STUB_CONTEXT` name, since any other key is capture
/// text.
fn context_cause(finding: &str) -> String {
    STUB_CONTEXT
        .iter()
        .find(|(key, expected)| {
            finding.starts_with(&format!(
                "CONTEXT MISMATCH {key}: expected {}, actual ",
                expected.unwrap_or("UNRESOLVED")
            ))
        })
        .map_or_else(
            || "CONTEXT MISMATCH".to_string(),
            |(key, _)| format!("CONTEXT MISMATCH {key}"),
        )
}

fn not_evaluated(reason: &str) -> String {
    format!("NOT EVALUATED ({reason})")
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The on-disk path of a crate-relative path.
fn crate_path(rel: &str) -> PathBuf {
    crate_dir().join(rel)
}

fn capture_rel(id: &str, file: &str) -> String {
    format!("{GOLDEN_REL}/{id}/{file}")
}

fn fixture_rel(file: &str) -> String {
    format!("{FIXTURES_REL}/{file}")
}

/// The fixture files whose manifest lines and digests must be present: the
/// repository exports and the runtime export.
fn required_fixtures() -> impl Iterator<Item = &'static str> {
    REPO_FIXTURES.into_iter().chain([RUNTIME_EXPORT])
}

/// Every file a case needs, crate-relative, in readiness order.
fn required_files(id: &str) -> Vec<String> {
    CAPTURE_FILES
        .iter()
        .map(|file| capture_rel(id, file))
        .chain(required_fixtures().map(fixture_rel))
        .chain([fixture_rel(MANIFEST)])
        .collect()
}

/// A file is missing only when nothing exists at its path. Anything else
/// that stands there, even a directory or an unreadable file, is present and
/// is judged by the format checks instead.
fn is_missing(rel: &str) -> bool {
    matches!(fs::metadata(crate_path(rel)), Err(err) if err.kind() == ErrorKind::NotFound)
}

/// The UTF-8 text of a present file.
fn read_text(rel: &str) -> Result<String, (String, String)> {
    let bytes = fs::read(crate_path(rel)).map_err(|err| {
        (
            format!("UNREADABLE {rel}"),
            format!("UNREADABLE {rel}: {err}"),
        )
    })?;
    String::from_utf8(bytes).map_err(|_| plain(format!("NON-UTF8 {rel}")))
}

/// Parses `MANIFEST.txt`: blank lines are ignored, and every other line holds
/// exactly four tab-separated, non-empty fields (file, origin, `snapshot_id`,
/// SHA-256 as 64 hex digits), with no file named twice. Each row is (file,
/// `snapshot_id`, lowercase SHA-256), in file order.
fn parse_manifest(text: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut rows = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (index, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let malformed = || format!("MALFORMED MANIFEST line {}", index.saturating_add(1));
        let fields: Vec<&str> = line.split('\t').collect();
        let [file, origin, snapshot_id, sha256] = fields.as_slice() else {
            return Err(malformed());
        };
        let digest_ok = sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit());
        if file.is_empty() || origin.is_empty() || snapshot_id.is_empty() || !digest_ok {
            return Err(malformed());
        }
        if !seen.insert(file) {
            return Err(malformed());
        }
        rows.push((
            file.to_string(),
            snapshot_id.to_string(),
            sha256.to_ascii_lowercase(),
        ));
    }
    Ok(rows)
}

/// The SHA-256 digest of `path` from `sha256sum`, run with an argument list
/// and never through a shell. `Err` says why no digest is available; a tool
/// failure is never a digest.
fn sha256(path: &Path) -> Result<String, String> {
    let output = match Command::new("sha256sum").arg(path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Err("sha256sum not found".to_string());
        }
        Err(err) => return Err(format!("sha256sum: {err}")),
    };
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map_or_else(|| "NONE".to_string(), |code| code.to_string());
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sha256sum exit {code}: {}", stderr.trim_end()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // GNU-compatible sha256sum marks a digest with a leading backslash when
    // the file name needed escaping.
    let token = stdout.split_whitespace().next().unwrap_or("");
    let digest = token.strip_prefix('\\').unwrap_or(token);
    if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(digest.to_ascii_lowercase())
    } else {
        Err(format!("sha256sum output unparsable: {}", visible(&stdout)))
    }
}

// ---------------------------------------------------------------------------
// Comparisons
// ---------------------------------------------------------------------------

/// The first `capture` node, in key order, that the fixture store lacks or
/// holds with another value, as a finding.
fn first_baseline_mismatch(capture: &Store, fixtures: &Store) -> Option<(String, String)> {
    capture.iter().find_map(|(key, value)| {
        let fixture = fixtures.get(key);
        (fixture != Some(value)).then(|| {
            (
                "BASELINE MISMATCH".to_string(),
                format!(
                    "BASELINE MISMATCH {}: capture {}, fixtures {}",
                    Store::format_ref(key),
                    quoted(value),
                    value_or_absent(fixture)
                ),
            )
        })
    })
}

/// The first node, in key order, where `before` and `after` differ, as a
/// finding: a node only one holds, or one both hold with different values.
/// `None` exactly when the stores are equal, since a store is its ordered
/// set of nodes.
fn first_store_difference(before: &Store, after: &Store) -> Option<(String, String)> {
    let mut left = before.iter().peekable();
    let mut right = after.iter().peekable();
    let (key, old, new) = loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return None,
            (Some((key, value)), None) => break (key, Some(value), None),
            (None, Some((key, value))) => break (key, None, Some(value)),
            (Some((key_l, value_l)), Some((key_r, value_r))) => match key_l.cmp(key_r) {
                Ordering::Less => break (key_l, Some(value_l), None),
                Ordering::Greater => break (key_r, None, Some(value_r)),
                Ordering::Equal if value_l != value_r => {
                    break (key_l, Some(value_l), Some(value_r));
                }
                Ordering::Equal => {
                    left.next();
                    right.next();
                }
            },
        }
    };
    Some((
        "ORIGINAL WROTE DATA".to_string(),
        format!(
            "ORIGINAL WROTE DATA {}: before {}, after {}",
            Store::format_ref(key),
            value_or_absent(old),
            value_or_absent(new)
        ),
    ))
}

/// Where two normalised texts first differ: the position, as the 1-based
/// line and the byte offset in the expected text, and the position followed
/// by the line holding that offset on each side, through its newline, made
/// visible. Only the position is free of capture text.
fn first_text_difference(expected: &str, actual: &str) -> Option<(String, String)> {
    if expected == actual {
        return None;
    }
    // Every character before the first differing one is equal on both
    // sides, so the byte offset is the same in both texts.
    let offset = expected
        .char_indices()
        .zip(actual.chars())
        .find(|((_, e), a)| e != a)
        .map_or_else(|| expected.len().min(actual.len()), |((index, _), _)| index);
    let line = expected
        .get(..offset)
        .map_or(0, |prefix| prefix.matches('\n').count())
        .saturating_add(1);
    let position = format!("line {line}, byte {offset}");
    let difference = format!(
        "{position}: expected {}, actual {}",
        excerpt(expected, offset),
        excerpt(actual, offset)
    );
    Some((position, difference))
}

/// The line of `text` that holds byte `offset`, through its newline, quoted
/// and made visible, cut to [`EXCERPT_CHARS`] characters.
fn excerpt(text: &str, offset: usize) -> String {
    let start = text
        .get(..offset)
        .and_then(|prefix| prefix.rfind('\n'))
        .map_or(0, |newline| newline.saturating_add(1));
    let rest = text.get(start..).unwrap_or("");
    let line = rest
        .find('\n')
        .and_then(|newline| rest.get(..=newline))
        .unwrap_or(rest);
    let shown: String = line.chars().take(EXCERPT_CHARS).collect();
    if shown.len() < line.len() {
        format!("{} (cut at {EXCERPT_CHARS} characters)", quoted(&shown))
    } else {
        quoted(&shown)
    }
}

/// The original texts `rule` replaces in `raw`, in order, for the report.
///
/// The spans are recovered from the shared `normalise` applied to `raw` with
/// this rule alone, so no second copy of the rule logic exists: the output
/// is split on `<NORM>` into literal segments, and each replaced span is the
/// shortest text at the next position that starts with the open delimiter,
/// ends with the close delimiter and is followed by the next literal.
/// `None` means NOT DERIVABLE: `raw` already holds `<NORM>`, so the output
/// cannot be split unambiguously.
fn replaced_spans(raw: &str, rule: &(String, String)) -> Option<Vec<String>> {
    if raw.contains(NORM_TOKEN) {
        return None;
    }
    let (open, close) = rule;
    let normalised = normalise(raw, std::slice::from_ref(rule));
    let mut literals = normalised.split(NORM_TOKEN);
    let first = literals.next().unwrap_or("");
    let literals: Vec<&str> = literals.collect();
    if !raw.starts_with(first) {
        return None;
    }
    let close_step = close.chars().next().map_or(1, char::len_utf8);
    let mut position = first.len();
    let mut spans = Vec::with_capacity(literals.len());
    for (number, literal) in literals.iter().enumerate() {
        let tail = raw.get(position..)?;
        if !tail.starts_with(open.as_str()) {
            return None;
        }
        let last = number.saturating_add(1) == literals.len();
        let mut search = open.len();
        let end = loop {
            let found = search.saturating_add(tail.get(search..)?.find(close.as_str())?);
            let end = found.saturating_add(close.len());
            let after = tail.get(end..)?;
            if (last && after == *literal) || (!last && after.starts_with(literal)) {
                break end;
            }
            search = found.saturating_add(close_step);
        };
        spans.push(tail.get(..end)?.to_string());
        position = position.saturating_add(end).saturating_add(literal.len());
    }
    (position == raw.len()).then_some(spans)
}

/// The contract routine of a branch ID `PO-NN.bM`.
fn routine_of(branch: &str) -> &'static str {
    let contract = branch.split('.').next().unwrap_or(branch);
    CONTRACTS
        .iter()
        .find(|(id, _)| *id == contract)
        .map_or("UNKNOWN CONTRACT", |(_, routine)| routine)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// `text` with control bytes made visible: `<BEL>` for 0x07, `\t`, `\n`,
/// `\r`, `\xNN` for any other control byte, and `\\` and `\"` so that the
/// rendering is unambiguous inside quotes.
fn visible(text: &str) -> String {
    escape(text, true)
}

/// `text` with only its control bytes made visible, as [`visible`] does, so
/// a finding that embeds capture or error text stays on one line.
fn one_line(text: &str) -> String {
    escape(text, false)
}

fn escape(text: &str, quotes: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{7}' => out.push_str("<BEL>"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' if quotes => out.push_str("\\\\"),
            '"' if quotes => out.push_str("\\\""),
            c if c.is_control() => {
                let mut buffer = [0u8; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    out.push_str(&format!("\\x{byte:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// `text` quoted and made visible; `""` for empty text.
fn quoted(text: &str) -> String {
    format!("\"{}\"", visible(text))
}

/// A stored value quoted, or `ABSENT`.
fn value_or_absent(value: Option<&str>) -> String {
    value.map_or_else(|| "ABSENT".to_string(), quoted)
}

/// A Markdown code span holding `text` exactly, fenced with more backticks
/// than the longest run inside it.
fn code(text: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        run = if c == '`' { run.saturating_add(1) } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat(longest.saturating_add(1));
    let pad = if text.starts_with('`') || text.ends_with('`') || text.is_empty() {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// The inputs, each quoted and made visible, in order.
fn render_inputs(inputs: &[String]) -> String {
    if inputs.is_empty() {
        return "none (`input.txt` holds no input)".to_string();
    }
    let shown: Vec<String> = inputs.iter().map(|input| code(&quoted(input))).collect();
    format!("{} inputs: {}", inputs.len(), shown.join(", "))
}

/// A return value as text: `None`, or `Some("...")` with the value quoted.
fn returned_value(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_string(),
        |value| format!("Some({})", quoted(value)),
    )
}

/// A return value as a code span.
fn render_returned(value: Option<&str>) -> String {
    code(&returned_value(value))
}

/// Context pairs as `KEY="value"` code spans, in order.
fn render_context(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return "none".to_string();
    }
    let shown: Vec<String> = pairs
        .iter()
        .map(|(key, value)| code(&format!("{key}={}", quoted(value))))
        .collect();
    shown.join(", ")
}

/// `STUB_CONTEXT`, for display only; `validate_context` is the comparator.
fn render_stub_context() -> String {
    let shown: Vec<String> = STUB_CONTEXT
        .iter()
        .map(|(key, value)| {
            code(&value.map_or_else(
                || format!("{key}=UNRESOLVED"),
                |value| format!("{key}={}", quoted(value)),
            ))
        })
        .collect();
    shown.join(", ")
}

/// The meta values the report shows; `docker_exec_args` is the live mode's
/// concern and is shown as its argument count.
fn render_meta(meta: &Meta) -> String {
    format!(
        "image_digest {}, source_revision {}, snapshot_id {}, record_ids {}, entry_point {}, docker_exec_args {} arguments",
        code(&quoted(&meta.image_digest)),
        code(&quoted(&meta.source_revision)),
        code(&quoted(&meta.snapshot_id)),
        code(&quoted(&meta.record_ids)),
        code(&quoted(&meta.entry_point)),
        meta.docker_exec_args.len()
    )
}

/// One normalisation rule and, for each side, the texts it replaced there.
fn render_rule(
    number: usize,
    rule: &(String, String),
    sides: &[(&str, Result<&str, String>)],
) -> String {
    let (open, close) = rule;
    let shown: Vec<String> = sides
        .iter()
        .map(|(label, raw)| {
            let replaced = match raw {
                Err(reason) => reason.clone(),
                Ok(raw) => match replaced_spans(raw, rule) {
                    None => "NOT DERIVABLE (the text already holds `<NORM>`)".to_string(),
                    Some(spans) if spans.is_empty() => "nothing replaced".to_string(),
                    Some(spans) => {
                        let spans: Vec<String> =
                            spans.iter().map(|span| code(&quoted(span))).collect();
                        spans.join(", ")
                    }
                },
            };
            format!("{label}: {replaced}")
        })
        .collect();
    format!(
        "rule {number}: open {}, close {}; {}",
        code(&quoted(open)),
        code(&quoted(close)),
        shown.join("; ")
    )
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// `Store::from_zwr` on a capture export.
fn capture_store(rel: &str, text: &str) -> Result<Store, (String, String)> {
    Store::from_zwr(text).map_err(|err| {
        (
            format!("MALFORMED CAPTURE {rel}"),
            format!("MALFORMED CAPTURE {rel}: {err}"),
        )
    })
}

/// The return value a `returned.txt` records: zero bytes is `None`, and
/// otherwise one trailing newline ends the file and is not part of the
/// value, as the bridge's replay reading has it.
fn recorded_returned(text: &str) -> Option<&str> {
    if text.is_empty() {
        None
    } else {
        Some(text.strip_suffix('\n').unwrap_or(text))
    }
}

/// Evaluates one case against its capture. Pure: it reads the capture, the
/// fixtures and `MANIFEST.txt`, and writes nothing. It returns the case's
/// status, its console summary and its report subsection.
///
/// Readiness comes first and alone decides NOT COMPARED; the later steps run
/// only when every required file is present.
fn evaluate_case(case: (&str, &str, &str, &str, &str)) -> (&'static str, String, String) {
    let (id, test, ..) = case;
    let mut failures = Vec::new();
    let mut blocked = Vec::new();

    // 1. Readiness.
    let missing: Vec<String> = required_files(id)
        .into_iter()
        .filter(|rel| is_missing(rel))
        .collect();
    let (status, actual_branch, steps) = if missing.is_empty() {
        let (actual_branch, steps) = evaluate_steps(id, &mut failures, &mut blocked);
        let status = if !failures.is_empty() {
            FAIL
        } else if !blocked.is_empty() {
            BLOCKED
        } else {
            PASS
        };
        (status, actual_branch, steps)
    } else {
        let skipped = not_evaluated("readiness: required files missing");
        let steps = STEP_FIELDS.map(|label| {
            vec![if label == CONTEXT_FIELD {
                context_field(&skipped)
            } else {
                skipped.clone()
            }]
        });
        (NOT_COMPARED, skipped, steps)
    };
    let findings = deciding(status, &failures, &blocked);
    let summary = console_summary(id, test, status, &missing, findings);
    let section = render_case(
        case,
        status,
        &missing,
        &actual_branch,
        &steps,
        &failures,
        &blocked,
    );
    (status, summary, section)
}

/// Steps 2 to 7 of a case whose required files are all present. Every FAIL
/// finding goes to `failures` and every unavailable-tool finding to
/// `blocked`, in step order. It returns the actual branch and the step
/// fields, in [`STEP_FIELDS`] order.
///
/// Every check runs whenever its inputs could be read and parsed, even after
/// an earlier FAIL, so one evaluation reports every finding; the first is
/// the case's first difference.
fn evaluate_steps(
    id: &str,
    failures: &mut Vec<(String, String)>,
    blocked: &mut Vec<(String, String)>,
) -> (String, [Vec<String>; STEP_COUNT]) {
    // 2. Format checks.
    let cap = |file: &str| capture_rel(id, file);
    let mut unreadable: Vec<String> = Vec::new();
    let mut read = |rel: String| {
        let text = read_text(&rel);
        if text.is_err() {
            unreadable.push(rel);
        }
        text
    };
    let input_text = take(failures, read(cap("input.txt")));
    let transcript_text = take(failures, read(cap("transcript.txt")));
    let returned_text = take(failures, read(cap("returned.txt")));
    let before_text = take(failures, read(cap("db-before.zwr")));
    let after_text = take(failures, read(cap("db-after.zwr")));
    let meta_text = take(failures, read(cap("meta.txt")));
    let norm_text = take(failures, read(cap("normalisation.txt")));
    for file in required_fixtures() {
        // Only readability and UTF-8 are checked here; `load_fixtures` parses.
        if let Err(finding) = read(fixture_rel(file)) {
            record(failures, finding);
        }
    }
    let manifest = take(failures, read(fixture_rel(MANIFEST)))
        .and_then(|text| take(failures, parse_manifest(&text).map_err(plain)));
    if let Some(rows) = &manifest {
        for file in required_fixtures() {
            if !rows.iter().any(|(name, _, _)| name == file) {
                record(
                    failures,
                    plain(format!("MANIFEST INCONSISTENT: no line for {file}")),
                );
            }
        }
    }
    let meta_rel = cap("meta.txt");
    let meta = meta_text.and_then(|text| {
        take(
            failures,
            parse_meta(&text).map_err(|msg| {
                (
                    format!("MALFORMED META {meta_rel}"),
                    format!("MALFORMED META {meta_rel}: {msg}"),
                )
            }),
        )
    });
    let norm_rel = cap("normalisation.txt");
    let rules = norm_text.and_then(|text| {
        take(
            failures,
            parse_normalisation(&text)
                .map_err(|line| plain(format!("MALFORMED NORMALISATION {norm_rel} line {line}"))),
        )
    });
    let before =
        before_text.and_then(|text| take(failures, capture_store(&cap("db-before.zwr"), &text)));
    let after =
        after_text.and_then(|text| take(failures, capture_store(&cap("db-after.zwr"), &text)));
    let (fixture_hashes, capture_hashes, hash_summary) =
        check_hashes(id, manifest.as_deref(), &unreadable, failures, blocked);

    // 3. Baseline identity.
    let fixtures = take(
        failures,
        load_fixtures(&crate_path(FIXTURES_REL))
            .map_err(|err| ("FIXTURE LOAD".to_string(), format!("FIXTURE LOAD {err}"))),
    );
    let baseline = match (&before, &fixtures) {
        (Some(capture), Some(fixture_store)) => {
            match first_baseline_mismatch(capture, fixture_store) {
                None => format!(
                    "IDENTICAL: all {} `db-before.zwr` nodes are in the fixture store with identical values",
                    capture.len()
                ),
                Some(finding) => failed(failures, finding),
            }
        }
        (None, _) => not_evaluated("db-before.zwr unavailable"),
        (_, None) => not_evaluated("fixture store unavailable"),
    };
    let snapshot = match &manifest {
        None => not_evaluated("MANIFEST.txt unavailable"),
        Some(rows) => {
            let mut ids: Vec<&str> = rows
                .iter()
                .map(|(_, snapshot_id, _)| snapshot_id.as_str())
                .collect();
            ids.sort_unstable();
            ids.dedup();
            match (ids.as_slice(), &meta) {
                ([only], None) => format!(
                    "MANIFEST.txt carries {}; meta.txt {}",
                    code(&quoted(only)),
                    not_evaluated("meta.txt unavailable")
                ),
                ([only], Some(meta)) if meta.snapshot_id == *only => {
                    format!(
                        "IDENTICAL: MANIFEST.txt and meta.txt carry {}",
                        code(&quoted(only))
                    )
                }
                ([only], Some(meta)) => failed(
                    failures,
                    (
                        "SNAPSHOT MISMATCH".to_string(),
                        format!(
                            "SNAPSHOT MISMATCH: meta.txt {}, MANIFEST.txt {}",
                            quoted(&meta.snapshot_id),
                            quoted(only)
                        ),
                    ),
                ),
                _ => {
                    let shown: Vec<String> = ids.iter().map(|id| quoted(id)).collect();
                    failed(
                        failures,
                        (
                            "MANIFEST INCONSISTENT snapshot_id".to_string(),
                            format!(
                                "MANIFEST INCONSISTENT snapshot_id: {}",
                                if shown.is_empty() {
                                    "none".to_string()
                                } else {
                                    shown.join(", ")
                                }
                            ),
                        ),
                    )
                }
            }
        }
    };
    let meta_field = meta
        .as_ref()
        .map_or_else(|| not_evaluated("meta.txt unavailable"), render_meta);

    // 4. Context.
    let context = match &meta {
        None => not_evaluated("meta.txt unavailable"),
        Some(meta) => {
            let actual = render_context(&meta.context);
            match validate_context(&meta.context) {
                Ok(()) => format!("MATCH; actual {actual}"),
                Err(finding) => {
                    let shown = failed(failures, (context_cause(&finding), finding));
                    format!("{shown}; actual {actual}")
                }
            }
        }
    };

    // 5. Original side: the session wrote no data.
    let original_side = match (&before, &after) {
        (Some(before), Some(after)) => match first_store_difference(before, after) {
            None => format!("IDENTICAL ({} nodes)", before.len()),
            Some(finding) => failed(failures, finding),
        },
        _ => not_evaluated("db-before.zwr or db-after.zwr unavailable"),
    };

    // 6. Rust side.
    let inputs = input_text.as_deref().map(split_input_lines);
    let inputs_field = inputs
        .as_deref()
        .map_or_else(|| not_evaluated("input.txt unavailable"), render_inputs);
    let (output, rust_side, actual_branch) = match (&inputs, &fixtures) {
        (Some(inputs), Some(store)) => {
            let untouched = store.clone();
            let result = inquire(store, inputs);
            if *store != untouched {
                record(failures, plain("STORE CHANGED".to_string()));
            }
            match result {
                Ok(output) => {
                    let rust_side = format!(
                        "Ok: transcript of {} bytes, returned {}",
                        output.transcript.len(),
                        render_returned(output.returned.as_deref())
                    );
                    let actual_branch =
                        "none: the session ended without reaching an unsupported branch"
                            .to_string();
                    (Some(output), rust_side, actual_branch)
                }
                Err(InquiryError::UnsupportedBranch(branch)) => {
                    let routine = routine_of(&branch);
                    let actual_branch = format!("{} ({})", code(&branch), code(routine));
                    let rust_side = failed(
                        failures,
                        plain(format!("UNSUPPORTED BRANCH {branch} ({routine})")),
                    );
                    (None, rust_side, actual_branch)
                }
                Err(InquiryError::CorruptStore(missing_node)) => {
                    let rust_side = failed(
                        failures,
                        (
                            "CORRUPT STORE".to_string(),
                            format!("CORRUPT STORE {missing_node}"),
                        ),
                    );
                    let actual_branch = "none: the session stopped on a missing node".to_string();
                    (None, rust_side, actual_branch)
                }
            }
        }
        (None, _) => {
            let reason = not_evaluated("input.txt unavailable");
            (None, reason.clone(), reason)
        }
        (_, None) => {
            let reason = not_evaluated("fixture store unavailable");
            (None, reason.clone(), reason)
        }
    };

    // 7. Comparison, on the transcript and on `returned`.
    let expected_returned = returned_text.as_deref().map(recorded_returned);
    let expected_field = expected_returned.map_or_else(
        || not_evaluated("returned.txt unavailable"),
        render_returned,
    );
    let actual_returned = output.as_ref().map_or_else(
        || not_evaluated("the Rust side did not return Ok"),
        |output| render_returned(output.returned.as_deref()),
    );
    let (transcript_comparison, returned_comparison) =
        match (&output, &rules, &transcript_text, expected_returned) {
            (Some(output), Some(rules), Some(transcript), Some(expected_returned)) => {
                let expected = normalise(transcript, rules);
                let actual = normalise(&output.transcript, rules);
                let transcript_comparison = match first_text_difference(&expected, &actual) {
                    None => format!("MATCH after normalisation ({} bytes)", expected.len()),
                    Some((position, difference)) => failed(
                        failures,
                        (
                            format!("TRANSCRIPT DIFFERS {position}"),
                            format!("TRANSCRIPT DIFFERS {difference}"),
                        ),
                    ),
                };
                let expected_norm = expected_returned.map(|value| normalise(value, rules));
                let actual_norm = output
                    .returned
                    .as_deref()
                    .map(|value| normalise(value, rules));
                let returned_comparison = if expected_norm == actual_norm {
                    "MATCH after normalisation".to_string()
                } else {
                    failed(
                        failures,
                        (
                            "RETURNED DIFFERS".to_string(),
                            format!(
                                "RETURNED DIFFERS: expected {}, actual {}",
                                returned_value(expected_norm.as_deref()),
                                returned_value(actual_norm.as_deref())
                            ),
                        ),
                    )
                };
                (transcript_comparison, returned_comparison)
            }
            _ => {
                let mut reasons = Vec::new();
                if output.is_none() {
                    reasons.push("the Rust side did not return Ok");
                }
                if rules.is_none() {
                    reasons.push("normalisation.txt unavailable");
                }
                if transcript_text.is_none() {
                    reasons.push("transcript.txt unavailable");
                }
                if expected_returned.is_none() {
                    reasons.push("returned.txt unavailable");
                }
                let reason = not_evaluated(&reasons.join("; "));
                (reason.clone(), reason)
            }
        };

    // Normalisation reporting: what each rule replaced; it decides nothing.
    let normalisation = match &rules {
        None => vec![not_evaluated("normalisation.txt unavailable")],
        Some(rules) if rules.is_empty() => {
            vec!["none: `normalisation.txt` holds no rule".to_string()]
        }
        Some(rules) => {
            let no_rust = || not_evaluated("the Rust side did not return Ok");
            let no_value = || "nothing to replace: the value is `None`".to_string();
            let sides = [
                (
                    "expected transcript",
                    transcript_text
                        .as_deref()
                        .ok_or_else(|| not_evaluated("transcript.txt unavailable")),
                ),
                (
                    "actual transcript",
                    output
                        .as_ref()
                        .map(|output| output.transcript.as_str())
                        .ok_or_else(no_rust),
                ),
                (
                    "expected returned",
                    match expected_returned {
                        None => Err(not_evaluated("returned.txt unavailable")),
                        Some(None) => Err(no_value()),
                        Some(Some(value)) => Ok(value),
                    },
                ),
                (
                    "actual returned",
                    match &output {
                        None => Err(no_rust()),
                        Some(output) => output.returned.as_deref().ok_or_else(no_value),
                    },
                ),
            ];
            rules
                .iter()
                .enumerate()
                .map(|(index, rule)| render_rule(index.saturating_add(1), rule, &sides))
                .collect()
        }
    };

    let steps = [
        vec![inputs_field],
        fixture_hashes,
        capture_hashes,
        vec![hash_summary],
        vec![meta_field],
        vec![context_field(&context)],
        normalisation,
        vec![expected_field],
        vec![actual_returned],
        vec![returned_comparison],
        vec![transcript_comparison],
        vec![baseline],
        vec![snapshot],
        vec![original_side],
        vec![rust_side],
    ];
    (actual_branch, steps)
}

/// Hashes every fixture and capture file with `sha256sum`, and checks each
/// fixture digest against its `MANIFEST.txt` row when the manifest parsed.
/// A digest the tool cannot give is BLOCKED, never a match, except for a file
/// already found unreadable, whose FAIL already stands. It returns the
/// rendered fixture rows, the rendered capture rows and the hash summary.
fn check_hashes(
    id: &str,
    manifest: Option<&[(String, String, String)]>,
    unreadable: &[String],
    failures: &mut Vec<(String, String)>,
    blocked: &mut Vec<(String, String)>,
) -> (Vec<String>, Vec<String>, String) {
    let mut matched = 0usize;
    let mut differing = 0usize;
    let mut unavailable = 0usize;
    let mut digest_of = |blocked: &mut Vec<(String, String)>, rel: &str| {
        sha256(&crate_path(rel)).map_err(|reason| {
            unavailable = unavailable.saturating_add(1);
            if unreadable.iter().any(|path| path == rel) {
                format!("NOT AVAILABLE (file unreadable; {reason})")
            } else {
                record(
                    blocked,
                    (
                        format!("HASH NOT AVAILABLE {rel}"),
                        format!("HASH NOT AVAILABLE {rel}: {reason}"),
                    ),
                );
                format!("NOT AVAILABLE ({reason})")
            }
        })
    };
    // Each row is (path, manifest digest when listed, computed digest or
    // why none was).
    let mut fixture_rows = Vec::new();
    for file in required_fixtures() {
        let rel = fixture_rel(file);
        let listed = manifest
            .and_then(|rows| rows.iter().find(|(name, _, _)| name == file))
            .map(|(_, _, digest)| digest.clone());
        let computed = match digest_of(blocked, &rel) {
            Ok(digest) => {
                if let Some(listed) = &listed {
                    if *listed == digest {
                        matched = matched.saturating_add(1);
                    } else {
                        differing = differing.saturating_add(1);
                        record(
                            failures,
                            (
                                format!("HASH MISMATCH {file}"),
                                format!(
                                    "HASH MISMATCH {file}: manifest {listed}, computed {digest}"
                                ),
                            ),
                        );
                    }
                }
                digest
            }
            Err(shown) => shown,
        };
        fixture_rows.push((rel, listed, computed));
    }
    let mut capture_rows = Vec::new();
    for file in CAPTURE_FILES {
        let rel = capture_rel(id, file);
        let computed = digest_of(blocked, &rel).unwrap_or_else(|shown| shown);
        capture_rows.push((rel, None, computed));
    }
    let summary = format!(
        "{matched} of {} fixture digests match MANIFEST.txt, {differing} differ; {unavailable} digests NOT AVAILABLE",
        REPO_FIXTURES.len().saturating_add(1)
    );
    (
        render_hashes(&fixture_rows),
        render_hashes(&capture_rows),
        summary,
    )
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// Hash rows (path, manifest digest when listed, computed digest or why none
/// was) as report items.
fn render_hashes(rows: &[(String, Option<String>, String)]) -> Vec<String> {
    rows.iter()
        .map(|(path, listed, computed)| {
            let listed = listed.as_deref().map_or_else(
                || "no manifest line".to_string(),
                |digest| format!("manifest {}", code(digest)),
            );
            format!("{}: {listed}, computed {}", code(path), code(computed))
        })
        .collect()
}

/// The context field: the expected `STUB_CONTEXT`, then `actual`, which is
/// the check's outcome or why it did not run.
fn context_field(actual: &str) -> String {
    format!(
        "expected (`STUB_CONTEXT`) {}; {actual}",
        render_stub_context()
    )
}

/// A bullet whose value is a list: inline when it holds one item, nested
/// otherwise.
fn bullet_list(out: &mut Vec<String>, label: &str, items: &[String]) {
    match items {
        [] => out.push(format!("- **{label}:** none")),
        [one] => out.push(format!("- **{label}:** {one}")),
        many => {
            out.push(format!("- **{label}:**"));
            out.extend(many.iter().map(|item| format!("  - {item}")));
        }
    }
}

/// The report subsection of one case, with every per-case field and the
/// full detail of every finding.
fn render_case(
    case: (&str, &str, &str, &str, &str),
    status: &str,
    missing: &[String],
    actual_branch: &str,
    steps: &[Vec<String>; STEP_COUNT],
    failures: &[(String, String)],
    blocked: &[(String, String)],
) -> String {
    let (id, test, description, predicted, predicted_note) = case;
    let first = first_difference(status, missing, deciding(status, failures, blocked));
    let mut out = vec![
        format!("### {id} — {test}"),
        String::new(),
        format!("- **Case:** {description}"),
        format!("- **Actual test name:** {}", code(test)),
        format!("- **Status:** {status}"),
        format!("- **First difference:** {}", code(&first)),
        format!("- **Predicted contracts and branches:** {predicted} ({predicted_note})"),
        format!("- **Actual branch:** {actual_branch}"),
    ];
    if missing.is_empty() {
        out.push(format!(
            "- **Readiness:** READY: all {} capture files and {} fixture files present",
            CAPTURE_FILES.len(),
            REPO_FIXTURES.len().saturating_add(2)
        ));
    } else {
        let missing: Vec<String> = missing.iter().map(|rel| code(rel)).collect();
        bullet_list(
            &mut out,
            &format!(
                "Readiness: NOT READY, {} required files missing",
                missing.len()
            ),
            &missing,
        );
    }
    for (label, items) in STEP_FIELDS.iter().zip(steps) {
        bullet_list(&mut out, label, items);
    }
    let details = |findings: &[(String, String)]| -> Vec<String> {
        findings.iter().map(|(_, detail)| code(detail)).collect()
    };
    bullet_list(&mut out, "Failures", &details(failures));
    if !blocked.is_empty() {
        bullet_list(&mut out, "Blocked", &details(blocked));
    }
    out.join("\n")
}

/// The text of `spec` from the line exactly equal to `heading` through the
/// line before the next line starting with `## `, or to the end; `None` when
/// no line equals `heading`.
fn spec_section(spec: &str, heading: &str) -> Option<String> {
    let mut lines = spec.split('\n').skip_while(|line| *line != heading);
    let first = lines.next()?;
    let section: Vec<&str> = std::iter::once(first)
        .chain(lines.take_while(|line| !line.starts_with("## ")))
        .collect();
    Some(section.join("\n"))
}

/// The comparison-section body, from each case's (status, console summary,
/// report subsection), and the errors that must fail the report test after
/// it is written.
fn render_body(cases: &[(&str, String, String)]) -> (String, Vec<String>) {
    let mut errors = Vec::new();
    let count = |status: &str| {
        cases
            .iter()
            .filter(|(outcome, _, _)| *outcome == status)
            .count()
    };
    let mut totals = format!(
        "PASS {}, FAIL {}, NOT COMPARED {}",
        count(PASS),
        count(FAIL),
        count(NOT_COMPARED)
    );
    let blocked = count(BLOCKED);
    if blocked > 0 {
        totals.push_str(&format!(", BLOCKED {blocked}"));
    }
    let mut blocks = vec![
        "Written by `comparison_report` in `tests/equivalence.rs`, the only writer of this section. \
         NOT COMPARED means a required file is missing; exit 0 of the equivalence suite never means \
         equivalence is accepted."
            .to_string(),
        format!("**Totals:** {totals}"),
    ];
    blocks.extend(cases.iter().map(|(_, _, section)| section.clone()));
    let spec_rel = "SPEC.md";
    match fs::read_to_string(crate_path(spec_rel)) {
        Ok(spec) => {
            for heading in SPEC_SECTIONS {
                match spec_section(&spec, heading) {
                    Some(section) => blocks.push(section),
                    None => {
                        let missing = format!("MISSING SECTION {heading} IN SPEC.md");
                        blocks.push(missing.clone());
                        errors.push(missing);
                    }
                }
            }
        }
        Err(err) => {
            for heading in SPEC_SECTIONS {
                let missing =
                    format!("MISSING SECTION {heading} IN SPEC.md (SPEC.md unreadable: {err})");
                blocks.push(missing.clone());
                errors.push(missing);
            }
        }
    }
    (join_blocks(&blocks), errors)
}

/// Joins Markdown blocks with one blank line between each pair, adding only
/// the newlines a block lacks, so a verbatim section that already ends in a
/// blank line is followed by no second one.
fn join_blocks(blocks: &[String]) -> String {
    let mut body = String::new();
    for block in blocks {
        if !body.is_empty() {
            while !body.ends_with("\n\n") {
                body.push('\n');
            }
        }
        body.push_str(block);
    }
    body
}

/// Writes `body` between the comparison markers of the report at the
/// crate-relative path `rel`, creating the file with the markers when it is
/// absent. Every byte outside the markers is kept; with missing or
/// out-of-order markers, or a body that would itself hold a marker, the file
/// is left untouched. Errors name the file by `rel`, never by its absolute
/// path.
fn write_comparison(rel: &str, body: &str) -> Result<(), String> {
    if body.contains(BEGIN_MARKER) || body.contains(END_MARKER) {
        return Err(format!(
            "the comparison body holds a marker; {rel} left untouched"
        ));
    }
    let path = crate_path(rel);
    let existing = match fs::read(&path) {
        Ok(bytes) => Some(
            String::from_utf8(bytes).map_err(|_| format!("{rel} is not UTF-8; left untouched"))?,
        ),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(format!("cannot read {rel}: {err}")),
    };
    let text = match existing {
        None => format!("# REPORT\n\n## Comparison\n\n{BEGIN_MARKER}\n{body}\n{END_MARKER}\n"),
        Some(existing) => {
            let bad =
                || format!("{rel} has missing or out-of-order comparison markers; left untouched");
            let begin = existing.find(BEGIN_MARKER).ok_or_else(bad)?;
            if existing.find(END_MARKER).is_some_and(|end| end < begin) {
                return Err(bad());
            }
            let inner_start = begin.saturating_add(BEGIN_MARKER.len());
            let after_begin = existing.get(inner_start..).ok_or_else(bad)?;
            let inner_end =
                inner_start.saturating_add(after_begin.find(END_MARKER).ok_or_else(bad)?);
            let head = existing.get(..inner_start).ok_or_else(bad)?;
            let tail = existing.get(inner_end..).ok_or_else(bad)?;
            format!("{head}\n{body}\n{tail}")
        }
    };
    fs::write(&path, text).map_err(|err| format!("cannot write {rel}: {err}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Evaluates one case, prints its one-line console summary, and fails only
/// on FAIL, with the same summary.
fn run_case(case: (&str, &str, &str, &str, &str)) {
    let (status, summary, _) = evaluate_case(case);
    println!("{summary}");
    assert!(status != FAIL, "{summary}");
}

#[test]
fn gc_01_standard_order() {
    run_case(GC_01);
}

#[test]
fn gc_02_multiple_lines_and_decimals() {
    run_case(GC_02);
}

#[test]
fn gc_03_missing_order() {
    run_case(GC_03);
}

#[test]
fn gc_04_invalid_input() {
    run_case(GC_04);
}

#[test]
fn gc_05_cancelled_order() {
    run_case(GC_05);
}

/// Evaluates all five cases and writes the comparison section of
/// `REPORT.md`. It asserts only on its own errors, never on case statuses,
/// which the per-case tests decide.
#[test]
fn comparison_report() {
    let cases: Vec<(&str, String, String)> = CASES.into_iter().map(evaluate_case).collect();
    let (body, mut errors) = render_body(&cases);
    if let Err(err) = write_comparison(REPORT, &body) {
        errors.push(err);
    }
    assert!(
        errors.is_empty(),
        "comparison_report: {}",
        errors.join("; ")
    );
}

/// `returned.txt` framing: zero bytes is `None`, and exactly one trailing
/// newline is removed from any other content.
#[test]
fn returned_txt_framing() {
    let table = [
        ("", None),
        ("abc", Some("abc")),
        ("abc\n", Some("abc")),
        ("abc\n\n", Some("abc\n")),
        ("\n", Some("")),
    ];
    for (text, expected) in table {
        assert_eq!(recorded_returned(text), expected, "returned.txt {text:?}");
    }
}

/// `MANIFEST.txt` rows keep file order with blank lines skipped, and a file
/// named twice is malformed on the repeat's own line, however far apart.
#[test]
fn manifest_rows_keep_order_and_reject_repeats() {
    let digest = "AB".repeat(32);
    let row = |file: &str| format!("{file}\trepository source\tgit:snapshot\t{digest}\n");
    let table = [
        (
            format!("{}\n{}{}", row("5.zwr"), row("411.zwr"), row("cases.zwr")),
            Ok(vec!["5.zwr", "411.zwr", "cases.zwr"]),
        ),
        (
            format!("{}{}", row("5.zwr"), row("5.zwr")),
            Err("MALFORMED MANIFEST line 2"),
        ),
        (
            format!(
                "{}{}\n{}{}",
                row("5.zwr"),
                row("411.zwr"),
                row("cases.zwr"),
                row("411.zwr")
            ),
            Err("MALFORMED MANIFEST line 5"),
        ),
    ];
    for (text, expected) in table {
        let expected = expected
            .map(|files| {
                files
                    .into_iter()
                    .map(|file| {
                        (
                            file.to_string(),
                            "git:snapshot".to_string(),
                            digest.to_ascii_lowercase(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .map_err(str::to_string);
        assert_eq!(parse_manifest(&text), expected, "{text:?}");
    }
}

/// A FAIL or BLOCKED console summary names the first deciding finding's
/// cause and position only, never a detail; NOT COMPARED and PASS keep
/// their form.
#[test]
fn console_summary_shows_causes_never_details() {
    let finding = |cause: &str, detail: &str| (cause.to_string(), detail.to_string());
    let failures = [
        finding(
            "BASELINE MISMATCH",
            "BASELINE MISMATCH ^PRC(442,\"C\",\"SECRET\",9): capture \"SECRET\", fixtures ABSENT",
        ),
        finding(
            "TRANSCRIPT DIFFERS line 3, byte 41",
            "TRANSCRIPT DIFFERS line 3, byte 41: expected \"SECRET\\n\", actual \"\\n\"",
        ),
    ];
    let blocked = [finding(
        "HASH NOT AVAILABLE tests/fixtures/ifcap/5.zwr",
        "HASH NOT AVAILABLE tests/fixtures/ifcap/5.zwr: sha256sum exit 1: sha256sum: /checkout/SECRET/5.zwr: Permission denied",
    )];
    let missing = [
        "tests/golden/ifcap/GC-01/input.txt".to_string(),
        "tests/fixtures/ifcap/cases.zwr".to_string(),
    ];
    let no_missing: &[String] = &[];
    let no_findings: &[(String, String)] = &[];
    let table = [
        (
            FAIL,
            no_missing,
            &failures[..],
            "GC-01 gc_01_standard_order: FAIL BASELINE MISMATCH (finding 1 of 2; details in the REPORT.md comparison section)",
        ),
        (
            FAIL,
            no_missing,
            &failures[1..],
            "GC-01 gc_01_standard_order: FAIL TRANSCRIPT DIFFERS line 3, byte 41 (finding 1 of 1; details in the REPORT.md comparison section)",
        ),
        (
            BLOCKED,
            no_missing,
            &blocked[..],
            "GC-01 gc_01_standard_order: BLOCKED HASH NOT AVAILABLE tests/fixtures/ifcap/5.zwr (finding 1 of 1; details in the REPORT.md comparison section)",
        ),
        (
            NOT_COMPARED,
            &missing[..],
            no_findings,
            "GC-01 gc_01_standard_order: NOT COMPARED missing: tests/golden/ifcap/GC-01/input.txt, tests/fixtures/ifcap/cases.zwr",
        ),
        (
            PASS,
            no_missing,
            no_findings,
            "GC-01 gc_01_standard_order: PASS none",
        ),
    ];
    for (status, missing, findings, expected) in table {
        let summary = console_summary("GC-01", "gc_01_standard_order", status, missing, findings);
        assert_eq!(summary, expected, "{status}");
        assert!(!summary.contains("SECRET"), "{status}: {summary}");
    }
}

/// A context finding's console cause names its key only when the key is a
/// `STUB_CONTEXT` name.
#[test]
fn context_cause_names_only_stub_keys() {
    let table = [
        (
            "CONTEXT MISMATCH IOF: expected UNRESOLVED, actual #SECRET",
            "CONTEXT MISMATCH IOF",
        ),
        (
            "CONTEXT MISMATCH DUZ: expected .5, actual MISSING",
            "CONTEXT MISMATCH DUZ",
        ),
        (
            "CONTEXT MISMATCH SECRETKEY: expected ABSENT, actual 1",
            "CONTEXT MISMATCH",
        ),
        (
            "CONTEXT MISMATCH IOF: SECRET: expected ABSENT, actual 1",
            "CONTEXT MISMATCH",
        ),
    ];
    for (finding, expected) in table {
        assert_eq!(context_cause(finding), expected, "{finding}");
    }
}
