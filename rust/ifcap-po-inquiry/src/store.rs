//! The read-only global store that stands in for the FileMan globals the
//! ported `PRCH DISPLAY PO` routines read (`^PRC(442)`, `^PRCD(442.3)`,
//! `^PRC(440)`, `^PRC(411)` and the rest), loaded from ZWR exports.
//!
//! This is the one ZWR loader of the crate: the bridge, the equivalence
//! harness and the bridge tests all load fixtures through [`Store::from_zwr`]
//! and [`load_fixtures`]. A [`Store`] is borrowed immutably by the inquiry
//! and has no public mutator, so nothing an inquiry does can write to it.
//!
//! Keys are subscript vectors whose first element is the global name
//! without its caret (`^PRC(442,1,0)` is `[Str("PRC"), Num("442"),
//! Num("1"), Num("0")]`). They order by M default collation: every number
//! before every string, numbers by exact value, strings by bytes. Values are
//! the node data exactly as exported, with only the ZWR quoting removed.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::mumps;

/// One subscript of a global reference, or the global name in the first
/// position of a key.
///
/// Invariant: `Num` holds the canonical text of a number (`.5`, `500`,
/// `10000000044`; never `0.5`, `01` or `1.0`), and `Str` holds text that is
/// not a canonical number. [`Sub::from_text`] and the loader keep it; a
/// caller that builds `Num` from non-canonical text, or `Str` from canonical
/// text, gets a key that matches no loaded node. Under the invariant, equal
/// values always have equal text, so the derived equality agrees with the
/// collation order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sub {
    /// A canonical number, as its canonical text.
    Num(String),
    /// A string subscript, or the global name.
    Str(String),
}

impl Sub {
    /// The subscript that `text` names. In M a subscript's identity is its
    /// text: canonical number text is a number, anything else is a string,
    /// so the quoted subscript `"500"` is the number 500 while `"001"` and
    /// `"04"` stay strings.
    pub fn from_text(text: &str) -> Sub {
        if mumps::is_canonical(text) {
            Sub::Num(text.to_string())
        } else {
            Sub::Str(text.to_string())
        }
    }

    /// The subscript's text: the canonical number text or the string.
    pub fn text(&self) -> &str {
        match self {
            Sub::Num(text) | Sub::Str(text) => text,
        }
    }

    /// True for a numeric subscript.
    pub fn is_num(&self) -> bool {
        matches!(self, Sub::Num(_))
    }
}

impl Ord for Sub {
    /// M default collation: numbers before strings, numbers by exact
    /// decimal value, strings by bytes. The 411 export lists its numeric
    /// IENs before `"AC"` and `"B"` in this order, and the item loop
    /// `Q:PRCHDI'>0` at PRCHDP2.m:11 relies on numeric items preceding
    /// the `"B"` subscript.
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Sub::Num(a), Sub::Num(b)) => mumps::cmp_numbers(a, b),
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

/// Why a ZWR text or a fixture directory could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A data line that is not `^name(subscripts)=value`. `line` is 1-based
    /// and counts every line of the text, header lines included. `file` is
    /// `None` when the text did not come from a file.
    Parse {
        file: Option<PathBuf>,
        line: usize,
        message: String,
    },
    /// A fixture directory that cannot be listed, or a fixture file that
    /// cannot be read or is not UTF-8.
    Io { file: PathBuf, message: String },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Parse {
                file: Some(file),
                line,
                message,
            } => write!(f, "{}: line {line}: {message}", file.display()),
            StoreError::Parse {
                file: None,
                line,
                message,
            } => write!(f, "line {line}: {message}"),
            StoreError::Io { file, message } => {
                write!(f, "{}: cannot read: {message}", file.display())
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// The globals of one database snapshot, in M collation order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Store {
    nodes: BTreeMap<Vec<Sub>, String>,
}

impl Store {
    /// Loads ZWR text: every line before the first line that starts with
    /// `^` is a header line and is ignored, blank lines are ignored, and
    /// every other line must be `^name(subscripts)=value`. A repeated key
    /// replaces the earlier value, as `SET` does. Text with no data lines
    /// gives an empty store. One trailing carriage return per line is
    /// dropped, so a CRLF export loads like an LF one.
    ///
    /// Subscripts are canonical numbers or quoted strings (`""` stands for
    /// one `"`), and a quoted canonical number is stored as a number, as M
    /// does. Values are quoted strings or canonical numbers. `$C(n)` and
    /// `_` concatenation are rejected rather than evaluated, because no
    /// fixture holds them; any other form is an error, never a guess.
    pub fn from_zwr(text: &str) -> Result<Store, StoreError> {
        let mut store = Store::default();
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
            store.nodes.insert(key, value);
        }
        Ok(store)
    }

    /// The value stored at `key`, or `None` when the node has no value.
    /// `None` is how an unguarded reference finds a missing node, where M
    /// raises an `UNDEF` error.
    pub fn get(&self, key: &[Sub]) -> Option<&str> {
        mumps::get_opt(&self.nodes, key)
    }

    /// M `$GET`: the value stored at `key`, or `""` when the node has no
    /// value. A guarded read goes through this accessor, an unguarded one
    /// through [`Store::get`], so every `$G` shares the one `$GET` model in
    /// `mumps.rs`.
    pub fn get_or_empty(&self, key: &[Sub]) -> String {
        mumps::get(&self.nodes, key)
    }

    /// M `$DATA`: 0 for no value and no descendants, 1 for a value only,
    /// 10 for descendants only, 11 for both.
    pub fn data(&self, key: &[Sub]) -> u8 {
        mumps::data(&self.nodes, key)
    }

    /// M forward `$ORDER`: the next child subscript of `parent` after
    /// `after` in collation order, or `None` when there is none. `after` of
    /// `None` is the `""` seed and gives the first child.
    pub fn order(&self, parent: &[Sub], after: Option<&Sub>) -> Option<Sub> {
        mumps::order(&self.nodes, parent, after)
    }

    /// Every node with its value, in collation order.
    pub fn iter(&self) -> impl Iterator<Item = (&[Sub], &str)> {
        self.nodes
            .iter()
            .map(|(key, value)| (key.as_slice(), value.as_str()))
    }

    /// The number of nodes holding a value.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the store holds no node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The ZWR-style global reference of `key`: `^PRC(442,1,2,1,0)`,
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

    /// Adds every node of `other`, whose values replace equal keys here.
    /// Private, so that only [`load_fixtures`] ever builds a store from
    /// several texts.
    fn absorb(&mut self, other: Store) {
        self.nodes.extend(other.nodes);
    }
}

/// The fixture file that holds the runtime export of the capture snapshot.
/// It loads last, so its values win over the repository exports.
const RUNTIME_EXPORT: &str = "cases.zwr";

/// Loads every regular `*.zwr` file of `dir` into one store. Files load in
/// byte order of name with `cases.zwr` last, so the runtime export wins on
/// repeated keys; a missing `cases.zwr` is not an error. Other files, such
/// as `MANIFEST.txt`, and directories are ignored. A directory that cannot
/// be listed, or a `*.zwr` file that cannot be read or is not UTF-8, is a
/// [`StoreError::Io`] naming it; a parse error names its file and line.
pub fn load_fixtures(dir: &Path) -> Result<Store, StoreError> {
    let io_error = |file: &Path, err: std::io::Error| StoreError::Io {
        file: file.to_path_buf(),
        message: err.to_string(),
    };
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|err| io_error(dir, err))? {
        let entry = entry.map_err(|err| io_error(dir, err))?;
        let name = entry.file_name();
        if !name.as_encoded_bytes().ends_with(b".zwr") {
            continue;
        }
        let path = entry.path();
        // Follows symbolic links, so a linked fixture loads like a copy.
        let metadata = fs::metadata(&path).map_err(|err| io_error(&path, err))?;
        if metadata.is_file() {
            files.push((
                name.as_encoded_bytes() == RUNTIME_EXPORT.as_bytes(),
                name,
                path,
            ));
        }
    }
    files.sort_by(|(a_last, a_name, _), (b_last, b_name, _)| {
        a_last
            .cmp(b_last)
            .then_with(|| a_name.as_encoded_bytes().cmp(b_name.as_encoded_bytes()))
    });
    let mut store = Store::default();
    for (_, _, path) in files {
        let text = fs::read_to_string(&path).map_err(|err| io_error(&path, err))?;
        let loaded = Store::from_zwr(&text).map_err(|err| match err {
            StoreError::Parse { line, message, .. } => StoreError::Parse {
                file: Some(path.clone()),
                line,
                message,
            },
            other => other,
        })?;
        store.absorb(loaded);
    }
    Ok(store)
}

/// A left-to-right scanner over one ZWR data line. Quoted strings may hold
/// `,`, `)`, `=` and doubled quotes, so the line is never split on them.
struct Cursor<'a> {
    rest: &'a str,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<char> {
        self.rest.chars().next()
    }

    /// Consumes `c` when it is the next character.
    fn eat(&mut self, c: char) -> bool {
        match self.rest.strip_prefix(c) {
            Some(tail) => {
                self.rest = tail;
                true
            }
            None => false,
        }
    }

    /// Consumes and returns the longest prefix whose characters satisfy
    /// `keep`.
    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> &'a str {
        let end = self
            .rest
            .find(|c: char| !keep(c))
            .unwrap_or(self.rest.len());
        let (head, tail) = self.rest.split_at_checked(end).unwrap_or((self.rest, ""));
        self.rest = tail;
        head
    }

    /// Consumes a quoted string whose opening quote has been consumed,
    /// through its closing quote, turning each `""` into one `"`.
    fn quoted(&mut self) -> Result<String, String> {
        let mut out = String::new();
        loop {
            let Some((chunk, after)) = self.rest.split_once('"') else {
                return Err("unterminated string".to_string());
            };
            out.push_str(chunk);
            match after.strip_prefix('"') {
                Some(next) => {
                    out.push('"');
                    self.rest = next;
                }
                None => {
                    self.rest = after;
                    return Ok(out);
                }
            }
        }
    }

    /// Consumes an unquoted number token: digits, letters (so that
    /// exponent forms are caught as non-canonical), `.`, `+` and `-`.
    fn number_token(&mut self) -> &'a str {
        self.take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
    }
}

/// The message for a value this loader declines to evaluate.
const DOLLAR_C_MESSAGE: &str = "$C() values are not supported";

/// Parses one data line into its key and value, or a message naming what
/// was expected.
fn parse_line(line: &str) -> Result<(Vec<Sub>, String), String> {
    let mut cur = Cursor { rest: line };
    if !cur.eat('^') {
        return Err("expected '^' at the start of a data line".to_string());
    }
    let percent = cur.eat('%');
    let name_starts_with_letter = cur.peek().is_some_and(|c| c.is_ascii_alphabetic());
    let name = cur.take_while(|c| c.is_ascii_alphanumeric());
    if !percent && !name_starts_with_letter {
        return Err("expected a global name after '^'".to_string());
    }
    let mut key = vec![Sub::Str(format!(
        "{}{name}",
        if percent { "%" } else { "" }
    ))];
    if !cur.eat('(') {
        return Err("expected '(' after global name".to_string());
    }
    loop {
        key.push(parse_subscript(&mut cur)?);
        if cur.eat(',') {
            continue;
        }
        if cur.eat(')') {
            break;
        }
        return Err("expected ',' or ')' after subscript".to_string());
    }
    if !cur.eat('=') {
        return Err("expected '=' after subscripts".to_string());
    }
    let value = parse_value(&mut cur)?;
    Ok((key, value))
}

/// One subscript: a quoted string, stored through [`Sub::from_text`], or an
/// unquoted canonical number.
fn parse_subscript(cur: &mut Cursor<'_>) -> Result<Sub, String> {
    if cur.eat('"') {
        return cur.quoted().map(|text| Sub::from_text(&text));
    }
    let token = cur.number_token();
    if token.is_empty() {
        return Err("expected a subscript".to_string());
    }
    if mumps::is_canonical(token) {
        Ok(Sub::Num(token.to_string()))
    } else {
        Err(format!(
            "unquoted subscript {token} is not a canonical number"
        ))
    }
}

/// The value after `=`: a quoted string or a canonical number, and nothing
/// after it.
fn parse_value(cur: &mut Cursor<'_>) -> Result<String, String> {
    let value = if cur.eat('"') {
        cur.quoted()?
    } else if cur.peek() == Some('$') {
        return Err(DOLLAR_C_MESSAGE.to_string());
    } else {
        let token = cur.number_token();
        if token.is_empty() {
            return Err("expected a quoted string or a number after '='".to_string());
        }
        if !mumps::is_canonical(token) {
            return Err(format!("unquoted value {token} is not a canonical number"));
        }
        token.to_string()
    };
    match cur.peek() {
        None => Ok(value),
        Some('_') => Err(format!("{DOLLAR_C_MESSAGE} ('_' concatenation)")),
        Some(_) => Err("unexpected text after the value".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn num(text: &str) -> Sub {
        Sub::Num(text.to_string())
    }

    fn string(text: &str) -> Sub {
        Sub::Str(text.to_string())
    }

    /// A key under global `name`, each subscript classified by its text.
    fn key(name: &str, subs: &[&str]) -> Vec<Sub> {
        let mut key = vec![string(name)];
        key.extend(subs.iter().map(|sub| Sub::from_text(sub)));
        key
    }

    fn load(text: &str) -> Result<Store, String> {
        Store::from_zwr(text).map_err(|e| e.to_string())
    }

    /// A fresh directory under the system temporary directory, unique to
    /// this process and call.
    fn scratch_dir() -> Result<PathBuf, String> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ifcap-store-unit-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
        fs::write(dir.join(name), bytes).map_err(|e| e.to_string())
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
        assert!(message.contains("line 4"), "message was {message:?}");
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
            assert!(
                matches!(
                    Store::from_zwr(&text),
                    Err(StoreError::Parse { line: 1, .. })
                ),
                "unquoted subscript {sub:?} should be rejected"
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
        let _ = fs::remove_dir_all(&dir);
        match outcome? {
            Err(err @ StoreError::Io { .. }) => {
                let message = err.to_string();
                assert!(message.contains("bad.zwr"), "message was {message:?}");
                assert!(message.contains("cannot read"), "message was {message:?}");
            }
            other => return Err(format!("expected an Io error, got {other:?}")),
        }
        let missing = scratch_dir()?.join("absent");
        let message = load_fixtures(&missing)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(message.contains("absent"), "message was {message:?}");
        if let Some(parent) = missing.parent() {
            let _ = fs::remove_dir_all(parent);
        }
        Ok(())
    }

    #[test]
    fn fixture_parse_error_names_file_and_line() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(&dir, "broken.zwr", b"ZWR\n^X(1)=\"a\"\n^X(01)=\"b\"\n")
            .map(|()| load_fixtures(&dir));
        let _ = fs::remove_dir_all(&dir);
        match outcome? {
            Err(StoreError::Parse {
                file: Some(file),
                line: 3,
                ..
            }) => assert!(file.ends_with("broken.zwr"), "file was {file:?}"),
            other => {
                return Err(format!(
                    "expected a parse error naming broken.zwr, got {other:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn load_fixtures_order_puts_cases_last() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(
            &dir,
            "a.zwr",
            b"ZWR\n^X(1)=\"a\"\n^X(2)=\"a\"\n^X(3)=\"a\"\n",
        )
        .and_then(|()| write_file(&dir, "cases.zwr", b"ZWR\n^X(1)=\"cases\"\n"))
        .and_then(|()| write_file(&dir, "z.zwr", b"ZWR\n^X(1)=\"z\"\n^X(2)=\"z\"\n"))
        .and_then(|()| write_file(&dir, "MANIFEST.txt", b"not ZWR at all\n"))
        .and_then(|()| fs::create_dir(dir.join("sub.zwr")).map_err(|e| e.to_string()))
        .map(|()| load_fixtures(&dir));
        let _ = fs::remove_dir_all(&dir);
        let store = outcome?.map_err(|e| e.to_string())?;
        assert_eq!(store.get(&key("X", &["1"])), Some("cases"));
        assert_eq!(store.get(&key("X", &["2"])), Some("z"));
        assert_eq!(store.get(&key("X", &["3"])), Some("a"));
        assert_eq!(store.len(), 3);
        Ok(())
    }

    #[test]
    fn load_fixtures_without_cases_is_not_an_error() -> Result<(), String> {
        let dir = scratch_dir()?;
        let outcome = write_file(&dir, "442.3.zwr", b"ZWR\n^PRCD(442.3,1,0)=\"x^1^3\"\n")
            .map(|()| load_fixtures(&dir));
        let _ = fs::remove_dir_all(&dir);
        let store = outcome?.map_err(|e| e.to_string())?;
        assert_eq!(store.get(&key("PRCD", &["442.3", "1", "0"])), Some("x^1^3"));
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
