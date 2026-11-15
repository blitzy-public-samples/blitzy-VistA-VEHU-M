//! The golden captures of OPTION `PRCH DISPLAY PO` (`EN3^PRCHRPT`) recorded
//! on `worldvista/vehu:202609`, and the only reader of their files (AAP
//! 0.3.2; conventions restated in `tests/golden/ifcap/README.md`).
//!
//! Each case `GC-01` to `GC-05` is one folder holding the seven
//! [`CAPTURE_FILES`], every line ended by `\n`:
//!
//! - `input.txt`: the typed inputs, one per line ([`split_input_lines`]);
//! - `transcript.txt`: the original's standard output, compared raw;
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
//! The bridge, its binary and both integration tests obtain capture content,
//! readiness and status only through this module, and none of them
//! re-implements a check. Nothing here writes a file. Golden content judges
//! the port and never derives a rule: the inquiry, store and M model never
//! reference this module.

use std::collections::HashSet;
use std::fs;
use std::io;
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
///   readable and byte-identical;
/// - `no_carriage_return`: no capture file that is present contains the byte
///   `\r`; a present file that cannot be read fails this check.
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
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n').map(str::to_string).collect()
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
    pub open: String,
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
/// stands) accepts any value, and consumers reading `STUB_CONTEXT` report
/// the key as `UNRESOLVED (O-01)`. Then each capture key `STUB_CONTEXT`
/// lacks fails, in capture order, as `CONTEXT MISMATCH <key>: expected
/// ABSENT, actual <value>`. The first failure is returned. No capture value
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

/// What one capture file gave when read.
enum Content {
    /// Nothing exists at its path.
    Missing,
    /// Something exists at its path but gave no bytes: the cause, free of
    /// parentheses so it can stand inside a `NOT RECORDED (unreadable: …)`
    /// body.
    Unreadable(String),
    Bytes(Vec<u8>),
}

/// One capture file of a case folder, read once.
struct CaptureFile {
    name: &'static str,
    path: PathBuf,
    /// It exists as a regular file, a symbolic link to one included.
    regular: bool,
    content: Content,
}

/// The seven capture files of one case folder, each read once, so that the
/// readiness checks and the loaded content describe the same bytes.
struct Snapshot {
    files: Vec<CaptureFile>,
}

impl Snapshot {
    fn read(case_dir: &Path) -> Snapshot {
        let files = CAPTURE_FILES
            .iter()
            .map(|&name| {
                let path = case_dir.join(name);
                let (regular, content) = read_capture_file(&path);
                CaptureFile {
                    name,
                    path,
                    regular,
                    content,
                }
            })
            .collect();
        Snapshot { files }
    }

    fn file(&self, name: &str) -> Option<&CaptureFile> {
        self.files.iter().find(|file| file.name == name)
    }

    fn bytes(&self, name: &str) -> Option<&[u8]> {
        match self.file(name).map(|file| &file.content) {
            Some(Content::Bytes(bytes)) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    /// The file's text, when it was read and is UTF-8.
    fn text(&self, name: &str) -> Option<&str> {
        self.bytes(name)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }

    fn passes(&self, check: &str) -> bool {
        match check {
            SEVEN_FILES_PRESENT => self.files.iter().all(|file| file.regular),
            META_KEYS_PRESENT => self
                .text(META)
                .and_then(|text| parse_meta(text).ok())
                .is_some_and(|meta| META_KEYS.iter().all(|key| meta.get(key).is_some())),
            INPUT_ENDS_WITH_EXIT => self.text(INPUT).is_some_and(|text| {
                split_input_lines(text).last().map(String::as_str) == Some(EXIT_INPUT)
            }),
            NORMALISATION_PARSES => self
                .text(NORMALISATION)
                .is_some_and(|text| parse_normalisation(text).is_ok()),
            DB_BEFORE_EQUALS_AFTER => match (self.bytes(DB_BEFORE), self.bytes(DB_AFTER)) {
                (Some(before), Some(after)) => before == after,
                _ => false,
            },
            NO_CARRIAGE_RETURN => self.files.iter().all(|file| match &file.content {
                Content::Missing => true,
                Content::Unreadable(_) => false,
                Content::Bytes(bytes) => !bytes.contains(&b'\r'),
            }),
            // Every name in CHECKS has its arm above; any other name is not a
            // check this module knows, so it fails rather than passes.
            _ => false,
        }
    }

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
}

/// Whether `path` is a regular file, and what reading it gave. Only a
/// regular file is read, so a FIFO or device in a case folder can never
/// block a reader. Nothing is written.
fn read_capture_file(path: &Path) -> (bool, Content) {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (false, Content::Missing),
        Err(e) => (false, Content::Unreadable(io_cause(&e))),
        Ok(entry) if !entry.is_file() => {
            (false, Content::Unreadable("not a regular file".to_string()))
        }
        Ok(_) => match fs::read(path) {
            Ok(bytes) => (true, Content::Bytes(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (false, Content::Missing),
            Err(e) => (true, Content::Unreadable(io_cause(&e))),
        },
    }
}

/// The cause of a failed read as its `io::ErrorKind` description, such as
/// `permission denied`. The `io::Error` text itself ends in `(os error N)`,
/// and a cause must stay free of parentheses because the bridge sets it
/// inside `NOT RECORDED (unreadable: <cause>)`.
fn io_cause(error: &io::Error) -> String {
    error.kind().to_string()
}

fn is_directory(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|entry| entry.is_dir())
}

fn is_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|entry| entry.is_file())
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
    Absent,
    Incomplete(Vec<&'static str>),
    Ready,
}

/// The [`Readiness`] of the case folder `case_dir` (AAP 0.3.2): `Absent`
/// when it is not an existing directory, otherwise the six [`CHECKS`]
/// evaluated in order over its files, read once.
pub fn readiness(case_dir: &Path) -> Readiness {
    if !is_directory(case_dir) {
        return Readiness::Absent;
    }
    Snapshot::read(case_dir).readiness()
}

/// Whether the case folder `case_dir` is recorded, by the request capture
/// convention (AAP 0.3.2): its `input.txt` and `transcript.txt` both exist
/// as files. A case that is not recorded is NOT COMPARED.
pub fn is_recorded(case_dir: &Path) -> bool {
    is_file(&case_dir.join(INPUT)) && is_file(&case_dir.join(TRANSCRIPT))
}

/// The capture state of the five cases (AAP 0.3.2), each list in
/// [`CASE_IDS`] order: the recorded ids, the READY ids, and each INCOMPLETE
/// id with its failed checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub recorded: Vec<&'static str>,
    pub ready: Vec<&'static str>,
    pub incomplete: Vec<(&'static str, Vec<&'static str>)>,
}

/// The [`Status`] of the golden folder `golden_dir`, read afresh on every
/// call (AAP 0.3.2): what `GET /api/status` reports and what the bridge
/// binary counts as `RECORDED <n> OF 5 READY <m> OF 5`. A missing folder
/// gives three empty lists.
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
    /// The case id, which is also its folder name.
    pub id: String,
    /// [`split_input_lines`] of `input.txt`.
    pub inputs: Vec<String>,
    /// `transcript.txt`, raw.
    pub transcript: String,
    /// [`read_returned`] of `returned.txt`; `None` when it is absent.
    pub returned: Option<String>,
    /// The rules of `normalisation.txt`; none when it is absent or does not
    /// parse.
    pub rules: Vec<Rule>,
    /// `meta.txt`, when it is present and parses.
    pub meta: Option<Meta>,
    pub readiness: Readiness,
}

/// What [`load_case`] found for one case id (AAP 0.3.2; verdict terms of AAP
/// 0.7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseLoad {
    /// The case folder does not exist: NOT COMPARED.
    Absent,
    /// The folder exists but `input.txt` or `transcript.txt` is missing: NOT
    /// COMPARED. The failed checks are carried for the readiness report.
    NotRecorded(Vec<&'static str>),
    Recorded(Case),
    /// A capture file that is present cannot be read or is not UTF-8: its
    /// path, `: ` and the cause. The case is FAIL.
    Unreadable(String),
}

/// Loads case `case_id` from `golden_dir/case_id`, the one way any consumer
/// obtains a case's content (AAP 0.3.2). Each of the seven files is read
/// once, and every file present must be readable UTF-8; then a case missing
/// `input.txt` or `transcript.txt` is `NotRecorded`, and any other is
/// `Recorded`.
pub fn load_case(golden_dir: &Path, case_id: &str) -> CaseLoad {
    let case_dir = golden_dir.join(case_id);
    if !is_directory(&case_dir) {
        return CaseLoad::Absent;
    }
    let snapshot = Snapshot::read(&case_dir);
    // Every capture file is text by convention, the ZWR exports included, so
    // any present file that is not UTF-8 makes the whole case unreadable
    // rather than loading part of it.
    for file in &snapshot.files {
        let cause = match &file.content {
            Content::Missing => continue,
            Content::Unreadable(cause) => cause.clone(),
            Content::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => "not UTF-8".to_string(),
            Content::Bytes(_) => continue,
        };
        return CaseLoad::Unreadable(format!("{}: {cause}", file.path.display()));
    }
    let readiness = snapshot.readiness();
    let (Some(input), Some(transcript)) = (snapshot.text(INPUT), snapshot.text(TRANSCRIPT)) else {
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
        inputs: split_input_lines(input),
        transcript: transcript.to_string(),
        returned: snapshot.text(RETURNED).and_then(read_returned),
        rules: snapshot
            .text(NORMALISATION)
            .and_then(|text| parse_normalisation(text).ok())
            .unwrap_or_default(),
        meta: snapshot.text(META).and_then(|text| parse_meta(text).ok()),
        readiness,
    })
}

/// The capture the MUMPS replay side answers `inputs` with (AAP 0.3.2): the
/// first `Recorded` case, in [`CASE_IDS`] order, whose inputs equal
/// `inputs`, or `None`. An `Unreadable` case met on the way is `Err` with
/// its cause at once. The golden folder is read on every call, so new
/// captures need no restart.
pub fn find_capture(golden_dir: &Path, inputs: &[String]) -> Result<Option<Case>, String> {
    for id in CASE_IDS {
        match load_case(golden_dir, id) {
            CaseLoad::Unreadable(cause) => return Err(cause),
            CaseLoad::Recorded(case) if case.inputs == inputs => return Ok(Some(case)),
            CaseLoad::Absent | CaseLoad::NotRecorded(_) | CaseLoad::Recorded(_) => {}
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
        let (_, content) = read_capture_file(&golden_dir.join(id).join(META));
        let Content::Bytes(bytes) = content else {
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
        "context={\"DUZ\":\".5\",\"IOF\":\"#\",\"IOSL\":\"24\",\"STATION\":\"500\"}\n",
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
            context(r#"{"DUZ":".5","IOF":"\u001b[2J","IOSL":"24","STATION":"500"}"#),
            Ok(pairs(&[
                ("DUZ", ".5"),
                ("IOF", "\u{1b}[2J"),
                ("IOSL", "24"),
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
        let accepted = [
            pairs(&[
                ("DUZ", ".5"),
                ("IOF", "#"),
                ("IOSL", "24"),
                ("STATION", "500"),
            ]),
            pairs(&[
                ("STATION", "500"),
                ("IOSL", "60"),
                ("IOF", "#,$C(27,91,50,74,27,91,72)"),
                ("DUZ", ".5"),
            ]),
        ];
        for context in accepted {
            assert_eq!(validate_context(&context), Ok(()), "for {context:?}");
        }
        let rejected = [
            (
                pairs(&[
                    ("DUZ", "1"),
                    ("IOF", "#"),
                    ("IOSL", "24"),
                    ("STATION", "500"),
                ]),
                "CONTEXT MISMATCH DUZ: expected .5, actual 1",
            ),
            (
                pairs(&[("DUZ", ".5"), ("IOF", "#"), ("IOSL", "24")]),
                "CONTEXT MISMATCH STATION: expected 500, actual ABSENT",
            ),
            (
                pairs(&[("DUZ", ".5"), ("IOSL", "24"), ("STATION", "500")]),
                "CONTEXT MISMATCH IOF: expected UNRESOLVED (O-01), actual ABSENT",
            ),
            (
                pairs(&[
                    ("DUZ", ".5"),
                    ("IOF", "#"),
                    ("IOSL", "24"),
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
                    ("IOSL", "24"),
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
        // An unreadable case is an error at once, before a later match.
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
                    ("IOSL", "24"),
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
