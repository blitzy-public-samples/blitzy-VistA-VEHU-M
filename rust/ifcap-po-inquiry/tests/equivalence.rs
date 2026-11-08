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

use std::cmp::Ordering;
use std::fmt;
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

/// One golden case.
#[derive(Clone, Copy, Debug)]
struct Case {
    id: &'static str,
    test: &'static str,
    description: &'static str,
    predicted: &'static str,
    predicted_note: &'static str,
}

const GC_01: Case = Case {
    id: "GC-01",
    test: "gc_01_standard_order",
    description: "existing order with one line (2 × $125.50)",
    predicted: PREDICTED_FOUND,
    predicted_note: NOTE_FOUND,
};
const GC_02: Case = Case {
    id: "GC-02",
    test: "gc_02_multiple_lines_and_decimals",
    description: "three lines (3 × $19.95, 2 × $125.50, 1 × $0.99)",
    predicted: PREDICTED_FOUND,
    predicted_note: NOTE_FOUND,
};
const GC_03: Case = Case {
    id: "GC-03",
    test: "gc_03_missing_order",
    description: "syntactically valid identifier absent from the fixture database",
    predicted: PREDICTED_MISS,
    predicted_note: NOTE_MISS,
};
const GC_04: Case = Case {
    id: "GC-04",
    test: "gc_04_invalid_input",
    description: "`INVALID!` at the order-selection prompt, then `^`",
    predicted: PREDICTED_MISS,
    predicted_note: NOTE_MISS,
};
const GC_05: Case = Case {
    id: "GC-05",
    test: "gc_05_cancelled_order",
    description: "existing cancelled order with retained line-item history",
    predicted: PREDICTED_FOUND,
    predicted_note: NOTE_FOUND,
};

const CASES: [Case; 5] = [GC_01, GC_02, GC_03, GC_04, GC_05];

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

// ---------------------------------------------------------------------------
// Outcome model
// ---------------------------------------------------------------------------

/// A case outcome (AAP 0.8.3). The four outcomes are never merged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    NotCompared,
    /// A tool a check needs, such as `sha256sum`, is unavailable.
    Blocked,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::NotCompared => "NOT COMPARED",
            Status::Blocked => "BLOCKED",
        })
    }
}

/// The hash of one file: its manifest digest, when it has a line, and the
/// digest `sha256sum` computed, or why none was.
struct HashRow {
    path: String,
    manifest: Option<String>,
    computed: String,
}

/// Everything the report shows for one case. Text fields hold rendered
/// Markdown fragments; a step that did not run holds `NOT EVALUATED (...)`.
struct CaseReport {
    case: Case,
    status: Status,
    missing: Vec<String>,
    /// Every FAIL finding, in step order; the first is the first difference.
    failures: Vec<String>,
    /// Every unavailable-tool finding.
    blocked: Vec<String>,
    inputs: String,
    fixture_hashes: Vec<HashRow>,
    capture_hashes: Vec<HashRow>,
    hashes_state: String,
    meta: String,
    context: String,
    normalisation: Vec<String>,
    expected_returned: String,
    actual_returned: String,
    returned_comparison: String,
    transcript_comparison: String,
    baseline: String,
    snapshot: String,
    original_side: String,
    rust_side: String,
    actual_branch: String,
}

impl CaseReport {
    fn new(case: Case) -> CaseReport {
        let pending = not_evaluated("not reached");
        CaseReport {
            case,
            status: Status::NotCompared,
            missing: Vec::new(),
            failures: Vec::new(),
            blocked: Vec::new(),
            inputs: pending.clone(),
            fixture_hashes: Vec::new(),
            capture_hashes: Vec::new(),
            hashes_state: pending.clone(),
            meta: pending.clone(),
            context: pending.clone(),
            normalisation: vec![pending.clone()],
            expected_returned: pending.clone(),
            actual_returned: pending.clone(),
            returned_comparison: pending.clone(),
            transcript_comparison: pending.clone(),
            baseline: pending.clone(),
            snapshot: pending.clone(),
            original_side: pending.clone(),
            rust_side: pending.clone(),
            actual_branch: pending,
        }
    }

    /// Marks every step as not run for `reason`.
    fn skip_all(&mut self, reason: &str) {
        let text = not_evaluated(reason);
        for field in [
            &mut self.inputs,
            &mut self.hashes_state,
            &mut self.meta,
            &mut self.context,
            &mut self.expected_returned,
            &mut self.actual_returned,
            &mut self.returned_comparison,
            &mut self.transcript_comparison,
            &mut self.baseline,
            &mut self.snapshot,
            &mut self.original_side,
            &mut self.rust_side,
            &mut self.actual_branch,
        ] {
            field.clone_from(&text);
        }
        self.normalisation = vec![text];
    }

    /// Records a FAIL finding. Findings can quote capture text, so their
    /// control bytes are made visible to keep each finding on one line.
    fn fail(&mut self, finding: String) {
        self.failures.push(one_line(&finding));
    }

    /// Records a FAIL finding and returns it as a code span, for the step's
    /// field.
    fn failed(&mut self, finding: String) -> String {
        let finding = one_line(&finding);
        let shown = code(&finding);
        self.failures.push(finding);
        shown
    }

    /// Records an unavailable-tool finding.
    fn block(&mut self, finding: String) {
        self.blocked.push(one_line(&finding));
    }

    /// The value of `result`, or `None` after recording its error as FAIL.
    fn take<T>(&mut self, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(finding) => {
                self.fail(finding);
                None
            }
        }
    }

    /// The case's first difference: the missing files for NOT COMPARED, the
    /// first finding for FAIL or BLOCKED, and `none` for PASS.
    fn first_difference(&self) -> String {
        match self.status {
            Status::NotCompared => format!("missing: {}", self.missing.join(", ")),
            Status::Fail => self.failures.first().cloned().unwrap_or_default(),
            Status::Blocked => self.blocked.first().cloned().unwrap_or_default(),
            Status::Pass => "none".to_string(),
        }
    }
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
fn read_text(rel: &str) -> Result<String, String> {
    let bytes = fs::read(crate_path(rel)).map_err(|err| format!("UNREADABLE {rel}: {err}"))?;
    String::from_utf8(bytes).map_err(|_| format!("NON-UTF8 {rel}"))
}

/// One line of `MANIFEST.txt`.
struct ManifestLine {
    file: String,
    snapshot_id: String,
    sha256: String,
}

/// Parses `MANIFEST.txt`: blank lines are ignored, and every other line holds
/// exactly four tab-separated, non-empty fields (file, origin, `snapshot_id`,
/// SHA-256 as 64 hex digits), with no file named twice.
fn parse_manifest(text: &str) -> Result<Vec<ManifestLine>, String> {
    let mut lines: Vec<ManifestLine> = Vec::new();
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
        if lines.iter().any(|seen| seen.file == *file) {
            return Err(malformed());
        }
        lines.push(ManifestLine {
            file: file.to_string(),
            snapshot_id: snapshot_id.to_string(),
            sha256: sha256.to_ascii_lowercase(),
        });
    }
    Ok(lines)
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
/// holds with another value.
fn first_baseline_mismatch(capture: &Store, fixtures: &Store) -> Option<String> {
    capture.iter().find_map(|(key, value)| {
        let fixture = fixtures.get(key);
        (fixture != Some(value)).then(|| {
            format!(
                "BASELINE MISMATCH {}: capture {}, fixtures {}",
                Store::format_ref(key),
                quoted(value),
                value_or_absent(fixture)
            )
        })
    })
}

/// The first node, in key order, where `before` and `after` differ: a node
/// only one holds, or one both hold with different values. `None` exactly
/// when the stores are equal, since a store is its ordered set of nodes.
fn first_store_difference(before: &Store, after: &Store) -> Option<String> {
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
    Some(format!(
        "ORIGINAL WROTE DATA {}: before {}, after {}",
        Store::format_ref(key),
        value_or_absent(old),
        value_or_absent(new)
    ))
}

/// Where two normalised texts first differ: the 1-based line and byte offset
/// in the expected text, and the line holding that offset on each side,
/// through its newline, made visible.
fn first_text_difference(expected: &str, actual: &str) -> Option<String> {
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
    Some(format!(
        "line {line}, byte {offset}: expected {}, actual {}",
        excerpt(expected, offset),
        excerpt(actual, offset)
    ))
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
fn capture_store(rel: &str, text: &str) -> Result<Store, String> {
    Store::from_zwr(text).map_err(|err| format!("MALFORMED CAPTURE {rel}: {err}"))
}

/// Evaluates one case against its capture. Pure: it reads the capture, the
/// fixtures and `MANIFEST.txt`, and writes nothing.
///
/// Readiness comes first and alone decides NOT COMPARED. After it, every
/// later check runs whenever its inputs could be read and parsed, even after
/// an earlier FAIL, so one evaluation reports every finding; the first is
/// the case's first difference.
fn evaluate_case(case: &Case) -> CaseReport {
    let mut report = CaseReport::new(*case);

    // 1. Readiness.
    report.missing = required_files(case.id)
        .into_iter()
        .filter(|rel| is_missing(rel))
        .collect();
    if !report.missing.is_empty() {
        report.skip_all("readiness: required files missing");
        report.status = Status::NotCompared;
        return report;
    }

    // 2. Format checks.
    let cap = |file: &str| capture_rel(case.id, file);
    let mut unreadable: Vec<String> = Vec::new();
    let mut read = |rel: String| {
        let text = read_text(&rel);
        if text.is_err() {
            unreadable.push(rel);
        }
        text
    };
    let input_text = report.take(read(cap("input.txt")));
    let transcript_text = report.take(read(cap("transcript.txt")));
    let returned_text = report.take(read(cap("returned.txt")));
    let before_text = report.take(read(cap("db-before.zwr")));
    let after_text = report.take(read(cap("db-after.zwr")));
    let meta_text = report.take(read(cap("meta.txt")));
    let norm_text = report.take(read(cap("normalisation.txt")));
    for file in required_fixtures() {
        // Only readability and UTF-8 are checked here; `load_fixtures` parses.
        if let Err(finding) = read(fixture_rel(file)) {
            report.fail(finding);
        }
    }
    let manifest = report
        .take(read(fixture_rel(MANIFEST)))
        .and_then(|text| report.take(parse_manifest(&text)));
    if let Some(lines) = &manifest {
        for file in required_fixtures() {
            if !lines.iter().any(|line| line.file == file) {
                report.fail(format!("MANIFEST INCONSISTENT: no line for {file}"));
            }
        }
    }
    let meta_rel = cap("meta.txt");
    let meta = meta_text.and_then(|text| {
        report.take(parse_meta(&text).map_err(|msg| format!("MALFORMED META {meta_rel}: {msg}")))
    });
    let norm_rel = cap("normalisation.txt");
    let rules = norm_text.and_then(|text| {
        report.take(
            parse_normalisation(&text)
                .map_err(|line| format!("MALFORMED NORMALISATION {norm_rel} line {line}")),
        )
    });
    let before =
        before_text.and_then(|text| report.take(capture_store(&cap("db-before.zwr"), &text)));
    let after = after_text.and_then(|text| report.take(capture_store(&cap("db-after.zwr"), &text)));
    check_hashes(&mut report, manifest.as_deref(), &unreadable);

    // 3. Baseline identity.
    let fixtures = match load_fixtures(&crate_path(FIXTURES_REL)) {
        Ok(store) => Some(store),
        Err(err) => {
            report.fail(format!("FIXTURE LOAD {err}"));
            None
        }
    };
    report.baseline = match (&before, &fixtures) {
        (Some(capture), Some(fixture_store)) => {
            match first_baseline_mismatch(capture, fixture_store) {
                None => format!(
                    "IDENTICAL: all {} `db-before.zwr` nodes are in the fixture store with identical values",
                    capture.len()
                ),
                Some(finding) => report.failed(finding),
            }
        }
        (None, _) => not_evaluated("db-before.zwr unavailable"),
        (_, None) => not_evaluated("fixture store unavailable"),
    };
    report.snapshot = match &manifest {
        None => not_evaluated("MANIFEST.txt unavailable"),
        Some(lines) => {
            let mut ids: Vec<&str> = lines.iter().map(|line| line.snapshot_id.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            match (ids.as_slice(), &meta) {
                ([id], None) => format!(
                    "MANIFEST.txt carries {}; meta.txt {}",
                    code(&quoted(id)),
                    not_evaluated("meta.txt unavailable")
                ),
                ([id], Some(meta)) if meta.snapshot_id == *id => {
                    format!(
                        "IDENTICAL: MANIFEST.txt and meta.txt carry {}",
                        code(&quoted(id))
                    )
                }
                ([id], Some(meta)) => report.failed(format!(
                    "SNAPSHOT MISMATCH: meta.txt {}, MANIFEST.txt {}",
                    quoted(&meta.snapshot_id),
                    quoted(id)
                )),
                _ => {
                    let shown: Vec<String> = ids.iter().map(|id| quoted(id)).collect();
                    report.failed(format!(
                        "MANIFEST INCONSISTENT snapshot_id: {}",
                        if shown.is_empty() {
                            "none".to_string()
                        } else {
                            shown.join(", ")
                        }
                    ))
                }
            }
        }
    };
    report.meta = meta
        .as_ref()
        .map_or_else(|| not_evaluated("meta.txt unavailable"), render_meta);

    // 4. Context.
    report.context = match &meta {
        None => not_evaluated("meta.txt unavailable"),
        Some(meta) => {
            let actual = render_context(&meta.context);
            match validate_context(&meta.context) {
                Ok(()) => format!("MATCH; actual {actual}"),
                Err(finding) => {
                    let finding = report.failed(finding);
                    format!("{finding}; actual {actual}")
                }
            }
        }
    };

    // 5. Original side: the session wrote no data.
    report.original_side = match (&before, &after) {
        (Some(before), Some(after)) => match first_store_difference(before, after) {
            None => format!("IDENTICAL ({} nodes)", before.len()),
            Some(finding) => report.failed(finding),
        },
        _ => not_evaluated("db-before.zwr or db-after.zwr unavailable"),
    };

    // 6. Rust side.
    let inputs = input_text.as_deref().map(split_input_lines);
    report.inputs = inputs
        .as_deref()
        .map_or_else(|| not_evaluated("input.txt unavailable"), render_inputs);
    let output = match (&inputs, &fixtures) {
        (Some(inputs), Some(store)) => {
            let untouched = store.clone();
            let result = inquire(store, inputs);
            if *store != untouched {
                report.fail("STORE CHANGED".to_string());
            }
            match result {
                Ok(output) => {
                    report.rust_side = format!(
                        "Ok: transcript of {} bytes, returned {}",
                        output.transcript.len(),
                        render_returned(output.returned.as_deref())
                    );
                    report.actual_branch =
                        "none: the session ended without reaching an unsupported branch"
                            .to_string();
                    Some(output)
                }
                Err(InquiryError::UnsupportedBranch(branch)) => {
                    let routine = routine_of(&branch);
                    report.actual_branch = format!("{} ({})", code(&branch), code(routine));
                    report.rust_side =
                        report.failed(format!("UNSUPPORTED BRANCH {branch} ({routine})"));
                    None
                }
                Err(InquiryError::CorruptStore(record)) => {
                    report.actual_branch =
                        "none: the session stopped on a missing node".to_string();
                    report.rust_side = report.failed(format!("CORRUPT STORE {record}"));
                    None
                }
            }
        }
        (None, _) => {
            let reason = not_evaluated("input.txt unavailable");
            report.actual_branch.clone_from(&reason);
            report.rust_side = reason;
            None
        }
        (_, None) => {
            let reason = not_evaluated("fixture store unavailable");
            report.actual_branch.clone_from(&reason);
            report.rust_side = reason;
            None
        }
    };

    // 7. Comparison, on the transcript and on `returned`.
    let expected_returned = returned_text
        .as_deref()
        .map(|text| (!text.is_empty()).then_some(text));
    report.expected_returned = expected_returned.map_or_else(
        || not_evaluated("returned.txt unavailable"),
        render_returned,
    );
    report.actual_returned = output.as_ref().map_or_else(
        || not_evaluated("the Rust side did not return Ok"),
        |output| render_returned(output.returned.as_deref()),
    );
    match (&output, &rules, &transcript_text, expected_returned) {
        (Some(output), Some(rules), Some(transcript), Some(expected_returned)) => {
            let expected = normalise(transcript, rules);
            let actual = normalise(&output.transcript, rules);
            report.transcript_comparison = match first_text_difference(&expected, &actual) {
                None => format!("MATCH after normalisation ({} bytes)", expected.len()),
                Some(difference) => report.failed(format!("TRANSCRIPT DIFFERS {difference}")),
            };
            let expected_norm = expected_returned.map(|value| normalise(value, rules));
            let actual_norm = output
                .returned
                .as_deref()
                .map(|value| normalise(value, rules));
            report.returned_comparison = if expected_norm == actual_norm {
                "MATCH after normalisation".to_string()
            } else {
                report.failed(format!(
                    "RETURNED DIFFERS: expected {}, actual {}",
                    returned_value(expected_norm.as_deref()),
                    returned_value(actual_norm.as_deref())
                ))
            };
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
            report.transcript_comparison.clone_from(&reason);
            report.returned_comparison = reason;
        }
    }

    // Normalisation reporting: what each rule replaced; it decides nothing.
    report.normalisation = match &rules {
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

    report.status = if !report.failures.is_empty() {
        Status::Fail
    } else if !report.blocked.is_empty() {
        Status::Blocked
    } else {
        Status::Pass
    };
    report
}

/// Hashes every fixture and capture file with `sha256sum`, and checks each
/// fixture digest against its `MANIFEST.txt` line when the manifest parsed.
/// A digest the tool cannot give is BLOCKED, never a match, except for a file
/// already found unreadable, whose FAIL already stands.
fn check_hashes(report: &mut CaseReport, manifest: Option<&[ManifestLine]>, unreadable: &[String]) {
    let mut matched = 0usize;
    let mut differing = 0usize;
    let mut unavailable = 0usize;
    let mut digest_of = |report: &mut CaseReport, rel: &str| {
        sha256(&crate_path(rel)).map_err(|reason| {
            unavailable = unavailable.saturating_add(1);
            if unreadable.iter().any(|path| path == rel) {
                format!("NOT AVAILABLE (file unreadable; {reason})")
            } else {
                report.block(format!("HASH NOT AVAILABLE {rel}: {reason}"));
                format!("NOT AVAILABLE ({reason})")
            }
        })
    };
    for file in required_fixtures() {
        let rel = fixture_rel(file);
        let listed = manifest
            .and_then(|lines| lines.iter().find(|line| line.file == file))
            .map(|line| line.sha256.clone());
        let computed = match digest_of(report, &rel) {
            Ok(digest) => {
                if let Some(listed) = &listed {
                    if *listed == digest {
                        matched = matched.saturating_add(1);
                    } else {
                        differing = differing.saturating_add(1);
                        report.fail(format!(
                            "HASH MISMATCH {file}: manifest {listed}, computed {digest}"
                        ));
                    }
                }
                digest
            }
            Err(shown) => shown,
        };
        report.fixture_hashes.push(HashRow {
            path: rel,
            manifest: listed,
            computed,
        });
    }
    for file in CAPTURE_FILES {
        let rel = capture_rel(report.case.id, file);
        let computed = digest_of(report, &rel).unwrap_or_else(|shown| shown);
        report.capture_hashes.push(HashRow {
            path: rel,
            manifest: None,
            computed,
        });
    }
    report.hashes_state = format!(
        "{matched} of {} fixture digests match MANIFEST.txt, {differing} differ; {unavailable} digests NOT AVAILABLE",
        REPO_FIXTURES.len().saturating_add(1)
    );
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn render_hashes(rows: &[HashRow], state: &str) -> Vec<String> {
    if rows.is_empty() {
        return vec![state.to_string()];
    }
    rows.iter()
        .map(|row| {
            let listed = row.manifest.as_deref().map_or_else(
                || "no manifest line".to_string(),
                |digest| format!("manifest {}", code(digest)),
            );
            format!(
                "{}: {listed}, computed {}",
                code(&row.path),
                code(&row.computed)
            )
        })
        .collect()
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

/// The report subsection of one case, with every per-case field.
fn render_case(report: &CaseReport) -> String {
    let case = &report.case;
    let mut out = vec![
        format!("### {} — {}", case.id, case.test),
        String::new(),
        format!("- **Case:** {}", case.description),
        format!("- **Actual test name:** {}", code(case.test)),
        format!("- **Status:** {}", report.status),
        format!(
            "- **First difference:** {}",
            code(&report.first_difference())
        ),
        format!(
            "- **Predicted contracts and branches:** {} ({})",
            case.predicted, case.predicted_note
        ),
        format!("- **Actual branch:** {}", report.actual_branch),
    ];
    if report.missing.is_empty() {
        out.push(format!(
            "- **Readiness:** READY: all {} capture files and {} fixture files present",
            CAPTURE_FILES.len(),
            REPO_FIXTURES.len().saturating_add(2)
        ));
    } else {
        let missing: Vec<String> = report.missing.iter().map(|rel| code(rel)).collect();
        bullet_list(
            &mut out,
            &format!(
                "Readiness: NOT READY, {} required files missing",
                missing.len()
            ),
            &missing,
        );
    }
    out.push(format!("- **Inputs:** {}", report.inputs));
    bullet_list(
        &mut out,
        "Fixture hashes",
        &render_hashes(&report.fixture_hashes, &report.hashes_state),
    );
    bullet_list(
        &mut out,
        "Capture hashes",
        &render_hashes(&report.capture_hashes, &report.hashes_state),
    );
    out.push(format!("- **Hash summary:** {}", report.hashes_state));
    out.push(format!("- **Meta:** {}", report.meta));
    out.push(format!(
        "- **Context:** expected (`STUB_CONTEXT`) {}; {}",
        render_stub_context(),
        report.context
    ));
    bullet_list(&mut out, "Normalisation rules", &report.normalisation);
    out.push(format!(
        "- **Expected returned:** {}",
        report.expected_returned
    ));
    out.push(format!("- **Actual returned:** {}", report.actual_returned));
    out.push(format!(
        "- **Returned comparison:** {}",
        report.returned_comparison
    ));
    out.push(format!(
        "- **Transcript comparison:** {}",
        report.transcript_comparison
    ));
    out.push(format!(
        "- **Baseline (db-before.zwr against the fixture store):** {}",
        report.baseline
    ));
    out.push(format!("- **Snapshot identity:** {}", report.snapshot));
    out.push(format!(
        "- **Original side (db-before.zwr against db-after.zwr):** {}",
        report.original_side
    ));
    out.push(format!("- **Rust side:** {}", report.rust_side));
    let failures: Vec<String> = report
        .failures
        .iter()
        .map(|finding| code(finding))
        .collect();
    bullet_list(&mut out, "Failures", &failures);
    if !report.blocked.is_empty() {
        let blocked: Vec<String> = report.blocked.iter().map(|finding| code(finding)).collect();
        bullet_list(&mut out, "Blocked", &blocked);
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

/// The comparison-section body and the errors that must fail the report
/// test after it is written.
fn render_body(reports: &[CaseReport]) -> (String, Vec<String>) {
    let mut errors = Vec::new();
    let count = |status: Status| {
        reports
            .iter()
            .filter(|report| report.status == status)
            .count()
    };
    let mut totals = format!(
        "PASS {}, FAIL {}, NOT COMPARED {}",
        count(Status::Pass),
        count(Status::Fail),
        count(Status::NotCompared)
    );
    let blocked = count(Status::Blocked);
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
    blocks.extend(reports.iter().map(render_case));
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

/// Writes `body` between the comparison markers of the report at `path`,
/// creating the file with the markers when it is absent. Every byte outside
/// the markers is kept; with missing or out-of-order markers, or a body that
/// would itself hold a marker, the file is left untouched.
fn write_comparison(path: &Path, body: &str) -> Result<(), String> {
    if body.contains(BEGIN_MARKER) || body.contains(END_MARKER) {
        return Err(format!(
            "the comparison body holds a marker; {} left untouched",
            path.display()
        ));
    }
    let existing = match fs::read(path) {
        Ok(bytes) => Some(
            String::from_utf8(bytes)
                .map_err(|_| format!("{} is not UTF-8; left untouched", path.display()))?,
        ),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(format!("cannot read {}: {err}", path.display())),
    };
    let text = match existing {
        None => format!("# REPORT\n\n## Comparison\n\n{BEGIN_MARKER}\n{body}\n{END_MARKER}\n"),
        Some(existing) => {
            let bad = || {
                format!(
                    "{} has missing or out-of-order comparison markers; left untouched",
                    path.display()
                )
            };
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
    fs::write(path, text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Evaluates one case, prints its one-line summary, and fails only on FAIL.
fn run_case(case: &Case) {
    let report = evaluate_case(case);
    let difference = report.first_difference();
    println!("{} {}: {} {difference}", case.id, case.test, report.status);
    assert!(report.status != Status::Fail, "{}: {difference}", case.id);
}

#[test]
fn gc_01_standard_order() {
    run_case(&GC_01);
}

#[test]
fn gc_02_multiple_lines_and_decimals() {
    run_case(&GC_02);
}

#[test]
fn gc_03_missing_order() {
    run_case(&GC_03);
}

#[test]
fn gc_04_invalid_input() {
    run_case(&GC_04);
}

#[test]
fn gc_05_cancelled_order() {
    run_case(&GC_05);
}

/// Evaluates all five cases and writes the comparison section of
/// `REPORT.md`. It asserts only on its own errors, never on case statuses,
/// which the per-case tests decide.
#[test]
fn comparison_report() {
    let reports: Vec<CaseReport> = CASES.iter().map(evaluate_case).collect();
    let (body, mut errors) = render_body(&reports);
    if let Err(err) = write_comparison(&crate_path("REPORT.md"), &body) {
        errors.push(err);
    }
    assert!(
        errors.is_empty(),
        "comparison_report: {}",
        errors.join("; ")
    );
}
