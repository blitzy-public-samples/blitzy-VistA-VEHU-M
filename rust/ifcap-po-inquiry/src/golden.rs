//! The golden captures of OPTION `PRCH DISPLAY PO` (`EN3^PRCHRPT`) recorded
//! on `worldvista/vehu:202609`, and the only reader of their files (AAP
//! 0.3.2; conventions restated in `tests/golden/ifcap/README.md`).
//!
//! Each case `GC-01` to `GC-05` is one folder holding the seven
//! [`CAPTURE_FILES`], every line ended by `\n`:
//!
//! - `input.txt`: the typed inputs, one per line ([`split_input_lines`]);
//! - `transcript.txt`: the original's raw standard output, compared only
//!   after the case's normalisation rules ([`normalise`]);
//! - `returned.txt`: the entry point's return value, zero bytes meaning none
//!   ([`read_returned`]). `EN3^PRCHRPT` is entered by `DO` and returns
//!   nothing, so this workflow always records zero bytes;
//! - `db-before.zwr` and `db-after.zwr`: the export scope immediately before
//!   and after the session, which must be byte-identical;
//! - `meta.txt`: `key=value` lines with the [`META_KEYS`] ([`parse_meta`]).
//!   `context` holds exactly the four [`STUB_CONTEXT`] keys as a flat JSON
//!   object of strings ([`validate_context`]); `docker_exec_args` is split on
//!   single spaces ([`Meta::docker_exec_args`]);
//! - `normalisation.txt`: the case's normalisation rules ([`normalise`]).
//!
//! Every call reads the capture files afresh, nothing is cached, and each
//! file is read within a size budget that is enforced while it is read:
//! 1 MiB (1048576 bytes) for `input.txt` and `transcript.txt`, 64 KiB (65536)
//! for `returned.txt`, `meta.txt` and `normalisation.txt`, and 64 MiB
//! (67108864) for each export. A larger file is present but unreadable,
//! with the cause `larger than <n> bytes`. Only the files a caller parses
//! or returns are held; the two exports are read in lockstep in 64 KiB
//! pieces, compared and checked piece by piece, and never held whole. A case
//! folder whose metadata cannot be read, as under a golden folder the reader
//! may not search, is never taken for absent: it is unreadable, with the
//! folder's path and the cause.
//!
//! The bridge, its binary and both integration tests obtain capture content,
//! readiness and status only through this module, and none of them
//! re-implements a check. Nothing here writes a file. Golden content judges
//! the port and never derives a rule: the inquiry, store and M model never
//! reference this module.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::inquiry::STUB_CONTEXT;

/// The five case ids, which are also their folder names, in the order every
/// walk over the captures takes (AAP 0.3.2).
pub const CASE_IDS: [&str; 5] = ["GC-01", "GC-02", "GC-03", "GC-04", "GC-05"];

const INPUT: &str = "input.txt";
const TRANSCRIPT: &str = "transcript.txt";
const RETURNED: &str = "returned.txt";
const DB_BEFORE: &str = "db-before.zwr";
const DB_AFTER: &str = "db-after.zwr";
const META: &str = "meta.txt";
const NORMALISATION: &str = "normalisation.txt";

/// The seven files of one case folder, by the request capture convention
/// (AAP 0.3.2).
pub const CAPTURE_FILES: [&str; 7] = [
    INPUT,
    TRANSCRIPT,
    RETURNED,
    DB_BEFORE,
    DB_AFTER,
    META,
    NORMALISATION,
];

/// The keys of `meta.txt`, by the request capture convention (AAP 0.3.2).
/// [`parse_meta`] accepts no other key, and the `meta_keys_present` check
/// requires all eight.
pub const META_KEYS: [&str; 8] = [
    "image_digest",
    "source_revision",
    "snapshot_id",
    "record_ids",
    "entry_point",
    "context",
    "setup",
    "docker_exec_args",
];

/// The input that ends a session, `SPEC.md` "Exit input": `^` at the order
/// prompt ends the lookup [DIC11.m:23] [DIC.m:45]. Every `input.txt` ends
/// with it. `frontend/app.js` holds the one other copy, because the
/// front-end cannot import Rust.
pub const EXIT_INPUT: &str = "^";

const SEVEN_FILES_PRESENT: &str = "seven_files_present";
const META_KEYS_PRESENT: &str = "meta_keys_present";
const INPUT_ENDS_WITH_EXIT: &str = "input_ends_with_exit";
const NORMALISATION_PARSES: &str = "normalisation_parses";
const DB_BEFORE_EQUALS_AFTER: &str = "db_before_equals_after";
const NO_CARRIAGE_RETURN: &str = "no_carriage_return";

/// The six readiness checks, in evaluation and report order (AAP 0.3.2). A
/// check passes only when the files it reads exist and satisfy it:
///
/// - `seven_files_present`: each of the seven [`CAPTURE_FILES`] exists as a
///   regular file in the case folder;
/// - `meta_keys_present`: `meta.txt` is UTF-8, [`parse_meta`] accepts it and
///   every one of the eight [`META_KEYS`] is present;
/// - `input_ends_with_exit`: `input.txt` is UTF-8 and the last of its
///   [`split_input_lines`] is [`EXIT_INPUT`];
/// - `normalisation_parses`: `normalisation.txt` is UTF-8 and
///   [`parse_normalisation`] accepts it (an empty file holds no rules);
/// - `db_before_equals_after`: `db-before.zwr` and `db-after.zwr` are both
///   readable within their budgets and byte-identical;
/// - `no_carriage_return`: no capture file that is present contains the byte
///   `\r`; a present file that cannot be read, or is larger than its budget,
///   fails this check.
pub const CHECKS: [&str; 6] = [
    SEVEN_FILES_PRESENT,
    META_KEYS_PRESENT,
    INPUT_ENDS_WITH_EXIT,
    NORMALISATION_PARSES,
    DB_BEFORE_EQUALS_AFTER,
    NO_CARRIAGE_RETURN,
];

/// What `validate_context` reports as the expected value of a
/// [`STUB_CONTEXT`] key whose constant is unresolved.
const UNRESOLVED: &str = "UNRESOLVED (O-01)";
/// What `validate_context` reports for a key one side lacks.
const ABSENT: &str = "ABSENT";

/// A capture's `context` members, key and value, in document order.
type Context = Vec<(String, String)>;

// ---------------------------------------------------------------------------
// Capture parsing
// ---------------------------------------------------------------------------

/// The inputs of an `input.txt`, by the request capture convention (AAP
/// 0.3.2): one per `\n`-terminated line. One trailing `\n` ends the last line
/// and adds no input; inner empty lines are empty inputs. `""` gives no
/// inputs and `"\n"` one empty input. A `\r` is part of its line, never
/// stripped.
pub fn split_input_lines(text: &str) -> Vec<String> {
    input_lines(text).map(str::to_string).collect()
}

/// The inputs of an `input.txt` exactly as [`split_input_lines`] gives them,
/// borrowed from `text` rather than copied, so that a recorded input
/// sequence can be compared with a request's, and its last input found,
/// without allocating.
fn input_lines(text: &str) -> impl DoubleEndedIterator<Item = &str> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    (!text.is_empty())
        .then(|| body.split('\n'))
        .into_iter()
        .flatten()
}

/// The return value a `returned.txt` records, by the request capture
/// convention (AAP 0.3.2): zero bytes are `None`; otherwise the text with one
/// trailing `\n` removed. This workflow returns nothing, so its captures
/// always hold zero bytes.
pub fn read_returned(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    Some(text.strip_suffix('\n').unwrap_or(text).to_string())
}

// ---------------------------------------------------------------------------
// Normalisation
// ---------------------------------------------------------------------------

/// One normalisation rule, `SPEC.md` "Normalisation": the span from `open`
/// through the next `close`, both literal and included, becomes `<NORM>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The open delimiter, literal and non-empty: the text before the tab of
    /// a `normalisation.txt` line ([`parse_normalisation`]).
    pub open: String,
    /// The close delimiter, literal and non-empty: the text after the tab of
    /// a `normalisation.txt` line ([`parse_normalisation`]).
    pub close: String,
}

/// Parses a `normalisation.txt` into its rules in file order, `SPEC.md`
/// "Normalisation".
///
/// Lines are split on `\n` and nothing is trimmed. A zero-length line is
/// blank and ignored. Every other line holds exactly one tab separating two
/// literal, non-empty delimiters, open then close; any other line is `Err`
/// naming its 1-based number. A `\r` before a line end belongs to the close
/// delimiter (and fails the `no_carriage_return` check).
pub fn parse_normalisation(text: &str) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut delimiters = line.split('\t');
        match (delimiters.next(), delimiters.next(), delimiters.next()) {
            (Some(open), Some(close), None) if !open.is_empty() && !close.is_empty() => {
                rules.push(Rule {
                    open: open.to_string(),
                    close: close.to_string(),
                });
            }
            _ => {
                return Err(format!(
                    "normalisation line {}: expected one tab between two non-empty delimiters",
                    index.saturating_add(1)
                ));
            }
        }
    }
    Ok(rules)
}

/// Applies `rules` to `text` in order, each to the result of the one before,
/// `SPEC.md` "Normalisation". COMPARE and the equivalence harness both use
/// this one function.
///
/// For each rule the text is scanned left to right: from each occurrence of
/// `open` through the next `close` after it, both delimiters included, the
/// span becomes `<NORM>`, and scanning resumes after the span. An `open` with
/// no later `close` leaves the rest of the text unchanged. A rule with an
/// empty delimiter, which [`parse_normalisation`] never produces, changes
/// nothing. No other transformation exists.
pub fn normalise(text: &str, rules: &[Rule]) -> String {
    let mut current = text.to_string();
    for rule in rules {
        current = apply_rule(&current, &rule.open, &rule.close);
    }
    current
}

fn apply_rule(text: &str, open: &str, close: &str) -> String {
    if open.is_empty() || close.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let Some((before, from_open)) = rest.split_at_checked(start) else {
            break;
        };
        let after_open = from_open.get(open.len()..).unwrap_or("");
        let Some(end) = after_open.find(close) else {
            break;
        };
        out.push_str(before);
        out.push_str("<NORM>");
        rest = after_open
            .get(end.saturating_add(close.len())..)
            .unwrap_or("");
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// meta.txt
// ---------------------------------------------------------------------------

/// One parsed `meta.txt`, by the request capture convention (AAP 0.3.2): its
/// `key=value` entries in document order, each key one of [`META_KEYS`] at
/// most once. A key may be missing; the `meta_keys_present` check reports
/// that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    entries: Vec<(String, String)>,
}

impl Meta {
    /// The value of `key` exactly as recorded, or `None` when the file lacks
    /// it; for example `image_digest` and `snapshot_id`, which `REPORT.md`
    /// shows per case (AAP 0.3.2).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The `context` value: a flat JSON object whose values are all strings,
    /// by the request capture convention (AAP 0.3.2), as its members in
    /// document order. Whitespace between tokens is allowed, and strings take
    /// the escapes `\" \\ \/ \b \f \n \r \t` and `\uXXXX`, surrogate pairs
    /// included, because a device's `IOF` may hold control characters. A
    /// missing key, malformed JSON, a value that is not a string, nesting or
    /// a repeated key is `Err` with a short cause. [`validate_context`]
    /// judges the members.
    pub fn context(&self) -> Result<Vec<(String, String)>, String> {
        let value = self
            .get("context")
            .ok_or_else(|| "missing key context".to_string())?;
        let members = parse_flat_object(value)
            .ok_or_else(|| "context: not a flat JSON object of strings".to_string())?;
        {
            let mut seen: HashSet<&str> = HashSet::with_capacity(members.len());
            for (key, _) in &members {
                if !seen.insert(key.as_str()) {
                    return Err(format!("context: repeated key {key}"));
                }
            }
        }
        Ok(members)
    }

    /// The `docker_exec_args` value: the arguments after
    /// `docker exec -i <container>`, split on single spaces by the request
    /// capture convention (AAP 0.3.2). A missing key, an empty value, or an
    /// empty argument from a leading, trailing or doubled space is `Err`.
    ///
    /// An argument that itself contains a space cannot be expressed, so the
    /// documented docker-vista start command `su - vehu -c '<script>'` does
    /// not fit this convention (open item O-07).
    pub fn docker_exec_args(&self) -> Result<Vec<String>, String> {
        let value = self
            .get("docker_exec_args")
            .ok_or_else(|| "missing key docker_exec_args".to_string())?;
        if value.is_empty() {
            return Err("docker_exec_args: empty".to_string());
        }
        let args: Vec<String> = value.split(' ').map(str::to_string).collect();
        if args.iter().any(String::is_empty) {
            return Err(
                "docker_exec_args: empty argument from a leading, trailing or doubled space"
                    .to_string(),
            );
        }
        Ok(args)
    }
}

/// Parses a `meta.txt`, by the request capture convention (AAP 0.3.2):
/// `key=value` lines split at the first `=`, keys from [`META_KEYS`].
/// Whitespace-only lines are blank and ignored, and values are kept exactly,
/// a `\r` included (it fails the `no_carriage_return` check). A non-blank
/// line without `=`, an unknown key or a repeated key is `Err` naming the
/// 1-based line. A missing key is not an error here: the
/// `meta_keys_present` check reports it.
pub fn parse_meta(text: &str) -> Result<Meta, String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index.saturating_add(1);
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("meta line {number}: no '='"));
        };
        if !META_KEYS.contains(&key) {
            return Err(format!("meta line {number}: unknown key {key}"));
        }
        if entries.iter().any(|(seen, _)| seen == key) {
            return Err(format!("meta line {number}: repeated key {key}"));
        }
        entries.push((key.to_string(), value.to_string()));
    }
    Ok(Meta { entries })
}

// ---------------------------------------------------------------------------
// Flat JSON object reader (private, minimal)
// ---------------------------------------------------------------------------

/// The members of `text`, which must be exactly one JSON object whose values
/// are all strings, with optional surrounding whitespace; keys repeat as the
/// text repeats them. `None` means anything else.
fn parse_flat_object(text: &str) -> Option<Vec<(String, String)>> {
    let mut pos = 0;
    skip_whitespace(text, &mut pos);
    eat(text, &mut pos, b'{')?;
    let mut members = Vec::new();
    skip_whitespace(text, &mut pos);
    if peek(text, pos) == Some(b'}') {
        pos = pos.saturating_add(1);
    } else {
        loop {
            skip_whitespace(text, &mut pos);
            let key = read_string(text, &mut pos)?;
            skip_whitespace(text, &mut pos);
            eat(text, &mut pos, b':')?;
            skip_whitespace(text, &mut pos);
            let value = read_string(text, &mut pos)?;
            members.push((key, value));
            skip_whitespace(text, &mut pos);
            match bump(text, &mut pos) {
                Some(b',') => {}
                Some(b'}') => break,
                _ => return None,
            }
        }
    }
    skip_whitespace(text, &mut pos);
    if pos == text.len() {
        Some(members)
    } else {
        None
    }
}

fn peek(text: &str, pos: usize) -> Option<u8> {
    text.as_bytes().get(pos).copied()
}

fn bump(text: &str, pos: &mut usize) -> Option<u8> {
    let byte = peek(text, *pos)?;
    *pos = pos.saturating_add(1);
    Some(byte)
}

fn eat(text: &str, pos: &mut usize, byte: u8) -> Option<()> {
    if bump(text, pos) == Some(byte) {
        Some(())
    } else {
        None
    }
}

fn skip_whitespace(text: &str, pos: &mut usize) {
    while matches!(peek(text, *pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *pos = pos.saturating_add(1);
    }
}

/// The string at `*pos`, unescaped, moving past it. A high-surrogate `\u`
/// escape must be followed by a low-surrogate one; an unpaired surrogate, an
/// unknown escape or an unescaped control character is malformed. Every
/// position sliced at is an ASCII `"` or `\`, so always a char boundary.
fn read_string(text: &str, pos: &mut usize) -> Option<String> {
    eat(text, pos, b'"')?;
    let mut out = String::new();
    let mut run_start = *pos;
    loop {
        match peek(text, *pos) {
            None => return None,
            Some(b'"') => {
                out.push_str(text.get(run_start..*pos)?);
                *pos = pos.saturating_add(1);
                return Some(out);
            }
            Some(b'\\') => {
                out.push_str(text.get(run_start..*pos)?);
                *pos = pos.saturating_add(1);
                let unescaped = match bump(text, pos) {
                    Some(b'"') => '"',
                    Some(b'\\') => '\\',
                    Some(b'/') => '/',
                    Some(b'b') => '\u{8}',
                    Some(b'f') => '\u{c}',
                    Some(b'n') => '\n',
                    Some(b'r') => '\r',
                    Some(b't') => '\t',
                    Some(b'u') => read_unicode_escape(text, pos)?,
                    _ => return None,
                };
                out.push(unescaped);
                run_start = *pos;
            }
            Some(byte) if byte < 0x20 => return None,
            Some(_) => *pos = pos.saturating_add(1),
        }
    }
}

/// The character of a `\u` escape whose `\u` is already consumed: four
/// hexadecimal digits, a surrogate pair being combined into one character.
fn read_unicode_escape(text: &str, pos: &mut usize) -> Option<char> {
    let first = read_hex4(text, pos)?;
    match first {
        0xD800..=0xDBFF => {
            eat(text, pos, b'\\')?;
            eat(text, pos, b'u')?;
            let second = read_hex4(text, pos)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return None;
            }
            char::from_u32(0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00))
        }
        0xDC00..=0xDFFF => None,
        _ => char::from_u32(first),
    }
}

/// Four hexadecimal digits at `*pos` as one code unit, moving past them.
fn read_hex4(text: &str, pos: &mut usize) -> Option<u32> {
    let mut code = 0u32;
    for _ in 0..4 {
        let digit = bump(text, pos).and_then(|byte| char::from(byte).to_digit(16))?;
        code = code * 16 + digit;
    }
    Some(code)
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Checks a capture's `meta.txt` `context` against [`STUB_CONTEXT`], the
/// fixed context of every Rust session (AAP 0.3.2; conflict K4).
///
/// Each `STUB_CONTEXT` key is taken in table order (`DUZ`, `IOF`, `IOSL`,
/// `STATION`). A key the capture lacks fails as
/// `CONTEXT MISMATCH <key>: expected <value>, actual ABSENT`, where the
/// expected value of a key whose constant is unresolved is
/// `UNRESOLVED (O-01)`. A resolved constant must equal every occurrence of
/// its key, else `CONTEXT MISMATCH <key>: expected <constant>, actual
/// <value>`. An unresolved constant (`IOF`, `IOSL` while open item O-01
/// stands) accepts any value. Then each capture key `STUB_CONTEXT` lacks
/// fails, in capture order, as `CONTEXT MISMATCH <key>: expected ABSENT,
/// actual <value>`. The first failure is returned. No capture value
/// overrides a constant.
///
/// Diagnostic values such as `IOM` and the HOME device's name belong in
/// `meta.txt` `setup`, never in `context`.
pub fn validate_context(context: &[(String, String)]) -> Result<(), String> {
    for (key, constant) in STUB_CONTEXT {
        let mut present = false;
        for (_, actual) in context.iter().filter(|(name, _)| name == key) {
            present = true;
            if let Some(expected) = constant
                && actual != expected
            {
                return Err(mismatch(key, expected, actual));
            }
        }
        if !present {
            return Err(mismatch(key, constant.unwrap_or(UNRESOLVED), ABSENT));
        }
    }
    for (key, actual) in context {
        if !STUB_CONTEXT.iter().any(|(name, _)| name == key) {
            return Err(mismatch(key, ABSENT, actual));
        }
    }
    Ok(())
}

fn mismatch(key: &str, expected: &str, actual: &str) -> String {
    format!("CONTEXT MISMATCH {key}: expected {expected}, actual {actual}")
}

// ---------------------------------------------------------------------------
// Case folder snapshot
// ---------------------------------------------------------------------------

/// The most bytes `input.txt` may hold: 1 MiB (1048576), the largest request
/// body the bridge accepts, so a longer `input.txt` could never equal a
/// request's inputs.
const INPUT_LIMIT: u64 = 1024 * 1024;
/// The most bytes `transcript.txt` may hold: 1 MiB (1048576), the live
/// adapter's cap on the original's standard output.
const TRANSCRIPT_LIMIT: u64 = 1024 * 1024;
/// The most bytes `returned.txt`, `meta.txt` and `normalisation.txt` may
/// each hold: 64 KiB (65536), far above what they record, which is zero
/// bytes for this workflow's return value, eight short `key=value` lines and
/// a case's few normalisation rules.
const TEXT_LIMIT: u64 = 64 * 1024;
/// The most bytes `db-before.zwr` and `db-after.zwr` may each hold: 64 MiB
/// (67108864), far above a legitimate export of the export scope (the
/// eleven fixture roots plus `^PRC(442)`, `^VA(200,.5)` and `^DIC(4,500)`),
/// which is about 1.2 MB. Neither export is ever held whole.
const EXPORT_LIMIT: u64 = 64 * 1024 * 1024;
/// The size of the pieces a capture file that is not kept is read in. Each
/// piece is filled fully except at end of file, so the two exports, read in
/// lockstep, give equal pieces exactly when they are equal.
const CHUNK: usize = 64 * 1024;

/// The most bytes the capture file `name` may hold, enforced while it is
/// read. A larger file is present but unreadable, so no file is ever held
/// beyond its budget.
fn size_limit(name: &str) -> u64 {
    match name {
        INPUT => INPUT_LIMIT,
        TRANSCRIPT => TRANSCRIPT_LIMIT,
        DB_BEFORE | DB_AFTER => EXPORT_LIMIT,
        // `returned.txt`, `meta.txt` and `normalisation.txt`.
        _ => TEXT_LIMIT,
    }
}

/// The cause of a capture file larger than `budget`, free of parentheses.
fn oversize(budget: u64) -> String {
    format!("larger than {budget} bytes")
}

/// What one capture file gave when read.
enum Content {
    /// Nothing exists at its path.
    Missing,
    /// Something exists at its path but could not be read whole within its
    /// budget ([`size_limit`]): the cause, such as `permission denied` or
    /// `larger than 65536 bytes`, free of parentheses so it can stand inside
    /// a `NOT RECORDED (unreadable: …)` body.
    Unreadable(String),
    /// Its bytes, kept because a check or a [`Case`] field parses them.
    Bytes(Vec<u8>),
    /// Read through to its end in [`CHUNK`]-byte pieces that were not kept:
    /// whether the whole file is UTF-8 and whether it holds a `\r`.
    Scanned { utf8: bool, carriage_return: bool },
}

/// One capture file of a case folder, read once.
struct CaptureFile {
    name: &'static str,
    path: PathBuf,
    /// It exists as a regular file, a symbolic link to one included.
    regular: bool,
    content: Content,
}

impl CaptureFile {
    /// Reads the capture file `name` of `case_dir` within its budget, keeping
    /// its bytes when `keep` holds and otherwise scanning them.
    fn read(case_dir: &Path, name: &'static str, keep: bool) -> CaptureFile {
        let path = case_dir.join(name);
        let (regular, content) = read_capture_file(&path, size_limit(name), keep);
        CaptureFile {
            name,
            path,
            regular,
            content,
        }
    }

    /// Why this file stops its case from loading, as `<path>: <cause>`: it
    /// is present but could not be read within its budget, or it is not
    /// UTF-8. Every capture file is text by convention, the ZWR exports
    /// included, so no part of such a case is loaded.
    fn failure(&self) -> Option<String> {
        let cause = match &self.content {
            Content::Missing => return None,
            Content::Unreadable(cause) => cause.as_str(),
            Content::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => "not UTF-8",
            Content::Scanned { utf8: false, .. } => "not UTF-8",
            Content::Bytes(_) | Content::Scanned { .. } => return None,
        };
        Some(format!("{}: {cause}", self.path.display()))
    }

    /// Whether something exists at its path, readable or not: a directory,
    /// a FIFO, or a file that cannot be read, is too large or is not UTF-8
    /// is present. Only a path whose metadata reports it missing
    /// (`NotFound`, as [`open_regular`] gives it) is not.
    fn present(&self) -> bool {
        !matches!(self.content, Content::Missing)
    }

    /// The file's text, when it was kept and is UTF-8.
    fn text(&self) -> Option<&str> {
        match &self.content {
            Content::Bytes(bytes) => std::str::from_utf8(bytes).ok(),
            Content::Missing | Content::Unreadable(_) | Content::Scanned { .. } => None,
        }
    }
}

/// Which capture files a [`Snapshot`] keeps whole, each within its budget.
/// Every other file is read through in [`CHUNK`]-byte pieces and dropped,
/// and the two exports always are.
#[derive(Clone, Copy)]
enum Keep {
    /// The files the readiness checks parse: `input.txt`, `meta.txt` and
    /// `normalisation.txt`. What [`readiness`] and [`status`] need.
    Parsed,
    /// Those and the files a [`Case`] holds besides, `transcript.txt` and
    /// `returned.txt`. What a loaded case needs.
    Loaded,
    /// None. The files are only validated, as for a case whose inputs
    /// [`find_capture`] does not match, and the checks are not evaluated.
    Nothing,
}

impl Keep {
    fn keeps(self, name: &str) -> bool {
        match self {
            Keep::Parsed => matches!(name, INPUT | META | NORMALISATION),
            Keep::Loaded => matches!(name, INPUT | TRANSCRIPT | RETURNED | META | NORMALISATION),
            Keep::Nothing => false,
        }
    }
}

/// The seven capture files of one case folder, each read once, so that the
/// readiness checks and the loaded content describe the same bytes. Only
/// the files its [`Keep`] names are held, each within its budget; the two
/// exports are compared while they are read and never held whole.
struct Snapshot {
    files: Vec<CaptureFile>,
    /// `db-before.zwr` and `db-after.zwr` are both regular files, both read
    /// to their ends within their budgets, and byte-identical.
    exports_equal: bool,
}

impl Snapshot {
    /// Reads the seven files of `case_dir` in [`CAPTURE_FILES`] order,
    /// keeping those `keep` names. `input`, when given, is the folder's
    /// `input.txt` as already read, which is then not read again.
    fn read(case_dir: &Path, keep: Keep, mut input: Option<CaptureFile>) -> Snapshot {
        let mut files = Vec::with_capacity(CAPTURE_FILES.len());
        let mut exports_equal = false;
        for name in CAPTURE_FILES {
            match name {
                INPUT => files.push(
                    input
                        .take()
                        .unwrap_or_else(|| CaptureFile::read(case_dir, INPUT, keep.keeps(INPUT))),
                ),
                DB_BEFORE => {
                    let (before, after, equal) = read_exports(case_dir);
                    files.push(before);
                    files.push(after);
                    exports_equal = equal;
                }
                // Read with `db-before.zwr`, which comes just before it.
                DB_AFTER => {}
                _ => files.push(CaptureFile::read(case_dir, name, keep.keeps(name))),
            }
        }
        Snapshot {
            files,
            exports_equal,
        }
    }

    fn file(&self, name: &str) -> Option<&CaptureFile> {
        self.files.iter().find(|file| file.name == name)
    }

    /// The file's text, when it was kept and is UTF-8.
    fn text(&self, name: &str) -> Option<&str> {
        self.file(name).and_then(CaptureFile::text)
    }

    fn passes(&self, check: &str) -> bool {
        match check {
            SEVEN_FILES_PRESENT => self.files.iter().all(|file| file.regular),
            META_KEYS_PRESENT => self
                .text(META)
                .and_then(|text| parse_meta(text).ok())
                .is_some_and(|meta| META_KEYS.iter().all(|key| meta.get(key).is_some())),
            INPUT_ENDS_WITH_EXIT => self
                .text(INPUT)
                .is_some_and(|text| input_lines(text).next_back() == Some(EXIT_INPUT)),
            NORMALISATION_PARSES => self
                .text(NORMALISATION)
                .is_some_and(|text| parse_normalisation(text).is_ok()),
            DB_BEFORE_EQUALS_AFTER => self.exports_equal,
            NO_CARRIAGE_RETURN => self.files.iter().all(|file| match &file.content {
                Content::Missing => true,
                Content::Unreadable(_) => false,
                Content::Bytes(bytes) => !bytes.contains(&b'\r'),
                Content::Scanned {
                    carriage_return, ..
                } => !carriage_return,
            }),
            // Every name in CHECKS has its arm above; any other name is not a
            // check this module knows, so it fails rather than passes.
            _ => false,
        }
    }

    /// The six [`CHECKS`] evaluated in order. The snapshot must keep at
    /// least the files [`Keep::Parsed`] names, which the checks parse.
    fn readiness(&self) -> Readiness {
        let failed: Vec<&'static str> = CHECKS
            .into_iter()
            .filter(|check| !self.passes(check))
            .collect();
        if failed.is_empty() {
            Readiness::Ready
        } else {
            Readiness::Incomplete(failed)
        }
    }

    /// Whether the case is recorded, by the request capture convention (AAP
    /// 0.3.2): its `input.txt` and `transcript.txt` both exist, readable or
    /// not ([`CaptureFile::present`]). A case that is not recorded is NOT
    /// COMPARED whatever its other files hold, so [`Snapshot::failure`]
    /// counts only for a recorded case.
    fn records(&self) -> bool {
        [INPUT, TRANSCRIPT]
            .into_iter()
            .all(|name| self.file(name).is_some_and(CaptureFile::present))
    }

    /// The first file, in [`CAPTURE_FILES`] order, that stops the case from
    /// loading ([`CaptureFile::failure`]), as `<path>: <cause>`.
    fn failure(&self) -> Option<String> {
        self.files.iter().find_map(CaptureFile::failure)
    }

    /// Moves the kept UTF-8 text of `name` out of the snapshot, without
    /// copying it, and leaves that file `Missing`. Only a snapshot whose
    /// checks, and [`Snapshot::records`] with [`Snapshot::failure`] for a
    /// recorded case, are already evaluated is emptied so.
    fn take_text(&mut self, name: &str) -> Option<String> {
        let file = self.files.iter_mut().find(|file| file.name == name)?;
        match std::mem::replace(&mut file.content, Content::Missing) {
            Content::Bytes(bytes) => String::from_utf8(bytes).ok(),
            Content::Missing | Content::Unreadable(_) | Content::Scanned { .. } => None,
        }
    }
}

/// Whether `path` is a regular file, and what reading it within `budget`
/// bytes gave: its bytes when `keep` holds, else a [`Content::Scanned`]
/// summary of them. A path whose metadata already shows something other
/// than a regular file, such as a FIFO or device in a case folder, is never
/// opened. Otherwise the open does not wait for a FIFO writer on a platform
/// whose `O_NONBLOCK` value is known, and the opened descriptor's own type
/// decides ([`admit`]), so an object swapped in after that inspection is
/// refused unread. The budget is enforced while reading, never by the
/// metadata length alone, so a file that grows meanwhile is still cut off.
/// Nothing is written.
fn read_capture_file(path: &Path, budget: u64, keep: bool) -> (bool, Content) {
    match open_regular(path) {
        Err(found) => found,
        Ok((file, length)) if keep => (true, read_kept(file, length, budget)),
        Ok((file, _)) => {
            let mut chunk = vec![0u8; CHUNK];
            (true, Stream::new(file, budget).finish(&mut chunk))
        }
    }
}

/// Opens `path` for reading once its metadata, links followed, shows a
/// regular file, and keeps what was opened only as [`admit`] does. `Err` is
/// what [`read_capture_file`] reports for anything else: whether a regular
/// file exists, and the content.
fn open_regular(path: &Path) -> Result<(File, u64), (bool, Content)> {
    let entry = match fs::metadata(path) {
        Ok(entry) => entry,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err((false, Content::Missing)),
        Err(e) => return Err((false, Content::Unreadable(io_cause(&e)))),
    };
    if !entry.is_file() {
        return Err((false, Content::Unreadable("not a regular file".to_string())));
    }
    admit(path)
}

/// Opens `path` without waiting ([`open_read_only`]) and keeps the opened
/// file, with the length its own metadata gives, only when that metadata
/// shows a regular file: whatever stood at `path` when it was inspected, an
/// object that is not a regular file once opened is refused unread, as
/// `not a regular file`. Nothing at `path` is `Missing`. A failed open, or
/// a failed read of the opened file's metadata, is unreadable with its
/// cause and counts as a regular file, which the inspection saw.
fn admit(path: &Path) -> Result<(File, u64), (bool, Content)> {
    let file = match open_read_only(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err((false, Content::Missing)),
        Err(e) => return Err((true, Content::Unreadable(io_cause(&e)))),
    };
    match file.metadata() {
        Ok(opened) if opened.is_file() => Ok((file, opened.len())),
        Ok(_) => Err((false, Content::Unreadable("not a regular file".to_string()))),
        Err(e) => Err((true, Content::Unreadable(io_cause(&e)))),
    }
}

/// Opens `path` for reading only, requesting no write, append, create or
/// truncate access, and with `O_NONBLOCK` on Unix, so that opening a FIFO
/// does not wait for a writer where that flag's value is known.
fn open_read_only(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::prelude::*;
        options.custom_flags(O_NONBLOCK);
    }
    options.open(path)
}

/// The `O_NONBLOCK` open flag, the platform's `<fcntl.h>` value, which std
/// does not export: opening a FIFO with it returns at once instead of
/// waiting for a writer, and reads of a regular file ignore it. It is 0, a
/// plain read-only open, on a platform whose value is not known here.
#[cfg(unix)]
const O_NONBLOCK: i32 = if cfg!(all(
    any(target_os = "linux", target_os = "android"),
    not(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "mips32r6",
        target_arch = "mips64r6",
        target_arch = "sparc",
        target_arch = "sparc64"
    ))
)) {
    0o4000
} else if cfg!(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)) {
    0x0004
} else {
    0
};

/// The bytes of the open capture file `file`, read to its end through a
/// limit of one byte past `budget`, so that a file over budget is detected
/// having read no more than that. `length`, its metadata length, only sizes
/// the buffer. `read_to_end` retries an interrupted read.
fn read_kept(file: File, length: u64, budget: u64) -> Content {
    let limit = budget.saturating_add(1);
    let mut bytes = Vec::with_capacity(usize::try_from(length.min(limit)).unwrap_or_default());
    match file.take(limit).read_to_end(&mut bytes) {
        Err(e) => Content::Unreadable(io_cause(&e)),
        Ok(_) if exceeds(bytes.len(), budget) => Content::Unreadable(oversize(budget)),
        Ok(_) => Content::Bytes(bytes),
    }
}

/// Whether `count` bytes are more than `budget`.
fn exceeds(count: usize, budget: u64) -> bool {
    !u64::try_from(count).is_ok_and(|count| count <= budget)
}

/// Reads from `reader` into `chunk` until it is full or the reader is at its
/// end, retrying a read that was interrupted. The count read is below the
/// chunk's length only at end of file.
fn fill(reader: &mut impl Read, chunk: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while let Some(rest) = chunk.get_mut(filled..).filter(|rest| !rest.is_empty()) {
        match reader.read(rest) {
            Ok(0) => break,
            Ok(count) => filled = filled.saturating_add(count),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// An open capture file read through to its end in [`CHUNK`]-byte pieces
/// that are not kept, each filled fully except at end of file ([`fill`]),
/// within a budget enforced while reading: its UTF-8 validity and whether it
/// holds a `\r` are tracked piece by piece.
struct Stream {
    /// The file, limited to one byte past the budget.
    reader: io::Take<File>,
    budget: u64,
    /// The bytes read so far.
    read: u64,
    utf8: Utf8Check,
    carriage_return: bool,
    /// `None` while more may follow, `Some(Ok(()))` at end of file, and
    /// `Some(Err(cause))` once a read failed or passed the budget.
    end: Option<Result<(), String>>,
}

impl Stream {
    fn new(file: File, budget: u64) -> Stream {
        Stream {
            reader: file.take(budget.saturating_add(1)),
            budget,
            read: 0,
            utf8: Utf8Check::default(),
            carriage_return: false,
            end: None,
        }
    }

    /// The next piece, read into `chunk`, or `None` once the stream has
    /// ended, at end of file or on a failure.
    fn next_piece<'a>(&mut self, chunk: &'a mut [u8]) -> Option<&'a [u8]> {
        if self.end.is_some() {
            return None;
        }
        let count = match fill(&mut self.reader, chunk) {
            Ok(count) => count,
            Err(e) => {
                self.end = Some(Err(io_cause(&e)));
                return None;
            }
        };
        self.read = self
            .read
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        if self.read > self.budget {
            self.end = Some(Err(oversize(self.budget)));
            return None;
        }
        let chunk: &'a [u8] = chunk;
        let piece = chunk.get(..count).unwrap_or_default();
        if piece.is_empty() {
            self.end = Some(Ok(()));
            return None;
        }
        self.utf8.feed(piece);
        self.carriage_return = self.carriage_return || piece.contains(&b'\r');
        Some(piece)
    }

    /// Whether the stream reached end of file within its budget.
    fn ended_whole(&self) -> bool {
        matches!(self.end, Some(Ok(())))
    }

    /// What the whole file was, once the rest of it is read into `chunk`:
    /// `Unreadable` with the cause of a failed read or of passing the
    /// budget, else `Scanned`.
    fn finish(mut self, chunk: &mut [u8]) -> Content {
        while self.next_piece(chunk).is_some() {}
        match self.end {
            Some(Err(cause)) => Content::Unreadable(cause),
            Some(Ok(())) | None => Content::Scanned {
                utf8: self.utf8.is_valid(),
                carriage_return: self.carriage_return,
            },
        }
    }
}

/// The UTF-8 validity of a byte stream fed in pieces, the same as
/// `std::str::from_utf8` of the whole: a sequence the end of one piece cuts,
/// at most three bytes, is carried into the next.
#[derive(Default)]
struct Utf8Check {
    /// The start of the sequence the last piece ended inside.
    carry: Vec<u8>,
    invalid: bool,
}

impl Utf8Check {
    fn feed(&mut self, piece: &[u8]) {
        if self.invalid {
            return;
        }
        let mut rest = piece;
        // Complete the carried sequence one byte at a time: it either becomes
        // a character, which empties the carry, or proves invalid.
        while !self.carry.is_empty() {
            let Some((&byte, after)) = rest.split_first() else {
                return;
            };
            rest = after;
            self.carry.push(byte);
            match std::str::from_utf8(&self.carry) {
                Ok(_) => self.carry.clear(),
                Err(e) if e.error_len().is_some() => {
                    self.invalid = true;
                    return;
                }
                Err(_) => {}
            }
        }
        if let Err(e) = std::str::from_utf8(rest) {
            match e.error_len() {
                Some(_) => self.invalid = true,
                None => self
                    .carry
                    .extend_from_slice(rest.get(e.valid_up_to()..).unwrap_or_default()),
            }
        }
    }

    /// Whether every byte fed so far forms UTF-8, no sequence left open.
    fn is_valid(&self) -> bool {
        !self.invalid && self.carry.is_empty()
    }
}

/// `db-before.zwr` and `db-after.zwr` of `case_dir`, read through together
/// in [`CHUNK`]-byte pieces and never held whole, and whether they are
/// byte-identical: both regular, both read to their ends within
/// [`EXPORT_LIMIT`], and equal piece for piece. A file that cannot be
/// opened, fails a read or passes its budget is unreadable and makes them
/// unequal, and the other is still read to its end, so that its own UTF-8
/// and `\r` state is known.
fn read_exports(case_dir: &Path) -> (CaptureFile, CaptureFile, bool) {
    let before_path = case_dir.join(DB_BEFORE);
    let after_path = case_dir.join(DB_AFTER);
    let mut before_chunk = vec![0u8; CHUNK];
    let mut after_chunk = vec![0u8; CHUNK];
    let alone = |opened: Result<(File, u64), (bool, Content)>, chunk: &mut [u8]| match opened {
        Ok((file, _)) => (true, Stream::new(file, EXPORT_LIMIT).finish(chunk)),
        Err(found) => found,
    };
    let (before, after, equal) = match (open_regular(&before_path), open_regular(&after_path)) {
        (Ok((before, _)), Ok((after, _))) => {
            let mut before = Stream::new(before, EXPORT_LIMIT);
            let mut after = Stream::new(after, EXPORT_LIMIT);
            let mut equal = true;
            loop {
                let before_piece = before.next_piece(&mut before_chunk);
                let after_piece = after.next_piece(&mut after_chunk);
                if before_piece.is_none() && after_piece.is_none() {
                    break;
                }
                equal = equal && before_piece == after_piece;
            }
            let equal = equal && before.ended_whole() && after.ended_whole();
            (
                (true, before.finish(&mut before_chunk)),
                (true, after.finish(&mut after_chunk)),
                equal,
            )
        }
        (before, after) => (
            alone(before, &mut before_chunk),
            alone(after, &mut after_chunk),
            false,
        ),
    };
    let ((before_regular, before_content), (after_regular, after_content)) = (before, after);
    (
        CaptureFile {
            name: DB_BEFORE,
            path: before_path,
            regular: before_regular,
            content: before_content,
        },
        CaptureFile {
            name: DB_AFTER,
            path: after_path,
            regular: after_regular,
            content: after_content,
        },
        equal,
    )
}

/// The cause of a failed read as its `io::ErrorKind` description, such as
/// `permission denied`, cut before any parenthesised aside and trimmed, so
/// that `filesystem loop or indirection limit (e.g. symlink loop)` gives
/// `filesystem loop or indirection limit`. The `io::Error` text itself ends
/// in `(os error N)`, and a cause must stay free of parentheses because the
/// bridge sets it inside `NOT RECORDED (unreadable: <cause>)`.
fn io_cause(error: &io::Error) -> String {
    let description = error.kind().to_string();
    let head = description.split('(').next().unwrap_or_default().trim();
    if head.is_empty() {
        "input/output error".to_string()
    } else {
        head.to_string()
    }
}

/// What the metadata of a case folder's own path shows, by the request
/// capture convention (AAP 0.3.2: `Absent` means the folder does not exist).
enum Folder {
    /// A directory exists at the path.
    Present,
    /// Nothing exists at the path, or what exists is not a directory: the
    /// path's metadata reports it missing (`NotFound`), a file stands in
    /// place of one of its folders (`NotADirectory`, as when the golden
    /// folder is a file), or it is a file itself.
    Absent,
    /// The path's metadata cannot be read for any other reason, such as a
    /// golden folder the reader may not search or a symbolic-link loop, so
    /// whether the folder exists is unknown: `<case_dir>: <cause>`, the
    /// cause free of parentheses as [`io_cause`] gives it.
    Uninspectable(String),
}

/// Probes the case folder `case_dir` without reading it. A metadata error is
/// never taken for absence unless it says the path does not exist.
fn probe_folder(case_dir: &Path) -> Folder {
    match fs::metadata(case_dir) {
        Ok(entry) if entry.is_dir() => Folder::Present,
        Ok(_) => Folder::Absent,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Folder::Absent
        }
        Err(e) => Folder::Uninspectable(format!("{}: {}", case_dir.display(), io_cause(&e))),
    }
}

/// Whether something exists at `path`, readable or not, by the rule
/// [`open_regular`] applies: only metadata that reports the path missing
/// (`NotFound`) means nothing is there.
fn exists(path: &Path) -> bool {
    !fs::metadata(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
}

// ---------------------------------------------------------------------------
// Readiness and status
// ---------------------------------------------------------------------------

/// How far one case folder meets the capture conventions (AAP 0.3.2; verdict
/// terms of AAP 0.7.2): `Absent` when the folder does not exist, `Ready`
/// when all six [`CHECKS`] pass, else `Incomplete` with each failed check's
/// name, in [`CHECKS`] order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// The case folder does not exist (AAP 0.3.2): nothing, or something
    /// other than a directory, is at its path.
    Absent,
    /// INCOMPLETE: the names of the failed [`CHECKS`], in that order.
    Incomplete(Vec<&'static str>),
    /// READY: all six [`CHECKS`] pass.
    Ready,
}

/// The [`Readiness`] of the case folder `case_dir` (AAP 0.3.2): `Absent`
/// when nothing, or something other than a directory, exists at its path;
/// otherwise the six [`CHECKS`] evaluated in order over its files, read
/// once within their budgets, where only `input.txt`, `meta.txt` and
/// `normalisation.txt` are held and the others are read through in pieces
/// and dropped. A folder whose metadata cannot be read for another reason,
/// such as a golden folder the reader may not search, is not taken for
/// absent: none of its files can be confirmed, so it fails all six checks,
/// in order.
pub fn readiness(case_dir: &Path) -> Readiness {
    match probe_folder(case_dir) {
        Folder::Absent => Readiness::Absent,
        Folder::Uninspectable(_) => Readiness::Incomplete(CHECKS.to_vec()),
        Folder::Present => Snapshot::read(case_dir, Keep::Parsed, None).readiness(),
    }
}

/// Whether the case folder `case_dir` is recorded, by the request capture
/// convention (AAP 0.3.2): its `input.txt` and `transcript.txt` both exist,
/// readable or not, so a directory at either path counts. A case folder
/// whose metadata cannot be read is recorded too, since its capture cannot
/// be confirmed absent. These are exactly the cases [`load_case`] finds
/// `Recorded` or `Unreadable`, which are PASS or FAIL; a case that is not
/// recorded is NOT COMPARED.
pub fn is_recorded(case_dir: &Path) -> bool {
    match probe_folder(case_dir) {
        Folder::Absent => false,
        Folder::Uninspectable(_) => true,
        Folder::Present => [INPUT, TRANSCRIPT]
            .into_iter()
            .all(|name| exists(&case_dir.join(name))),
    }
}

/// The capture state of the five cases (AAP 0.3.2), each list in
/// [`CASE_IDS`] order: the recorded ids, the READY ids, and each INCOMPLETE
/// id with its failed checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The ids [`is_recorded`] accepts, in [`CASE_IDS`] order.
    pub recorded: Vec<&'static str>,
    /// The ids whose [`readiness`] is `Ready`, in [`CASE_IDS`] order.
    pub ready: Vec<&'static str>,
    /// Each id whose [`readiness`] is `Incomplete`, with its failed
    /// [`CHECKS`], in [`CASE_IDS`] order.
    pub incomplete: Vec<(&'static str, Vec<&'static str>)>,
}

/// The [`Status`] of the golden folder `golden_dir`, read afresh on every
/// call (AAP 0.3.2): what `GET /api/status` reports and what the bridge
/// binary counts as `RECORDED <n> OF 5 READY <m> OF 5`. A missing folder
/// gives three empty lists. The recorded ids are those [`is_recorded`]
/// accepts, the cases [`load_case`] finds `Recorded` or `Unreadable`, so a
/// case folder whose metadata cannot be read, as in a golden folder that
/// cannot be searched, is recorded and INCOMPLETE with all six checks, as
/// [`readiness`] gives it. Only the files the checks parse are held, each
/// within its budget; the others are read through in pieces and dropped.
pub fn status(golden_dir: &Path) -> Status {
    let mut status = Status {
        recorded: Vec::new(),
        ready: Vec::new(),
        incomplete: Vec::new(),
    };
    for id in CASE_IDS {
        let case_dir = golden_dir.join(id);
        if is_recorded(&case_dir) {
            status.recorded.push(id);
        }
        match readiness(&case_dir) {
            Readiness::Absent => {}
            Readiness::Incomplete(failed) => status.incomplete.push((id, failed)),
            Readiness::Ready => status.ready.push(id),
        }
    }
    status
}

// ---------------------------------------------------------------------------
// Per-case loading
// ---------------------------------------------------------------------------

/// One recorded case's content, by the request capture convention (AAP
/// 0.3.2). A missing or unparsable optional file leaves its field empty and
/// shows only as a failed check in `readiness`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The case id, one of [`CASE_IDS`], which is also its folder name.
    pub id: String,
    /// [`split_input_lines`] of `input.txt`.
    pub inputs: Vec<String>,
    /// Raw `transcript.txt` (request capture convention, AAP 0.3.2), never
    /// normalised here: comparisons apply `rules` through [`normalise`].
    pub transcript: String,
    /// [`read_returned`] of `returned.txt`; `None` when it is absent.
    pub returned: Option<String>,
    /// [`parse_normalisation`] of `normalisation.txt`, `SPEC.md`
    /// "Normalisation"; none when it is absent or does not parse.
    pub rules: Vec<Rule>,
    /// [`parse_meta`] of `meta.txt`, when it is present and parses.
    pub meta: Option<Meta>,
    /// The case's [`readiness`], from the same reading as its content.
    pub readiness: Readiness,
}

/// What [`load_case`] found for one case id (AAP 0.3.2; verdict terms of AAP
/// 0.7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseLoad {
    /// The case folder does not exist (AAP 0.3.2): nothing, or something
    /// other than a directory, is at its path. NOT COMPARED.
    Absent,
    /// The folder exists but `input.txt` or `transcript.txt` is missing,
    /// whatever the other files hold, readable or not: NOT COMPARED. The
    /// failed [`CHECKS`] are carried for the readiness report.
    NotRecorded(Vec<&'static str>),
    /// `input.txt` and `transcript.txt` present, every present capture file
    /// readable and UTF-8 (AAP 0.3.2): PASS or FAIL by checks and comparison.
    Recorded(Case),
    /// The case is recorded ([`is_recorded`]), its `input.txt` and
    /// `transcript.txt` both present, but a capture file that is present
    /// cannot be read or is not UTF-8; or the case folder's own metadata
    /// cannot be read, so that whether it exists is unknown: that file's or
    /// folder's path, `: ` and the cause, free of parentheses. The case is
    /// FAIL.
    Unreadable(String),
}

/// Loads case `case_id` from `golden_dir/case_id`, the one way any consumer
/// obtains a case's content (AAP 0.3.2), read afresh on every call.
///
/// The case folder is probed first: nothing, or something other than a
/// directory, at its path is `Absent`, and a folder whose metadata cannot
/// be read for another reason is `Unreadable` with the folder's path and
/// cause. Then each of the seven files is read once within its budget, the
/// two exports in pieces that are compared and checked but never held
/// whole. A case missing `input.txt` or `transcript.txt` is `NotRecorded`,
/// whatever its other files hold. Of any other case, every file present
/// must be readable within its budget and UTF-8, else the first that is
/// not, in [`CAPTURE_FILES`] order, makes the case `Unreadable`; a case
/// whose files all are is `Recorded`.
pub fn load_case(golden_dir: &Path, case_id: &str) -> CaseLoad {
    let case_dir = golden_dir.join(case_id);
    match probe_folder(&case_dir) {
        Folder::Absent => CaseLoad::Absent,
        Folder::Uninspectable(cause) => CaseLoad::Unreadable(cause),
        Folder::Present => {
            let input = CaptureFile::read(&case_dir, INPUT, true);
            hydrate(case_id, &case_dir, input)
        }
    }
}

/// The case `case_id` loaded from its existing folder `case_dir`, the one
/// reading of a whole case that [`load_case`] and [`find_capture`] share.
/// `input` is the folder's `input.txt` as already read and kept, which is
/// not read again. The other files are read in [`CAPTURE_FILES`] order,
/// keeping what a [`Case`] holds. A case missing `input.txt` or
/// `transcript.txt` is `NotRecorded` before any file is judged unreadable;
/// of a recorded case, the first file that fails makes it `Unreadable`.
/// The texts of a loaded case then move into it uncopied.
fn hydrate(case_id: &str, case_dir: &Path, input: CaptureFile) -> CaseLoad {
    let mut snapshot = Snapshot::read(case_dir, Keep::Loaded, Some(input));
    if snapshot.records()
        && let Some(cause) = snapshot.failure()
    {
        return CaseLoad::Unreadable(cause);
    }
    let readiness = snapshot.readiness();
    // Both texts of a recorded case are held and UTF-8 by now, so the `else`
    // is taken only for a case that is not recorded, whichever of its input
    // and transcript is present then being readable or not.
    let (Some(input), Some(transcript)) =
        (snapshot.take_text(INPUT), snapshot.take_text(TRANSCRIPT))
    else {
        // A missing input or transcript fails `seven_files_present`, so the
        // readiness is always `Incomplete` here.
        let failed = match readiness {
            Readiness::Incomplete(failed) => failed,
            Readiness::Absent | Readiness::Ready => Vec::new(),
        };
        return CaseLoad::NotRecorded(failed);
    };
    CaseLoad::Recorded(Case {
        id: case_id.to_string(),
        inputs: split_input_lines(&input),
        transcript,
        returned: snapshot
            .take_text(RETURNED)
            .as_deref()
            .and_then(read_returned),
        rules: snapshot
            .take_text(NORMALISATION)
            .and_then(|text| parse_normalisation(&text).ok())
            .unwrap_or_default(),
        meta: snapshot
            .take_text(META)
            .and_then(|text| parse_meta(&text).ok()),
        readiness,
    })
}

/// The capture the MUMPS replay side answers `inputs` with (AAP 0.3.2): the
/// first `Recorded` case, in [`CASE_IDS`] order, whose inputs equal
/// `inputs`, or `None`. `Absent` and `NotRecorded` cases are passed over. A
/// case met on the way that [`load_case`] would find `Unreadable`, whether
/// or not its inputs match, ends the lookup as `Err` with that same cause.
/// The golden folder is read afresh on every call, each file within its
/// size budget, so new captures need no restart.
pub fn find_capture(golden_dir: &Path, inputs: &[String]) -> Result<Option<Case>, String> {
    for id in CASE_IDS {
        let case_dir = golden_dir.join(id);
        match probe_folder(&case_dir) {
            Folder::Absent => continue,
            Folder::Uninspectable(cause) => return Err(cause),
            Folder::Present => {}
        }
        // Only a matching case is loaded in full, by `hydrate` reusing this
        // `input.txt`; any other case with an `input.txt` is only validated
        // (`Keep::Nothing`) and fails only when it records, so the result
        // equals `load_case`'s.
        let input = CaptureFile::read(&case_dir, INPUT, true);
        let matches = input
            .text()
            .is_some_and(|text| input_lines(text).eq(inputs.iter().map(String::as_str)));
        if matches {
            match hydrate(id, &case_dir, input) {
                CaseLoad::Recorded(case) => return Ok(Some(case)),
                CaseLoad::Unreadable(cause) => return Err(cause),
                CaseLoad::Absent | CaseLoad::NotRecorded(_) => {}
            }
        } else if input.present() {
            let snapshot = Snapshot::read(&case_dir, Keep::Nothing, Some(input));
            if snapshot.records()
                && let Some(cause) = snapshot.failure()
            {
                return Err(cause);
            }
        }
    }
    Ok(None)
}

/// The live adapter's start arguments and context (AAP 0.3.2): from the
/// first case folder, in [`CASE_IDS`] order, whose `meta.txt` is readable
/// UTF-8 and parses, its [`Meta::docker_exec_args`] and [`Meta::context`],
/// either one's error passed on. With no such file the error is
/// `NO META.TXT IN <golden_dir>`.
pub fn live_start(golden_dir: &Path) -> Result<(Vec<String>, Context), String> {
    for id in CASE_IDS {
        let meta_file = CaptureFile::read(&golden_dir.join(id), META, true);
        let Content::Bytes(bytes) = meta_file.content else {
            continue;
        };
        let Some(meta) = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| parse_meta(text).ok())
        else {
            continue;
        };
        return Ok((meta.docker_exec_args()?, meta.context()?));
    }
    Err(format!("NO META.TXT IN {}", golden_dir.display()))
}

/// The golden folder of this checkout, `tests/golden/ifcap` under the crate
/// folder (AAP 0.3.2): the bridge binary's `GOLDEN_DIR` default and the
/// folder the integration tests read. This is the only place in `src/` that
/// names it.
pub fn default_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("ifcap")
}

// ---------------------------------------------------------------------------
// Parity corpus
// ---------------------------------------------------------------------------

// The parity corpus folder, `corpus` inside the golden folder, holds per
// case `<id>/input.txt` and, once recorded, `<id>/transcript.txt`, and per
// recording run `meta.txt`, `SHA256SUMS`, `run-summary.txt` and `BASE.txt`.
// `corpus_gen` writes the inputs, `load_corpus.sh` writes `BASE.txt` and
// `record_corpus.sh` writes the rest. The rules below are the five-case
// reader's rules applied to the corpus: every file is read within its
// budget, a present file must be UTF-8 and free of `\r`, and a reader's
// failure is `<path>: <cause>`, naming the file or folder at fault, or
// `<id>: not a corpus case id` for an id outside the series. A cause may
// quote a value read from a file as it stands, and `parse_corpus_meta`,
// which reads no file, gives its cause without a path. The corpus folder,
// each case folder and every corpus file are read only as they stand,
// never through a symbolic link: a link at any of them, dangling or not,
// is `<path>: symbolic link`, judged from the link's own metadata before
// anything is read, so nothing outside the corpus folder a reader is given
// is read. The corpus generator, its gate and the equivalence harness read
// these case and run files only through the items below, and nothing here
// writes.

/// The recording run's digest list, `shasum -a 256` output over the run's
/// files, written last by `record_corpus.sh`.
const SHA256SUMS: &str = "SHA256SUMS";
/// The recording run's summary: export and run identity, each case's
/// status, and every difference the run collected.
const RUN_SUMMARY: &str = "run-summary.txt";
/// The loaded corpus snapshot's identity, written by `load_corpus.sh`
/// before any recording.
const BASE_FILE: &str = "BASE.txt";

/// The keys of a corpus recording run's `meta.txt`, each required exactly
/// once ([`parse_corpus_meta`]), in the order `record_corpus.sh` writes them.
pub const CORPUS_META_KEYS: [&str; 5] = [
    "image_digest",
    "snapshot_id",
    "source_revision",
    "context",
    "start_command",
];

/// The corpus case id series in report order: each id is the prefix and a
/// number from 1 to the count, zero-padded to the width.
const CORPUS_ID_SERIES: [(&str, usize, usize); 3] =
    [("CP-", 200, 3), ("CE-", 26, 2), ("CR-", 5, 2)];

/// The parity corpus folder of this checkout, `corpus` inside
/// [`default_dir`]: the one place its name is given.
pub fn corpus_dir() -> PathBuf {
    default_dir().join("corpus")
}

/// The 231 corpus case ids in report order, computed rather than found by
/// walking a folder: `CP-001` to `CP-200`, `CE-01` to `CE-26`, then `CR-01`
/// to `CR-05`. Each is also its case folder's name.
pub fn corpus_case_ids() -> Vec<String> {
    CORPUS_ID_SERIES
        .iter()
        .flat_map(|&(prefix, count, width)| {
            (1..=count).map(move |number| format!("{prefix}{number:0width$}"))
        })
        .collect()
}

/// The path of case `case_id`'s `input.txt` under the corpus folder
/// `corpus_dir`.
pub fn corpus_input_path(corpus_dir: &Path, case_id: &str) -> PathBuf {
    corpus_dir.join(case_id).join(INPUT)
}

/// Whether `case_id` is one of [`corpus_case_ids`], judged from the same
/// series without allocating. Every corpus reader refuses any other id, so
/// no id can lead a path outside its case folder.
fn corpus_case_known(case_id: &str) -> bool {
    CORPUS_ID_SERIES.iter().any(|&(prefix, count, width)| {
        case_id.strip_prefix(prefix).is_some_and(|digits| {
            digits.len() == width
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && digits
                    .parse::<usize>()
                    .is_ok_and(|number| (1..=count).contains(&number))
        })
    })
}

/// The error for an id outside [`corpus_case_ids`].
fn unknown_corpus_case(case_id: &str) -> String {
    format!("{case_id}: not a corpus case id")
}

/// Whether a symbolic link, dangling or not, stands at `path` itself,
/// judged from the link's own metadata, so the link is never followed.
/// Nothing at `path`, or metadata that cannot be read, is no link here: the
/// reader that follows reports it.
fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|entry| entry.file_type().is_symlink())
}

/// `Err` as `<path>: symbolic link` when a symbolic link stands at `path`
/// itself ([`is_link`]), else `Ok`, so that no corpus reader follows one.
fn refuse_link(path: &Path) -> Result<(), String> {
    if is_link(path) {
        return Err(format!("{}: symbolic link", path.display()));
    }
    Ok(())
}

/// The corpus folder `corpus_dir` without any trailing separator or `.`,
/// borrowed from it, for judging whether it is itself a symbolic link:
/// through either of those, a path's own metadata would be the link's
/// target's.
fn corpus_root(corpus_dir: &Path) -> &Path {
    corpus_dir.components().as_path()
}

/// Whether anything stands at `path` itself, readable or not, a symbolic
/// link included whether or not it dangles, judged without following a
/// link: only metadata that reports the path missing (`NotFound`) means
/// nothing is there. A link so found is then refused by the reader
/// ([`read_corpus_text`]), never taken for absence.
fn stands(path: &Path) -> bool {
    !fs::symlink_metadata(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
}

/// The corpus file `name` of `dir`, read whole within its budget: `None`
/// when nothing is at its path. A symbolic link at `dir` or at the file,
/// dangling or not, is `Err` as `<path>: symbolic link` and is never
/// followed, so only a file standing in `dir` itself is read. Both checks
/// are made just before [`CaptureFile::read`] opens the file, so they hold
/// for a folder that nothing changes while it is read. A file that
/// is present but unreadable, over budget or not UTF-8 is `Err` as
/// [`CaptureFile::failure`] gives it, and one holding a `\r` is `Err` with
/// the cause `carriage return`.
fn read_corpus_text(dir: &Path, name: &'static str) -> Result<Option<String>, String> {
    refuse_link(dir)?;
    refuse_link(&dir.join(name))?;
    let file = CaptureFile::read(dir, name, true);
    if let Some(cause) = file.failure() {
        return Err(cause);
    }
    let path = file.path.display().to_string();
    let text = match file.content {
        Content::Missing => return Ok(None),
        Content::Bytes(bytes) => {
            String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8"))?
        }
        Content::Unreadable(cause) => return Err(format!("{path}: {cause}")),
        // A kept file is never scanned; this arm only keeps the match whole.
        Content::Scanned { .. } => return Err(format!("{path}: not read whole")),
    };
    if text.contains('\r') {
        return Err(format!("{path}: carriage return"));
    }
    Ok(Some(text))
}

/// The text of corpus case `case_id`'s `input.txt` under `corpus_dir`,
/// exactly as stored, never touching its `transcript.txt`. It is `Err` as
/// `<id>: not a corpus case id` when the id is not one of
/// [`corpus_case_ids`]. It is `Err` as `<path>: symbolic link` when the
/// corpus folder, the case folder or the file is a symbolic link, dangling
/// or not, which is never followed, so nothing outside `corpus_dir` is
/// read. It is `Err` as `<path>: <cause>`, naming the folder or file at
/// fault, when the case folder is missing or cannot be inspected, the file
/// is missing, unreadable, over its 1 MiB budget, not UTF-8 or holds a
/// `\r`, it is empty, or the last of its [`split_input_lines`] does not
/// begin with `^`: every corpus session ends with a caret-prefixed exit,
/// `SPEC.md` "Exit input" (PO-09.b3). The five-case check that the last
/// input is exactly [`EXIT_INPUT`] does not apply, since an edge case may
/// exit with `^FOO`.
pub fn read_corpus_input(corpus_dir: &Path, case_id: &str) -> Result<String, String> {
    if !corpus_case_known(case_id) {
        return Err(unknown_corpus_case(case_id));
    }
    refuse_link(corpus_root(corpus_dir))?;
    let case_dir = corpus_dir.join(case_id);
    // Refused before the probe, which follows a link, so that a dangling
    // case folder link is reported as the link and not as missing.
    refuse_link(&case_dir)?;
    match probe_folder(&case_dir) {
        Folder::Present => {}
        Folder::Absent => return Err(format!("{}: missing", case_dir.display())),
        Folder::Uninspectable(cause) => return Err(cause),
    }
    let path = case_dir.join(INPUT);
    let Some(text) = read_corpus_text(&case_dir, INPUT)? else {
        return Err(format!("{}: missing", path.display()));
    };
    if text.is_empty() {
        return Err(format!("{}: empty", path.display()));
    }
    if !input_lines(&text)
        .next_back()
        .is_some_and(|last| last.starts_with('^'))
    {
        return Err(format!(
            "{}: last input does not begin with ^",
            path.display()
        ));
    }
    Ok(text)
}

/// One corpus case as stored: its inputs and, once recorded, the original's
/// raw standard output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusCase {
    /// The case id, one of [`corpus_case_ids`], which is also its folder
    /// name.
    pub id: String,
    /// [`split_input_lines`] of `input.txt`, as [`read_corpus_input`]
    /// accepts it.
    pub inputs: Vec<String>,
    /// Raw `transcript.txt`, never normalised; `None` while the case is not
    /// recorded.
    pub transcript: Option<String>,
}

/// Loads corpus case `case_id` from `corpus_dir`, read afresh on every
/// call. The folder comes first, as for [`load_case`], so that a caller may
/// point it at any corpus folder. The inputs are [`read_corpus_input`]'s,
/// with its errors. A missing `transcript.txt` leaves the case unrecorded;
/// a present one must be readable within its 1 MiB budget, UTF-8, free of
/// `\r` and not empty, else the case is `Err` as `<path>: <cause>`. As for
/// the input, a symbolic link at the corpus folder, the case folder,
/// `transcript.txt` or `SHA256SUMS`, dangling or not, is `Err` as
/// `<path>: symbolic link` and is never followed, so nothing outside
/// `corpus_dir` is read.
///
/// When recording evidence exists, a present `transcript.txt` or anything
/// standing as `SHA256SUMS` in `corpus_dir`, a dangling link included, the
/// very text returned is bound to that listing, so that a file replaced
/// after [`load_corpus_run`] verified the run never reaches a verdict
/// unhashed. `SHA256SUMS` is read once, within its 64 KiB budget, UTF-8
/// and free of `\r`, and each of its lines must pass the line rules of
/// [`load_corpus_run`], with every id of [`corpus_case_ids`] allowed; a
/// line that fails is the same `Err` that function gives. Then, each `Err`
/// as `<path>: <cause>`:
///
/// - a missing `SHA256SUMS` has the cause `missing`;
/// - a present transcript not listed as `<id>/transcript.txt` has the
///   cause `not listed in SHA256SUMS`, and a listed one that is absent
///   `listed in SHA256SUMS but missing`;
/// - the returned transcript, and the returned input text when
///   `<id>/input.txt` is listed, must have the listed SHA-256
///   ([`sha256_hex`]) of their raw bytes, else the cause is
///   `SHA-256 differs from SHA256SUMS`. An unlisted input is accepted, as
///   [`load_corpus_run`] verifies only the files listed.
///
/// With neither, `SHA256SUMS` is not read and the case is unrecorded.
pub fn load_corpus_case(corpus_dir: &Path, case_id: &str) -> Result<CorpusCase, String> {
    let input = read_corpus_input(corpus_dir, case_id)?;
    let case_dir = corpus_dir.join(case_id);
    let transcript = read_corpus_text(&case_dir, TRANSCRIPT)?;
    if transcript.as_deref() == Some("") {
        return Err(format!("{}: empty", case_dir.join(TRANSCRIPT).display()));
    }
    if transcript.is_some() || stands(&corpus_dir.join(SHA256SUMS)) {
        bind_case_to_sums(corpus_dir, case_id, &input, transcript.as_deref())?;
    }
    Ok(CorpusCase {
        id: case_id.to_string(),
        inputs: split_input_lines(&input),
        transcript,
    })
}

/// Binds corpus case `case_id`'s `input` text and `transcript`, the very
/// text [`load_corpus_case`] returns, to the `SHA256SUMS` of `corpus_dir`
/// by the rules that function states. The listing is read once here and
/// walked by [`walk_sha256sums`] with every corpus id allowed. Only the
/// case's own `input.txt` and `transcript.txt` lines are compared, each
/// with the digest of the text in hand, so no case file is read again, and
/// a member's path is built only for an error that names one of them.
fn bind_case_to_sums(
    corpus_dir: &Path,
    case_id: &str,
    input: &str,
    transcript: Option<&str>,
) -> Result<(), String> {
    let sums_text = read_run_file(corpus_dir, SHA256SUMS)?;
    let mut transcript_listed = false;
    walk_sha256sums(corpus_dir, &sums_text, corpus_case_known, |line| {
        let file_path = || line.member.path(corpus_dir).display().to_string();
        let held = match line.listed.split_once('/') {
            Some((id, INPUT)) if id == case_id => input,
            Some((id, TRANSCRIPT)) if id == case_id => {
                transcript_listed = true;
                transcript
                    .ok_or_else(|| format!("{}: listed in SHA256SUMS but missing", file_path()))?
            }
            _ => return Ok(()),
        };
        if sha256_hex(held.as_bytes()) != line.digest {
            return Err(format!("{}: SHA-256 differs from SHA256SUMS", file_path()));
        }
        Ok(())
    })?;
    if transcript.is_some() && !transcript_listed {
        return Err(format!(
            "{}: not listed in SHA256SUMS",
            corpus_dir.join(case_id).join(TRANSCRIPT).display()
        ));
    }
    Ok(())
}

/// One parsed corpus `meta.txt`: each of the [`CORPUS_META_KEYS`] exactly
/// once, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusMeta {
    entries: Vec<(String, String)>,
}

impl CorpusMeta {
    /// The value of `key` exactly as recorded, or `None` for a key outside
    /// [`CORPUS_META_KEYS`].
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// Parses a corpus `meta.txt` by the line rules of [`parse_meta`]:
/// `key=value` lines split at the first `=`, whitespace-only lines ignored
/// and values kept exactly. A non-blank line without `=`, a key outside
/// [`CORPUS_META_KEYS`] or a repeated key is `Err` naming the 1-based line.
/// Unlike [`parse_meta`], a missing key is `Err` too, naming the first one
/// missing in [`CORPUS_META_KEYS`] order.
pub fn parse_corpus_meta(text: &str) -> Result<CorpusMeta, String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index.saturating_add(1);
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("meta line {number}: no '='"));
        };
        if !CORPUS_META_KEYS.contains(&key) {
            return Err(format!("meta line {number}: unknown key {key}"));
        }
        if entries.iter().any(|(seen, _)| seen == key) {
            return Err(format!("meta line {number}: repeated key {key}"));
        }
        entries.push((key.to_string(), value.to_string()));
    }
    if let Some(key) = CORPUS_META_KEYS
        .iter()
        .find(|key| !entries.iter().any(|(seen, _)| seen == *key))
    {
        return Err(format!("meta: missing key {key}"));
    }
    Ok(CorpusMeta { entries })
}

/// The state of the corpus recording run, as [`load_corpus_run`] verifies
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorpusRun {
    /// Nothing is recorded: the corpus folder does not exist, or it holds
    /// no `transcript.txt` in any case folder and none of `meta.txt`,
    /// `SHA256SUMS` and `run-summary.txt`. A `BASE.txt` alone, written
    /// before any recording, is still absent. A symbolic link is never
    /// absent: one at the corpus folder, dangling or not, is `Err`, and one
    /// at a case folder, a transcript or a run file counts as present.
    Absent,
    /// A recording run whose files all verified.
    Recorded {
        /// The run's `meta.txt`.
        meta: CorpusMeta,
        /// The ids `run-summary.txt` marks `QUARANTINED`, in file order:
        /// cases whose transcript shows a paging prompt.
        quarantined: Vec<String>,
        /// The SHA-256 of the export taken before the run, as
        /// `run-summary.txt` records it: 64 lower-case hex digits.
        export_sha256: String,
    },
}

/// The recording run of the corpus folder `corpus_dir` for the cases
/// `case_ids`, verified, read afresh on every call.
///
/// Every id must be one of [`corpus_case_ids`]. No symbolic link is
/// followed, so nothing outside `corpus_dir` is read: a link, dangling or
/// not, at the corpus folder, at a case folder or at a file this function
/// reads is `Err` as `<path>: symbolic link`. A missing corpus folder, or
/// one holding no `transcript.txt` for any of `case_ids` and none of
/// `meta.txt`, `SHA256SUMS` and `run-summary.txt`, is
/// [`CorpusRun::Absent`]; a case folder whose metadata cannot be read, or
/// that is a link, counts as holding a transcript, since its absence cannot
/// be confirmed, and a link in place of a transcript or a run file counts
/// as that file. Otherwise all three run files must exist, each readable
/// within its 64 KiB budget, UTF-8 and free of `\r`, and:
///
/// - `meta.txt` must pass [`parse_corpus_meta`];
/// - `run-summary.txt` must hold exactly one `exports=IDENTICAL`, one
///   `runs=IDENTICAL` and one `export_sha256=` line with 64 lower-case hex
///   digits, then exactly one case line `<id>\t<status>\t<exit 1>\t<exit 2>`
///   for each of `case_ids` and none for any other id, its status
///   `RECORDED` or `QUARANTINED`. A summary missing a case line is `Err`
///   naming the first such id in `case_ids` order. The exit statuses are
///   evidence only and are not read. Each `DIFF\t<subject>` line, a
///   difference the run collected, is evidence for people too, while the
///   identity lines decide the run: it is accepted when its subject is
///   exactly `export` or one of `case_ids`, and is `Err` otherwise. Any
///   other line is `Err`;
/// - each `SHA256SUMS` line is 64 lower-case hex digits, two spaces or a
///   space and `*`, and a path relative to the corpus folder that is
///   exactly `BASE.txt`, `meta.txt`, `run-summary.txt`,
///   `<id>/input.txt` or `<id>/transcript.txt` for an id of `case_ids`,
///   listed once. Each listed file must exist, be readable within its
///   budget, UTF-8, free of `\r`, not empty when it is a transcript, and
///   have that SHA-256 ([`sha256_hex`]). A listed `meta.txt` or
///   `run-summary.txt` is checked against the very bytes parsed above,
///   which are read once, so the run returned comes from the bytes the
///   listing covers. The listing need not name `BASE.txt`, `meta.txt` or
///   `run-summary.txt`;
/// - every `transcript.txt` of `case_ids` that exists must be listed.
///
/// Each failure is `Err` as `<path>: <cause>`, naming the file or folder
/// at fault, except an id outside [`corpus_case_ids`], which is
/// `<id>: not a corpus case id`. A cause may quote a value read from the
/// file as it stands, such as an `exports=` value other than `IDENTICAL`.
pub fn load_corpus_run(corpus_dir: &Path, case_ids: &[String]) -> Result<CorpusRun, String> {
    if let Some(unknown) = case_ids.iter().find(|id| !corpus_case_known(id)) {
        return Err(unknown_corpus_case(unknown));
    }
    // Refused before the probe, which follows a link, so that a linked or
    // dangling corpus folder is reported as the link and never as absent.
    refuse_link(corpus_root(corpus_dir))?;
    match probe_folder(corpus_dir) {
        Folder::Present => {}
        Folder::Absent => return Ok(CorpusRun::Absent),
        Folder::Uninspectable(cause) => return Err(cause),
    }
    let recorded: Vec<&str> = case_ids
        .iter()
        .map(String::as_str)
        .filter(|id| corpus_transcript_exists(corpus_dir, id))
        .collect();
    if recorded.is_empty()
        && ![META, SHA256SUMS, RUN_SUMMARY]
            .into_iter()
            .any(|name| stands(&corpus_dir.join(name)))
    {
        return Ok(CorpusRun::Absent);
    }
    let meta_text = read_run_file(corpus_dir, META)?;
    let sums_text = read_run_file(corpus_dir, SHA256SUMS)?;
    let summary_text = read_run_file(corpus_dir, RUN_SUMMARY)?;
    let meta = parse_corpus_meta(&meta_text)
        .map_err(|cause| format!("{}: {cause}", corpus_dir.join(META).display()))?;
    let known: HashSet<&str> = case_ids.iter().map(String::as_str).collect();
    let summary = parse_run_summary(&corpus_dir.join(RUN_SUMMARY), &summary_text, case_ids)?;
    let retained = [
        (META, meta_text.as_str()),
        (RUN_SUMMARY, summary_text.as_str()),
    ];
    let listed = verify_sha256sums(corpus_dir, &sums_text, &known, &retained)?;
    if let Some(id) = recorded.into_iter().find(|id| !listed.contains(id)) {
        return Err(format!(
            "{}: not listed in SHA256SUMS",
            corpus_dir.join(id).join(TRANSCRIPT).display()
        ));
    }
    Ok(CorpusRun::Recorded {
        meta,
        quarantined: summary.quarantined,
        export_sha256: summary.export_sha256,
    })
}

/// Whether corpus case `case_id` under `corpus_dir` holds a
/// `transcript.txt`, judged from file metadata alone, so no file content
/// is read, and never through a symbolic link. It is `true` for a
/// transcript whether or not it is readable or valid, a symbolic link in
/// its place included, dangling or not, and, by the rule [`is_recorded`]
/// applies, for a case folder whose metadata cannot be read, since its
/// transcript cannot be confirmed absent. For the same reason it is `true`
/// when the corpus folder or the case folder is a symbolic link, which is
/// not followed to look inside. It is `false` for an id outside
/// [`corpus_case_ids`], so no id leads a path outside its case folder.
pub fn corpus_transcript_exists(corpus_dir: &Path, case_id: &str) -> bool {
    if !corpus_case_known(case_id) {
        return false;
    }
    let case_dir = corpus_dir.join(case_id);
    if is_link(corpus_root(corpus_dir)) || is_link(&case_dir) {
        return true;
    }
    match probe_folder(&case_dir) {
        Folder::Absent => false,
        Folder::Uninspectable(_) => true,
        Folder::Present => stands(&case_dir.join(TRANSCRIPT)),
    }
}

/// The run file `name` of `corpus_dir`, which must exist and pass
/// [`read_corpus_text`].
fn read_run_file(corpus_dir: &Path, name: &'static str) -> Result<String, String> {
    read_corpus_text(corpus_dir, name)?
        .ok_or_else(|| format!("{}: missing", corpus_dir.join(name).display()))
}

/// Whether `text` is a SHA-256 digest as `shasum -a 256` prints it: 64
/// lower-case hex digits.
fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// What a verified `run-summary.txt` records beyond its identity lines.
struct RunSummary {
    quarantined: Vec<String>,
    export_sha256: String,
}

/// Parses the `run-summary.txt` at `path` by the rules of
/// [`load_corpus_run`], for the cases `case_ids`: each must have exactly
/// one case line, and a summary missing any is `Err` naming the first one
/// missing in `case_ids` order. Blank lines are ignored, and line numbers
/// are 1-based.
fn parse_run_summary(path: &Path, text: &str, case_ids: &[String]) -> Result<RunSummary, String> {
    let path = path.display().to_string();
    let known: HashSet<&str> = case_ids.iter().map(String::as_str).collect();
    let mut exports = false;
    let mut runs = false;
    let mut export_sha256: Option<&str> = None;
    let mut cases: HashSet<&str> = HashSet::new();
    let mut quarantined = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index.saturating_add(1);
        if line.trim().is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("exports=") {
            identity_line(&path, number, "exports", value, &mut exports)?;
        } else if let Some(value) = line.strip_prefix("runs=") {
            identity_line(&path, number, "runs", value, &mut runs)?;
        } else if let Some(value) = line.strip_prefix("export_sha256=") {
            if export_sha256.replace(value).is_some() {
                return Err(format!("{path}: line {number}: repeated export_sha256="));
            }
            if !is_sha256_hex(value) {
                return Err(format!(
                    "{path}: line {number}: export_sha256 is not 64 lower-case hex digits"
                ));
            }
        } else if let Some(subject) = line.strip_prefix("DIFF\t") {
            // A DIFF line is evidence for people, as the exit statuses are:
            // the identity lines decide the run. Its subject must still be
            // the export or one of the cases.
            if subject != "export" && !known.contains(subject) {
                return Err(format!(
                    "{path}: line {number}: DIFF names neither the export nor a case: {subject}"
                ));
            }
        } else if let Some((id, rest)) = line.split_once('\t') {
            // Only the id and the status are read: the two exit statuses
            // after them are evidence for people, never for a verdict.
            let status = rest.split('\t').next().unwrap_or_default();
            if !known.contains(id) {
                return Err(format!("{path}: line {number}: unexpected case {id}"));
            }
            if !cases.insert(id) {
                return Err(format!("{path}: line {number}: repeated case {id}"));
            }
            match status {
                "RECORDED" => {}
                "QUARANTINED" => quarantined.push(id.to_string()),
                _ => {
                    return Err(format!(
                        "{path}: line {number}: status {status} is neither RECORDED nor QUARANTINED"
                    ));
                }
            }
        } else {
            return Err(format!("{path}: line {number}: not recognised"));
        }
    }
    if !exports {
        return Err(format!("{path}: missing exports= line"));
    }
    if !runs {
        return Err(format!("{path}: missing runs= line"));
    }
    let Some(export_sha256) = export_sha256 else {
        return Err(format!("{path}: missing export_sha256= line"));
    };
    // Each of `case_ids` must have its line: a dropped line would otherwise
    // pass unseen, and with it any quarantine the line carried.
    if let Some(id) = case_ids.iter().find(|id| !cases.contains(id.as_str())) {
        return Err(format!("{path}: missing case {id}"));
    }
    Ok(RunSummary {
        quarantined,
        export_sha256: export_sha256.to_string(),
    })
}

/// One `exports=` or `runs=` line of a `run-summary.txt`: it may appear
/// once only, and its value must be `IDENTICAL`. `seen` records that it
/// appeared.
fn identity_line(
    path: &str,
    number: usize,
    key: &str,
    value: &str,
    seen: &mut bool,
) -> Result<(), String> {
    if std::mem::replace(seen, true) {
        return Err(format!("{path}: line {number}: repeated {key}="));
    }
    if value != "IDENTICAL" {
        return Err(format!("{path}: {key}={value}"));
    }
    Ok(())
}

/// One file a `SHA256SUMS` line may list, borrowed from the line: the case
/// id of a case file (`None` for a run file in the corpus folder itself)
/// and the file's name. No path is held: a member's folder is built only
/// when the member is read or named in an error ([`RunMember::folder`]),
/// so a walk allocates nothing for a member its visitor passes over. The
/// file at that path is read, like every corpus file, only through
/// [`read_corpus_text`], never through a symbolic link.
struct RunMember<'a> {
    case: Option<&'a str>,
    name: &'static str,
}

impl<'a> RunMember<'a> {
    /// The folder holding the member under `corpus_dir`: `corpus_dir`
    /// itself, borrowed, for a run file, and the case folder, built here,
    /// for a case file.
    fn folder<'d>(&self, corpus_dir: &'d Path) -> std::borrow::Cow<'d, Path> {
        match self.case {
            None => std::borrow::Cow::Borrowed(corpus_dir),
            Some(id) => std::borrow::Cow::Owned(corpus_dir.join(id)),
        }
    }

    /// The member's path under `corpus_dir`, for an error that names it.
    fn path(&self, corpus_dir: &Path) -> PathBuf {
        self.folder(corpus_dir).join(self.name)
    }

    /// The case id when the member is that case's transcript.
    fn transcript_of(&self) -> Option<&'a str> {
        self.case.filter(|_| self.name == TRANSCRIPT)
    }
}

/// The file the `SHA256SUMS` path `listed` names, when it is one the run
/// may list: `BASE.txt`, `meta.txt` or `run-summary.txt` in the corpus
/// folder, or `input.txt` or `transcript.txt` in the folder of an id
/// `known` accepts. Any other path, such as an absolute one, one through
/// `..` or one starting `./`, is `None`. It only borrows from `listed`, so
/// it allocates nothing.
fn run_member<'a>(listed: &'a str, known: impl Fn(&str) -> bool) -> Option<RunMember<'a>> {
    let run_file = |name: &'static str| RunMember { case: None, name };
    match listed {
        BASE_FILE => return Some(run_file(BASE_FILE)),
        META => return Some(run_file(META)),
        RUN_SUMMARY => return Some(run_file(RUN_SUMMARY)),
        _ => {}
    }
    let (id, file) = listed.split_once('/')?;
    if !known(id) {
        return None;
    }
    let name = match file {
        INPUT => INPUT,
        TRANSCRIPT => TRANSCRIPT,
        _ => return None,
    };
    Some(RunMember {
        case: Some(id),
        name,
    })
}

/// One `SHA256SUMS` line that passed the line rules of [`walk_sha256sums`].
struct SumsLine<'a> {
    /// The listed SHA-256: 64 lower-case hex digits.
    digest: &'a str,
    /// The path exactly as listed, relative to the corpus folder.
    listed: &'a str,
    /// The file that path names.
    member: RunMember<'a>,
}

/// Walks `text`, the `SHA256SUMS` of `corpus_dir`, and hands each line that
/// passes the line rules to `visit`, in file order, stopping at the first
/// `Err`, its own or `visit`'s. This is the one home of the line rules that
/// [`load_corpus_run`] and [`load_corpus_case`] both apply: blank lines are
/// ignored, and every other line is 64 lower-case hex digits, two spaces or
/// a space and `*`, and a path [`run_member`] allows, `known` judging its
/// case id, listed once. A line that breaks a rule is `Err` as
/// `<SHA256SUMS path>: line <n>: <cause>`, with `n` 1-based, that path
/// built only then.
fn walk_sha256sums<'a>(
    corpus_dir: &Path,
    text: &'a str,
    known: impl Fn(&str) -> bool,
    mut visit: impl FnMut(SumsLine<'a>) -> Result<(), String>,
) -> Result<(), String> {
    let sums_path = || corpus_dir.join(SHA256SUMS).display().to_string();
    let mut listed: HashSet<&str> = HashSet::new();
    for (index, line) in text.split('\n').enumerate() {
        let number = index.saturating_add(1);
        if line.trim().is_empty() {
            continue;
        }
        let parsed = line.split_at_checked(64).and_then(|(digest, rest)| {
            let path = rest
                .strip_prefix("  ")
                .or_else(|| rest.strip_prefix(" *"))?;
            is_sha256_hex(digest).then_some((digest, path))
        });
        let Some((digest, listed_path)) = parsed else {
            return Err(format!(
                "{}: line {number}: not a SHA-256 digest and path",
                sums_path()
            ));
        };
        let Some(member) = run_member(listed_path, &known) else {
            return Err(format!("{}: line {number}: path not allowed", sums_path()));
        };
        if !listed.insert(listed_path) {
            return Err(format!("{}: line {number}: path listed twice", sums_path()));
        }
        visit(SumsLine {
            digest,
            listed: listed_path,
            member,
        })?;
    }
    Ok(())
}

/// Verifies `text`, the `SHA256SUMS` of `corpus_dir`, by the rules of
/// [`load_corpus_run`] for the cases `case_ids`, and gives the ids whose
/// transcript it lists. Its lines are walked by [`walk_sha256sums`] with
/// the ids of `case_ids` allowed. `retained` pairs each run file the caller
/// has already read and parsed with that text: a listed path equal to one
/// of those names is hashed from that very text and never read again, so
/// the digest covers the bytes the caller parsed. Every other listed file
/// is read here, its case folder built once for that read, and a member's
/// path is shown only in an error.
fn verify_sha256sums<'a>(
    corpus_dir: &Path,
    text: &'a str,
    case_ids: &HashSet<&str>,
    retained: &[(&str, &str)],
) -> Result<HashSet<&'a str>, String> {
    let mut transcripts: HashSet<&str> = HashSet::new();
    walk_sha256sums(
        corpus_dir,
        text,
        |id| case_ids.contains(id),
        |line| {
            let member = line.member;
            let folder = member.folder(corpus_dir);
            let file_path = || folder.join(member.name).display().to_string();
            let content = match retained.iter().find(|(name, _)| *name == line.listed) {
                Some(&(_, kept)) => std::borrow::Cow::Borrowed(kept),
                None => match read_corpus_text(&folder, member.name)? {
                    Some(read) => std::borrow::Cow::Owned(read),
                    None => {
                        return Err(format!("{}: listed in SHA256SUMS but missing", file_path()));
                    }
                },
            };
            let transcript_of = member.transcript_of();
            if transcript_of.is_some() && content.is_empty() {
                return Err(format!("{}: empty", file_path()));
            }
            if sha256_hex(content.as_bytes()) != line.digest {
                return Err(format!("{}: SHA-256 differs from SHA256SUMS", file_path()));
            }
            transcripts.extend(transcript_of);
            Ok(())
        },
    )?;
    Ok(transcripts)
}

/// The SHA-256 round constants, FIPS 180-4 section 4.2.2: the first 32 bits
/// of the fractional parts of the cube roots of the first 64 primes.
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The SHA-256 initial hash value, FIPS 180-4 section 5.3.3: the first 32
/// bits of the fractional parts of the square roots of the first 8 primes.
const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// One SHA-256 compression of `block` into `state`, FIPS 180-4 section
/// 6.2.2: the 64-word message schedule, the 64 rounds over [`SHA256_K`],
/// then the working variables added back into `state`. The schedule is a
/// fixed array on the stack, so compressing a block allocates nothing.
fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut schedule = [0u32; 64];
    for (slot, word) in schedule.iter_mut().zip(block.as_chunks::<4>().0) {
        *slot = u32::from_be_bytes(*word);
    }
    for t in 16usize..64 {
        let back = |distance: usize| {
            schedule
                .get(t.wrapping_sub(distance))
                .copied()
                .unwrap_or_default()
        };
        let (w2, w7, w15, w16) = (back(2), back(7), back(15), back(16));
        let sigma0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
        let sigma1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
        if let Some(slot) = schedule.get_mut(t) {
            *slot = w16
                .wrapping_add(sigma0)
                .wrapping_add(w7)
                .wrapping_add(sigma1);
        }
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (constant, word) in SHA256_K.iter().zip(&schedule) {
        let big_sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(big_sigma1)
            .wrapping_add(choose)
            .wrapping_add(*constant)
            .wrapping_add(*word);
        let big_sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = big_sigma0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (word, add) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(add);
    }
}

/// The SHA-256 digest of `bytes`, FIPS 180-4, as 64 lower-case hex digits:
/// the digest `shasum -a 256` prints, with which [`load_corpus_run`]
/// verifies a `SHA256SUMS` listing.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut state = SHA256_H0;
    // Fixed-size `as_chunks` rather than `chunks_exact`, which clippy's
    // `chunks_exact_to_as_chunks` rejects for a constant size. The complete
    // blocks are compressed straight from the borrowed `bytes`, so the
    // message is never copied, and the words need neither indexing nor a
    // fallible conversion.
    let (blocks, rest) = bytes.as_chunks::<64>();
    for block in blocks {
        sha256_compress(&mut state, block);
    }
    // Padding, of the remainder of fewer than 64 bytes only: the remainder,
    // one 1 bit, zeros up to 56 bytes past a block boundary, then the
    // message length in bits as a big-endian 64-bit number. That fills one
    // tail block when the remainder is shorter than 56 bytes and two
    // otherwise, so the zeroed scratch holds two blocks.
    let tail_blocks = if rest.len() < 56 { 1 } else { 2 };
    let mut tail = [0u8; 128];
    let padded = rest.iter().copied().chain(std::iter::once(0x80));
    for (slot, byte) in tail.iter_mut().zip(padded) {
        *slot = byte;
    }
    let bit_length = (bytes.len() as u64).wrapping_mul(8).to_be_bytes();
    let length_at = tail_blocks * 64 - 8;
    for (slot, byte) in tail.iter_mut().skip(length_at).zip(bit_length) {
        *slot = byte;
    }
    for block in tail.as_chunks::<64>().0.iter().take(tail_blocks) {
        sha256_compress(&mut state, block);
    }
    let mut hex = String::with_capacity(64);
    for word in state {
        hex.push_str(&format!("{word:08x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A new directory under the system temporary directory, created by
    /// [`Scratch::new`] and therefore owned by it. A test that reaches its
    /// end removes it with [`Scratch::close`], which reports a failed
    /// removal; `Drop` removes it best effort when a test fails early or
    /// unwinds. Each attempt takes the next name
    /// `ifcap-golden-unit-<pid>-<n>` and creates it with `fs::create_dir`,
    /// which fails on any existing entry, a symbolic link included, without
    /// following it. An existing entry is never removed or reused.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Result<Scratch, String> {
            const ATTEMPTS: usize = 64;
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let base = std::env::temp_dir();
            for _ in 0..ATTEMPTS {
                let n = COUNTER.fetch_add(1, Ordering::Relaxed);
                let dir = base.join(format!("ifcap-golden-unit-{}-{n}", std::process::id()));
                match fs::create_dir(&dir) {
                    Ok(()) => return Ok(Scratch(dir)),
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
                }
            }
            Err(format!(
                "no unused scratch directory name under {} after {ATTEMPTS} attempts",
                base.display()
            ))
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// Removes the directory with everything in it, on a test's normal
        /// path. A failure, a directory already gone included, is an error
        /// naming the directory, so a passing test cannot leave it behind
        /// unnoticed.
        fn close(mut self) -> Result<(), String> {
            let dir = std::mem::take(&mut self.0);
            fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // Best-effort removal for a test that ends before `close`, by an
            // early error return or an unwind: that test has already failed,
            // and a removal failure cannot be reported from here. A closed
            // `Scratch` holds an empty path and is skipped.
            if !self.0.as_os_str().is_empty() {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn rules(items: &[(&str, &str)]) -> Vec<Rule> {
        items
            .iter()
            .map(|(open, close)| Rule {
                open: open.to_string(),
                close: close.to_string(),
            })
            .collect()
    }

    /// A complete `meta.txt`. Line 4 is whitespace-only, and `setup` holds
    /// further `=` signs, which belong to its value.
    const META_TEXT: &str = concat!(
        "image_digest=sha256:0123\n",
        "source_revision=dd3fd0f3124c8178657d9fff4173c324da30f7ae\n",
        "snapshot_id=vehu-202609-gc\n",
        "   \n",
        "record_ids=442:990001;440:990001\n",
        "entry_point=EN3^PRCHRPT\n",
        "context={\"DUZ\":\".5\",\"IOF\":\"#\",\"IOSL\":\"60\",\"STATION\":\"500\"}\n",
        "setup=IOM=80 HOME=CONSOLE\n",
        "docker_exec_args=su - vehu -c X\n",
    );
    const SETUP_LINE: &str = "setup=IOM=80 HOME=CONSOLE\n";
    const STATION_PROMPT: &str = "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// ";
    const DB_TEXT: &str = "ZWR\n^PRC(442,0)=\"PROCUREMENT & ACCOUNTING TRANSACTIONS^442I\"\n";

    /// The seven files of a READY case whose `input.txt` is `input`.
    fn ready_files(input: &str) -> Vec<(&'static str, Vec<u8>)> {
        vec![
            (INPUT, input.as_bytes().to_vec()),
            (TRANSCRIPT, STATION_PROMPT.as_bytes().to_vec()),
            (RETURNED, Vec::new()),
            (DB_BEFORE, DB_TEXT.as_bytes().to_vec()),
            (DB_AFTER, DB_TEXT.as_bytes().to_vec()),
            (META, META_TEXT.as_bytes().to_vec()),
            (NORMALISATION, b"@\t#\n".to_vec()),
        ]
    }

    /// `files` with `name` given `content`, or left out for `None`.
    fn replaced(
        files: Vec<(&'static str, Vec<u8>)>,
        name: &str,
        content: Option<&[u8]>,
    ) -> Vec<(&'static str, Vec<u8>)> {
        files
            .into_iter()
            .filter_map(|(file, bytes)| match (file == name, content) {
                (false, _) => Some((file, bytes)),
                (true, Some(content)) => Some((file, content.to_vec())),
                (true, None) => None,
            })
            .collect()
    }

    /// `head`, then `filler` bytes, then `tail`, `size` bytes in all.
    fn padded(head: &[u8], filler: u8, tail: &[u8], size: usize) -> Vec<u8> {
        let mut bytes = head.to_vec();
        bytes.resize(size.saturating_sub(tail.len()), filler);
        bytes.extend_from_slice(tail);
        bytes
    }

    /// Creates `path` as a sparse file of `length` zero bytes, so that a file
    /// past the exports' budget takes no time or space to write.
    fn write_sparse(path: &Path, length: u64) -> Result<(), String> {
        File::create(path)
            .and_then(|file| file.set_len(length))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// Creates the case folder `golden/id` holding `files`.
    fn write_case(golden: &Path, id: &str, files: &[(&str, Vec<u8>)]) -> Result<PathBuf, String> {
        let dir = golden.join(id);
        fs::create_dir(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        for (name, bytes) in files {
            let path = dir.join(name);
            fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        }
        Ok(dir)
    }

    #[test]
    fn split_input_lines_drops_one_trailing_newline() {
        let cases: [(&str, &[&str]); 8] = [
            ("", &[]),
            ("\n", &[""]),
            ("a", &["a"]),
            ("a\n", &["a"]),
            ("a\n\n", &["a", ""]),
            ("\nA1\n\n\n^\n", &["", "A1", "", "", "^"]),
            ("\nINVALID!\n^", &["", "INVALID!", "^"]),
            ("a\r\n", &["a\r"]),
        ];
        for (text, expected) in cases {
            assert_eq!(split_input_lines(text), strings(expected), "for {text:?}");
            // The borrowed lines and the last one, as the readiness check and
            // `find_capture` take them, agree with the split.
            assert!(
                input_lines(text).eq(expected.iter().copied()),
                "for {text:?}"
            );
            assert_eq!(
                input_lines(text).next_back(),
                expected.last().copied(),
                "for {text:?}"
            );
        }
    }

    #[test]
    fn read_returned_zero_bytes_is_none() {
        let cases = [
            ("", None),
            ("\n", Some("")),
            ("x\n", Some("x")),
            ("x", Some("x")),
            ("x\n\n", Some("x\n")),
        ];
        for (text, expected) in cases {
            assert_eq!(
                read_returned(text),
                expected.map(str::to_string),
                "for {text:?}"
            );
        }
    }

    #[test]
    fn parse_normalisation_rules_and_malformed_lines() {
        let malformed = |line: usize| {
            Err(format!(
                "normalisation line {line}: expected one tab between two non-empty delimiters"
            ))
        };
        let cases: [(&str, Result<Vec<Rule>, String>); 9] = [
            ("", Ok(Vec::new())),
            ("\n\n", Ok(Vec::new())),
            ("@\t#\n", Ok(rules(&[("@", "#")]))),
            ("\nT=\t;\n\n@\t#\r", Ok(rules(&[("T=", ";"), ("@", "#\r")]))),
            ("no tab", malformed(1)),
            ("@\t#\na\tb\tc", malformed(2)),
            ("\n\tb", malformed(2)),
            ("a\t", malformed(1)),
            (" ", malformed(1)),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_normalisation(text), expected, "for {text:?}");
        }
    }

    #[test]
    fn normalise_replaces_open_through_close() {
        type Rules<'a> = &'a [(&'a str, &'a str)];
        let cases: [(&str, Rules, &str); 9] = [
            ("a@1#b@2#c", &[("@", "#")], "a<NORM>b<NORM>c"),
            ("a@1#b@2", &[("@", "#")], "a<NORM>b@2"),
            ("a@1", &[("@", "#")], "a@1"),
            ("a|1|b|2|c", &[("|", "|")], "a<NORM>b<NORM>c"),
            ("abab", &[("ab", "b")], "<NORM>"),
            ("x@1#y", &[("@", "#"), ("x<", "y")], "<NORM>"),
            ("x@1#y", &[("x<", "y"), ("@", "#")], "x<NORM>y"),
            ("a@1#b", &[("", "#"), ("@", "")], "a@1#b"),
            ("é@ü#é", &[("@", "#")], "é<NORM>é"),
        ];
        for (text, rule_items, expected) in cases {
            assert_eq!(
                normalise(text, &rules(rule_items)),
                expected,
                "for {text:?}"
            );
        }
    }

    #[test]
    fn parse_meta_reads_entries_and_rejects_bad_lines() -> Result<(), String> {
        let meta = parse_meta(META_TEXT)?;
        assert_eq!(meta.get("image_digest"), Some("sha256:0123"));
        assert_eq!(meta.get("snapshot_id"), Some("vehu-202609-gc"));
        assert_eq!(meta.get("setup"), Some("IOM=80 HOME=CONSOLE"));
        assert!(META_KEYS.iter().all(|key| meta.get(key).is_some()));
        assert_eq!(meta.get("unknown"), None);
        // Values are kept exactly, a `\r` included.
        assert_eq!(parse_meta("setup= x \r\n")?.get("setup"), Some(" x \r"));
        // A missing key parses; the readiness check reports it.
        let partial = parse_meta(&META_TEXT.replacen(SETUP_LINE, "", 1))?;
        assert_eq!(partial.get("setup"), None);
        let edited = |from: &str, to: &str| parse_meta(&META_TEXT.replacen(from, to, 1));
        let rejected = [
            (
                edited("entry_point=", "entry=point="),
                "meta line 6: unknown key entry",
            ),
            (
                edited("entry_point=", "image_digest="),
                "meta line 6: repeated key image_digest",
            ),
            (
                edited("entry_point=", "entry_point "),
                "meta line 6: no '='",
            ),
            (
                edited("   \n", " setup=x\n"),
                "meta line 4: unknown key  setup",
            ),
        ];
        for (result, message) in rejected {
            assert_eq!(result, Err(message.to_string()));
        }
        Ok(())
    }

    #[test]
    fn docker_exec_args_split_on_single_spaces() -> Result<(), String> {
        let args =
            |value: &str| parse_meta(&format!("docker_exec_args={value}\n"))?.docker_exec_args();
        assert_eq!(
            args("su - vehu -c X"),
            Ok(strings(&["su", "-", "vehu", "-c", "X"]))
        );
        assert_eq!(args("--user vehu"), Ok(strings(&["--user", "vehu"])));
        let doubled = Err(
            "docker_exec_args: empty argument from a leading, trailing or doubled space"
                .to_string(),
        );
        for value in ["a  b", " a", "a "] {
            assert_eq!(args(value), doubled, "for {value:?}");
        }
        assert_eq!(args(""), Err("docker_exec_args: empty".to_string()));
        assert_eq!(
            parse_meta("setup=x\n")?.docker_exec_args(),
            Err("missing key docker_exec_args".to_string())
        );
        // Open item O-07: a quoted script cannot stay one argument.
        assert_eq!(
            args("su - vehu -c 'mumps -r ZU'"),
            Ok(strings(&["su", "-", "vehu", "-c", "'mumps", "-r", "ZU'"]))
        );
        Ok(())
    }

    #[test]
    fn context_reads_flat_object_of_strings() -> Result<(), String> {
        let context = |value: &str| parse_meta(&format!("context={value}\n"))?.context();
        assert_eq!(
            context(r#"{"DUZ":".5","IOF":"\u001b[2J","IOSL":"60","STATION":"500"}"#),
            Ok(pairs(&[
                ("DUZ", ".5"),
                ("IOF", "\u{1b}[2J"),
                ("IOSL", "60"),
                ("STATION", "500"),
            ]))
        );
        assert_eq!(
            context(r#" { "B" : "\"\\\/\b\f\n\r\t" , "A" : "\ud83d\ude00é" } "#),
            Ok(pairs(&[
                ("B", "\"\\/\u{8}\u{c}\n\r\t"),
                ("A", "\u{1f600}é")
            ]))
        );
        assert_eq!(context("{}"), Ok(Vec::new()));
        assert_eq!(
            context(r##"{"DUZ":".5","IOF":"#","DUZ":"1"}"##),
            Err("context: repeated key DUZ".to_string())
        );
        let not_flat = Err("context: not a flat JSON object of strings".to_string());
        let malformed = [
            r#"{"DUZ":5}"#,
            r#"{"DUZ":{"A":"1"}}"#,
            r#"{"DUZ":["1"]}"#,
            r#"{"DUZ":null}"#,
            r#"["DUZ"]"#,
            r#"{"DUZ":".5"}x"#,
            r#"{"DUZ":".5",}"#,
            r#"{"DUZ" ".5"}"#,
            r#"{"DUZ":"\ud800"}"#,
            r#"{"DUZ":"\udc00"}"#,
            r#"{"DUZ":"\x"}"#,
            "{\"DUZ\":\"\u{1}\"}",
            r#"{"DUZ":".5""#,
            "",
        ];
        for value in malformed {
            assert_eq!(context(value), not_flat, "for {value:?}");
        }
        assert_eq!(
            parse_meta("setup=x\n")?.context(),
            Err("missing key context".to_string())
        );
        Ok(())
    }

    #[test]
    fn validate_context_against_stub_context() {
        let accepted = [pairs(&[
            ("DUZ", ".5"),
            ("IOF", "#"),
            ("IOSL", "60"),
            ("STATION", "500"),
        ])];
        for context in accepted {
            assert_eq!(validate_context(&context), Ok(()), "for {context:?}");
        }
        let rejected = [
            (
                pairs(&[
                    ("STATION", "500"),
                    ("IOSL", "60"),
                    ("IOF", "#,$C(27,91,50,74,27,91,72)"),
                    ("DUZ", ".5"),
                ]),
                "CONTEXT MISMATCH IOF: expected #, actual #,$C(27,91,50,74,27,91,72)",
            ),
            (
                pairs(&[
                    ("DUZ", "1"),
                    ("IOF", "#"),
                    ("IOSL", "60"),
                    ("STATION", "500"),
                ]),
                "CONTEXT MISMATCH DUZ: expected .5, actual 1",
            ),
            (
                pairs(&[("DUZ", ".5"), ("IOF", "#"), ("IOSL", "60")]),
                "CONTEXT MISMATCH STATION: expected 500, actual ABSENT",
            ),
            (
                pairs(&[("DUZ", ".5"), ("IOSL", "60"), ("STATION", "500")]),
                "CONTEXT MISMATCH IOF: expected #, actual ABSENT",
            ),
            (
                pairs(&[
                    ("DUZ", ".5"),
                    ("IOF", "#"),
                    ("IOSL", "60"),
                    ("STATION", "500"),
                    ("IOM", "80"),
                ]),
                "CONTEXT MISMATCH IOM: expected ABSENT, actual 80",
            ),
            (
                pairs(&[
                    ("DUZ", ".5"),
                    ("DUZ", "1"),
                    ("IOF", "#"),
                    ("IOSL", "60"),
                    ("STATION", "500"),
                ]),
                "CONTEXT MISMATCH DUZ: expected .5, actual 1",
            ),
            (
                Vec::new(),
                "CONTEXT MISMATCH DUZ: expected .5, actual ABSENT",
            ),
        ];
        for (context, message) in rejected {
            assert_eq!(
                validate_context(&context),
                Err(message.to_string()),
                "for {context:?}"
            );
        }
    }

    #[test]
    fn readiness_ready_and_each_check_failing() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let without_setup = META_TEXT.replacen(SETUP_LINE, "", 1);
        let crlf_transcript = format!("{STATION_PROMPT}\r\n");
        // The file replaced, its new content (`None` leaves it out), and the
        // checks that then fail.
        type Mutation<'a> = (&'a str, Option<&'a [u8]>, Vec<&'static str>);
        let cases: [Mutation; 8] = [
            (INPUT, Some(b"^\n"), Vec::new()),
            (
                DB_AFTER,
                None,
                vec![SEVEN_FILES_PRESENT, DB_BEFORE_EQUALS_AFTER],
            ),
            (INPUT, Some(b"\nGC1001\n"), vec![INPUT_ENDS_WITH_EXIT]),
            (INPUT, Some(b""), vec![INPUT_ENDS_WITH_EXIT]),
            (NORMALISATION, Some(b"@#\n"), vec![NORMALISATION_PARSES]),
            (
                META,
                Some(without_setup.as_bytes()),
                vec![META_KEYS_PRESENT],
            ),
            (
                TRANSCRIPT,
                Some(crlf_transcript.as_bytes()),
                vec![NO_CARRIAGE_RETURN],
            ),
            (
                DB_AFTER,
                Some(b"ZWR\n^PRC(442,0)=\"CHANGED\"\n"),
                vec![DB_BEFORE_EQUALS_AFTER],
            ),
        ];
        for (n, (name, content, failed)) in cases.into_iter().enumerate() {
            let files = replaced(ready_files("^\n"), name, content);
            let dir = write_case(scratch.path(), &format!("case-{n}"), &files)?;
            let expected = if failed.is_empty() {
                Readiness::Ready
            } else {
                Readiness::Incomplete(failed)
            };
            assert_eq!(readiness(&dir), expected, "for {name} as {content:?}");
        }
        // A directory in place of a file is present but neither a regular
        // file nor readable.
        let files = replaced(ready_files("^\n"), TRANSCRIPT, None);
        let dir = write_case(scratch.path(), "directory-transcript", &files)?;
        fs::create_dir(dir.join(TRANSCRIPT)).map_err(|e| e.to_string())?;
        assert_eq!(
            readiness(&dir),
            Readiness::Incomplete(vec![SEVEN_FILES_PRESENT, NO_CARRIAGE_RETURN])
        );
        // An empty folder fails every check but the carriage-return one.
        let empty = write_case(scratch.path(), "empty", &[])?;
        assert_eq!(
            readiness(&empty),
            Readiness::Incomplete(vec![
                SEVEN_FILES_PRESENT,
                META_KEYS_PRESENT,
                INPUT_ENDS_WITH_EXIT,
                NORMALISATION_PARSES,
                DB_BEFORE_EQUALS_AFTER,
            ])
        );
        assert_eq!(
            readiness(&scratch.path().join("missing")),
            Readiness::Absent
        );
        let not_a_folder = dir.join(INPUT);
        assert_eq!(readiness(&not_a_folder), Readiness::Absent);
        assert_eq!(CHECKS.len(), 6);
        scratch.close()
    }

    #[test]
    fn status_lists_cases_in_order() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let empty = Status {
            recorded: Vec::new(),
            ready: Vec::new(),
            incomplete: Vec::new(),
        };
        assert_eq!(status(scratch.path()), empty);
        assert_eq!(status(&scratch.path().join("missing")), empty);
        write_case(
            scratch.path(),
            "GC-03",
            &[(INPUT, b"\nGC3003\n^\n".to_vec())],
        )?;
        write_case(scratch.path(), "GC-01", &ready_files("^\n"))?;
        assert_eq!(
            status(scratch.path()),
            Status {
                recorded: vec!["GC-01"],
                ready: vec!["GC-01"],
                incomplete: vec![(
                    "GC-03",
                    vec![
                        SEVEN_FILES_PRESENT,
                        META_KEYS_PRESENT,
                        NORMALISATION_PARSES,
                        DB_BEFORE_EQUALS_AFTER,
                    ]
                )],
            }
        );
        scratch.close()
    }

    #[test]
    fn load_case_absent_not_recorded_recorded() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        assert_eq!(load_case(golden, "GC-02"), CaseLoad::Absent);
        write_case(golden, "GC-03", &[(INPUT, b"\nGC3003\n^\n".to_vec())])?;
        assert_eq!(
            load_case(golden, "GC-03"),
            CaseLoad::NotRecorded(vec![
                SEVEN_FILES_PRESENT,
                META_KEYS_PRESENT,
                NORMALISATION_PARSES,
                DB_BEFORE_EQUALS_AFTER,
            ])
        );
        write_case(golden, "GC-01", &ready_files("\nGC1001\n\n\n^\n"))?;
        assert_eq!(
            load_case(golden, "GC-01"),
            CaseLoad::Recorded(Case {
                id: "GC-01".to_string(),
                inputs: strings(&["", "GC1001", "", "", "^"]),
                transcript: STATION_PROMPT.to_string(),
                returned: None,
                rules: rules(&[("@", "#")]),
                meta: Some(parse_meta(META_TEXT)?),
                readiness: Readiness::Ready,
            })
        );
        // Missing or unparsable optional files leave their fields empty and
        // show only as failed checks.
        let files = replaced(ready_files("^\n"), RETURNED, None);
        let files = replaced(files, NORMALISATION, Some(b"no tab\n"));
        let files = replaced(files, META, Some(b"unknown=1\n"));
        write_case(golden, "GC-04", &files)?;
        let CaseLoad::Recorded(case) = load_case(golden, "GC-04") else {
            return Err("GC-04 is not recorded".to_string());
        };
        assert_eq!(case.returned, None);
        assert_eq!(case.rules, Vec::new());
        assert_eq!(case.meta, None);
        assert_eq!(
            case.readiness,
            Readiness::Incomplete(vec![
                SEVEN_FILES_PRESENT,
                META_KEYS_PRESENT,
                NORMALISATION_PARSES,
            ])
        );
        let files = replaced(ready_files("^\n"), RETURNED, Some(b"1\n"));
        write_case(golden, "GC-05", &files)?;
        let CaseLoad::Recorded(case) = load_case(golden, "GC-05") else {
            return Err("GC-05 is not recorded".to_string());
        };
        assert_eq!(case.returned, Some("1".to_string()));
        scratch.close()
    }

    #[test]
    fn load_case_unreadable_names_path_and_cause() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        let files = replaced(ready_files("^\n"), TRANSCRIPT, Some(b"\xff\n"));
        let dir = write_case(golden, "GC-01", &files)?;
        let expected = format!("{}: not UTF-8", dir.join(TRANSCRIPT).display());
        assert_eq!(load_case(golden, "GC-01"), CaseLoad::Unreadable(expected));
        // No readiness check reads the transcript as text, and the readiness
        // path scans it without keeping it, so the case is still READY.
        assert_eq!(readiness(&dir), Readiness::Ready);
        // Every present file must be UTF-8, the exports included.
        let files = replaced(ready_files("^\n"), DB_AFTER, Some(b"ZWR\n\xe9\n"));
        let dir = write_case(golden, "GC-02", &files)?;
        let expected = format!("{}: not UTF-8", dir.join(DB_AFTER).display());
        assert_eq!(load_case(golden, "GC-02"), CaseLoad::Unreadable(expected));
        let files = replaced(ready_files("^\n"), META, None);
        let dir = write_case(golden, "GC-03", &files)?;
        fs::create_dir(dir.join(META)).map_err(|e| e.to_string())?;
        let expected = format!("{}: not a regular file", dir.join(META).display());
        assert_eq!(load_case(golden, "GC-03"), CaseLoad::Unreadable(expected));
        scratch.close()
    }

    #[test]
    fn a_case_missing_its_input_or_transcript_is_not_recorded_whatever_else_it_holds()
    -> Result<(), String> {
        /// What GC-01's bad file is made.
        #[derive(Clone, Copy)]
        enum Bad {
            Bytes(&'static [u8]),
            Directory,
        }
        // The file left out, the file then made bad, how, its cause once the
        // case is recorded, and the checks that fail while it is not.
        type Variant<'a> = (&'a str, &'a str, Bad, &'a str, Vec<&'static str>);
        let variants: [Variant; 4] = [
            (
                TRANSCRIPT,
                DB_AFTER,
                Bad::Bytes(b"\xff"),
                "not UTF-8",
                vec![SEVEN_FILES_PRESENT, DB_BEFORE_EQUALS_AFTER],
            ),
            (
                INPUT,
                META,
                Bad::Bytes(b"\xff"),
                "not UTF-8",
                vec![SEVEN_FILES_PRESENT, META_KEYS_PRESENT, INPUT_ENDS_WITH_EXIT],
            ),
            (
                TRANSCRIPT,
                INPUT,
                Bad::Bytes(b"\xff\n"),
                "not UTF-8",
                vec![SEVEN_FILES_PRESENT, INPUT_ENDS_WITH_EXIT],
            ),
            (
                TRANSCRIPT,
                NORMALISATION,
                Bad::Directory,
                "not a regular file",
                vec![
                    SEVEN_FILES_PRESENT,
                    NORMALISATION_PARSES,
                    NO_CARRIAGE_RETURN,
                ],
            ),
        ];
        for (missing, name, bad, cause, failed) in variants {
            // Without `missing` the case has no capture, so the bad file does
            // not count; with it, the same bad file makes the case unreadable.
            for recorded in [false, true] {
                let scratch = Scratch::new()?;
                let golden = scratch.path();
                let files = replaced(ready_files("^\n"), name, None);
                let files = if recorded {
                    files
                } else {
                    replaced(files, missing, None)
                };
                let dir = write_case(golden, "GC-01", &files)?;
                let path = dir.join(name);
                match bad {
                    Bad::Bytes(bytes) => fs::write(&path, bytes),
                    Bad::Directory => fs::create_dir(&path),
                }
                .map_err(|e| e.to_string())?;
                // A READY case whose inputs the request equals, after GC-01.
                write_case(golden, "GC-02", &ready_files("^\n"))?;
                let label = format!("{name} {cause}, {missing} present: {recorded}");
                if recorded {
                    let cause = format!("{}: {cause}", path.display());
                    assert_eq!(
                        load_case(golden, "GC-01"),
                        CaseLoad::Unreadable(cause.clone()),
                        "{label}"
                    );
                    assert!(is_recorded(&dir), "{label}");
                    for inputs in [strings(&["^"]), strings(&["x"])] {
                        assert_eq!(find_capture(golden, &inputs), Err(cause.clone()), "{label}");
                    }
                } else {
                    assert_eq!(
                        load_case(golden, "GC-01"),
                        CaseLoad::NotRecorded(failed.clone()),
                        "{label}"
                    );
                    assert_eq!(
                        readiness(&dir),
                        Readiness::Incomplete(failed.clone()),
                        "{label}"
                    );
                    assert!(!is_recorded(&dir), "{label}");
                    let found = find_capture(golden, &strings(&["^"]))?;
                    assert_eq!(
                        found.map(|case| case.id),
                        Some("GC-02".to_string()),
                        "{label}"
                    );
                    assert_eq!(find_capture(golden, &strings(&["x"])), Ok(None), "{label}");
                }
                scratch.close()?;
            }
        }
        Ok(())
    }

    #[test]
    fn a_directory_at_the_input_or_transcript_is_recorded_and_unreadable() -> Result<(), String> {
        // The file made a directory, and the checks that then fail.
        let cases = [
            (
                INPUT,
                vec![
                    SEVEN_FILES_PRESENT,
                    INPUT_ENDS_WITH_EXIT,
                    NO_CARRIAGE_RETURN,
                ],
            ),
            (TRANSCRIPT, vec![SEVEN_FILES_PRESENT, NO_CARRIAGE_RETURN]),
        ];
        for (name, failed) in cases {
            let scratch = Scratch::new()?;
            let golden = scratch.path();
            let dir = write_case(golden, "GC-01", &replaced(ready_files("^\n"), name, None))?;
            let path = dir.join(name);
            fs::create_dir(&path).map_err(|e| e.to_string())?;
            let cause = format!("{}: not a regular file", path.display());
            assert_eq!(
                load_case(golden, "GC-01"),
                CaseLoad::Unreadable(cause.clone()),
                "{name}"
            );
            assert!(is_recorded(&dir), "{name}");
            assert_eq!(
                status(golden),
                Status {
                    recorded: vec!["GC-01"],
                    ready: Vec::new(),
                    incomplete: vec![("GC-01", failed)],
                },
                "{name}"
            );
            assert_eq!(find_capture(golden, &strings(&["^"])), Err(cause), "{name}");
            scratch.close()?;
        }
        Ok(())
    }

    #[test]
    fn capture_files_load_at_their_budget_and_are_unreadable_past_it() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        // The file, its budget, the head, filler and tail of a content that
        // keeps the case READY at any size, and the checks that fail once
        // the file is one byte past its budget.
        type Budget<'a> = (&'a str, usize, &'a [u8], u8, &'a [u8], Vec<&'static str>);
        let cases: [Budget; 5] = [
            (
                INPUT,
                1_048_576,
                b"",
                b'a',
                b"\n^\n",
                vec![INPUT_ENDS_WITH_EXIT, NO_CARRIAGE_RETURN],
            ),
            (
                TRANSCRIPT,
                1_048_576,
                STATION_PROMPT.as_bytes(),
                b'a',
                b"",
                vec![NO_CARRIAGE_RETURN],
            ),
            (RETURNED, 65_536, b"", b'a', b"", vec![NO_CARRIAGE_RETURN]),
            (
                META,
                65_536,
                META_TEXT.as_bytes(),
                b' ',
                b"",
                vec![META_KEYS_PRESENT, NO_CARRIAGE_RETURN],
            ),
            (
                NORMALISATION,
                65_536,
                b"@\t#\n",
                b'\n',
                b"",
                vec![NORMALISATION_PARSES, NO_CARRIAGE_RETURN],
            ),
        ];
        for (n, (name, budget, head, filler, tail, failed)) in cases.into_iter().enumerate() {
            let at = format!("at-{n}");
            let content = padded(head, filler, tail, budget);
            let files = replaced(ready_files("^\n"), name, Some(&content));
            write_case(golden, &at, &files)?;
            let CaseLoad::Recorded(case) = load_case(golden, &at) else {
                return Err(format!("{name} of {budget} bytes did not load"));
            };
            assert_eq!(case.readiness, Readiness::Ready, "for {name}");
            let over = format!("over-{n}");
            let content = padded(head, filler, tail, budget.saturating_add(1));
            let files = replaced(ready_files("^\n"), name, Some(&content));
            let dir = write_case(golden, &over, &files)?;
            let cause = format!("{}: larger than {budget} bytes", dir.join(name).display());
            assert_eq!(load_case(golden, &over), CaseLoad::Unreadable(cause));
            assert_eq!(readiness(&dir), Readiness::Incomplete(failed), "for {name}");
        }
        scratch.close()
    }

    #[test]
    fn exports_past_their_budget_are_unreadable() -> Result<(), String> {
        const BUDGET: u64 = 67_108_864;
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        // Both exports at their budget, zero bytes being UTF-8, are READY.
        let files = replaced(ready_files("^\n"), DB_BEFORE, None);
        let files = replaced(files, DB_AFTER, None);
        let dir = write_case(golden, "at", &files)?;
        write_sparse(&dir.join(DB_BEFORE), BUDGET)?;
        write_sparse(&dir.join(DB_AFTER), BUDGET)?;
        let CaseLoad::Recorded(case) = load_case(golden, "at") else {
            return Err("exports of 64 MiB did not load".to_string());
        };
        assert_eq!(case.readiness, Readiness::Ready);
        // One byte more in either export makes it unreadable, the other
        // being read to its end.
        for (n, name) in [DB_BEFORE, DB_AFTER].into_iter().enumerate() {
            let id = format!("over-{n}");
            let dir = write_case(golden, &id, &replaced(ready_files("^\n"), name, None))?;
            write_sparse(&dir.join(name), BUDGET + 1)?;
            let cause = format!("{}: larger than {BUDGET} bytes", dir.join(name).display());
            assert_eq!(load_case(golden, &id), CaseLoad::Unreadable(cause));
            assert_eq!(
                readiness(&dir),
                Readiness::Incomplete(vec![DB_BEFORE_EQUALS_AFTER, NO_CARRIAGE_RETURN]),
                "for {name}"
            );
        }
        scratch.close()
    }

    #[test]
    fn exports_are_compared_and_checked_piece_by_piece() -> Result<(), String> {
        const SIZE: usize = 200 * 1024;
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        let export = padded(DB_TEXT.as_bytes(), b'a', b"\n", SIZE);
        let with = |at: usize, bytes: &[u8]| {
            let mut changed = export.clone();
            if let Some(span) = changed.get_mut(at..at.saturating_add(bytes.len())) {
                span.copy_from_slice(bytes);
            }
            changed
        };
        // A character that starts `first` bytes before the end of the first
        // piece, so that the piece boundary cuts it.
        let split =
            |character: &str, first: usize| with(CHUNK.saturating_sub(first), character.as_bytes());
        let mut longer = export.clone();
        longer.extend_from_slice(b"^X=1\n");
        let one_piece = export.get(..CHUNK).unwrap_or_default().to_vec();
        let mut cut = export.clone();
        cut.extend_from_slice(b"\xe2\x82");
        let incomplete = |failed: Vec<&'static str>| Ok(Readiness::Incomplete(failed));
        // What `db-before.zwr` and `db-after.zwr` hold, and the case's
        // readiness when it loads, else the unreadable file and its cause.
        type Exports = (
            &'static str,
            Vec<u8>,
            Vec<u8>,
            Result<Readiness, (&'static str, &'static str)>,
        );
        let mut cases: Vec<Exports> = vec![
            (
                "equal",
                export.clone(),
                export.clone(),
                Ok(Readiness::Ready),
            ),
            (
                "last byte of the second piece differs",
                export.clone(),
                with(2 * CHUNK - 1, b"b"),
                incomplete(vec![DB_BEFORE_EQUALS_AFTER]),
            ),
            (
                "db-after longer",
                export.clone(),
                longer,
                incomplete(vec![DB_BEFORE_EQUALS_AFTER]),
            ),
            (
                "db-before ends at a piece boundary",
                one_piece,
                export.clone(),
                incomplete(vec![DB_BEFORE_EQUALS_AFTER]),
            ),
            (
                "invalid byte in the third piece",
                with(150_000, b"\xff"),
                with(150_000, b"\xff"),
                Err((DB_BEFORE, "not UTF-8")),
            ),
            (
                "db-after ends inside a character",
                export.clone(),
                cut,
                Err((DB_AFTER, "not UTF-8")),
            ),
            (
                "carriage return in the third piece",
                with(150_000, b"\r"),
                with(150_000, b"\r"),
                incomplete(vec![NO_CARRIAGE_RETURN]),
            ),
            (
                "carriage return in db-after only",
                export.clone(),
                with(150_000, b"\r"),
                incomplete(vec![DB_BEFORE_EQUALS_AFTER, NO_CARRIAGE_RETURN]),
            ),
        ];
        // Every cut of a two-, three- and four-byte character by a piece
        // boundary is still UTF-8.
        let cuts = [
            ("é cut 1+1", "é", 1),
            ("€ cut 1+2", "€", 1),
            ("€ cut 2+1", "€", 2),
            ("😀 cut 1+3", "😀", 1),
            ("😀 cut 2+2", "😀", 2),
            ("😀 cut 3+1", "😀", 3),
        ];
        for (label, character, first) in cuts {
            let both = split(character, first);
            cases.push((label, both.clone(), both, Ok(Readiness::Ready)));
        }
        for (n, (label, before, after, expected)) in cases.into_iter().enumerate() {
            let files = replaced(ready_files("^\n"), DB_BEFORE, Some(&before));
            let files = replaced(files, DB_AFTER, Some(&after));
            let id = format!("case-{n}");
            let dir = write_case(golden, &id, &files)?;
            match (load_case(golden, &id), expected) {
                (CaseLoad::Recorded(case), Ok(expected)) => {
                    assert_eq!(case.readiness, expected, "for {label}");
                    assert_eq!(readiness(&dir), expected, "for {label}");
                }
                (loaded, Err((name, cause))) => assert_eq!(
                    loaded,
                    CaseLoad::Unreadable(format!("{}: {cause}", dir.join(name).display())),
                    "for {label}"
                ),
                (loaded, Ok(_)) => return Err(format!("{label} did not load: {loaded:?}")),
            }
        }
        scratch.close()
    }

    #[test]
    fn utf8_check_agrees_with_from_utf8_at_every_cut() {
        let samples: [&[u8]; 11] = [
            b"",
            b"ZWR\n",
            "aé€😀z".as_bytes(),
            b"\xc3",
            b"\xe2\x82",
            b"\xf0\x9f\x98",
            b"a\xff b",
            b"\xed\xa0\x80",
            b"\xc0\xaf",
            b"\xf4\x90\x80\x80",
            b"\xf0\x9f\x98\x80\x80",
        ];
        for sample in samples {
            let expected = std::str::from_utf8(sample).is_ok();
            for first in 0..=sample.len() {
                for second in first..=sample.len() {
                    let mut check = Utf8Check::default();
                    let pieces = [
                        sample.get(..first),
                        sample.get(first..second),
                        sample.get(second..),
                    ];
                    for piece in pieces.into_iter().flatten() {
                        check.feed(piece);
                    }
                    assert_eq!(
                        check.is_valid(),
                        expected,
                        "for {sample:?} cut at {first} and {second}"
                    );
                }
            }
        }
    }

    #[test]
    fn fill_fills_each_piece_and_retries_interrupted_reads() -> Result<(), String> {
        /// A reader giving `data` at most `step` bytes a read, every other
        /// read failing as interrupted.
        struct Trickle {
            data: Vec<u8>,
            at: usize,
            step: usize,
            interrupt: bool,
        }

        impl Read for Trickle {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.interrupt = !self.interrupt;
                if self.interrupt {
                    return Err(io::Error::from(ErrorKind::Interrupted));
                }
                let rest = self.data.get(self.at..).unwrap_or_default();
                let count = rest.len().min(self.step).min(buf.len());
                if let (Some(to), Some(from)) = (buf.get_mut(..count), rest.get(..count)) {
                    to.copy_from_slice(from);
                }
                self.at = self.at.saturating_add(count);
                Ok(count)
            }
        }

        let data: Vec<u8> = (0..25).collect();
        let mut reader = Trickle {
            data: data.clone(),
            at: 0,
            step: 3,
            interrupt: false,
        };
        let mut chunk = [0u8; 10];
        let mut read = Vec::new();
        for expected in [10, 10, 5, 0] {
            let count = fill(&mut reader, &mut chunk).map_err(|e| e.to_string())?;
            assert_eq!(count, expected);
            read.extend_from_slice(chunk.get(..count).unwrap_or_default());
        }
        assert_eq!(read, data);
        Ok(())
    }

    /// The opened file's own type decides admission, whatever its path
    /// showed when inspected: an object that is not a regular file once
    /// opened is refused unread, and a regular file is kept with its length
    /// and read through the same handle.
    #[test]
    fn admit_keeps_only_what_is_a_regular_file_once_opened() -> Result<(), String> {
        #[cfg(target_os = "linux")]
        use std::os::fd::AsRawFd;
        // A pipe whose writer is closed: its `/proc` path opens as a FIFO.
        #[cfg(target_os = "linux")]
        let (reader, writer) = io::pipe().map_err(|e| format!("cannot create a pipe: {e}"))?;
        #[cfg(target_os = "linux")]
        drop(writer);
        #[cfg(target_os = "linux")]
        let fifo = Some((
            "a FIFO",
            PathBuf::from(format!("/proc/self/fd/{}", reader.as_raw_fd())),
        ));
        #[cfg(not(target_os = "linux"))]
        let fifo = None;
        let scratch = Scratch::new()?;
        let refused = [
            ("a directory", scratch.path().to_path_buf()),
            ("a character device", PathBuf::from("/dev/null")),
        ];
        for (what, path) in refused.into_iter().chain(fifo) {
            let unread = matches!(
                admit(&path),
                Err((false, Content::Unreadable(cause))) if cause == "not a regular file"
            );
            assert!(
                unread,
                "{what} at {} was not refused as not a regular file",
                path.display()
            );
        }
        let regular = scratch.path().join("regular");
        let written = b"ZWR\n^X(1)=\"a\"\n";
        fs::write(&regular, written)
            .map_err(|e| format!("cannot write {}: {e}", regular.display()))?;
        let (mut file, length) = admit(&regular)
            .map_err(|(found, _)| format!("{} was refused, regular {found}", regular.display()))?;
        let mut read = Vec::new();
        file.read_to_end(&mut read)
            .map_err(|e| format!("cannot read {}: {e}", regular.display()))?;
        assert_eq!(usize::try_from(length).ok(), Some(written.len()));
        assert_eq!(read, written);
        scratch.close()
    }

    #[test]
    fn io_cause_is_free_of_parentheses() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let looped = scratch.path().join("loop");
        std::os::unix::fs::symlink("loop", &looped).map_err(|e| e.to_string())?;
        let Err(filesystem_loop) = fs::metadata(&looped) else {
            return Err("a symbolic link to itself gave metadata".to_string());
        };
        let cases = [
            (filesystem_loop, "filesystem loop or indirection limit"),
            (
                io::Error::from(ErrorKind::PermissionDenied),
                "permission denied",
            ),
            (io::Error::from(ErrorKind::NotFound), "entity not found"),
        ];
        for (error, cause) in cases {
            assert_eq!(io_cause(&error), cause, "for {error:?}");
        }
        scratch.close()
    }

    #[test]
    fn uninspectable_case_folder_is_unreadable_not_absent() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        // GC-01 is a symbolic link to itself, so its metadata fails with a
        // loop error. GC-02 is a READY case whose inputs the request equals.
        let looped = golden.join("GC-01");
        std::os::unix::fs::symlink("GC-01", &looped).map_err(|e| e.to_string())?;
        write_case(golden, "GC-02", &ready_files("^\n"))?;
        let cause = format!("{}: filesystem loop or indirection limit", looped.display());
        assert_eq!(
            load_case(golden, "GC-01"),
            CaseLoad::Unreadable(cause.clone())
        );
        assert_eq!(readiness(&looped), Readiness::Incomplete(CHECKS.to_vec()));
        // It cannot be confirmed absent, so it counts as recorded, as FAIL.
        assert!(is_recorded(&looped));
        assert_eq!(
            status(golden),
            Status {
                recorded: vec!["GC-01", "GC-02"],
                ready: vec!["GC-02"],
                incomplete: vec![("GC-01", CHECKS.to_vec())],
            }
        );
        assert_eq!(find_capture(golden, &strings(&["^"])), Err(cause));
        // A file used as the golden folder holds no case folder.
        let file = golden.join("GC-02").join(INPUT);
        assert_eq!(load_case(&file, "GC-01"), CaseLoad::Absent);
        assert_eq!(readiness(&file.join("GC-01")), Readiness::Absent);
        assert_eq!(
            status(&file),
            Status {
                recorded: Vec::new(),
                ready: Vec::new(),
                incomplete: Vec::new(),
            }
        );
        assert_eq!(find_capture(&file, &strings(&["^"])), Ok(None));
        scratch.close()
    }

    #[test]
    fn unsearchable_golden_folder_is_unreadable_not_absent() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        /// Gives the folder back its search permission when dropped, on a
        /// failed assertion too, before the [`Scratch`] owning it is removed;
        /// a test that reaches its end drops it before [`Scratch::close`].
        struct Searchable<'a>(&'a Path);

        impl Drop for Searchable<'_> {
            fn drop(&mut self) {
                let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o700));
            }
        }

        let scratch = Scratch::new()?;
        let golden = scratch.path();
        write_case(golden, "GC-01", &ready_files("^\n"))?;
        let searchable = Searchable(golden);
        fs::set_permissions(golden, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        if fs::metadata(golden.join("GC-01")).is_ok() {
            println!("NOT RUN: permission bits are not enforced for this user");
            drop(searchable);
            return scratch.close();
        }
        // In a golden folder the reader may not search, no case can be
        // confirmed absent either, so each is recorded and unreadable.
        for id in CASE_IDS {
            let case_dir = golden.join(id);
            let cause = format!("{}: permission denied", case_dir.display());
            assert_eq!(load_case(golden, id), CaseLoad::Unreadable(cause));
            assert_eq!(readiness(&case_dir), Readiness::Incomplete(CHECKS.to_vec()));
            assert!(is_recorded(&case_dir));
        }
        assert_eq!(
            status(golden),
            Status {
                recorded: CASE_IDS.to_vec(),
                ready: Vec::new(),
                incomplete: CASE_IDS.iter().map(|&id| (id, CHECKS.to_vec())).collect(),
            }
        );
        assert_eq!(
            find_capture(golden, &strings(&["^"])),
            Err(format!(
                "{}: permission denied",
                golden.join("GC-01").display()
            ))
        );
        drop(searchable);
        scratch.close()
    }

    #[test]
    fn find_capture_first_match_in_case_order() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        assert_eq!(find_capture(golden, &strings(&["^"])), Ok(None));
        // GC-01 holds the same inputs but is not recorded, so it is skipped.
        write_case(golden, "GC-01", &[(INPUT, b"^\n".to_vec())])?;
        write_case(golden, "GC-02", &ready_files("^\n"))?;
        write_case(golden, "GC-03", &ready_files("\nGC3003\n^\n"))?;
        let incomplete = replaced(ready_files("^\n"), DB_AFTER, None);
        write_case(golden, "GC-04", &incomplete)?;
        let found = find_capture(golden, &strings(&["^"]))?;
        assert_eq!(found.map(|case| case.id), Some("GC-02".to_string()));
        let found = find_capture(golden, &strings(&["", "GC3003", "^"]))?;
        assert_eq!(found.map(|case| case.id), Some("GC-03".to_string()));
        assert_eq!(find_capture(golden, &strings(&["x"])), Ok(None));
        assert_eq!(find_capture(golden, &[]), Ok(None));
        // An unreadable GC-05 fails, with its cause, a lookup that no earlier
        // case matched; a lookup GC-02 matches stops there and still answers.
        let files = replaced(ready_files("^\n"), TRANSCRIPT, Some(b"\xff"));
        let dir = write_case(golden, "GC-05", &files)?;
        let cause = format!("{}: not UTF-8", dir.join(TRANSCRIPT).display());
        assert_eq!(
            find_capture(golden, &strings(&["no", "such", "inputs"])),
            Err(cause)
        );
        let found = find_capture(golden, &strings(&["^"]))?;
        assert_eq!(found.map(|case| case.id), Some("GC-02".to_string()));
        scratch.close()
    }

    #[test]
    fn find_capture_reports_a_failing_case_before_a_later_match() -> Result<(), String> {
        /// What GC-01's file is made.
        enum Made {
            Bytes(Vec<u8>),
            Sparse(u64),
            Directory,
        }
        let not_utf8_late = padded(DB_TEXT.as_bytes(), b'a', b"\xff\n", 150_000);
        // GC-01's `input.txt`, the file then made unreadable, how, and the
        // cause. GC-02 is a READY case whose inputs the request equals, so
        // every GC-01 but the last is a case the request does not match.
        let cases = [
            (
                "\nGC1001\n^\n",
                TRANSCRIPT,
                Made::Bytes(b"\xff\n".to_vec()),
                "not UTF-8",
            ),
            (
                "\nGC1001\n^\n",
                META,
                Made::Sparse(65_537),
                "larger than 65536 bytes",
            ),
            (
                "\nGC1001\n^\n",
                DB_AFTER,
                Made::Bytes(not_utf8_late.clone()),
                "not UTF-8",
            ),
            (
                "\nGC1001\n^\n",
                DB_BEFORE,
                Made::Sparse(67_108_865),
                "larger than 67108864 bytes",
            ),
            (
                "\nGC1001\n^\n",
                NORMALISATION,
                Made::Directory,
                "not a regular file",
            ),
            ("", INPUT, Made::Bytes(b"\xff\n".to_vec()), "not UTF-8"),
            (
                "",
                INPUT,
                Made::Sparse(1_048_577),
                "larger than 1048576 bytes",
            ),
            ("^\n", DB_AFTER, Made::Bytes(not_utf8_late), "not UTF-8"),
        ];
        for (input, name, made, cause) in cases {
            let scratch = Scratch::new()?;
            let golden = scratch.path();
            let dir = write_case(golden, "GC-01", &replaced(ready_files(input), name, None))?;
            let path = dir.join(name);
            match made {
                Made::Bytes(bytes) => fs::write(&path, bytes).map_err(|e| e.to_string())?,
                Made::Sparse(length) => write_sparse(&path, length)?,
                Made::Directory => fs::create_dir(&path).map_err(|e| e.to_string())?,
            }
            write_case(golden, "GC-02", &ready_files("^\n"))?;
            let cause = format!("{}: {cause}", path.display());
            assert_eq!(
                load_case(golden, "GC-01"),
                CaseLoad::Unreadable(cause.clone())
            );
            assert_eq!(find_capture(golden, &strings(&["^"])), Err(cause));
            scratch.close()?;
        }
        Ok(())
    }

    #[test]
    fn find_capture_loads_the_matching_case_as_load_case_does() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        write_case(golden, "GC-01", &ready_files("\nGC1001\n^\n"))?;
        // GC-02 holds the same inputs as GC-03 but is not recorded.
        write_case(golden, "GC-02", &[(INPUT, b"\nGC3003\n^\n".to_vec())])?;
        let files = replaced(ready_files("\nGC3003\n^\n"), RETURNED, Some(b"1\n"));
        let files = replaced(files, DB_AFTER, Some(b"ZWR\n"));
        write_case(golden, "GC-03", &files)?;
        for (id, inputs) in [
            ("GC-01", ["", "GC1001", "^"]),
            ("GC-03", ["", "GC3003", "^"]),
        ] {
            let CaseLoad::Recorded(case) = load_case(golden, id) else {
                return Err(format!("{id} is not recorded"));
            };
            assert_eq!(find_capture(golden, &strings(&inputs)), Ok(Some(case)));
        }
        let CaseLoad::Recorded(case) = load_case(golden, "GC-03") else {
            return Err("GC-03 is not recorded".to_string());
        };
        assert_eq!(case.returned, Some("1".to_string()));
        assert_eq!(
            case.readiness,
            Readiness::Incomplete(vec![DB_BEFORE_EQUALS_AFTER])
        );
        scratch.close()
    }

    #[test]
    fn live_start_takes_first_parsable_meta() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        let none = Err(format!("NO META.TXT IN {}", golden.display()));
        assert_eq!(live_start(golden), none);
        write_case(golden, "GC-01", &[(INPUT, b"^\n".to_vec())])?;
        write_case(golden, "GC-02", &[(META, b"not meta\n".to_vec())])?;
        write_case(golden, "GC-03", &[(META, b"\xff\n".to_vec())])?;
        assert_eq!(live_start(golden), none);
        write_case(golden, "GC-04", &[(META, META_TEXT.as_bytes().to_vec())])?;
        write_case(
            golden,
            "GC-05",
            &[(META, b"docker_exec_args=other\ncontext={}\n".to_vec())],
        )?;
        assert_eq!(
            live_start(golden),
            Ok((
                strings(&["su", "-", "vehu", "-c", "X"]),
                pairs(&[
                    ("DUZ", ".5"),
                    ("IOF", "#"),
                    ("IOSL", "60"),
                    ("STATION", "500"),
                ])
            ))
        );
        scratch.close()?;
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        write_case(
            golden,
            "GC-01",
            &[(META, b"docker_exec_args=a  b\ncontext={}\n".to_vec())],
        )?;
        assert_eq!(
            live_start(golden),
            Err(
                "docker_exec_args: empty argument from a leading, trailing or doubled space"
                    .to_string()
            )
        );
        scratch.close()?;
        let scratch = Scratch::new()?;
        let golden = scratch.path();
        write_case(
            golden,
            "GC-02",
            &[(META, b"docker_exec_args=a b\n".to_vec())],
        )?;
        assert_eq!(live_start(golden), Err("missing key context".to_string()));
        scratch.close()
    }

    #[test]
    fn default_dir_is_the_crate_golden_folder() {
        let dir = default_dir();
        assert!(dir.starts_with(env!("CARGO_MANIFEST_DIR")));
        assert!(dir.ends_with(Path::new("tests").join("golden").join("ifcap")));
    }

    #[test]
    fn constants_follow_the_capture_convention() {
        assert_eq!(CASE_IDS, ["GC-01", "GC-02", "GC-03", "GC-04", "GC-05"]);
        assert_eq!(
            CAPTURE_FILES,
            [
                "input.txt",
                "transcript.txt",
                "returned.txt",
                "db-before.zwr",
                "db-after.zwr",
                "meta.txt",
                "normalisation.txt",
            ]
        );
        assert_eq!(
            CHECKS,
            [
                "seven_files_present",
                "meta_keys_present",
                "input_ends_with_exit",
                "normalisation_parses",
                "db_before_equals_after",
                "no_carriage_return",
            ]
        );
        assert_eq!(META_KEYS.len(), 8);
        assert_eq!(EXIT_INPUT, "^");
    }
}
