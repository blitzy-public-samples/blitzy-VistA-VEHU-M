//! The read-only global store that stands in for the FileMan globals the
//! ported `PRCH DISPLAY PO` routines read (`^PRC(442)`, `^PRCD(442.3)`,
//! `^PRC(440)`, `^PRC(411)` and the rest), loaded from ZWR exports: ZWR
//! parsing and the immutable ordered map with M collation (AAP 0.3.1). The
//! exports follow the fixture ZWR convention: header lines, such as
//! `OSEHRA ZGO Export: PURCHASE ORDER STATUS` and `ZWR` at
//! tests/fixtures/ifcap/442.3.zwr:1-2, then one `^name(subscripts)=value`
//! line per node.
//!
//! This is the one ZWR loader of the crate (AAP 0.3.3 Shared components and
//! Data reach): the bridge's `serve`, the equivalence harness and the unit
//! tests load fixtures through [`Store::from_zwr`] and [`load_fixtures`],
//! and the bridge tests reach the store only through `bridge::serve`. A
//! [`Store`] is borrowed immutably by the inquiry and has no mutator (AAP
//! 0.3.2): nothing changes it after construction, so nothing an inquiry
//! does can write to it.
//!
//! Keys are subscript vectors whose first element is the global name
//! without its caret (`^PRC(442,1,0)` is `[Str("PRC"), Num("442"),
//! Num("1"), Num("0")]`). They order by M default collation (AAP 0.3.4
//! Collation rule): every number before every string, numbers by exact
//! value, strings by bytes. Values are the node data exactly as exported,
//! with only the ZWR quoting removed.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::mumps;

/// One subscript of a global reference, or the global name in the first
/// position of a key, as the AAP 0.3.4 Collation rule defines a `Store`
/// key.
///
/// Invariant: `Num` holds the canonical text of a number (`.5`, `500`,
/// `10000000044`, the last at tests/fixtures/ifcap/411.zwr:124; never
/// `0.5`, `01` or `1.0`), and `Str` holds text that is not a canonical
/// number. [`Sub::from_text`] and the loader keep it, and callers must
/// construct subscripts through them unless the text is a literal already
/// known to meet it. A `Num` built from non-canonical text, or a `Str` from
/// canonical text, violates the invariant and is not the subscript M names
/// for that text. The order stays consistent with the derived `Eq` and
/// `Hash` for every value, because a numeric tie is broken on the raw text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sub {
    /// A canonical number, as its canonical text, such as the IENs of
    /// `^PRC(411)` at tests/fixtures/ifcap/411.zwr:4-288.
    Num(String),
    /// A string subscript, or the global name, such as `"AC"` at
    /// tests/fixtures/ifcap/411.zwr:290 or the quoted `"008202226"` at
    /// tests/fixtures/ifcap/440.zwr:14411.
    Str(String),
}

impl Sub {
    /// The subscript that `text` names. In M a subscript's identity is its
    /// text: canonical number text is a number, anything else is a string,
    /// so the quoted subscript `"500"` is the number 500 while `"001"` and
    /// `"04"` stay strings (`^DIC(49,"D","04",1013)` at
    /// tests/fixtures/ifcap/49.zwr:269). It is the canonical-number test
    /// FileMan applies as `+$P(X,"E")=X` under tag `GRV` at DIC.m:48 before
    /// `BYIEN2^DIC5` (DIC5.m:21) tries a lookup value as an IEN (PO-09).
    pub fn from_text(text: &str) -> Sub {
        if mumps::is_canonical(text) {
            Sub::Num(text.to_string())
        } else {
            Sub::Str(text.to_string())
        }
    }

    /// The subscript's text: the canonical number text or the string, the
    /// value `$ORDER` gives M code, as it gives `PRCHDI` in the item loop of
    /// `^PRCHDP2` at PRCHDP2.m:11 (PO-25).
    pub fn text(&self) -> &str {
        match self {
            Sub::Num(text) | Sub::Str(text) => text,
        }
    }

    /// True for a numeric subscript, which sorts before every string under
    /// the AAP 0.3.4 Collation rule.
    pub fn is_num(&self) -> bool {
        matches!(self, Sub::Num(_))
    }
}

impl Ord for Sub {
    /// M default collation (AAP 0.3.4 Collation rule): numbers before
    /// strings, numbers by exact decimal value, strings by bytes. The 411
    /// export lists its numeric IENs before `"AC"` and `"B"` in this order
    /// (tests/fixtures/ifcap/411.zwr:4-297). The item loop
    /// `$O(^PRC(442,D0,2,PRCHDI)) Q:PRCHDI'>0` of `^PRCHDP2` at
    /// PRCHDP2.m:11 (PO-25), over file 442 field 40 ITEM at node 2
    /// (DD.zwr:514888), relies on numeric items preceding the string
    /// subscripts, such as the `"AC"` index `CON^PRCHDP1` reads at
    /// PRCHDP1.m:61 (PO-20). The lookup's `"A["` test under tag `W` at
    /// DICM.m:71 and its next-index walk under tag `M` at DIC3.m:63 rely on
    /// byte order (PO-09).
    ///
    /// Two numbers of equal value are then ordered by the bytes of their
    /// text. Canonical texts of equal value are identical, so this never
    /// reorders loaded keys; it keeps the order total and consistent with
    /// the derived `Eq` for a `Num` built from non-canonical text.
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Sub::Num(a), Sub::Num(b)) => {
                mumps::cmp_numbers(a, b).then_with(|| a.as_bytes().cmp(b.as_bytes()))
            }
            (Sub::Num(_), Sub::Str(_)) => Ordering::Less,
            (Sub::Str(_), Sub::Num(_)) => Ordering::Greater,
            (Sub::Str(a), Sub::Str(b)) => a.as_bytes().cmp(b.as_bytes()),
        }
    }
}

impl PartialOrd for Sub {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for Sub {
    /// The null subscript `""`, M's `$ORDER` seed and end value: the `"AC"`
    /// walk of `CON^PRCHDP1` at PRCHDP1.m:61 (PO-20) advances only while
    /// `X'=""`. [`Store::order`] starts at the first child from it and never
    /// returns it.
    fn default() -> Sub {
        Sub::Str(String::new())
    }
}

/// A malformed ZWR data line. `Display` gives the line number and, when
/// known, the file (AAP interface contract): `MALFORMED ZWR LINE <line>:
/// <message>`, prefixed by `<file>: ` when the text came from a file.
///
/// The text never echoes fixture bytes: every `message` this crate builds is
/// one of the scanner's fixed texts, which name what was expected and quote
/// nothing from the line, because a malformed fixture line can hold a
/// person's or vendor's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A data line that is not `^name(subscripts)=value` under the fixture
    /// ZWR convention ([`Store::from_zwr`]; AAP 0.3.2). `line` is 1-based
    /// and counts every line of the text, header lines included. `file` is
    /// `None` when the text did not come from a file.
    Parse {
        file: Option<PathBuf>,
        line: usize,
        message: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let StoreError::Parse {
            file,
            line,
            message,
        } = self;
        if let Some(file) = file {
            write!(f, "{}: ", file.display())?;
        }
        write!(f, "MALFORMED ZWR LINE {line}: {message}")
    }
}

impl std::error::Error for StoreError {}

/// The globals of one database snapshot, in M collation order (AAP 0.3.4
/// Collation rule). Nothing changes it once built, and every method takes
/// `&self` (AAP 0.3.2; check A9 of AAP 0.7.4).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Store {
    nodes: BTreeMap<Vec<Sub>, String>,
}

impl Store {
    /// Loads ZWR text by the fixture ZWR convention (AAP 0.3.2 contract):
    /// every line before the first line that starts with `^` is a header
    /// line and is ignored, as `OSEHRA ZGO Export: PURCHASE ORDER STATUS`
    /// and `ZWR` are at tests/fixtures/ifcap/442.3.zwr:1-2, blank lines are
    /// ignored, and every other line must be `^name(subscripts)=value`. A
    /// repeated key replaces the earlier value, as `SET` does. Text with no
    /// data lines gives an empty store. One trailing carriage return per
    /// line is dropped, so a CRLF export loads like an LF one.
    ///
    /// Subscripts are canonical numbers or quoted strings (`""` stands for
    /// one `"`), and a quoted canonical number is stored as a number, as M
    /// does. Values are quoted strings or canonical numbers. `$C(n)` and
    /// `_` concatenation are rejected rather than evaluated, because no
    /// fixture holds them; any other form is an error, never a guess. A
    /// malformed data line is a [`StoreError::Parse`] whose `line` is
    /// 1-based and counts every line of `text`, header lines included, and
    /// whose `file` is `None`.
    pub fn from_zwr(text: &str) -> Result<Store, StoreError> {
        let mut nodes = BTreeMap::new();
        let mut in_data = false;
        for (index, raw) in text.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            if !in_data {
                if !line.starts_with('^') {
                    continue;
                }
                in_data = true;
            }
            if line.trim().is_empty() {
                continue;
            }
            let (key, value) = parse_line(line).map_err(|message| StoreError::Parse {
                file: None,
                line: index.saturating_add(1),
                message,
            })?;
            nodes.insert(key, value);
        }
        Ok(Store { nodes })
    }

    /// The value stored at `key`, or `None` when the node has no value.
    /// `None` is how an unguarded reference finds a missing node, where M
    /// raises an `UNDEF` error: the item loop's `^(PRCHDI,0)` of `^PRCHDP2`
    /// at PRCHDP2.m:11 (PO-25), or `$P(^(0),U)` after a bare `$D` under tag
    /// `S` at DIC3.m:110 (PO-09).
    pub fn get(&self, key: &[Sub]) -> Option<&str> {
        mumps::get_opt(&self.nodes, key)
    }

    /// M `$GET`: the value stored at `key`, or `""` when the node has no
    /// value, as `$G(^PRC(442,D0,0))` of `^PRCHDP1` reads the order's zero
    /// node at PRCHDP1.m:4 (PO-10). A guarded read goes through this
    /// accessor, an unguarded one through [`Store::get`], so every `$G`
    /// shares the one `$GET` model in `mumps.rs`.
    pub fn get_or_empty(&self, key: &[Sub]) -> String {
        mumps::get(&self.nodes, key)
    }

    /// M `$DATA`: 0 for no value and no descendants, 1 for a value only,
    /// 10 for descendants only, 11 for both, as `$D(^PRC(442,D0,4,1,0))` of
    /// `^PRCHDP1` tests at PRCHDP1.m:9 (PO-10) and `$D(^PRC(442,D0,6,0))` of
    /// `^PRCHDP2` at PRCHDP2.m:9 (PO-25).
    pub fn data(&self, key: &[Sub]) -> u8 {
        mumps::data(&self.nodes, key)
    }

    /// M forward `$ORDER`: the next child subscript of `parent` after
    /// `after` in collation order, or `None` when there is none. `after` of
    /// `None` or of the null subscript `""` is the seed and gives the first
    /// child. Null subscripts are invisible to it, as to M's `$ORDER`; a
    /// null node is still read through [`Store::get`] and [`Store::data`].
    ///
    /// Uses: the item loop `$O(^PRC(442,D0,2,PRCHDI))` of `^PRCHDP2` at
    /// PRCHDP2.m:11 (PO-25), the node-14 walk `$O(^PRC(442,D0,14,Y))` of
    /// `FOB^PRCHDP1` (tag at PRCHDP1.m:53) at PRCHDP1.m:56 (PO-13), and the
    /// `"AC"` walk `$O(^PRC(442,D0,2,"AC",X))` of `CON^PRCHDP1` at
    /// PRCHDP1.m:61 (PO-20).
    pub fn order(&self, parent: &[Sub], after: Option<&Sub>) -> Option<Sub> {
        mumps::order(&self.nodes, parent, after)
    }

    /// Every node with its value, in collation order (AAP 0.3.4 Collation
    /// rule). No M construct on the path walks the whole store: the
    /// inquiry reads only through `get`, `get_or_empty`, `data` and `order`
    /// (AAP 0.3.3 Data reach).
    pub fn iter(&self) -> impl Iterator<Item = (&[Sub], &str)> {
        self.nodes
            .iter()
            .map(|(key, value)| (key.as_slice(), value.as_str()))
    }

    /// The number of nodes holding a value: one per distinct key of the
    /// loaded ZWR data lines (fixture ZWR convention, [`Store::from_zwr`]).
    /// Like [`Store::iter`], it serves no M construct (AAP 0.3.3 Data
    /// reach).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the store holds no node, as ZWR text without data lines
    /// gives ([`Store::len`]).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The ZWR-style global reference of `key`, written as a fixture ZWR
    /// line writes it before its `=` (fixture ZWR convention), and the
    /// `<node>` of `CORRUPT STORE <node>` (AAP 0.3.2): `^PRC(442,1,2,1,0)`,
    /// `^PRC(442,"C","A12345",1)`. Numbers are written bare, and strings in
    /// quotes with each `"` doubled. A key holding only the global name
    /// gives `^NAME`, and an empty key gives `^`.
    pub fn format_ref(key: &[Sub]) -> String {
        let mut out = String::from("^");
        let Some((name, subscripts)) = key.split_first() else {
            return out;
        };
        out.push_str(name.text());
        if subscripts.is_empty() {
            return out;
        }
        out.push('(');
        for (position, sub) in subscripts.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            match sub {
                Sub::Num(text) => out.push_str(text),
                Sub::Str(text) => {
                    out.push('"');
                    out.push_str(&text.replace('"', "\"\""));
                    out.push('"');
                }
            }
        }
        out.push(')');
        out
    }
}

/// The fixture file that holds the runtime export of the capture snapshot.
/// It loads last, so its values win over the repository exports.
const RUNTIME_EXPORT: &str = "cases.zwr";

/// Loads every `*.zwr` file of `dir`, `tests/fixtures/ifcap` for the
/// bridge and the equivalence harness, into one store (AAP 0.3.3 Data
/// reach). Files load in byte order of name with `cases.zwr` last, so the
/// runtime export wins on repeated keys; a missing `cases.zwr` is not an
/// error. Symbolic links are followed. Entries whose names do not end in
/// `.zwr`, files and directories alike, are ignored.
///
/// Errors follow the AAP 0.3.2 contract. A directory that cannot be listed,
/// an entry or its metadata that cannot be read, or a `*.zwr` file that
/// cannot be read or is not UTF-8 is an [`io::Error`] of the original kind
/// whose message names the path and says `cannot read`. A `*.zwr` entry
/// that is not a regular file once links are followed, such as a directory
/// or a FIFO, is an [`io::ErrorKind::InvalidInput`] error reading
/// `<path>: cannot read: not a regular file`. An entry whose metadata
/// already shows it is not a regular file is never opened. Any other is
/// opened without waiting for a FIFO writer on a platform whose
/// `O_NONBLOCK` value is known, and the opened file's own type decides, so
/// an object swapped in after that inspection is refused unread. A
/// malformed line is an [`io::ErrorKind::InvalidData`] error carrying the
/// [`StoreError::Parse`] text, which names the file and line.
pub fn load_fixtures(dir: &Path) -> io::Result<Store> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|err| cannot_read(dir, err))? {
        let entry = entry.map_err(|err| cannot_read(dir, err))?;
        let name = entry.file_name();
        if !name.as_encoded_bytes().ends_with(b".zwr") {
            continue;
        }
        let path = entry.path();
        let file = open_fixture(&path)?;
        files.push((
            name.as_encoded_bytes() == RUNTIME_EXPORT.as_bytes(),
            name,
            path,
            file,
        ));
    }
    files.sort_by(|(a_last, a_name, ..), (b_last, b_name, ..)| {
        a_last
            .cmp(b_last)
            .then_with(|| a_name.as_encoded_bytes().cmp(b_name.as_encoded_bytes()))
    });
    // A later file's nodes replace equal keys of earlier files. The merge
    // runs on the map, before the one `Store` is built, so that `Store`
    // needs no mutator.
    let mut nodes = BTreeMap::new();
    for (_, _, path, mut file) in files {
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|err| cannot_read(&path, err))?;
        let loaded = Store::from_zwr(&text).map_err(|err| {
            let StoreError::Parse { line, message, .. } = err;
            let named = StoreError::Parse {
                file: Some(path.clone()),
                line,
                message,
            };
            io::Error::new(io::ErrorKind::InvalidData, named.to_string())
        })?;
        nodes.extend(loaded.nodes);
    }
    Ok(Store { nodes })
}

/// `err` with its kind kept and its message naming `path`.
fn cannot_read(path: &Path, err: io::Error) -> io::Error {
    io::Error::new(
        err.kind(),
        format!("{}: cannot read: {err}", path.display()),
    )
}

/// The `*.zwr` entry at `path`, opened for reading once its metadata shows
/// a regular file and kept only as [`admit`] keeps it.
fn open_fixture(path: &Path) -> io::Result<File> {
    // Follows symbolic links, so a linked fixture loads like a copy.
    let metadata = fs::metadata(path).map_err(|err| cannot_read(path, err))?;
    // An entry already known not to be a regular file is refused unopened.
    // One swapped in after this inspection is refused by `admit`, whose open
    // does not wait for a FIFO writer where `O_NONBLOCK` is known.
    if !metadata.is_file() {
        return Err(not_regular(path));
    }
    admit(path)
}

/// Opens `path` without waiting ([`open_read_only`]) and keeps the opened
/// file only when its own metadata shows a regular file: whatever stood at
/// `path` when it was inspected, an object that is not a regular file once
/// opened is refused unread ([`not_regular`]). A failed open or metadata
/// read keeps its kind, and every error names `path` ([`cannot_read`]).
fn admit(path: &Path) -> io::Result<File> {
    let file = open_read_only(path).map_err(|err| cannot_read(path, err))?;
    let opened = file.metadata().map_err(|err| cannot_read(path, err))?;
    if opened.is_file() {
        Ok(file)
    } else {
        Err(not_regular(path))
    }
}

/// The [`io::ErrorKind::InvalidInput`] error of an entry at `path` that is
/// not a regular file: `<path>: cannot read: not a regular file`.
fn not_regular(path: &Path) -> io::Error {
    cannot_read(
        path,
        io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"),
    )
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

// One ZWR data line is scanned left to right. The scanner state is `rest`,
// the unconsumed text of the line, and each function below consumes from
// its front. Quoted strings may hold `,`, `)`, `=` and doubled quotes, so
// the line is never split on them.

fn peek(rest: &str) -> Option<char> {
    rest.chars().next()
}

fn eat(rest: &mut &str, c: char) -> bool {
    match rest.strip_prefix(c) {
        Some(tail) => {
            *rest = tail;
            true
        }
        None => false,
    }
}

/// Consumes and returns the longest prefix whose characters satisfy
/// `keep`.
fn take_while<'a>(rest: &mut &'a str, keep: impl Fn(char) -> bool) -> &'a str {
    let text: &'a str = rest;
    let end = text.find(|c: char| !keep(c)).unwrap_or(text.len());
    let (head, tail) = text.split_at_checked(end).unwrap_or((text, ""));
    *rest = tail;
    head
}

/// Consumes a quoted string whose opening quote has been consumed,
/// through its closing quote, turning each `""` into one `"`.
fn quoted(rest: &mut &str) -> Result<String, String> {
    let mut out = String::new();
    loop {
        let Some((chunk, after)) = rest.split_once('"') else {
            return Err("unterminated string".to_string());
        };
        out.push_str(chunk);
        match after.strip_prefix('"') {
            Some(next) => {
                out.push('"');
                *rest = next;
            }
            None => {
                *rest = after;
                return Ok(out);
            }
        }
    }
}

/// Consumes an unquoted number token: digits, letters (so that
/// exponent forms are caught as non-canonical), `.`, `+` and `-`.
fn number_token<'a>(rest: &mut &'a str) -> &'a str {
    take_while(rest, |c| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-')
    })
}

const DOLLAR_C_MESSAGE: &str = "$C() values are not supported";

/// Parses one data line into its key and value, or a message naming what
/// was expected. No message quotes text from the line, because a malformed
/// fixture line can hold a person's or vendor's name and the message
/// reaches the bridge's startup error.
fn parse_line(line: &str) -> Result<(Vec<Sub>, String), String> {
    let mut rest = line;
    if !eat(&mut rest, '^') {
        return Err("expected '^' at the start of a data line".to_string());
    }
    let percent = eat(&mut rest, '%');
    let name_starts_with_letter = peek(rest).is_some_and(|c| c.is_ascii_alphabetic());
    let name = take_while(&mut rest, |c| c.is_ascii_alphanumeric());
    if !percent && !name_starts_with_letter {
        return Err("expected a global name after '^'".to_string());
    }
    let mut key = vec![Sub::Str(format!(
        "{}{name}",
        if percent { "%" } else { "" }
    ))];
    if !eat(&mut rest, '(') {
        return Err("expected '(' after global name".to_string());
    }
    loop {
        key.push(parse_subscript(&mut rest)?);
        if eat(&mut rest, ',') {
            continue;
        }
        if eat(&mut rest, ')') {
            break;
        }
        return Err("expected ',' or ')' after subscript".to_string());
    }
    if !eat(&mut rest, '=') {
        return Err("expected '=' after subscripts".to_string());
    }
    let value = parse_value(&mut rest)?;
    Ok((key, value))
}

/// One subscript: a quoted string, stored through [`Sub::from_text`], or an
/// unquoted canonical number.
fn parse_subscript(rest: &mut &str) -> Result<Sub, String> {
    if eat(rest, '"') {
        return quoted(rest).map(|text| Sub::from_text(&text));
    }
    let token = number_token(rest);
    if token.is_empty() {
        return Err("expected a subscript".to_string());
    }
    if mumps::is_canonical(token) {
        Ok(Sub::Num(token.to_string()))
    } else {
        Err("unquoted subscript is not a canonical number".to_string())
    }
}

/// The value after `=`: a quoted string or a canonical number, and nothing
/// after it.
fn parse_value(rest: &mut &str) -> Result<String, String> {
    let value = if eat(rest, '"') {
        quoted(rest)?
    } else if peek(rest) == Some('$') {
        return Err(DOLLAR_C_MESSAGE.to_string());
    } else {
        let token = number_token(rest);
        if token.is_empty() {
            return Err("expected a quoted string or a number after '='".to_string());
        }
        if !mumps::is_canonical(token) {
            return Err("unquoted value is not a canonical number".to_string());
        }
        token.to_string()
    };
    match peek(rest) {
        None => Ok(value),
        Some('_') => Err(format!("{DOLLAR_C_MESSAGE} ('_' concatenation)")),
        Some(_) => Err("unexpected text after the value".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn num(text: &str) -> Sub {
        Sub::Num(text.to_string())
    }

    fn string(text: &str) -> Sub {
        Sub::Str(text.to_string())
    }

    fn key(name: &str, subs: &[&str]) -> Vec<Sub> {
        let mut key = vec![string(name)];
        key.extend(subs.iter().map(|sub| Sub::from_text(sub)));
        key
    }

    fn load(text: &str) -> Result<Store, String> {
        Store::from_zwr(text).map_err(|e| e.to_string())
    }

    /// A new directory under the system temporary directory, created by this
    /// call and therefore owned by it. Each attempt takes the next name
    /// `ifcap-store-unit-<pid>-<n>` and creates it with `fs::create_dir`,
    /// which fails on any existing entry, a symbolic link included, without
    /// following it. An existing entry is never removed or reused: the next
    /// name is tried instead, up to `ATTEMPTS` names. Callers remove only the
    /// directory returned here.
    fn scratch_dir() -> Result<PathBuf, String> {
        const ATTEMPTS: usize = 64;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir();
        for _ in 0..ATTEMPTS {
            let n = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let dir = base.join(format!("ifcap-store-unit-{}-{n}", std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => return Ok(dir),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
            }
        }
        Err(format!(
            "no unused scratch directory name under {} after {ATTEMPTS} attempts",
            base.display()
        ))
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = dir.join(name);
        fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// Removes the [`scratch_dir`] directory `dir` with everything in it and
    /// then gives back `outcome`, the result of the work done in it. A
    /// failure in `outcome` is returned first; otherwise a directory that
    /// cannot be removed, a missing one included, is an error naming it, so
    /// a passing test cannot leave its directory behind unnoticed.
    fn remove_scratch<T>(dir: &Path, outcome: Result<T, String>) -> Result<T, String> {
        let removed = fs::remove_dir_all(dir);
        let value = outcome?;
        removed.map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
        Ok(value)
    }

    #[test]
    fn header_lines_ignored() -> Result<(), String> {
        let store = load(
            "OSEHRA ZGO Export: X\nZWR\n^PRCD(442.3,1,0)=\"Order Not Completely Prepared^1^3\"\n",
        )?;
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.get(&key("PRCD", &["442.3", "1", "0"])),
            Some("Order Not Completely Prepared^1^3")
        );
        Ok(())
    }

    #[test]
    fn blank_lines_ignored() -> Result<(), String> {
        let store = load("ZWR\n\n^X(1)=\"a\"\n\n   \n\t\n^X(2)=\"b\"\n\n")?;
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(&key("X", &["2"])), Some("b"));
        Ok(())
    }

    #[test]
    fn crlf_line_endings_accepted() -> Result<(), String> {
        let store = load("ZWR\r\n^X(1)=\"a\"\r\n^X(2)=5\r\n")?;
        assert_eq!(store.get(&key("X", &["1"])), Some("a"));
        assert_eq!(store.get(&key("X", &["2"])), Some("5"));
        Ok(())
    }

    #[test]
    fn malformed_line_reports_line_number() -> Result<(), String> {
        match Store::from_zwr("OSEHRA ZGO Export: X\nZWR\n^X(1)=\"a\"\n^X(1=\"b\"\n") {
            Err(StoreError::Parse {
                file: None,
                line: 4,
                ..
            }) => {}
            other => return Err(format!("expected a parse error on line 4, got {other:?}")),
        }
        let message = Store::from_zwr("ZWR\n^X(1)=\"a\"\n\n^X(1=\"b\"\n")
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            message.contains("MALFORMED ZWR LINE 4"),
            "message was {message:?}"
        );
        Ok(())
    }

    #[test]
    fn parse_error_display_without_file() -> Result<(), String> {
        let shown = match Store::from_zwr("ZWR\n^X(1=\"b\"\n") {
            Err(err) => err.to_string(),
            Ok(store) => return Err(format!("expected a parse error, got {store:?}")),
        };
        assert!(
            shown.starts_with("MALFORMED ZWR LINE 2"),
            "message was {shown:?}"
        );
        for data in ["^X(1", "=\"b\"", "\"b\""] {
            assert!(!shown.contains(data), "{shown:?} echoes {data:?}");
        }
        Ok(())
    }

    #[test]
    fn malformed_line_forms_rejected() {
        for line in [
            "^X",
            "^X()=1",
            "^X(1)",
            "^X(1)=",
            "^X(1)=\"a",
            "^X(\"a)=1",
            "^X(1)=\"a\" ",
            "^X(1)=\"a\"\"",
            "^X(1)=1E5",
            "^X(1)=abc",
            "^X(1 )=1",
            "^1X(1)=1",
            "^(1)=1",
            "X(1)=1",
            " ^X(1)=1",
        ] {
            let text = format!("^X(0)=1\n{line}\n");
            assert!(
                matches!(
                    Store::from_zwr(&text),
                    Err(StoreError::Parse { line: 2, .. })
                ),
                "{line:?} should be rejected on line 2"
            );
        }
        // The message gives the line and the cause, and never the line's
        // data, which can hold a person's or vendor's name.
        let value_cause = "unquoted value is not a canonical number";
        let subscript_cause = "unquoted subscript is not a canonical number";
        for (line, data, cause) in [
            ("^X(1)=SMITHJOHN", "SMITHJOHN", value_cause),
            ("^X(1)=7ACMECORP", "7ACMECORP", value_cause),
            ("^X(JONESMARY)=1", "JONESMARY", subscript_cause),
            ("^X(1,DOEJANE)=\"a\"", "DOEJANE", subscript_cause),
            ("^X(00417)=\"a\"", "00417", subscript_cause),
            (
                "^X(1)=\"a\"WILSONPAT",
                "WILSONPAT",
                "unexpected text after the value",
            ),
            (
                "^X(1)=\"a\"_BROWNKIM",
                "BROWNKIM",
                "$C() values are not supported ('_' concatenation)",
            ),
            ("^X(\"TAYLORANN)=1", "TAYLORANN", "unterminated string"),
            (
                "^GARCIALUIS=1",
                "GARCIALUIS",
                "expected '(' after global name",
            ),
        ] {
            let outcome = Store::from_zwr(&format!("^X(0)=1\n{line}\n"));
            assert!(
                matches!(
                    outcome,
                    Err(StoreError::Parse {
                        file: None,
                        line: 2,
                        ..
                    })
                ),
                "{line:?} should be rejected on line 2"
            );
            let shown = outcome.err().map(|e| e.to_string()).unwrap_or_default();
            assert_eq!(shown, format!("MALFORMED ZWR LINE 2: {cause}"), "{line:?}");
            assert!(!shown.contains(data), "{line:?} gave {shown:?}");
        }
    }

    #[test]
    fn repeated_key_replaces() -> Result<(), String> {
        let store = load("^X(1)=\"old\"\n^X(2)=\"keep\"\n^X(1)=\"new\"\n")?;
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(&key("X", &["1"])), Some("new"));
        Ok(())
    }

    #[test]
    fn empty_input_gives_empty_store() -> Result<(), String> {
        for text in ["", "OSEHRA ZGO Export: X\nZWR\n", "\n\n"] {
            let store = load(text)?;
            assert!(store.is_empty(), "{text:?} should load as empty");
            assert_eq!(store, Store::default());
        }
        Ok(())
    }

    #[test]
    fn non_canonical_unquoted_subscript_rejected() -> Result<(), String> {
        for sub in ["01", "1.0", "+1", "-0", "0.5", "1."] {
            let text = format!("^X({sub})=\"a\"\n");
            let outcome = Store::from_zwr(&text);
            assert!(
                matches!(outcome, Err(StoreError::Parse { line: 1, .. })),
                "unquoted subscript {sub:?} should be rejected"
            );
            let shown = outcome.err().map(|e| e.to_string()).unwrap_or_default();
            assert_eq!(
                shown, "MALFORMED ZWR LINE 1: unquoted subscript is not a canonical number",
                "unquoted subscript {sub:?}"
            );
        }
        let store = load("^X(.11,1.1,10000000044,-5)=\"a\"\n")?;
        assert_eq!(
            store.get(&[
                string("X"),
                num(".11"),
                num("1.1"),
                num("10000000044"),
                num("-5")
            ]),
            Some("a")
        );
        Ok(())
    }

    #[test]
    fn quoted_canonical_number_stored_as_number() -> Result<(), String> {
        assert_eq!(load("^X(\"500\",1)=\"a\"\n")?, load("^X(500,1)=\"a\"\n")?);
        let store = load("^X(\"001\")=\"a\"\n^X(\"04\",\"008202226\")=\"b\"\n")?;
        assert_eq!(store.get(&[string("X"), string("001")]), Some("a"));
        assert_eq!(store.get(&[string("X"), num("1")]), None);
        assert_eq!(
            store.get(&[string("X"), string("04"), string("008202226")]),
            Some("b")
        );
        Ok(())
    }

    #[test]
    fn doubled_quotes_unescaped() -> Result<(), String> {
        let store = load(
            "^PRC(442.4,17,0)=\"12^TEST^Test \"\"Purchase Authority\"\" \"\n\
             ^X(\"say \"\"hi\"\", (a=b)\",1)=\"\"\"\"\n",
        )?;
        assert_eq!(
            store.get(&key("PRC", &["442.4", "17", "0"])),
            Some("12^TEST^Test \"Purchase Authority\" ")
        );
        assert_eq!(
            store.get(&[string("X"), string("say \"hi\", (a=b)"), num("1")]),
            Some("\"")
        );
        Ok(())
    }

    #[test]
    fn unquoted_numeric_value_kept_as_text() -> Result<(), String> {
        let store = load(
            "^PRC(411,500,6,2,0)=10652\n^PRC(411,\"A IFCAP-Wide Parameters\",\"SO 2 AR Date\")=2981007\n",
        )?;
        assert_eq!(
            store.get(&key("PRC", &["411", "500", "6", "2", "0"])),
            Some("10652")
        );
        assert_eq!(
            store.get(&key(
                "PRC",
                &["411", "A IFCAP-Wide Parameters", "SO 2 AR Date"]
            )),
            Some("2981007")
        );
        Ok(())
    }

    #[test]
    fn dollar_c_value_rejected() {
        for line in [
            "^X(1)=\"a\"_$C(7)",
            "^X(1)=$C(7)",
            "^X(1)=$c(7)_\"a\"",
            "^X(1)=5_\"a\"",
        ] {
            let message = Store::from_zwr(line)
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            assert!(
                message.contains("$C() values are not supported"),
                "{line:?} gave {message:?}"
            );
        }
    }

    #[test]
    fn unreadable_fixture_file_named() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(&dir, "a.zwr", b"^X(1)=\"a\"\n")
            .and_then(|()| write_file(&dir, "bad.zwr", b"^X(2)=\"\xff\xfe\"\n"))
            .map(|()| load_fixtures(&dir));
        let outcome = remove_scratch(&dir, outcome)?;
        match outcome {
            Err(err) => {
                let message = err.to_string();
                assert!(message.contains("bad.zwr"), "message was {message:?}");
                assert!(message.contains("cannot read"), "message was {message:?}");
                assert_eq!(
                    err.kind(),
                    ErrorKind::InvalidData,
                    "message was {message:?}"
                );
            }
            Ok(store) => return Err(format!("expected an io error, got {store:?}")),
        }
        let scratch = scratch_dir()?;
        let missing = scratch.join("absent");
        let outcome = remove_scratch(&scratch, Ok(load_fixtures(&missing)))?;
        match outcome {
            Err(err) => {
                let message = err.to_string();
                assert!(message.contains("absent"), "message was {message:?}");
                assert!(message.contains("cannot read"), "message was {message:?}");
                assert_eq!(err.kind(), ErrorKind::NotFound, "message was {message:?}");
            }
            Ok(store) => return Err(format!("expected an io error, got {store:?}")),
        }
        Ok(())
    }

    #[test]
    fn fixture_parse_error_names_file_and_line() -> Result<(), String> {
        let dir = scratch_dir()?;
        let broken = dir.join("broken.zwr");
        let outcome = write_file(&dir, "broken.zwr", b"ZWR\n^X(1)=\"a\"\n^X(01)=\"b\"\n")
            .map(|()| load_fixtures(&dir));
        let outcome = remove_scratch(&dir, outcome)?;
        match outcome {
            Err(err) => {
                assert_eq!(err.kind(), ErrorKind::InvalidData, "got {err:?}");
                assert_eq!(
                    err.to_string(),
                    format!(
                        "{}: MALFORMED ZWR LINE 3: unquoted subscript is not a canonical number",
                        broken.display()
                    )
                );
            }
            Ok(store) => {
                return Err(format!(
                    "expected a parse error naming broken.zwr, got {store:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn load_fixtures_order_puts_cases_last() -> Result<(), String> {
        let dir = scratch_dir()?;
        // A directory whose name does not end in `.zwr` is ignored.
        let sub = dir.join("sub");
        let outcome = write_file(
            &dir,
            "a.zwr",
            b"ZWR\n^X(1)=\"a\"\n^X(2)=\"a\"\n^X(3)=\"a\"\n",
        )
        .and_then(|()| write_file(&dir, "cases.zwr", b"ZWR\n^X(1)=\"cases\"\n"))
        .and_then(|()| write_file(&dir, "z.zwr", b"ZWR\n^X(1)=\"z\"\n^X(2)=\"z\"\n"))
        // Valid ZWR under a key no `.zwr` file sets, so loading this
        // non-`.zwr` file would add a node rather than pass unseen.
        .and_then(|()| write_file(&dir, "notes.txt", b"ZWR\n^X(4)=\"notes\"\n"))
        .and_then(|()| {
            fs::create_dir(&sub).map_err(|e| format!("cannot create {}: {e}", sub.display()))
        })
        .map(|()| load_fixtures(&dir));
        let outcome = remove_scratch(&dir, outcome)?;
        let store = outcome.map_err(|e| e.to_string())?;
        assert_eq!(store.get(&key("X", &["1"])), Some("cases"));
        assert_eq!(store.get(&key("X", &["2"])), Some("z"));
        assert_eq!(store.get(&key("X", &["3"])), Some("a"));
        assert_eq!(
            store.get(&key("X", &["4"])),
            None,
            "notes.txt is not a .zwr file and must not load"
        );
        assert_eq!(store.len(), 3);
        Ok(())
    }

    /// A `*.zwr` entry that is not a regular file once symbolic links are
    /// followed fails the load, naming the entry, even beside a valid
    /// fixture, and is never opened.
    #[test]
    fn load_fixtures_rejects_non_regular_zwr_entry() -> Result<(), String> {
        let directory: fn(&Path) -> Result<(), String> = |entry| {
            fs::create_dir(entry).map_err(|e| format!("cannot create {}: {e}", entry.display()))
        };
        let link_to_directory: fn(&Path) -> Result<(), String> = |entry| {
            let target = entry.with_file_name("linked");
            fs::create_dir(&target)
                .and_then(|()| std::os::unix::fs::symlink(&target, entry))
                .map_err(|e| format!("cannot link {}: {e}", entry.display()))
        };
        for (what, make) in [
            ("a directory", directory),
            ("a link to a directory", link_to_directory),
        ] {
            let dir = scratch_dir()?;
            let entry = dir.join("x.zwr");
            let outcome = write_file(&dir, "a.zwr", b"ZWR\n^X(1)=\"a\"\n")
                .and_then(|()| make(&entry))
                .map(|()| load_fixtures(&dir));
            match remove_scratch(&dir, outcome)? {
                Err(err) => {
                    assert_eq!(err.kind(), ErrorKind::InvalidInput, "{what}: got {err:?}");
                    assert_eq!(
                        err.to_string(),
                        format!("{}: cannot read: not a regular file", entry.display()),
                        "{what}"
                    );
                }
                Ok(store) => return Err(format!("{what} named x.zwr loaded as {store:?}")),
            }
        }
        Ok(())
    }

    /// The opened file's own type decides admission, whatever its path
    /// showed when inspected: an object that is not a regular file once
    /// opened is refused unread, and a regular file is kept and read through
    /// the same handle.
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
        let dir = scratch_dir()?;
        let refused = [
            ("a directory", dir.clone()),
            ("a character device", PathBuf::from("/dev/null")),
        ];
        let outcome = refused
            .into_iter()
            .chain(fifo)
            .try_for_each(|(what, path)| match admit(&path) {
                Err(err) => {
                    assert_eq!(err.kind(), ErrorKind::InvalidInput, "{what}: got {err:?}");
                    assert_eq!(
                        err.to_string(),
                        format!("{}: cannot read: not a regular file", path.display()),
                        "{what}"
                    );
                    Ok(())
                }
                Ok(file) => Err(format!("{what} was admitted as {file:?}")),
            })
            .and_then(|()| write_file(&dir, "a.zwr", b"ZWR\n^X(1)=\"a\"\n"))
            .and_then(|()| {
                let path = dir.join("a.zwr");
                let mut text = String::new();
                admit(&path)
                    .and_then(|mut file| file.read_to_string(&mut text))
                    .map_err(|e| format!("a regular file was refused: {e}"))?;
                Ok(text)
            });
        let text = remove_scratch(&dir, outcome)?;
        assert_eq!(text, "ZWR\n^X(1)=\"a\"\n");
        Ok(())
    }

    #[test]
    fn load_fixtures_without_cases_is_not_an_error() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(&dir, "442.3.zwr", b"ZWR\n^PRCD(442.3,1,0)=\"x^1^3\"\n")
            .map(|()| load_fixtures(&dir));
        let outcome = remove_scratch(&dir, outcome)?;
        let store = outcome.map_err(|e| e.to_string())?;
        assert_eq!(store.get(&key("PRCD", &["442.3", "1", "0"])), Some("x^1^3"));
        Ok(())
    }

    /// Every method takes `&self`, so reads leave a loaded store equal to
    /// its clone taken before them.
    #[test]
    fn store_has_only_shared_methods() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(
            &dir,
            "a.zwr",
            b"ZWR\n^X(1)=\"a\"\n^X(1,2)=\"b\"\n^X(\"B\",\"a\",1)=\"\"\n",
        )
        .map(|()| load_fixtures(&dir));
        let outcome = remove_scratch(&dir, outcome)?;
        let store = outcome.map_err(|e| e.to_string())?;
        let before = store.clone();
        assert_eq!(store.get(&key("X", &["1"])), Some("a"));
        assert_eq!(store.get(&key("X", &["9"])), None);
        assert_eq!(store.get_or_empty(&key("X", &["9"])), "");
        assert_eq!(store.data(&key("X", &["1"])), 11);
        assert_eq!(store.data(&key("X", &["9", "1"])), 0);
        assert_eq!(store.order(&key("X", &[]), None), Some(num("1")));
        assert_eq!(store.order(&key("X", &[]), Some(&string("B"))), None);
        assert_eq!(store.iter().count(), 3);
        assert_eq!(store, before);
        Ok(())
    }

    #[test]
    fn order_collates_numbers_before_strings() -> Result<(), String> {
        let store = load(
            "^PRC(411,\"B\",500,500)=\"\"\n^PRC(411,10,0)=\"x\"\n^PRC(411,\"AC\",\"Y\",500)=\"\"\n\
             ^PRC(411,2,0)=\"x\"\n^PRC(411,.5,0)=\"x\"\n^PRC(411,\"001\",0)=\"x\"\n^PRC(411,0)=\"h\"\n",
        )?;
        let parent = key("PRC", &["411"]);
        let mut walked = Vec::new();
        let mut current = store.order(&parent, None);
        while let Some(sub) = current {
            walked.push(sub.clone());
            current = store.order(&parent, Some(&sub));
        }
        assert_eq!(
            walked,
            vec![
                num("0"),
                num(".5"),
                num("2"),
                num("10"),
                string("001"),
                string("AC"),
                string("B")
            ]
        );
        let texts: Vec<String> = store.iter().map(|(k, _)| Store::format_ref(k)).collect();
        assert_eq!(
            texts.first().map(String::as_str),
            Some("^PRC(411,0)"),
            "iter must follow collation order"
        );
        assert!(num("999") < string("A") && num("-1") < num(".5") && string("Z") < string("a"));

        // The order is total and agrees with the derived equality even for a
        // `Num` built directly from non-canonical text.
        let sample = [
            num("1"),
            num("01"),
            num("1.0"),
            num(".5"),
            num("0.5"),
            num("2"),
            string("1"),
            string("01"),
            string("B"),
        ];
        for a in &sample {
            for b in &sample {
                assert_eq!(a.cmp(b) == Ordering::Equal, a == b, "{a:?} vs {b:?}");
                assert_eq!(a.cmp(b), b.cmp(a).reverse(), "{a:?} vs {b:?}");
                for c in sample.iter().filter(|c| a <= b && b <= *c) {
                    assert!(a <= c, "{a:?}, {b:?}, {c:?}");
                }
            }
        }
        for odd in [num("01"), num("1.0")] {
            assert_ne!(odd, num("1"));
            assert_ne!(odd.cmp(&num("1")), Ordering::Equal, "{odd:?}");
            assert!(num(".5") < odd && odd < num("2"), "{odd:?}");
            assert!(odd < string("01") && odd < string("B"), "{odd:?}");
        }
        let store = load("^X(1)=\"a\"\n")?;
        assert_eq!(store.get(&[string("X"), num("1")]), Some("a"));
        assert_eq!(store.get(&[string("X"), num("01")]), None);
        assert_eq!(store.data(&[string("X"), num("01")]), 0);
        Ok(())
    }

    #[test]
    fn data_and_get_report_value_and_descendants() -> Result<(), String> {
        let store = load("^X(1)=\"a\"\n^X(1,2)=\"b\"\n^X(3,1)=\"c\"\n")?;
        assert_eq!(store.data(&key("X", &["1"])), 11);
        assert_eq!(store.data(&key("X", &["3"])), 10);
        assert_eq!(store.data(&key("X", &["1", "2"])), 1);
        assert_eq!(store.data(&key("X", &["9"])), 0);
        assert_eq!(store.data(&key("X", &[])), 10);
        assert_eq!(store.get(&key("X", &["3"])), None);
        assert_eq!(store.get_or_empty(&key("X", &["3"])), "");
        assert_eq!(store.get_or_empty(&key("X", &["1", "2"])), "b");
        Ok(())
    }

    #[test]
    fn null_subscript_loaded_and_invisible_to_order() -> Result<(), String> {
        let text = "^X(\"\")=\"v\"\n^X(\"B\")=\"w\"\n";
        let parent = key("X", &[]);
        let store = load(text)?;
        assert_eq!(store.order(&parent, None), Some(string("B")));

        let store = load(&format!("{text}^X(1)=\"n\"\n"))?;
        let mut walked = Vec::new();
        let mut current = store.order(&parent, None);
        while let Some(sub) = current {
            walked.push(sub.clone());
            current = store.order(&parent, Some(&sub));
        }
        assert_eq!(walked, vec![num("1"), string("B")]);
        assert_eq!(store.order(&parent, Some(&Sub::default())), Some(num("1")));
        assert_eq!(store.get(&key("X", &[""])), Some("v"));
        assert_eq!(store.data(&key("X", &[""])), 1);
        Ok(())
    }

    #[test]
    fn format_ref_quotes_strings_and_round_trips() -> Result<(), String> {
        let cases = [
            (
                key("PRC", &["442", "1", "2", "1", "0"]),
                "^PRC(442,1,2,1,0)",
            ),
            (
                key("PRC", &["442", "C", "A12345", "1"]),
                "^PRC(442,\"C\",\"A12345\",1)",
            ),
            (
                vec![string("X"), string("a\"b"), string("001")],
                "^X(\"a\"\"b\",\"001\")",
            ),
            (vec![string("PRC")], "^PRC"),
            (Vec::new(), "^"),
        ];
        for (k, expected) in &cases {
            assert_eq!(Store::format_ref(k), *expected);
        }
        for (k, expected) in cases.iter().take(3) {
            let store = load(&format!("{expected}=\"v\"\n"))?;
            assert_eq!(store.get(k), Some("v"), "{expected} should round-trip");
        }
        Ok(())
    }

    #[test]
    fn sub_from_text_classifies_by_canonical_text() {
        assert_eq!(Sub::from_text("500"), num("500"));
        assert_eq!(Sub::from_text(".5"), num(".5"));
        assert_eq!(Sub::from_text("0.5"), string("0.5"));
        assert_eq!(Sub::from_text("PRC"), string("PRC"));
        assert!(Sub::from_text("10000000044").is_num());
        assert!(!Sub::from_text("04").is_num());
        assert_eq!(Sub::from_text("AC").text(), "AC");
    }
}
