//! The parity corpus of `SPEC.md` "Parity corpus": the model of its 231
//! sessions, their deterministic generation, the one classification of a
//! session, the checks the generator and the offline gate share, the
//! rendering of the generated files, and world B, the store the corpus
//! sessions run on.
//!
//! The module does no I/O. It takes world A, the store of the fixtures in
//! `tests/fixtures/ifcap`, as a [`Store`], and the `SPEC.md` text as a
//! `&str`, and returns values: `corpus_gen` reads and writes the files, and
//! the gate and the equivalence harness read them.
//!
//! The corpus has three classes. CP-001 to CP-200 each display one generated
//! order, CP-n being IEN `991000 + n` with `C` value `CP` and n in three
//! digits. CE-01 to CE-26 are the edge sequences, specified as 16 class IN
//! and 10 class BOUNDARY. CR-01 to CR-05 replay the GC-01 to GC-05 inputs on
//! world B. World B is world A plus the generated orders; world A itself is
//! never changed and holds no corpus node.
//!
//! Every dimension value of an order comes from rotation over fixed lists:
//! CP-n takes the order row's value at position `n mod L` of its list, and
//! an item takes the item row's value at position `g mod L`, where `g` is its
//! 1-based position over all items in case order. A single [`Xorshift64`]
//! stream, restarted from [`SEED`] at each generation round, decides only
//! free text and amounts ([`generate`] gives the draw order). A session
//! outcome the corpus cannot keep as it is drops the variant that caused it
//! and regenerates, moves an edge case to the class its outcome shows, or
//! records a finding; `src/inquiry.rs` is never changed for it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::golden::{corpus_case_ids, corpus_dir, corpus_input_path};
use crate::inquiry::inquire_traced;
use crate::mumps::{SESSION_OUTPUT_LIMIT, is_canonical};
use crate::store::{Store, StoreError, Sub};

// ---------------------------------------------------------------------------
// Constants and the stream
// ---------------------------------------------------------------------------

/// The fixed seed of the corpus stream, `11400714819323198485` in decimal,
/// which every manifest row carries in its `seed` column.
pub const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// The largest `$Y` a corpus order may reach at any `ASK^PRCHDP2` or after
/// its last item ([`order_worst_y`]). The paging prompt, the not-implemented
/// PO-30.b2, fires on `$Y+5>IOSL`, which with `IOSL` 60 is `$Y` 56
/// [PRCHDP2.m:31].
pub const PAGING_BOUND: usize = 45;

/// The miss answer `ZZ9`.
const MISS_ZZ9: &str = "ZZ9";
/// The miss answer `NO-SUCH.ORDER`.
const MISS_NO_SUCH: &str = "NO-SUCH.ORDER";
/// The miss answer of thirty `Q`, the length of index `C`.
const MISS_THIRTY_Q: &str = "QQQQQQQQQQQQQQQQQQQQQQQQQQQQQQ";
/// The miss answer `12345`, a canonical number.
const MISS_NUMBER: &str = "12345";
/// The miss answer `gc9999`, lowercase with no candidate.
const MISS_LOWER: &str = "gc9999";
/// The miss answer `991001`, the IEN of CP-001 typed as a value.
const MISS_IEN: &str = "991001";

/// The answers the edge sequences type to match no order. No `C` value of
/// world B may begin with one of them, compared as written and in upper
/// case ([`c_value_violations`]).
pub const MISS_STRINGS: [&str; 6] = [
    MISS_ZZ9,
    MISS_NO_SUCH,
    MISS_THIRTY_Q,
    MISS_NUMBER,
    MISS_LOWER,
    MISS_IEN,
];

/// The CR cases: each case id, the GC case whose inputs it replays on world
/// B, and those inputs, the `SPEC.md` C9 sequences of GC-01 to GC-05.
pub const CR_INPUTS: [(&str, &str, &[&str]); 5] = [
    ("CR-01", "GC-01", &["", "GC1001", "", "", "^"]),
    ("CR-02", "GC-02", &["", "GC2002", "", "", "^"]),
    ("CR-03", "GC-03", &["", "GC3003", "^"]),
    ("CR-04", "GC-04", &["", "INVALID!", "^"]),
    ("CR-05", "GC-05", &["", "GC5005", "", "", "^"]),
];

/// The transcript suffix after a caret-prefixed exit from EN3^PRCHRPT:
/// the order prompt followed by the process-exit newline (SPEC.md K14).
/// The exit answer is not echoed.
const EXIT_TAIL: &str = "P.O./REQ.NO.: \n";

/// The two header lines of the corpus ZWR text, and of the text world B is
/// parsed from, in the form of the fixtures' `OSEHRA ZGO Export` headers.
const ZWR_HEADER: [&str; 2] = ["OSEHRA ZGO Export: IFCAP PO INQUIRY PARITY CORPUS", "ZWR"];

/// The most generation rounds [`generate`] runs before it gives up on a CP
/// case that still halts at a not-implemented branch.
const MAX_ROUNDS: usize = 5;

/// The station every corpus order belongs to: `STATION` of the inquiry's
/// fixed context, whose orders pass the lookup screen
/// `I +$P(^(0),U,1)=PRC("SITE")` [PRCHPAT.m:81].
const STATION: &str = "500";

const CP_COUNT: usize = 200;

/// CP-n is order IEN `IEN_BASE + n`.
const IEN_BASE: usize = 991_000;

/// The multi-item counts and how many orders take each, 50 orders and 191
/// items in all, drawn round-robin from 2 to 8 while a count's quota remains.
const MULTI_QUOTAS: [(usize, usize); 7] =
    [(2, 14), (3, 12), (4, 9), (5, 6), (6, 4), (7, 3), (8, 2)];

/// The number of multi-item orders, at the positions `n = 4k + (k mod 4) + 1`.
const MULTI_ORDERS: usize = 50;

/// The status IEN of the third order of CE-12, `Cancelled Order`.
const CANCELLED_STATUS: &str = "45";

/// The order row dimensions, in the matrix order the stream is drawn in and
/// the tags are written in, each also a key of the rotation lists.
const ORDER_ROWS: [&str; 17] = [
    "status",
    "type",
    "source",
    "mop",
    "vendor",
    "service",
    "shipto",
    "delivery",
    "verbal",
    "fob",
    "proposal",
    "authority",
    "date",
    "app",
    "agent",
    "total",
    "shipping",
];

/// The item row fields, in stream and tag order. Each item dimension's
/// rotation list is keyed `item.` followed by the field.
const ITEM_ROWS: [&str; 10] = [
    "desc", "qty", "unit", "cost", "ext", "stk", "nsn", "food", "per", "boc",
];

/// The FileMan dates of the `date` row, the order date and the delivery date
/// alike: year ends, leap days, 1999, 2026 and empty.
const DATES: [&str; 12] = [
    "2990101", "2991231", "3000229", "3111109", "3240229", "3260101", "3260115", "3260630",
    "3261001", "3261015", "3261231", "",
];

/// The order totals of the `total` row, node 0 piece 15.
const TOTALS: [&str; 12] = [
    "0",
    "1",
    "251",
    "1000",
    ".005",
    "1.995",
    ".5",
    "99.99",
    "12345.678",
    "123456789.5",
    "-5.25",
    "-1000",
];

/// The estimated shipping values of the `shipping` row, node 0 piece 13.
const SHIPPING: [&str; 4] = ["", ".5", "12.345", "1000"];

/// The proposal lengths of the `proposal` row, around the wrap width 16 and
/// the three printed lines [PRCHDP1.m:53-57].
const PROPOSAL_LENGTHS: [&str; 12] = [
    "0", "1", "15", "16", "17", "31", "32", "33", "47", "48", "49", "100",
];

/// The stored unit cost forms of the `item.cost` row. Together the six
/// exercise the four print arms of [PRCHDP2.m:52]: whole and 2 decimals
/// through `$J(,7,2)`, 3 decimals through `$J(,6,3)`, 4 and 5 decimals
/// through `$J(,5,4)`, and `N/C` through `"    N/C"`.
const COST_FORMS: [&str; 6] = ["N/C", "whole", "2dp", "3dp", "4dp", "5dp"];

/// The description word-processing header of every corpus item, as the GC
/// items carry it.
const DESCRIPTION_HEADER: &str = "^^1^1^3261001^";

/// The characters of stream text.
const ALPHABET: &[u8; 36] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// The highest FOOD GROUP code: field 442.01/41 is a set of the codes `1`
/// to `6` [DD.zwr:516400].
const FOOD_GROUPS: usize = 6;

/// The folder of the world A fixtures, which `store::load_fixtures` loads.
pub fn world_a_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ifcap")
}

/// The path of the corpus ZWR file, beside the world A fixtures and never
/// among them.
pub fn corpus_zwr_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ifcap-corpus")
        .join("corpus.zwr")
}

/// The corpus stream: xorshift64 with shifts 13, 7 and 17 on a `u64` state
/// that is never zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// A stream started from `seed`. A zero seed, from which xorshift would
    /// only ever give zero, is replaced by [`SEED`].
    pub fn new(seed: u64) -> Xorshift64 {
        Xorshift64 {
            state: if seed == 0 { SEED } else { seed },
        }
    }

    /// The next value of the stream, which is also its new state.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// The next value reduced below `bound`. A `bound` of 0 gives 0 and draws
    /// nothing.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        self.next_u64() % bound
    }
}

// ---------------------------------------------------------------------------
// Execution and classification
// ---------------------------------------------------------------------------

/// The raw result of one corpus session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseRun {
    /// What the session wrote; empty when `error` is set.
    pub transcript: String,
    /// The `SPEC.md` branch ID of the not-implemented branch that ended the
    /// session, or `None`.
    pub branch: Option<&'static str>,
    /// The text of the `InquiryError` the session ended with, if any.
    pub error: Option<String>,
}

/// Runs one session on `store` with `inputs` through
/// [`inquire_traced`] and returns it raw,
/// with no specification needed.
pub fn run_case(store: &Store, inputs: &[String]) -> CaseRun {
    let (result, branch) = inquire_traced(store, inputs);
    match result {
        Ok(output) => CaseRun {
            transcript: output.transcript,
            branch,
            error: None,
        },
        Err(error) => CaseRun {
            transcript: String::new(),
            branch,
            error: Some(error.to_string()),
        },
    }
}

/// The class of a session's outcome (`SPEC.md` "Parity corpus").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The session ran to its end: no error, no branch ID, the transcript
    /// ending with the order prompt and the exit `\n`, and under the session
    /// output limit.
    End,
    /// The session halted at this branch, which `SPEC.md` "Branch inventory"
    /// lists as not implemented.
    Unimplemented(&'static str),
    /// The session halted at this branch, which the inventory does not list
    /// as not implemented: a finding.
    Implemented(&'static str),
    /// An error, a session that ran out of input or was cut by the output
    /// limit, or a transcript at the limit, with its cause.
    Failure(String),
}

/// Classifies `run` against `not_implemented`, the branch IDs
/// [`ne_branches`] reads from `SPEC.md`. The checks run in this order: an
/// error, a transcript at the 1,048,576-byte session output limit, a branch
/// ID, and the exit `\n`.
pub fn classify(run: &CaseRun, not_implemented: &[String]) -> Outcome {
    if let Some(error) = &run.error {
        return Outcome::Failure(error.clone());
    }
    if run.transcript.len() >= SESSION_OUTPUT_LIMIT {
        return Outcome::Failure(format!(
            "transcript of {} bytes reaches the session output limit",
            run.transcript.len()
        ));
    }
    if let Some(id) = run.branch {
        return if not_implemented.iter().any(|known| known == id) {
            Outcome::Unimplemented(id)
        } else {
            Outcome::Implemented(id)
        };
    }
    if run.transcript.ends_with(EXIT_TAIL) {
        Outcome::End
    } else {
        Outcome::Failure(
            "session ended without the exit newline: its input ran out or the output limit cut it"
                .to_string(),
        )
    }
}

/// The IDs of the `SPEC.md` "### Branch inventory" rows whose last cell
/// begins `not exercised, not implemented`, in table order. The table ends
/// at the next heading; a row is a line beginning `| PO-`, its ID is the
/// first cell and its coverage the last.
pub fn ne_branches(spec: &str) -> Vec<String> {
    spec.lines()
        .skip_while(|line| line.trim_end() != "### Branch inventory")
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .filter(|line| line.starts_with("| PO-"))
        .filter_map(|line| {
            let (_, cells) = line.split_once('|')?;
            let (id, _) = cells.split_once('|')?;
            let (before_end, _) = line.trim_end().rsplit_once('|')?;
            let (_, last) = before_end.rsplit_once('|')?;
            last.trim()
                .starts_with("not exercised, not implemented")
                .then(|| id.trim().to_string())
        })
        .collect()
}

/// World B: every node of `world_a` and every data line of `corpus_zwr`,
/// parsed once by [`Store::from_zwr`] from one text made of the corpus
/// header lines, each world A node as `<reference>="<value>"` with every `"`
/// doubled, and the corpus lines from its first line beginning `^`. The
/// corpus text's own header lines are skipped, because a header line after
/// the first data line does not parse.
///
/// A text that is not a corpus text, or that would change world A, is
/// refused, so a corpus line can never overwrite a world A node:
///
/// - lines 1 and 2 must be the two corpus header lines [`generate`] writes,
///   and line 3 must be a data line, beginning `^`, so a text without that
///   header or with no data line is refused;
/// - no data line may name a key world A already holds as a node or above
///   one (`$D` not 0), the rule [`generate`] applies before it writes. Keys
///   are compared as parsed, so a quoted alias such as `^VA(200,".5",0)`
///   of `^VA(200,.5,0)` is refused too.
///
/// A refusal or a malformed line is a [`StoreError::Parse`] with `file`
/// `None` and a fixed `message` that quotes nothing from the text, a
/// malformed line's being the one [`Store::from_zwr`] gives. A line of
/// `corpus_zwr` carries its own 1-based number in `corpus_zwr`, counted as
/// [`Store::from_zwr`] counts lines, its header lines included, so a broken
/// corpus file is located from the error: a missing or different header
/// line gives its own number, 1 or 2; a third line that is missing or is no
/// data line gives 3; and keys world A holds give the first data line that
/// names one. Malformed lines are reported before keys world A holds. A
/// world A node's line keeps its number in the merged text.
pub fn world_b(world_a: &Store, corpus_zwr: &str) -> Result<Store, StoreError> {
    let refused = |line: usize, message: &str| StoreError::Parse {
        file: None,
        line,
        message: message.to_string(),
    };
    let mut corpus_lines = corpus_zwr.lines();
    for (index, header) in ZWR_HEADER.into_iter().enumerate() {
        if corpus_lines.next() != Some(header) {
            return Err(refused(index + 1, "expected the corpus header line"));
        }
    }
    if !corpus_lines
        .next()
        .is_some_and(|line| line.starts_with('^'))
    {
        return Err(refused(
            ZWR_HEADER.len() + 1,
            "expected a data line after the corpus header",
        ));
    }
    Store::from_zwr(corpus_zwr)?;
    let held = corpus_zwr
        .split('\n')
        .enumerate()
        .skip(ZWR_HEADER.len())
        .find(|(_, line)| {
            Store::from_zwr(line)
                .is_ok_and(|node| node.iter().any(|(key, _)| world_a.data(key) != 0))
        });
    if let Some((index, _)) = held {
        return Err(refused(
            index.saturating_add(1),
            "the key already exists in world A",
        ));
    }
    let mut text = String::new();
    for header in ZWR_HEADER {
        text.push_str(header);
        text.push('\n');
    }
    for (key, value) in world_a.iter() {
        text.push_str(&Store::format_ref(key));
        text.push_str("=\"");
        text.push_str(&value.replace('"', "\"\""));
        text.push_str("\"\n");
    }
    let merged_before_corpus = text.bytes().filter(|byte| *byte == b'\n').count();
    let skipped = corpus_zwr
        .lines()
        .take_while(|line| !line.starts_with('^'))
        .count();
    for line in corpus_zwr.lines().skip(skipped) {
        text.push_str(line);
        text.push('\n');
    }
    Store::from_zwr(&text).map_err(|error| {
        let StoreError::Parse {
            file,
            line,
            message,
        } = error;
        let line = if line > merged_before_corpus {
            line.saturating_sub(merged_before_corpus)
                .saturating_add(skipped)
        } else {
            line
        };
        StoreError::Parse {
            file,
            line,
            message,
        }
    })
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// A corpus case class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// A generated order displayed once, CP-001 to CP-200.
    Cp,
    /// An edge sequence inside the implemented branches.
    In,
    /// An edge sequence that ends at a not-implemented branch.
    Boundary,
    /// A GC sequence replayed on world B, CR-01 to CR-05.
    Cr,
}

impl Class {
    /// The class as the manifest and the report write it: `CP`, `IN`,
    /// `BOUNDARY` or `CR`.
    pub fn label(self) -> &'static str {
        match self {
            Class::Cp => "CP",
            Class::In => "IN",
            Class::Boundary => "BOUNDARY",
            Class::Cr => "CR",
        }
    }
}

/// One corpus case, one manifest row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseEntry {
    /// The case id, such as `CP-001`, `CE-01` or `CR-01`.
    pub id: String,
    /// The class after the outcome rules: a specified IN case that halts at
    /// a not-implemented branch is BOUNDARY, and a specified BOUNDARY case
    /// that runs to its end is IN.
    pub class: Class,
    /// The class the corpus specification gives the case.
    pub specified: Class,
    /// The input lines, each typed without its terminator.
    pub inputs: Vec<String>,
    /// The order IENs referenced by exact C-value input lines, without repeats,
    /// in first-use order, including unused tails after a boundary halt.
    pub orders: Vec<String>,
    /// The `C` values of those orders, in the same order.
    pub c_values: Vec<String>,
    /// The item count of a CP case's order; `None` for CE and CR cases.
    pub item_count: Option<usize>,
    /// The manifest tags: a CP order's `dimension=value` pairs, then
    /// `moved=…`, `gc=…` and `finding=…` where they apply.
    pub tags: Vec<String>,
    /// The branch ID the session is expected to halt at, or `None` for a
    /// session expected to run to its end (the manifest's `none`).
    pub expected_branch: Option<String>,
    /// The branch ID of a recorded finding: a halt at a branch `SPEC.md`
    /// lists as implemented.
    pub finding: Option<String>,
}

/// The nodes of one generated order, each key with its value.
pub type Node = (Vec<Sub>, String);

/// One generated order of world B.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderRecord {
    /// The CP case that displays the order.
    pub case_id: String,
    /// The order IEN, `991000 + n`.
    pub ien: String,
    /// The order's `C` index value, `CP` and n in three digits.
    pub c_value: String,
    /// The number of items.
    pub item_count: usize,
    /// Node 23 piece 11: empty, `P`, `S` or `D`.
    pub order_type: String,
    /// Every node the order adds to world B, its `B` and `C` index nodes
    /// included, in collation order.
    pub nodes: Vec<Node>,
}

/// A variant dropped because a CP case halted at a not-implemented branch
/// and reverting that one dimension value let the session run to its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedVariant {
    /// The generation round that exposed it, from 1.
    pub round: usize,
    /// The CP case that exposed it.
    pub case: String,
    /// The dimension: an order row such as `status`, or `item.` and an item
    /// field such as `item.cost`.
    pub dimension: String,
    /// The value dropped from the dimension's list, as its tag writes it.
    pub value: String,
    /// The not-implemented branch the case halted at.
    pub branch: String,
}

/// An edge case whose class or expected branch its outcome changed. A
/// BOUNDARY case that halts at a different not-implemented branch is listed
/// with `from` and `to` both [`Class::Boundary`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMove {
    /// The case id.
    pub case: String,
    /// The specified class.
    pub from: Class,
    /// The final class.
    pub to: Class,
    /// The final expected branch, or `none`.
    pub branch: String,
}

/// A session that halted at a branch `SPEC.md` lists as implemented. The
/// case keeps its record and inputs and is expected to halt there again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The case id.
    pub case: String,
    /// The branch ID the session halted at.
    pub branch: String,
    /// What the finding records.
    pub cause: String,
}

/// The generated corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    /// The 231 cases in the order of
    /// [`corpus_case_ids`].
    pub cases: Vec<CaseEntry>,
    /// The 200 generated orders, CP-001 to CP-200.
    pub records: Vec<OrderRecord>,
    /// The variants dropped over all rounds.
    pub drops: Vec<DroppedVariant>,
    /// The edge cases moved by their outcome.
    pub moves: Vec<ClassMove>,
    /// The recorded findings.
    pub findings: Vec<Finding>,
    /// The number of generation rounds run: 1 when no variant was dropped,
    /// one more for each round that dropped one.
    pub rounds: usize,
    /// The largest item count of any order.
    pub largest_item_count: usize,
    /// The largest [`order_worst_y`] of any order.
    pub worst_y: usize,
}

/// Why the corpus could not be generated. `Display` gives
/// `<case>: <rule>: <detail>`, without the case when there is none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusError {
    /// The case the failure concerns, if one does.
    pub case: Option<String>,
    /// The rule that failed, such as `profile violation`, `unmet minimum`,
    /// `unresolved reference`, `exhausted drop loop`, `item-count
    /// attribution`, `attribution isolation`, `session failure`, `finding
    /// evidence` or `CR unimplemented`.
    pub rule: String,
    /// What failed.
    pub detail: String,
}

impl CorpusError {
    fn new(case: Option<&str>, rule: &str, detail: impl Into<String>) -> CorpusError {
        CorpusError {
            case: case.map(str::to_string),
            rule: rule.to_string(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(case) = &self.case {
            write!(f, "{case}: ")?;
        }
        write!(f, "{}: {}", self.rule, self.detail)
    }
}

impl std::error::Error for CorpusError {}

// ---------------------------------------------------------------------------
// Store helpers and the reference lists of world A
// ---------------------------------------------------------------------------

/// The key of `^global(subs...)`, each subscript classified by its text.
fn key(global: &str, subs: &[&str]) -> Vec<Sub> {
    let mut key = Vec::with_capacity(subs.len() + 1);
    key.push(Sub::Str(global.to_string()));
    key.extend(subs.iter().map(|sub| Sub::from_text(sub)));
    key
}

/// `parent` extended by the subscripts `subs`.
fn under(parent: &[Sub], subs: &[&str]) -> Vec<Sub> {
    let mut key = parent.to_vec();
    key.extend(subs.iter().map(|sub| Sub::from_text(sub)));
    key
}

/// The numeric child subscripts of `parent` above 0, in collation order, up
/// to the first string subscript: the IENs of a file or the entries of a
/// multiple.
fn numeric_children(store: &Store, parent: &[Sub]) -> Vec<String> {
    let mut out = Vec::new();
    let mut after = Sub::Num("0".to_string());
    while let Some(next) = store.order(parent, Some(&after)) {
        if !next.is_num() {
            break;
        }
        out.push(next.text().to_string());
        after = next;
    }
    out
}

/// Every child subscript of `parent`, as text, in collation order.
fn child_texts(store: &Store, parent: &[Sub]) -> Vec<String> {
    let mut out = Vec::new();
    let mut after = Sub::default();
    while let Some(next) = store.order(parent, Some(&after)) {
        out.push(next.text().to_string());
        after = next;
    }
    out
}

/// The IENs of `^global(file,ien)` above 0 that hold a zero node.
fn file_entries(store: &Store, global: &str, file: &str) -> Vec<String> {
    let root = key(global, &[file]);
    numeric_children(store, &root)
        .into_iter()
        .filter(|ien| store.get(&under(&root, &[ien, "0"])).is_some())
        .collect()
}

/// Whether `^global(file,ien,0)` holds a value in `store`.
fn resolves(store: &Store, global: &str, file: &str, ien: &str) -> bool {
    !ien.is_empty() && store.get(&key(global, &[file, ien, "0"])).is_some()
}

/// The violation of world A's file 442 header `^PRC(442,0)` when its piece
/// 2 does not hold the file attributes `442I`, which the GC fixture
/// `cases.zwr` writes and the corpus never does, or `None` when it holds
/// them. The generator and the fixture profile both test it.
fn file_header_violation(world_a: &Store) -> Option<&'static str> {
    let header = world_a.get_or_empty(&key("PRC", &["442", "0"]));
    (crate::mumps::piece(&header, "^", 2) != "442I")
        .then_some("the world A file 442 header piece 2 is not 442I")
}

/// The vendor fields of node 0 that the order display prints when present
/// [PRCHDP1.m:26-28], in piece order: ORDERING ADDRESS2 to ADDRESS4, city,
/// state, zip and phone, at pieces 3 to 8 and 10.
const VENDOR_PIECES: [Field; 7] = [
    VENDOR_ADDRESS_2,
    VENDOR_ADDRESS_3,
    VENDOR_ADDRESS_4,
    VENDOR_CITY,
    VENDOR_STATE,
    VENDOR_ZIP,
    VENDOR_PHONE,
];

/// The names of the 16 vendor properties of the variation matrix, in order:
/// each of [`VENDOR_PIECES`], named by its piece, present then absent, then
/// ACCT #, the field [`VENDOR_ACCOUNT`] the display prints after them
/// [PRCHDP1.m:29], present then absent.
fn vendor_property_names() -> Vec<String> {
    let mut names: Vec<String> = VENDOR_PIECES
        .iter()
        .map(|field| field.piece)
        .flat_map(|piece| {
            [
                format!("piece {piece} present"),
                format!("piece {piece} absent"),
            ]
        })
        .collect();
    names.push("ACCT # present".to_string());
    names.push("ACCT # absent".to_string());
    names
}

/// Whether vendor `ien` has each of the 16 properties of
/// [`vendor_property_names`], in the same order: each field of
/// [`VENDOR_PIECES`], then [`VENDOR_ACCOUNT`], read on the vendor record
/// `^PRC(440,ien)` and present when its piece is not empty.
fn vendor_properties(store: &Store, ien: &str) -> Vec<bool> {
    let vendor = key("PRC", &["440", ien]);
    let mut out = Vec::with_capacity(16);
    for field in VENDOR_PIECES.into_iter().chain([VENDOR_ACCOUNT]) {
        let present = !field_value(store, &vendor, field).is_empty();
        out.push(present);
        out.push(!present);
    }
    out
}

/// Whether vendor `ien` may be used: it has a zero node, it is not EDI
/// (node 3 piece 2 is not `Y`, which leads to the not-implemented PO-10.b6
/// [PRCHDP1.m:32]), its state pointer is empty or resolves in file 5, and
/// neither node 2 nor node 3 holds only descendants, which the display reads
/// after a bare `$D` [PRCHDP1.m:29-30].
fn vendor_eligible(store: &Store, ien: &str) -> bool {
    let vendor = key("PRC", &["440", ien]);
    if store.get(&under(&vendor, &["0"])).is_none() {
        return false;
    }
    let state = field_value(store, &vendor, VENDOR_STATE);
    field_value(store, &vendor, VENDOR_EDI) != "Y"
        && (state.is_empty() || resolves(store, "DIC", "5", &crate::mumps::num(state)))
        && ["2", "3"]
            .iter()
            .all(|node| store.data(&key("PRC", &["440", ien, node])) != 10)
}

/// The reference entries of world A the corpus orders point to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct References {
    /// Every 442.3 status IEN, ascending.
    statuses: Vec<String>,
    /// The statuses whose piece 2 is below 10 or empty, which make the
    /// display write the non-specifiable message [PRCHRPT.m:73].
    low_statuses: Vec<String>,
    /// The statuses whose name is longer than the 34 characters the display
    /// keeps [PRCHDP1.m:15].
    long_statuses: Vec<String>,
    /// The 442.5 methods of processing below 10, which pass the lookup
    /// screen [PRCHRPT.m:9].
    mops: Vec<String>,
    /// Every 420.8 source code IEN.
    sources: Vec<String>,
    /// The five lowest 49 service IENs.
    services: Vec<String>,
    /// The ten lowest 420.5 unit IENs.
    units: Vec<String>,
    /// Every 442.4 purchase authority IEN.
    authorities: Vec<String>,
    /// The ship-to entries of the station.
    ship_tos: Vec<String>,
    /// The 40 vendors.
    vendors: Vec<String>,
}

const VENDOR_COUNT: usize = 40;

impl References {
    fn read(world_a: &Store) -> Result<References, CorpusError> {
        let statuses = file_entries(world_a, "PRCD", "442.3");
        let status_name = |ien: &String| world_a.get_or_empty(&key("PRCD", &["442.3", ien, "0"]));
        let low_statuses: Vec<String> = statuses
            .iter()
            .filter(|ien| {
                let level = crate::mumps::piece(&status_name(ien), "^", 2).to_string();
                level.is_empty() || crate::mumps::num_cmp(&level, "10") == Ordering::Less
            })
            .cloned()
            .collect();
        let long_statuses: Vec<String> = statuses
            .iter()
            .filter(|ien| {
                crate::mumps::piece(&status_name(ien), "^", 1)
                    .chars()
                    .count()
                    > 34
            })
            .cloned()
            .collect();
        let mops: Vec<String> = file_entries(world_a, "PRCD", "442.5")
            .into_iter()
            .filter(|ien| crate::mumps::num_cmp(ien, "10") == Ordering::Less)
            .collect();
        let sources = file_entries(world_a, "PRCD", "420.8");
        let services: Vec<String> = file_entries(world_a, "DIC", "49")
            .into_iter()
            .take(5)
            .collect();
        let units: Vec<String> = file_entries(world_a, "PRCD", "420.5")
            .into_iter()
            .take(10)
            .collect();
        let authorities = file_entries(world_a, "PRC", "442.4");
        let ship_root = key("PRC", &["411", STATION, "1"]);
        let ship_tos: Vec<String> = numeric_children(world_a, &ship_root)
            .into_iter()
            .filter(|entry| world_a.get(&under(&ship_root, &[entry, "0"])).is_some())
            .collect();
        let vendors = choose_vendors(world_a);
        let minimums: [(&str, usize, usize); 10] = [
            ("442.3 statuses", statuses.len(), 1),
            ("442.3 statuses below 10 or empty", low_statuses.len(), 1),
            (
                "442.3 statuses named over 34 characters",
                long_statuses.len(),
                3,
            ),
            ("442.5 methods of processing below 10", mops.len(), 1),
            ("420.8 source codes", sources.len(), 1),
            ("49 services", services.len(), 5),
            ("420.5 units", units.len(), 10),
            ("442.4 purchase authorities", authorities.len(), 3),
            ("411 ship-to entries of station 500", ship_tos.len(), 1),
            ("non-EDI 440 vendors", vendors.len(), VENDOR_COUNT),
        ];
        for (list, found, minimum) in minimums {
            if found < minimum {
                return Err(CorpusError::new(
                    None,
                    "unmet minimum",
                    format!("{list}: world A holds {found}, the corpus needs {minimum}"),
                ));
            }
        }
        let held: Vec<Vec<bool>> = vendors
            .iter()
            .map(|vendor| vendor_properties(world_a, vendor))
            .collect();
        let missing_property =
            vendor_property_names()
                .into_iter()
                .enumerate()
                .find(|(index, _)| {
                    !held
                        .iter()
                        .any(|properties| properties.get(*index).copied().unwrap_or(false))
                });
        if let Some((_, property)) = missing_property {
            return Err(CorpusError::new(
                None,
                "unmet minimum",
                format!("no eligible 440 vendor has the property {property}"),
            ));
        }
        Ok(References {
            statuses,
            low_statuses,
            long_statuses,
            mops,
            sources,
            services,
            units,
            authorities,
            ship_tos,
            vendors,
        })
    }
}

/// The 40 vendors, in choice order: first, for each of the 16 properties of
/// [`vendor_property_names`] in order, the lowest eligible IEN not yet chosen
/// that has it; then the lowest remaining eligible IENs.
fn choose_vendors(world_a: &Store) -> Vec<String> {
    let eligible: Vec<String> = file_entries(world_a, "PRC", "440")
        .into_iter()
        .filter(|ien| vendor_eligible(world_a, ien))
        .collect();
    let properties: Vec<Vec<bool>> = eligible
        .iter()
        .map(|ien| vendor_properties(world_a, ien))
        .collect();
    let mut chosen: Vec<String> = Vec::new();
    for property in 0..16 {
        let pick = eligible
            .iter()
            .zip(&properties)
            .find(|(ien, held)| {
                !chosen.contains(ien) && held.get(property).copied().unwrap_or(false)
            })
            .map(|(ien, _)| ien.clone());
        if let Some(ien) = pick {
            chosen.push(ien);
        }
    }
    for ien in &eligible {
        if chosen.len() >= VENDOR_COUNT {
            break;
        }
        if !chosen.contains(ien) {
            chosen.push(ien.clone());
        }
    }
    chosen
}

// ---------------------------------------------------------------------------
// Where the order and item fields are stored
// ---------------------------------------------------------------------------

/// Where a field of a file 442 order, of one of its 442.01 items, or of a
/// file 440 vendor is stored below its record, `^PRC(442,ien)`,
/// `^PRC(442,ien,2,j)` or `^PRC(440,ien)`: the node subscript and the
/// 1-based `^`-piece of the field's `node;piece` in its `^DD` zero node.
/// [`assemble`] writes every order and item field at its location,
/// [`reach`] takes each dimension's attribution footprint from the
/// locations, and the shared checks read every field at its location, so
/// the writer, the attribution guard and the checks place a field alike.
/// The vendor fields, which the corpus never writes, are read at their
/// locations alike by the vendor selection and the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Field {
    /// The node subscript below the record.
    node: &'static str,
    /// The 1-based `^`-piece of the node.
    piece: usize,
}

impl Field {
    /// The field stored at `^`-piece `piece` of node `node`.
    const fn at(node: &'static str, piece: usize) -> Field {
        Field { node, piece }
    }
}

/// Order field `.01` PURCHASE ORDER NUMBER, `0;1` [DD.zwr:514312].
const ORDER_NUMBER: Field = Field::at("0", 1);
/// Order field `.02` METHOD OF PROCESSING, `0;2` [DD.zwr:514331].
const ORDER_MOP: Field = Field::at("0", 2);
/// Order field `1` FCP, the fund control point, `0;3` [DD.zwr:514505].
const ORDER_FCP: Field = Field::at("0", 3);
/// Order field `1.4` APPROPRIATION, `0;4` [DD.zwr:514551].
const ORDER_APP: Field = Field::at("0", 4);
/// Order field `2` COST CENTER, `0;5` [DD.zwr:514557].
const ORDER_COST_CENTER: Field = Field::at("0", 5);
/// Order field `7` DELIVERY DATE, `0;10` [DD.zwr:514650].
const ORDER_DELIVERY_DATE: Field = Field::at("0", 10);
/// Order field `13` EST. SHIPPING AND/OR HANDLING, `0;13` [DD.zwr:514683].
const ORDER_SHIPPING: Field = Field::at("0", 13);
/// Order field `91` TOTAL AMOUNT, `0;15` [DD.zwr:515190].
const ORDER_TOTAL: Field = Field::at("0", 15);
/// Order field `5` VENDOR, `1;1` [DD.zwr:514589].
const ORDER_VENDOR: Field = Field::at("1", 1);
/// Order field `5.2` REQUESTING SERVICE, `1;2` [DD.zwr:514606].
const ORDER_SERVICE: Field = Field::at("1", 2);
/// Order field `5.4` SHIP TO, `1;3` [DD.zwr:514615].
const ORDER_SHIP_TO: Field = Field::at("1", 3);
/// Order field `6` VERBAL PURCHASE ORDER (Y/N), `1;4` [DD.zwr:514632].
const ORDER_VERBAL: Field = Field::at("1", 4);
/// Order field `6.2` CONFIRMATION COPY (Y/N), `1;5` [DD.zwr:514636].
const ORDER_CONFIRMATION: Field = Field::at("1", 5);
/// Order field `6.4` F.O.B. POINT, `1;6` [DD.zwr:514639].
const ORDER_FOB: Field = Field::at("1", 6);
/// Order field `8` SOURCE CODE, `1;7` [DD.zwr:514659].
const ORDER_SOURCE: Field = Field::at("1", 7);
/// Order field `8.2` PROPOSAL, `1;8` [DD.zwr:514672].
const ORDER_PROPOSAL: Field = Field::at("1", 8);
/// Order field `16` PA/PPM/AUTHORIZED BUYER, the agent, `1;10`
/// [DD.zwr:514742].
const ORDER_AGENT: Field = Field::at("1", 10);
/// Order field `5.6` DELIVERY LOCATION, `1;11` [DD.zwr:514624].
const ORDER_DELIVERY_LOCATION: Field = Field::at("1", 11);
/// Order field `5.3` DIRECT DELIVERY PATIENT, `1;12` [DD.zwr:514610], which
/// the profile keeps empty.
const ORDER_DIRECT_PATIENT: Field = Field::at("1", 12);
/// Order field `.1` P.O. DATE, `1;15` [DD.zwr:514394].
const ORDER_DATE: Field = Field::at("1", 15);
/// Order field `.5` SUPPLY STATUS, `7;1` [DD.zwr:514433].
const ORDER_STATUS: Field = Field::at("7", 1);
/// Order field `.08` ESTIMATED ORDER?, `7;3` [DD.zwr:514379], which the
/// profile keeps empty.
const ORDER_ESTIMATED_ORDER: Field = Field::at("7", 3);
/// Order field `31` SUBSTATION, `23;7` [DD.zwr:514881], which the profile
/// keeps empty.
const ORDER_SUBSTATION: Field = Field::at("23", 7);
/// Order field `46` PURCHASE CARD NUMBER, `23;8` [DD.zwr:514943], which the
/// profile keeps empty.
const ORDER_CARD_NUMBER: Field = Field::at("23", 8);
/// Order field `48` TYPE OF ORDER, `23;11` [DD.zwr:514968].
const ORDER_TYPE: Field = Field::at("23", 11);
/// Order field `61` PURCHASE CARD HOLDER, `23;22` [DD.zwr:515047], which
/// the profile keeps empty.
const ORDER_CARD_HOLDER: Field = Field::at("23", 22);
/// The node of order field `40` ITEM, the 442.01 item multiple `2;0`
/// [DD.zwr:514888]: its header at `2,0`, item j at `2,j` and the item index
/// `B` at `2,"B"`.
const ORDER_ITEMS: &str = "2";
/// The node of order field `8.3` PURCHASE METHOD, the 442.12 authority
/// multiple `14;0` [DD.zwr:514677]: its header at `14,0` and entry j at
/// `14,j,0`.
const ORDER_AUTHORITIES: &str = "14";
/// Item field `.01` LINE ITEM NUMBER, `0;1` [DD.zwr:515986].
const ITEM_NUMBER: Field = Field::at("0", 1);
/// Item field `2` QUANTITY, `0;2` [DD.zwr:516079].
const ITEM_QUANTITY: Field = Field::at("0", 2);
/// Item field `3` UNIT OF PURCHASE, `0;3` [DD.zwr:516096].
const ITEM_UNIT: Field = Field::at("0", 3);
/// Item field `3.5` BOC, `0;4` [DD.zwr:516110].
const ITEM_BOC: Field = Field::at("0", 4);
/// Item field `9` VENDOR STOCK NUMBER, the STK#, `0;6` [DD.zwr:516187].
const ITEM_STK: Field = Field::at("0", 6);
/// Item field `5` ACTUAL UNIT COST, `0;9` [DD.zwr:516145].
const ITEM_UNIT_COST: Field = Field::at("0", 9);
/// Item field `3.1` PACKAGING MULTIPLE, the Items per line, `0;12`
/// [DD.zwr:516100].
const ITEM_PER: Field = Field::at("0", 12);
/// Item field `9.5` NSN, `0;13` [DD.zwr:516208].
const ITEM_NSN: Field = Field::at("0", 13);
/// Item field `15` TOTAL COST, the extended cost, `2;1` [DD.zwr:516287].
const ITEM_EXTENDED_COST: Field = Field::at("2", 1);
/// Item field `4` CONTRACT/BOA #, `2;2` [DD.zwr:516132], which the profile
/// keeps empty.
const ITEM_CONTRACT: Field = Field::at("2", 2);
/// Item field `11` QUANTITY PREVIOUSLY RECEIVED, `2;8` [DD.zwr:516239],
/// which the profile keeps empty.
const ITEM_RECEIVED: Field = Field::at("2", 8);
/// Item field `12` EDI STATUS CODE 1, `2;9` [DD.zwr:516265], which the
/// profile keeps empty.
const ITEM_EDI_STATUS_1: Field = Field::at("2", 9);
/// Item field `13` EDI STATUS CODE 2, `2;11` [DD.zwr:516276], which the
/// profile keeps empty.
const ITEM_EDI_STATUS_2: Field = Field::at("2", 11);
/// Item field `41` FOOD GROUP, `4;12` [DD.zwr:516400].
const ITEM_FOOD_GROUP: Field = Field::at("4", 12);
/// The node of item field `1` DESCRIPTION, the 442.05 word-processing
/// multiple `1;0` [DD.zwr:516024]: its header at `1,0` and line n at
/// `1,n,0`.
const ITEM_DESCRIPTION: &str = "1";
/// Vendor field `2` ORDERING ADDRESS2, `0;3` [DD.zwr:510123].
const VENDOR_ADDRESS_2: Field = Field::at("0", 3);
/// Vendor field `3` ORDERING ADDRESS3, `0;4` [DD.zwr:510128].
const VENDOR_ADDRESS_3: Field = Field::at("0", 4);
/// Vendor field `4` ORDERING ADDRESS4, `0;5` [DD.zwr:510133].
const VENDOR_ADDRESS_4: Field = Field::at("0", 5);
/// Vendor field `4.2` ORDERING CITY, `0;6` [DD.zwr:510138].
const VENDOR_CITY: Field = Field::at("0", 6);
/// Vendor field `4.4` ORDERING STATE, a file 5 pointer, `0;7`
/// [DD.zwr:510143].
const VENDOR_STATE: Field = Field::at("0", 7);
/// Vendor field `4.6` ORDERING ZIP CODE, `0;8` [DD.zwr:510148].
const VENDOR_ZIP: Field = Field::at("0", 8);
/// Vendor field `5` VENDOR PHONE NUMBER, `0;10` [DD.zwr:510172].
const VENDOR_PHONE: Field = Field::at("0", 10);
/// Vendor field `5.1` ACCOUNT NO., the ACCT # line, `2;1` [DD.zwr:510177].
const VENDOR_ACCOUNT: Field = Field::at("2", 1);
/// Vendor field `.2` EDI VENDOR?, `3;2` [DD.zwr:510094].
const VENDOR_EDI: Field = Field::at("3", 2);

/// The value of `field` of the record `record` in `store`: the field's
/// piece of its node, empty when the node holds no value.
fn field_value<'s>(store: &'s Store, record: &[Sub], field: Field) -> &'s str {
    store
        .get(&under(record, &[field.node]))
        .map_or("", |node| crate::mumps::piece(node, "^", field.piece))
}

/// The nodes below `record` that `values` write, each value at its field's
/// piece: a node holds every piece up to the highest one a value is placed
/// at, and a piece no value is placed at is empty, as
/// `S $P(node,U,piece)=value` leaves them.
fn field_nodes(record: &[Sub], values: &[(Field, &str)]) -> BTreeMap<Vec<Sub>, String> {
    let mut nodes: BTreeMap<&'static str, Vec<&str>> = BTreeMap::new();
    for &(field, value) in values {
        let pieces = nodes.entry(field.node).or_default();
        if pieces.len() < field.piece {
            pieces.resize(field.piece, "");
        }
        if let Some(slot) = field.piece.checked_sub(1).and_then(|at| pieces.get_mut(at)) {
            *slot = value;
        }
    }
    nodes
        .into_iter()
        .map(|(node, pieces)| (under(record, &[node]), pieces.join("^")))
        .collect()
}

// ---------------------------------------------------------------------------
// Rotation lists and plans
// ---------------------------------------------------------------------------

/// The value lists every dimension rotates over, keyed by dimension name.
/// Values are labels: a reference IEN, a stored value, or a shape such as a
/// length or `present`; the empty label stands for an empty field. Dropping
/// a variant removes its value from its list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Lists {
    rows: BTreeMap<String, Vec<String>>,
}

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn with_empty(values: &[String]) -> Vec<String> {
    let mut out = values.to_vec();
    out.push(String::new());
    out
}

impl Lists {
    fn initial(refs: &References) -> Lists {
        let mut statuses = refs.statuses.clone();
        statuses.extend(refs.low_statuses.iter().cloned());
        let lengths: Vec<String> = (1..=32).map(|length: usize| length.to_string()).collect();
        let rows: [(&str, Vec<String>); 27] = [
            ("status", statuses),
            ("type", owned(&["", "P", "S", "D"])),
            ("source", with_empty(&refs.sources)),
            ("mop", refs.mops.clone()),
            ("vendor", refs.vendors.clone()),
            ("service", with_empty(&refs.services)),
            ("shipto", with_empty(&refs.ship_tos)),
            ("delivery", owned(&["present", "absent"])),
            ("verbal", owned(&["verbal", "confirmation", ""])),
            ("fob", owned(&["O", "D", ""])),
            ("proposal", owned(&PROPOSAL_LENGTHS)),
            ("authority", owned(&["0", "1", "3"])),
            ("date", owned(&DATES)),
            ("app", owned(&["1", "4", "7", "10"])),
            ("agent", owned(&[".5", ""])),
            ("total", owned(&TOTALS)),
            ("shipping", owned(&SHIPPING)),
            ("item.desc", lengths),
            ("item.qty", owned(&["whole", "decimal"])),
            ("item.unit", with_empty(&refs.units)),
            ("item.cost", owned(&COST_FORMS)),
            ("item.ext", owned(&["short", "long"])),
            ("item.stk", owned(&["absent", "present"])),
            ("item.nsn", owned(&["absent", "present", "absent"])),
            (
                "item.food",
                owned(&["absent", "present", "absent", "absent", "absent"]),
            ),
            (
                "item.per",
                owned(&["absent", "absent", "present", "absent"]),
            ),
            ("item.boc", owned(&["one", "several"])),
        ];
        Lists {
            rows: rows
                .into_iter()
                .map(|(name, values)| (name.to_string(), values))
                .collect(),
        }
    }

    /// The list of `dimension`, empty when every value was dropped.
    fn list(&self, dimension: &str) -> &[String] {
        self.rows.get(dimension).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The value at `position mod L` of `dimension`'s list of length L.
    fn rotate(&self, dimension: &str, position: usize) -> Result<String, CorpusError> {
        let list = self.list(dimension);
        position
            .checked_rem(list.len())
            .and_then(|at| list.get(at))
            .cloned()
            .ok_or_else(|| {
                CorpusError::new(
                    None,
                    "unmet minimum",
                    format!("every value of {dimension} was dropped"),
                )
            })
    }

    /// Removes every occurrence of `value` from `dimension`'s list.
    fn drop_value(&mut self, dimension: &str, value: &str) {
        if let Some(list) = self.rows.get_mut(dimension) {
            list.retain(|kept| kept != value);
        }
    }
}

/// The dimension values chosen for one order, before any stream draw.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Plan {
    /// The case number n of CP-n.
    n: usize,
    /// The order row values, in [`ORDER_ROWS`] order.
    order: Vec<(String, String)>,
    /// Per item, the item row values keyed `item.` and the field, in
    /// [`ITEM_ROWS`] order.
    items: Vec<Vec<(String, String)>>,
}

impl Plan {
    /// The value of order row `dimension`.
    fn value(&self, dimension: &str) -> &str {
        self.order
            .iter()
            .find(|(name, _)| name == dimension)
            .map_or("", |(_, value)| value.as_str())
    }

    /// The value of item row `dimension` of `item`.
    fn item_value<'a>(item: &'a [(String, String)], dimension: &str) -> &'a str {
        item.iter()
            .find(|(name, _)| name == dimension)
            .map_or("", |(_, value)| value.as_str())
    }
}

/// The item count of every CP order, CP-001 first: 1, except at the
/// [`MULTI_ORDERS`] positions `n = 4k + (k mod 4) + 1`, which take their
/// counts round-robin from [`MULTI_QUOTAS`].
fn item_counts() -> Vec<usize> {
    let mut counts = vec![1; CP_COUNT];
    let mut quotas = MULTI_QUOTAS.to_vec();
    let mut cursor = 0;
    for k in 0..MULTI_ORDERS {
        let length = quotas.len();
        let taken = (0..length).find_map(|step| {
            let at = (cursor + step) % length;
            let quota = quotas.get_mut(at)?;
            if quota.1 == 0 {
                return None;
            }
            quota.1 -= 1;
            Some((at, quota.0))
        });
        let Some((at, count)) = taken else {
            break;
        };
        cursor = at + 1;
        if let Some(slot) = counts.get_mut(4 * k + k % 4) {
            *slot = count;
        }
    }
    counts
}

/// The plans of the 200 orders under `lists`, by rotation.
fn plans(lists: &Lists) -> Result<Vec<Plan>, CorpusError> {
    let mut plans = Vec::with_capacity(CP_COUNT);
    let mut g = 0;
    for (index, count) in item_counts().into_iter().enumerate() {
        let n = index + 1;
        let order = ORDER_ROWS
            .iter()
            .map(|row| Ok((row.to_string(), lists.rotate(row, n)?)))
            .collect::<Result<Vec<_>, CorpusError>>()?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            g += 1;
            let item = ITEM_ROWS
                .iter()
                .map(|field| {
                    let dimension = format!("item.{field}");
                    let value = lists.rotate(&dimension, g)?;
                    Ok((dimension, value))
                })
                .collect::<Result<Vec<_>, CorpusError>>()?;
            items.push(item);
        }
        plans.push(Plan { n, order, items });
    }
    Ok(plans)
}

// ---------------------------------------------------------------------------
// Records: stream draws, nodes, thinning and tags
// ---------------------------------------------------------------------------

/// A stream draw below `bound`, as an index.
fn pick(rng: &mut Xorshift64, bound: usize) -> usize {
    let drawn = rng.below(u64::try_from(bound).unwrap_or(u64::MAX));
    usize::try_from(drawn).unwrap_or(0)
}

/// One stream character of [`ALPHABET`].
fn letter(rng: &mut Xorshift64) -> char {
    ALPHABET
        .get(pick(rng, ALPHABET.len()))
        .map_or('A', |byte| char::from(*byte))
}

/// One word of 1 to 8 stream characters.
fn word(rng: &mut Xorshift64) -> String {
    let length = 1 + pick(rng, 8);
    (0..length).map(|_| letter(rng)).collect()
}

/// Stream text of exactly `length` characters: words of 1 to 8 characters
/// joined by single spaces, with no leading or trailing space. A word that
/// would leave exactly one character, which could only be a trailing space,
/// is lengthened to the end or shortened by one.
fn stream_text(rng: &mut Xorshift64, length: usize) -> String {
    let mut out = String::with_capacity(length);
    let mut remaining = length;
    while remaining > 0 {
        let mut size = 1 + pick(rng, remaining.min(8));
        if remaining - size == 1 {
            size = if remaining <= 8 { remaining } else { size - 1 };
        }
        out.extend((0..size).map(|_| letter(rng)));
        remaining -= size;
        if remaining > 0 {
            out.push(' ');
            remaining -= 1;
        }
    }
    out
}

/// `length` stream digits, the first 1 to 9 so the text is a canonical
/// integer.
fn digits(rng: &mut Xorshift64, length: usize) -> String {
    (0..length)
        .map(|at| {
            let digit = if at == 0 {
                1 + pick(rng, 9)
            } else {
                pick(rng, 10)
            };
            char::from_digit(u32::try_from(digit).unwrap_or(1), 10).unwrap_or('1')
        })
        .collect()
}

/// An NSN in the 16-character shape the input transform of field 442.01/9.5
/// accepts [DD.zwr:516208]: 4 stream digits, `-`, 2 stream characters, `-`,
/// 3 stream characters, `-` and 4 stream digits. The stream characters of
/// [`ALPHABET`] are the pattern's `UN` class.
fn nsn_code(rng: &mut Xorshift64) -> String {
    let group = digits(rng, 4);
    let first: String = (0..2).map(|_| letter(rng)).collect();
    let second: String = (0..3).map(|_| letter(rng)).collect();
    let serial = digits(rng, 4);
    format!("{group}-{first}-{second}-{serial}")
}

/// A FOOD GROUP code of field 442.01/41, `1` to [`FOOD_GROUPS`]
/// [DD.zwr:516400].
fn food_group_code(rng: &mut Xorshift64) -> String {
    (1 + pick(rng, FOOD_GROUPS)).to_string()
}

/// A canonical decimal with an integer part of 1 to `whole_below` and
/// `places` fraction digits, the last 1 to 9.
fn decimal(rng: &mut Xorshift64, whole_below: usize, places: usize) -> String {
    let whole = 1 + pick(rng, whole_below);
    let mut fraction = String::with_capacity(places);
    for at in 0..places {
        let digit = if at + 1 == places {
            1 + pick(rng, 9)
        } else {
            pick(rng, 10)
        };
        fraction.push(char::from_digit(u32::try_from(digit).unwrap_or(1), 10).unwrap_or('1'));
    }
    format!("{whole}.{fraction}")
}

/// The tag form of a value: `empty` for the empty label.
fn tag_value(value: &str) -> &str {
    if value.is_empty() { "empty" } else { value }
}

/// The label a tag value stands for: the empty label for `empty`, inverting
/// [`tag_value`].
fn tag_label(value: &str) -> &str {
    if value == "empty" { "" } else { value }
}

/// A label that names a length or count, as a number.
fn label_number(case: &str, dimension: &str, value: &str) -> Result<usize, CorpusError> {
    value.parse().map_err(|_| {
        CorpusError::new(
            Some(case),
            "dimension value",
            format!("{dimension} value {value} is not a count"),
        )
    })
}

/// One item of an order as drawn, before its nodes are written. The unit,
/// which draws nothing, is read from the plan when the nodes are written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemDraft {
    desc: String,
    qty: String,
    cost: String,
    ext: String,
    stk: Option<String>,
    nsn: Option<String>,
    food: Option<String>,
    per: Option<String>,
    boc: String,
    /// The optional lines thinning removed, by field.
    thinned: Vec<&'static str>,
}

impl ItemDraft {
    /// The item's paging shape: its printed detail lines among STK#, NSN
    /// and FOOD GROUP [PRCHDP2.m:56], and whether its Items per line prints,
    /// which it does only when the order type is not `S` [PRCHDP2.m:59].
    fn shape(&self, purchase_card: bool) -> (usize, usize) {
        let details = [&self.stk, &self.nsn, &self.food]
            .iter()
            .filter(|line| line.is_some())
            .count();
        let per = usize::from(!purchase_card && self.per.is_some());
        (details, per)
    }

    /// Removes the item's next optional printed line in thinning order,
    /// Items per, FOOD GROUP, NSN, then STK#, and reports whether it removed
    /// one. Items per counts only where it prints.
    fn thin_one(&mut self, purchase_card: bool) -> bool {
        let slots: [(&'static str, &mut Option<String>); 4] = [
            ("per", &mut self.per),
            ("food", &mut self.food),
            ("nsn", &mut self.nsn),
            ("stk", &mut self.stk),
        ];
        for (field, slot) in slots {
            if field == "per" && purchase_card {
                continue;
            }
            if slot.take().is_some() {
                self.thinned.push(field);
                return true;
            }
        }
        false
    }
}

/// The values one order draws from the stream, before its nodes are
/// written: every value of a drawn dimension, and the fund control point
/// and cost center, which no dimension decides.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OrderDraft {
    /// Node 1 piece 11, empty when the plan has no delivery location.
    delivery: String,
    /// Node 1 piece 8.
    proposal: String,
    /// Node 0 piece 4.
    app: String,
    /// Node 0 piece 3.
    fund_control_point: String,
    /// Node 0 piece 5.
    cost_center: String,
    /// The items, in item order.
    items: Vec<ItemDraft>,
}

/// The largest `$Y` of an order whose items have the paging `shapes` of
/// [`ItemDraft::shape`], with the estimated-shipping line when `shipping`.
/// `HDR` leaves `$Y` at 4 [PRCHDP2.m:42]; item k's `ASK` [PRCHDP2.m:60]
/// sees `4 + sum over j<k of (3 + d_j + ip_j) + 1 + d_k + 2*ip_k`, and the
/// end is `4 + sum over all items of (3 + d_j + ip_j)`, plus 1 for the
/// shipping line [PRCHDP2.m:13].
fn worst_y(shapes: &[(usize, usize)], shipping: bool) -> usize {
    let mut y = 4;
    let mut worst = 0;
    for (details, per) in shapes {
        worst = worst.max(y + 1 + details + 2 * per);
        y += 3 + details + per;
    }
    worst.max(y + usize::from(shipping))
}

/// One built order with what produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Built {
    plan: Plan,
    /// The stream state the order's draws started from. [`revert`] draws
    /// a reverted plan afresh from a copy of it, so the round's own
    /// stream is never touched. That fresh draw starts where the order's did
    /// but shifts every later value once a presence, a length or a form
    /// changes, so only the reverted dimension's values are taken from it.
    start: Xorshift64,
    /// The order's drawn values after thinning. A reverted order keeps each
    /// of them, the fund control point, the cost center and every item's
    /// thinning included, except those of the reverted dimension.
    draft: OrderDraft,
    record: OrderRecord,
    tags: Vec<String>,
}

/// Draws the values of `plan`'s order, CP case `case_id`, from `rng` in
/// this order: the delivery location (a draw for its length,
/// `1 + r mod 20`, then its text, when present), the proposal text, the APP
/// digits, the fund control point's three digits and word, the cost
/// center's four digits; then per item the description text, the
/// quantity, the unit cost (none for `N/C`), the extended cost, the STK#
/// word, the NSN code ([`nsn_code`]), the FOOD GROUP code
/// ([`food_group_code`]) and the Items per count when present, and the BOC
/// with its words. Because a draw happens only where the plan gives a
/// value to draw, a different presence, length or form shifts every later
/// draw.
fn draw_order(plan: &Plan, case_id: &str, rng: &mut Xorshift64) -> Result<OrderDraft, CorpusError> {
    let delivery = if plan.value("delivery") == "present" {
        let length = 1 + usize::try_from(rng.next_u64() % 20).unwrap_or(0);
        stream_text(rng, length)
    } else {
        String::new()
    };
    let proposal_length = label_number(case_id, "proposal", plan.value("proposal"))?;
    let proposal = stream_text(rng, proposal_length);
    let app_length = label_number(case_id, "app", plan.value("app"))?;
    let app = digits(rng, app_length);
    let fund_control_point = format!("{} {}", digits(rng, 3), word(rng));
    let cost_center = digits(rng, 4);

    let mut drafts = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let field = |name: &str| Plan::item_value(item, &format!("item.{name}")).to_string();
        let desc_length = label_number(case_id, "item.desc", &field("desc"))?;
        let desc = stream_text(rng, desc_length);
        let qty = if field("qty") == "decimal" {
            decimal(rng, 99, 1)
        } else {
            (1 + pick(rng, 999)).to_string()
        };
        let cost = match field("cost").as_str() {
            "N/C" => "N/C".to_string(),
            "whole" => (1 + pick(rng, 9999)).to_string(),
            form => {
                let places = form
                    .strip_suffix("dp")
                    .and_then(|places| places.parse().ok())
                    .unwrap_or(2);
                decimal(rng, 999, places)
            }
        };
        let ext = if field("ext") == "long" {
            format!(
                "{}.{}{}",
                10_000 + pick(rng, 90_000),
                pick(rng, 10),
                1 + pick(rng, 9)
            )
        } else {
            decimal(rng, 9999, 1)
        };
        let stk = (field("stk") == "present").then(|| word(rng));
        let nsn = (field("nsn") == "present").then(|| nsn_code(rng));
        let food = (field("food") == "present").then(|| food_group_code(rng));
        let per = (field("per") == "present").then(|| (1 + pick(rng, 99)).to_string());
        let boc = if field("boc") == "several" {
            format!("{} {} {}", digits(rng, 4), word(rng), word(rng))
        } else {
            digits(rng, 4)
        };
        drafts.push(ItemDraft {
            desc,
            qty,
            cost,
            ext,
            stk,
            nsn,
            food,
            per,
            boc,
            thinned: Vec::new(),
        });
    }
    Ok(OrderDraft {
        delivery,
        proposal,
        app,
        fund_control_point,
        cost_center,
        items: drafts,
    })
}

/// Thins the drawn `items` of `plan`'s order when it has five or more,
/// until [`worst_y`] is within [`PAGING_BOUND`]: from the last item back,
/// Items per, FOOD GROUP, NSN and STK#, one printed line at a time.
fn thin(plan: &Plan, items: &mut [ItemDraft]) {
    let purchase_card = plan.value("type") == "S";
    let shipping = crate::mumps::num_cmp(plan.value("shipping"), "0") == Ordering::Greater;
    if items.len() >= 5 {
        let shapes = |drafts: &[ItemDraft]| -> Vec<(usize, usize)> {
            drafts
                .iter()
                .map(|draft| draft.shape(purchase_card))
                .collect()
        };
        while worst_y(&shapes(items), shipping) > PAGING_BOUND {
            if !items
                .iter_mut()
                .rev()
                .any(|draft| draft.thin_one(purchase_card))
            {
                break;
            }
        }
    }
}

/// Builds CP-n of `plan` with the stream `rng`: [`draw_order`] draws its
/// values, [`thin`] thins an order of five or more items, and [`assemble`]
/// writes its nodes and tags.
fn build_record(
    plan: &Plan,
    refs: &References,
    rng: &mut Xorshift64,
) -> Result<Built, CorpusError> {
    let start = rng.clone();
    let mut draft = draw_order(plan, &format!("CP-{:03}", plan.n), rng)?;
    thin(plan, &mut draft.items);
    assemble(plan, refs, start, draft)
}

/// CP-n of `plan` with the drawn and thinned values `draft`, its draws
/// started from `start`: its nodes, each field written at its [`Field`]
/// location from the plan's rotation values and the draft's drawn ones, the
/// authority entries from `refs`, and its tags, where a thinned optional
/// line is tagged `thinned`.
fn assemble(
    plan: &Plan,
    refs: &References,
    start: Xorshift64,
    draft: OrderDraft,
) -> Result<Built, CorpusError> {
    let n = plan.n;
    let case_id = format!("CP-{n:03}");
    let ien = (IEN_BASE + n).to_string();
    let c_value = format!("CP{n:03}");
    let order_type = plan.value("type").to_string();
    if draft.items.len() != plan.items.len() {
        return Err(CorpusError::new(
            Some(&case_id),
            "order draft",
            format!(
                "{} drawn items for {} planned",
                draft.items.len(),
                plan.items.len()
            ),
        ));
    }
    let (verbal, confirmation) = match plan.value("verbal") {
        "verbal" => ("Y", ""),
        "confirmation" => ("Y", "Y"),
        _ => ("", ""),
    };
    let authorities = label_number(&case_id, "authority", plan.value("authority"))?;
    let drafts = &draft.items;

    let date = plan.value("date");
    let order = key("PRC", &["442", &ien]);
    // Nodes 0, 1 and 7. The P.O. date takes the delivery date's value, and
    // `.08` is placed empty so node 7 keeps its three pieces.
    let mut nodes: BTreeMap<Vec<Sub>, String> = field_nodes(
        &order,
        &[
            (ORDER_NUMBER, format!("{STATION}-{c_value}").as_str()),
            (ORDER_MOP, plan.value("mop")),
            (ORDER_FCP, &draft.fund_control_point),
            (ORDER_APP, &draft.app),
            (ORDER_COST_CENTER, &draft.cost_center),
            (ORDER_DELIVERY_DATE, date),
            (ORDER_SHIPPING, plan.value("shipping")),
            (ORDER_TOTAL, plan.value("total")),
            (ORDER_VENDOR, plan.value("vendor")),
            (ORDER_SERVICE, plan.value("service")),
            (ORDER_SHIP_TO, plan.value("shipto")),
            (ORDER_VERBAL, verbal),
            (ORDER_CONFIRMATION, confirmation),
            (ORDER_FOB, plan.value("fob")),
            (ORDER_SOURCE, plan.value("source")),
            (ORDER_PROPOSAL, &draft.proposal),
            (ORDER_AGENT, plan.value("agent")),
            (ORDER_DELIVERY_LOCATION, &draft.delivery),
            (ORDER_DATE, date),
            (ORDER_STATUS, plan.value("status")),
            (ORDER_ESTIMATED_ORDER, ""),
        ],
    );
    let items = under(&order, &[ORDER_ITEMS]);
    let count = drafts.len().to_string();
    nodes.insert(under(&items, &["0"]), format!("^442.01^{count}^{count}"));
    for (index, (planned, draft)) in plan.items.iter().zip(drafts).enumerate() {
        let j = (index + 1).to_string();
        let item = under(&items, &[&j]);
        let mut values = vec![
            (ITEM_NUMBER, j.as_str()),
            (ITEM_QUANTITY, &draft.qty),
            (ITEM_UNIT, Plan::item_value(planned, "item.unit")),
            (ITEM_BOC, &draft.boc),
            (ITEM_STK, draft.stk.as_deref().unwrap_or("")),
            (ITEM_UNIT_COST, &draft.cost),
            (ITEM_PER, draft.per.as_deref().unwrap_or("")),
            (ITEM_NSN, draft.nsn.as_deref().unwrap_or("")),
            (ITEM_EXTENDED_COST, &draft.ext),
        ];
        if let Some(food) = &draft.food {
            values.push((ITEM_FOOD_GROUP, food));
        }
        nodes.extend(field_nodes(&item, &values));
        nodes.insert(
            under(&item, &[ITEM_DESCRIPTION, "0"]),
            DESCRIPTION_HEADER.to_string(),
        );
        nodes.insert(
            under(&item, &[ITEM_DESCRIPTION, "1", "0"]),
            draft.desc.clone(),
        );
        nodes.insert(under(&items, &["B", &j, &j]), String::new());
    }
    if authorities > 0 {
        let size = refs.authorities.len();
        nodes.insert(
            under(&order, &[ORDER_AUTHORITIES, "0"]),
            format!("^442.12PA^{authorities}^{authorities}"),
        );
        for j in 1..=authorities {
            let authority = (n + j)
                .checked_rem(size)
                .and_then(|at| refs.authorities.get(at))
                .ok_or_else(|| {
                    CorpusError::new(Some(&case_id), "unresolved reference", "no 442.4 authority")
                })?;
            nodes.insert(
                under(&order, &[ORDER_AUTHORITIES, &j.to_string(), "0"]),
                authority.clone(),
            );
        }
    }
    if !order_type.is_empty() {
        nodes.extend(field_nodes(&order, &[(ORDER_TYPE, order_type.as_str())]));
    }
    nodes.insert(
        key("PRC", &["442", "B", &format!("{STATION}-{c_value}"), &ien]),
        String::new(),
    );
    nodes.insert(key("PRC", &["442", "C", &c_value, &ien]), String::new());

    let mut tags: Vec<String> = plan
        .order
        .iter()
        .map(|(dimension, value)| format!("{dimension}={}", tag_value(value)))
        .collect();
    tags.push(format!("items={}", drafts.len()));
    for (index, (item, draft)) in plan.items.iter().zip(drafts).enumerate() {
        for field in ITEM_ROWS {
            let planned = Plan::item_value(item, &format!("item.{field}"));
            let value = if draft.thinned.contains(&field) {
                "thinned"
            } else {
                tag_value(planned)
            };
            tags.push(format!("item{}.{field}={value}", index + 1));
        }
    }
    if let Some(bad) = tags.iter().find(|tag| tag.contains([',', '\t', '\n'])) {
        return Err(CorpusError::new(
            Some(&case_id),
            "tag",
            format!("tag {bad} holds a comma, tab or newline"),
        ));
    }

    let item_count = drafts.len();
    Ok(Built {
        plan: plan.clone(),
        start,
        draft,
        record: OrderRecord {
            case_id,
            ien,
            c_value,
            item_count,
            order_type,
            nodes: nodes.into_iter().collect(),
        },
        tags,
    })
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// A value as a ZWR line writes it: bare when it is a canonical number,
/// otherwise quoted with each `"` doubled.
fn zwr_value(value: &str) -> String {
    if is_canonical(value) {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}

/// The corpus ZWR text of `nodes`: the two header lines, then one
/// `<reference>=<value>` line per node in collation order, each ending `\n`.
fn render_nodes<'a>(nodes: impl Iterator<Item = &'a Node>) -> String {
    let sorted: BTreeMap<&Vec<Sub>, &String> = nodes.map(|(key, value)| (key, value)).collect();
    let mut text = String::new();
    for header in ZWR_HEADER {
        text.push_str(header);
        text.push('\n');
    }
    for (key, value) in sorted {
        text.push_str(&Store::format_ref(key));
        text.push('=');
        text.push_str(&zwr_value(value));
        text.push('\n');
    }
    text
}

/// The inputs of a CP case: the station default, the order's `C` value,
/// the two pause answers, and the `^` that ends the session.
fn cp_inputs(c_value: &str) -> Vec<String> {
    owned(&["", c_value, "", "", "^"])
}

/// The CP orders kept for a finding, by their 0-based position among the
/// 200, each with the implemented branch its session halted at.
type Retained = BTreeMap<usize, (Built, &'static str)>;

/// One generation round under `lists`: the 200 orders built from one
/// stream started at [`SEED`], then each order of `retained` in place of
/// the one built at its position, so that no other order's draws move;
/// all checked to add no node world A holds and to survive their own ZWR
/// text unchanged, and world B built from them.
fn run_round(
    world_a: &Store,
    refs: &References,
    lists: &Lists,
    retained: &Retained,
) -> Result<(Vec<Built>, Store), CorpusError> {
    let mut rng = Xorshift64::new(SEED);
    let mut built = plans(lists)?
        .iter()
        .map(|plan| build_record(plan, refs, &mut rng))
        .collect::<Result<Vec<_>, CorpusError>>()?;
    for (index, (kept, _)) in retained {
        if let Some(slot) = built.get_mut(*index) {
            slot.clone_from(kept);
        }
    }
    for order in &built {
        let record = &order.record;
        let clash = world_a.data(&key("PRC", &["442", &record.ien])) != 0
            || record.nodes.iter().any(|(node, _)| world_a.data(node) != 0);
        if clash {
            return Err(CorpusError::new(
                Some(&record.case_id),
                "corpus key in world A",
                format!("world A already holds a node of order {}", record.ien),
            ));
        }
    }
    let text = render_nodes(built.iter().flat_map(|order| order.record.nodes.iter()));
    let generated: BTreeMap<&[Sub], &str> = built
        .iter()
        .flat_map(|order| order.record.nodes.iter())
        .map(|(node, value)| (node.as_slice(), value.as_str()))
        .collect();
    let parsed = Store::from_zwr(&text)
        .map_err(|error| CorpusError::new(None, "ZWR round trip", error.to_string()))?;
    let same = parsed.len() == generated.len()
        && parsed
            .iter()
            .all(|(node, value)| generated.get(node) == Some(&value));
    if !same {
        return Err(CorpusError::new(
            None,
            "ZWR round trip",
            "the corpus ZWR text does not parse back to exactly the generated nodes",
        ));
    }
    let world = world_b(world_a, &text)
        .map_err(|error| CorpusError::new(None, "world B", error.to_string()))?;
    Ok((built, world))
}

/// One single-dimension reversion of an order: which value it reverts and
/// the value it puts in its place. It holds no plan; [`Reversion::apply`]
/// builds the reverted plan when the reversion's candidate is about to run.
struct Reversion {
    /// The dimension: an order row such as `status`, or `item.` and an item
    /// field such as `item.cost`.
    dimension: String,
    /// The 0-based position of the item an item row reverts; `None` for an
    /// order row.
    item: Option<usize>,
    /// The value the reversion replaces.
    current: String,
    /// The value the reversion puts in place of `current`.
    value: String,
}

impl Reversion {
    /// The name of the tag the reversion changes: the order row, or
    /// `item<j>.<field>` for item j.
    fn tag_name(&self) -> String {
        match (self.item, self.dimension.strip_prefix("item.")) {
            (Some(at), Some(field)) => format!("item{}.{field}", at + 1),
            _ => self.dimension.clone(),
        }
    }

    /// A copy of `plan` with this reversion's one value, the order row
    /// `dimension` or that field of item `item`, set to `value`. The order
    /// rows are distinct, so the row's name finds its slot.
    fn apply(&self, plan: &Plan) -> Plan {
        let mut reverted = plan.clone();
        let slot = match self.item {
            None => reverted
                .order
                .iter_mut()
                .find(|(name, _)| *name == self.dimension),
            Some(at) => reverted
                .items
                .get_mut(at)
                .and_then(|item| item.iter_mut().find(|(name, _)| *name == self.dimension)),
        };
        if let Some(slot) = slot {
            slot.1.clone_from(&self.value);
        }
        reverted
    }
}

/// Every single-dimension reversion of `plan` under `lists`, produced one
/// at a time as the caller takes them, so that no reversion is prepared
/// before its turn: the order rows in matrix order, then the item rows,
/// each over the items in order. A value reverts to its list's first value,
/// or to the second when it is the first, and a value whose list offers
/// neither has no reversion. The item count is not a dimension and never
/// reverts.
fn reversions<'a>(plan: &'a Plan, lists: &'a Lists) -> impl Iterator<Item = Reversion> + 'a {
    let target = move |dimension: &str, current: &str| -> Option<String> {
        let list = lists.list(dimension);
        let first = list.first()?;
        if first != current {
            Some(first.clone())
        } else {
            list.get(1).filter(|second| *second != current).cloned()
        }
    };
    let rows = plan.order.iter().filter_map(move |(dimension, current)| {
        Some(Reversion {
            value: target(dimension, current)?,
            dimension: dimension.clone(),
            item: None,
            current: current.clone(),
        })
    });
    let items = ITEM_ROWS.into_iter().flat_map(move |field| {
        let dimension = format!("item.{field}");
        plan.items.iter().enumerate().filter_map(move |(at, item)| {
            let current = Plan::item_value(item, &dimension);
            Some(Reversion {
                value: target(&dimension, current)?,
                dimension: dimension.clone(),
                item: Some(at),
                current: current.to_string(),
            })
        })
    });
    rows.chain(items)
}

/// The candidate order of `reversion` for `target`: `target`'s own draft
/// with only the reverted dimension changed. The reverted plan is built
/// here, once, from `target`'s plan by [`Reversion::apply`], serves both
/// the fresh draw and the assembly, and is gone when the call returns. A
/// rotation-only dimension, every order row but `delivery`, `proposal` and
/// `app`, and `item.unit`, changes through the reverted plan alone. A drawn
/// one takes its new values from a fresh draw of the reverted plan started
/// at [`Built::start`], and a reverted optional line is no longer marked
/// thinned. Every other drawn value is the target's, and nothing is thinned
/// again.
fn revert(target: &Built, refs: &References, reversion: &Reversion) -> Result<Built, CorpusError> {
    let case = target.record.case_id.as_str();
    let plan = reversion.apply(&target.plan);
    let fresh = draw_order(&plan, case, &mut target.start.clone())?;
    let mut draft = target.draft.clone();
    match reversion.item {
        None => match reversion.dimension.as_str() {
            "delivery" => draft.delivery = fresh.delivery,
            "proposal" => draft.proposal = fresh.proposal,
            "app" => draft.app = fresh.app,
            _ => {}
        },
        Some(at) => {
            let field = reversion
                .dimension
                .strip_prefix("item.")
                .unwrap_or(&reversion.dimension);
            let (Some(item), Some(drawn)) = (draft.items.get_mut(at), fresh.items.get(at)) else {
                return Err(CorpusError::new(
                    Some(case),
                    "attribution isolation",
                    format!("{} names no item of the order", reversion.tag_name()),
                ));
            };
            match field {
                "desc" => item.desc.clone_from(&drawn.desc),
                "qty" => item.qty.clone_from(&drawn.qty),
                "cost" => item.cost.clone_from(&drawn.cost),
                "ext" => item.ext.clone_from(&drawn.ext),
                "stk" => item.stk.clone_from(&drawn.stk),
                "nsn" => item.nsn.clone_from(&drawn.nsn),
                "food" => item.food.clone_from(&drawn.food),
                "per" => item.per.clone_from(&drawn.per),
                "boc" => item.boc.clone_from(&drawn.boc),
                _ => {}
            }
            // Only optional lines are ever marked thinned; a reverted one
            // now holds its reverted state.
            item.thinned.retain(|thinned| *thinned != field);
        }
    }
    assemble(&plan, refs, target.start.clone(), draft)
}

/// A node one dimension writes, below its order: the node's key, and the
/// `^`-pieces the dimension sets, or `None` when it sets the whole node and
/// every node below it.
type Reach = (Vec<Sub>, Option<Vec<usize>>);

/// The nodes and pieces [`assemble`] writes from the dimension of
/// `reversion` below `order`, the key of its order, for an item row in the
/// reverted item only, taken from the dimension's [`Field`] locations: the
/// pieces one dimension sets in a node form that node's one entry. The
/// type, the authority, the description, the extended cost and the FOOD
/// GROUP each own a node no other dimension writes, so each reaches that
/// whole node. An unknown dimension writes nothing.
fn reach(reversion: &Reversion, order: &[Sub]) -> Vec<Reach> {
    let j = reversion
        .item
        .map(|at| (at + 1).to_string())
        .unwrap_or_default();
    let item = under(order, &[ORDER_ITEMS, &j]);
    let pieces = |record: &[Sub], fields: &[Field]| -> Vec<Reach> {
        let mut nodes: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
        for field in fields {
            nodes.entry(field.node).or_default().push(field.piece);
        }
        nodes
            .into_iter()
            .map(|(node, set)| (under(record, &[node]), Some(set)))
            .collect()
    };
    let whole = |record: &[Sub], subs: &[&str]| -> Vec<Reach> { vec![(under(record, subs), None)] };
    match reversion.dimension.as_str() {
        "status" => pieces(order, &[ORDER_STATUS]),
        "type" => whole(order, &[ORDER_TYPE.node]),
        "source" => pieces(order, &[ORDER_SOURCE]),
        "mop" => pieces(order, &[ORDER_MOP]),
        "vendor" => pieces(order, &[ORDER_VENDOR]),
        "service" => pieces(order, &[ORDER_SERVICE]),
        "shipto" => pieces(order, &[ORDER_SHIP_TO]),
        "delivery" => pieces(order, &[ORDER_DELIVERY_LOCATION]),
        "verbal" => pieces(order, &[ORDER_VERBAL, ORDER_CONFIRMATION]),
        "fob" => pieces(order, &[ORDER_FOB]),
        "proposal" => pieces(order, &[ORDER_PROPOSAL]),
        "authority" => whole(order, &[ORDER_AUTHORITIES]),
        "date" => pieces(order, &[ORDER_DELIVERY_DATE, ORDER_DATE]),
        "app" => pieces(order, &[ORDER_APP]),
        "agent" => pieces(order, &[ORDER_AGENT]),
        "total" => pieces(order, &[ORDER_TOTAL]),
        "shipping" => pieces(order, &[ORDER_SHIPPING]),
        "item.desc" => whole(&item, &[ITEM_DESCRIPTION, "1", "0"]),
        "item.qty" => pieces(&item, &[ITEM_QUANTITY]),
        "item.unit" => pieces(&item, &[ITEM_UNIT]),
        "item.cost" => pieces(&item, &[ITEM_UNIT_COST]),
        "item.ext" => whole(&item, &[ITEM_EXTENDED_COST.node]),
        "item.stk" => pieces(&item, &[ITEM_STK]),
        "item.nsn" => pieces(&item, &[ITEM_NSN]),
        "item.food" => whole(&item, &[ITEM_FOOD_GROUP.node]),
        "item.per" => pieces(&item, &[ITEM_PER]),
        "item.boc" => pieces(&item, &[ITEM_BOC]),
        _ => Vec::new(),
    }
}

/// Whether the `^`-pieces of `old` and `new` agree at every position but
/// the 1-based `pieces`.
fn same_outside(old: &str, new: &str, pieces: &[usize]) -> bool {
    let count = old.split('^').count().max(new.split('^').count());
    (1..=count)
        .filter(|at| !pieces.contains(at))
        .all(|at| crate::mumps::piece(old, "^", at) == crate::mumps::piece(new, "^", at))
}

/// What `candidate` changes outside the dimension of `reversion`, against
/// the order `original` it reverts, or `None` when it changes nothing else:
/// the case, IEN, `C` value and item count stay, the order type changes
/// only for `type`, every tag but the reverted one stays, and every node
/// stays but the nodes and pieces of [`reach`].
fn isolation_breach(original: &Built, candidate: &Built, reversion: &Reversion) -> Option<String> {
    let (before, after) = (&original.record, &candidate.record);
    let same_identity = before.case_id == after.case_id
        && before.ien == after.ien
        && before.c_value == after.c_value
        && before.item_count == after.item_count;
    if !same_identity {
        return Some("the order's case, IEN, C value or item count changed".to_string());
    }
    if before.order_type != after.order_type && reversion.dimension != "type" {
        return Some(format!(
            "the order type {} became {}",
            tag_value(&before.order_type),
            tag_value(&after.order_type)
        ));
    }
    if original.tags.len() != candidate.tags.len() {
        return Some(format!(
            "its {} tags became {}",
            original.tags.len(),
            candidate.tags.len()
        ));
    }
    let name = reversion.tag_name();
    let reverted = |tag: &str| tag.split_once('=').is_some_and(|(tag, _)| tag == name);
    if let Some((old, new)) = original
        .tags
        .iter()
        .zip(&candidate.tags)
        .find(|(old, new)| old != new && !(reverted(old) && reverted(new)))
    {
        return Some(format!("tag {old} became {new}"));
    }
    let reach = reach(reversion, &key("PRC", &["442", &before.ien]));
    let nodes = |record: &OrderRecord| -> BTreeMap<Vec<Sub>, String> {
        record.nodes.iter().cloned().collect()
    };
    let (old_nodes, new_nodes) = (nodes(before), nodes(after));
    let keys: BTreeSet<&Vec<Sub>> = old_nodes.keys().chain(new_nodes.keys()).collect();
    for node in keys {
        let (old, new) = (
            old_nodes.get(node).map(String::as_str),
            new_nodes.get(node).map(String::as_str),
        );
        if old == new {
            continue;
        }
        let allowed = reach.iter().any(|(path, pieces)| match (pieces, old, new) {
            (None, _, _) => node.starts_with(path),
            (Some(pieces), Some(old), Some(new)) => node == path && same_outside(old, new, pieces),
            (Some(_), _, _) => false,
        });
        if !allowed {
            let shown = |value: Option<&str>| value.map_or("nothing".to_string(), str::to_string);
            return Some(format!(
                "node {} changed from {} to {}",
                Store::format_ref(node),
                shown(old),
                shown(new)
            ));
        }
    }
    None
}

/// The dimension and value that make CP order `index` of `built` halt at a
/// not-implemented branch: the first of its [`reversions`] after which its
/// session runs to its end. The reversions are taken one at a time, and
/// each candidate is built only when its turn comes and dropped before the
/// next, so no later reversion is prepared once one has decided.
///
/// Each candidate is [`revert`]'s: the order's own draft with only the
/// reverted dimension changed, its new drawn values from a fresh draw
/// started at [`Built::start`], so the round's stream is untouched; every
/// other value, the fund control point, the cost center and each item's
/// thinning included, stays as it was, and nothing is thinned again.
/// Before its session runs, the candidate must differ from the order only
/// in the reverted dimension's tag and in the nodes and pieces that
/// dimension writes ([`isolation_breach`]), else the attribution stops
/// with an `attribution isolation` error; world B is then rebuilt around
/// it. A candidate whose session fails stops the attribution with a
/// `session failure` error naming the reversion and the cause, since a
/// failure is never accepted; one that halts again gives way to the next
/// reversion, and with none left the case's item count is all that
/// remains, an `item-count attribution` error.
fn attribute(
    world_a: &Store,
    refs: &References,
    lists: &Lists,
    built: &[Built],
    index: usize,
    not_implemented: &[String],
) -> Result<(String, String), CorpusError> {
    let target = built
        .get(index)
        .ok_or_else(|| CorpusError::new(None, "item-count attribution", "no order to attribute"))?;
    let case = target.record.case_id.as_str();
    for reversion in reversions(&target.plan, lists) {
        let candidate = revert(target, refs, &reversion)?;
        if let Some(breach) = isolation_breach(target, &candidate, &reversion) {
            return Err(CorpusError::new(
                Some(case),
                "attribution isolation",
                format!(
                    "reverting {} from {} also changes more: {breach}",
                    reversion.tag_name(),
                    tag_value(&reversion.current)
                ),
            ));
        }
        let text = render_nodes(built.iter().enumerate().flat_map(|(at, order)| {
            if at == index {
                candidate.record.nodes.iter()
            } else {
                order.record.nodes.iter()
            }
        }));
        let world = world_b(world_a, &text)
            .map_err(|error| CorpusError::new(Some(case), "world B", error.to_string()))?;
        let run = run_case(&world, &cp_inputs(&candidate.record.c_value));
        match classify(&run, not_implemented) {
            Outcome::End => return Ok((reversion.dimension, reversion.current)),
            Outcome::Failure(cause) => {
                return Err(CorpusError::new(
                    Some(case),
                    "session failure",
                    format!(
                        "with {} reverted from {}: {cause}",
                        reversion.tag_name(),
                        tag_value(&reversion.current)
                    ),
                ));
            }
            Outcome::Unimplemented(_) | Outcome::Implemented(_) => {}
        }
    }
    Err(CorpusError::new(
        Some(case),
        "item-count attribution",
        "no single-dimension reversion lets the session run to its end, which leaves only its \
         item count, and item counts are never dropped",
    ))
}

/// One edge or replay case before its outcome is applied.
struct CaseSpec {
    id: String,
    class: Class,
    inputs: Vec<String>,
    expected: Option<&'static str>,
    tags: Vec<String>,
}

/// The world B `C` values that are exact input lines of `inputs`, and the
/// IENs filed under them, each without repeats, in first-use order.
fn orders_opened(world_b: &Store, inputs: &[String]) -> (Vec<String>, Vec<String>) {
    let index = key("PRC", &["442", "C"]);
    let mut orders: Vec<String> = Vec::new();
    let mut c_values: Vec<String> = Vec::new();
    for line in inputs.iter().filter(|line| !line.is_empty()) {
        let entry = under(&index, &[line]);
        if world_b.data(&entry) == 0 {
            continue;
        }
        if !c_values.contains(line) {
            c_values.push(line.clone());
        }
        for ien in numeric_children(world_b, &entry) {
            if !orders.contains(&ien) {
                orders.push(ien);
            }
        }
    }
    (orders, c_values)
}

/// The 26 edge sequences of `SPEC.md` "Parity corpus", over the built
/// orders: `one` is the lowest one-item order, `multi` the lowest
/// multi-item order, `s45` the lowest status-45 order that is neither, and
/// the CE-15 window the lowest five consecutive orders whose shapes, each
/// the item count and order type, differ pairwise. Their specified classes
/// are 16 IN and 10 BOUNDARY.
fn edge_specs(built: &[Built], world_b: &Store) -> Result<Vec<CaseSpec>, CorpusError> {
    let missing = |what: &str| CorpusError::new(None, "edge sequence", format!("no {what}"));
    let one = built
        .iter()
        .find(|order| order.record.item_count == 1)
        .map(|order| order.record.c_value.clone())
        .ok_or_else(|| missing("one-item order"))?;
    let multi = built
        .iter()
        .find(|order| order.record.item_count > 1)
        .map(|order| order.record.c_value.clone())
        .ok_or_else(|| missing("multi-item order"))?;
    let s45 = built
        .iter()
        .find(|order| {
            order.plan.value("status") == CANCELLED_STATUS
                && order.record.c_value != one
                && order.record.c_value != multi
        })
        .map(|order| order.record.c_value.clone())
        .ok_or_else(|| missing("status-45 order"))?;
    let window: Vec<String> = built
        .windows(5)
        .find(|orders| {
            let shapes: BTreeSet<(usize, &str)> = orders
                .iter()
                .map(|order| (order.record.item_count, order.record.order_type.as_str()))
                .collect();
            shapes.len() == 5
        })
        .map(|orders| {
            orders
                .iter()
                .map(|order| order.record.c_value.clone())
                .collect()
        })
        .ok_or_else(|| missing("window of five orders of distinct shape"))?;

    let c_index = child_texts(world_b, &key("PRC", &["442", "C"]));
    let starting = |prefix: &str| -> Vec<&String> {
        c_index
            .iter()
            .filter(|value| value.starts_with(prefix))
            .collect()
    };
    let ten: Vec<String> = (10..20).map(|k| format!("CP0{k}")).collect();
    if starting("CP01") != ten.iter().collect::<Vec<_>>() {
        return Err(missing(
            "ten C values CP010 to CP019 and no other beginning CP01",
        ));
    }
    if starting("CP20") != vec!["CP200"] {
        return Err(missing("single C value CP200 beginning CP20"));
    }
    let overlong = "Z".repeat(251);
    if overlong.chars().count() != 251 || MISS_THIRTY_Q.chars().count() != 30 {
        return Err(missing("251-character or 30-character answer"));
    }

    let pause = |c_value: &str| vec![c_value.to_string(), String::new(), String::new()];
    let tail = |head: &[&str]| -> Vec<String> {
        let mut inputs = owned(head);
        inputs.extend(pause(&one));
        inputs.extend(owned(&["^", "^", "^"]));
        inputs
    };
    let mut ce12 = vec![String::new()];
    for c_value in [&one, &multi, &s45] {
        ce12.extend(pause(c_value));
    }
    ce12.push("^".to_string());
    let mut ce13 = vec![String::new()];
    ce13.extend(pause(&one));
    ce13.push(MISS_ZZ9.to_string());
    ce13.extend(pause(&multi));
    ce13.extend(owned(&[MISS_NO_SUCH, "^"]));
    let mut ce15 = vec![String::new()];
    for c_value in &window {
        ce15.extend(pause(c_value));
    }
    ce15.push("^".to_string());
    let mut ce09 = owned(&["", MISS_ZZ9]);
    ce09.extend(pause(&one));
    ce09.push("^".to_string());
    let mut ce10 = vec![String::new()];
    ce10.extend(pause(&one));
    ce10.extend(owned(&[MISS_ZZ9, "^"]));
    let mut ce11 = vec![String::new()];
    ce11.extend(pause(&one));
    ce11.extend(pause(&one));
    ce11.push("^".to_string());
    let lower = one.to_ascii_lowercase();

    let rows: Vec<(Class, Vec<String>, Option<&'static str>)> = vec![
        (Class::In, owned(&["", "^"]), None),
        (Class::In, owned(&["", "^FOO"]), None),
        (Class::In, owned(&["", MISS_ZZ9, "^"]), None),
        (Class::In, owned(&["", MISS_NUMBER, "^"]), None),
        (Class::In, owned(&["", MISS_NO_SUCH, "^"]), None),
        (Class::In, owned(&["", MISS_LOWER, "^"]), None),
        (Class::In, owned(&["", MISS_THIRTY_Q, "^"]), None),
        (Class::In, owned(&["", MISS_IEN, "^"]), None),
        (Class::In, ce09, None),
        (Class::In, ce10, None),
        (Class::In, ce11, None),
        (Class::In, ce12, None),
        (Class::In, ce13, None),
        (Class::In, owned(&["", MISS_ZZ9, MISS_NO_SUCH, "^"]), None),
        (Class::In, ce15, None),
        (Class::In, owned(&["", &one, "", "^", "^"]), None),
        (Class::Boundary, tail(&["", ""]), Some("PO-09.b6")),
        (Class::Boundary, tail(&["", "?"]), Some("PO-09.b4")),
        (Class::Boundary, tail(&["", &lower]), Some("PO-09.b13")),
        (Class::Boundary, tail(&["", "CP01"]), Some("PO-09.b10")),
        (Class::Boundary, tail(&["", "CP20"]), Some("PO-09.b9")),
        (Class::Boundary, tail(&[STATION]), Some("PO-05.b2")),
        (Class::Boundary, tail(&["", "`"]), Some("PO-09.b5")),
        (Class::Boundary, tail(&["", " "]), Some("PO-09.b5")),
        (Class::Boundary, tail(&["", &one, "^"]), Some("PO-25.b1")),
        (Class::Boundary, tail(&["", &overlong]), Some("PO-09.b2")),
    ];
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(at, (class, inputs, expected))| CaseSpec {
            id: format!("CE-{:02}", at + 1),
            class,
            inputs,
            expected,
            tags: Vec::new(),
        })
        .collect())
}

/// The cause a finding records.
fn finding_cause(id: &str) -> String {
    format!("the session halted at {id}, which SPEC.md \"Branch inventory\" lists as implemented")
}

/// Applies the outcome rules of an edge or replay case: a specified IN
/// case that halts at a not-implemented branch moves to BOUNDARY, a
/// specified BOUNDARY case that runs to its end moves to IN, and one that
/// halts at a different not-implemented branch takes it; a halt at an
/// implemented branch is a finding. A failure, or a CR case at a
/// not-implemented branch, stops the generation.
fn resolve_case(
    spec: CaseSpec,
    outcome: &Outcome,
    world_b: &Store,
    moves: &mut Vec<ClassMove>,
    findings: &mut Vec<Finding>,
) -> Result<CaseEntry, CorpusError> {
    let (orders, c_values) = orders_opened(world_b, &spec.inputs);
    let mut tags = spec.tags;
    let mut class = spec.class;
    let mut expected = spec.expected.map(str::to_string);
    let mut finding = None;
    let moved = |to: Class, branch: Option<&str>| ClassMove {
        case: spec.id.clone(),
        from: spec.class,
        to,
        branch: branch.unwrap_or("none").to_string(),
    };
    match (spec.class, outcome) {
        (_, Outcome::Failure(cause)) => {
            return Err(CorpusError::new(
                Some(&spec.id),
                "session failure",
                cause.clone(),
            ));
        }
        (_, Outcome::Implemented(id)) => {
            expected = Some(id.to_string());
            finding = Some(id.to_string());
            tags.push(format!("finding={id}"));
            findings.push(Finding {
                case: spec.id.clone(),
                branch: id.to_string(),
                cause: finding_cause(id),
            });
        }
        (Class::Boundary, Outcome::End) => {
            class = Class::In;
            expected = None;
            tags.push("moved=BOUNDARY>IN".to_string());
            moves.push(moved(Class::In, None));
        }
        (_, Outcome::End) => expected = None,
        (Class::In, Outcome::Unimplemented(id)) => {
            class = Class::Boundary;
            expected = Some(id.to_string());
            tags.push("moved=IN>BOUNDARY".to_string());
            moves.push(moved(Class::Boundary, Some(id)));
        }
        (Class::Boundary, Outcome::Unimplemented(id)) => {
            if expected.as_deref() != Some(*id) {
                expected = Some(id.to_string());
                moves.push(moved(Class::Boundary, Some(id)));
            }
        }
        (Class::Cr | Class::Cp, Outcome::Unimplemented(id)) => {
            return Err(CorpusError::new(
                Some(&spec.id),
                &format!("{} unimplemented", spec.class.label()),
                format!("the session halted at the not-implemented {id}, which no class describes"),
            ));
        }
    }
    Ok(CaseEntry {
        id: spec.id,
        class,
        specified: spec.class,
        inputs: spec.inputs,
        orders,
        c_values,
        item_count: None,
        tags,
        expected_branch: expected,
        finding,
    })
}

/// What the CP rounds settled: the final round's orders, world B and CP
/// outcomes, the drops of every round, and the number of rounds run.
struct Settled {
    built: Vec<Built>,
    world: Store,
    outcomes: Vec<Outcome>,
    drops: Vec<DroppedVariant>,
    rounds: usize,
}

/// A session outcome as an error detail writes it.
fn outcome_text(outcome: &Outcome) -> String {
    match outcome {
        Outcome::End => "a session that runs to its end".to_string(),
        Outcome::Unimplemented(id) => format!("a halt at the not-implemented {id}"),
        Outcome::Implemented(id) => format!("a halt at {id}"),
        Outcome::Failure(cause) => format!("a failure: {cause}"),
    }
}

/// The CP rounds of [`generate`], starting from `lists`. Each round
/// ([`run_round`]) builds the orders, with every order kept for a finding
/// in its place, and runs every CP case once on its world B. A failed
/// session stops the rounds. A kept order must halt at its branch again,
/// else the rounds stop with a `finding evidence` error. An order that
/// first halts at an implemented branch is kept, record and inputs, for
/// every later round. An order that halts at a not-implemented branch has
/// its variant attributed ([`attribute`]) and dropped from its list, and
/// another round starts, up to [`MAX_ROUNDS`].
fn settle_cp(
    world_a: &Store,
    refs: &References,
    mut lists: Lists,
    not_implemented: &[String],
) -> Result<Settled, CorpusError> {
    let mut drops: Vec<DroppedVariant> = Vec::new();
    let mut retained: Retained = BTreeMap::new();
    let mut round = 0;
    loop {
        round += 1;
        let (built, world) = run_round(world_a, refs, &lists, &retained)?;
        let mut outcomes = Vec::with_capacity(built.len());
        let mut unresolved = Vec::new();
        for (index, order) in built.iter().enumerate() {
            let run = run_case(&world, &cp_inputs(&order.record.c_value));
            let outcome = classify(&run, not_implemented);
            if let Outcome::Failure(cause) = &outcome {
                return Err(CorpusError::new(
                    Some(&order.record.case_id),
                    "session failure",
                    cause.clone(),
                ));
            }
            if let Some((_, id)) = retained.get(&index)
                && outcome != Outcome::Implemented(id)
            {
                return Err(CorpusError::new(
                    Some(&order.record.case_id),
                    "finding evidence",
                    format!(
                        "the record kept in an earlier round for its finding at {id} now gives \
                         {} in round {round}",
                        outcome_text(&outcome)
                    ),
                ));
            }
            match &outcome {
                Outcome::Unimplemented(id) => unresolved.push((index, *id)),
                Outcome::Implemented(id) => {
                    retained
                        .entry(index)
                        .or_insert_with(|| (order.clone(), *id));
                }
                Outcome::End | Outcome::Failure(_) => {}
            }
            outcomes.push(outcome);
        }
        if unresolved.is_empty() {
            return Ok(Settled {
                built,
                world,
                outcomes,
                drops,
                rounds: round,
            });
        }
        if round >= MAX_ROUNDS {
            let remaining: Vec<String> = unresolved
                .iter()
                .filter_map(|(index, id)| {
                    built.get(*index).map(|order| {
                        format!(
                            "{} at {id} with tags {}",
                            order.record.case_id,
                            order.tags.join(",")
                        )
                    })
                })
                .collect();
            return Err(CorpusError::new(
                None,
                "exhausted drop loop",
                format!(
                    "after {MAX_ROUNDS} rounds these CP cases still halt at a not-implemented \
                     branch: {}",
                    remaining.join("; ")
                ),
            ));
        }
        let round_lists = lists.clone();
        for (index, branch) in unresolved {
            let (dimension, value) =
                attribute(world_a, refs, &round_lists, &built, index, not_implemented)?;
            if !lists.list(&dimension).contains(&value) {
                continue;
            }
            lists.drop_value(&dimension, &value);
            let case = built
                .get(index)
                .map(|order| order.record.case_id.clone())
                .unwrap_or_default();
            drops.push(DroppedVariant {
                round,
                case,
                dimension,
                value: tag_value(&value).to_string(),
                branch: branch.to_string(),
            });
        }
    }
}

/// Generates the corpus from `world_a` and the `SPEC.md` text `spec`.
///
/// 1. The not-implemented branch IDs come from `spec` ([`ne_branches`]); the
///    file 442 attributes `442I` and the reference lists from `world_a`.
/// 2. Each round (`settle_cp`) restarts the stream at [`SEED`] and builds
///    the 200 orders under the current lists. Orders are built in case
///    order, and each draws its delivery location, proposal, APP, fund
///    control point and cost center, then per item its description,
///    quantity, unit cost, extended cost, STK#, NSN, FOOD GROUP, Items per
///    and BOC, each only when the rotation gives it a value to draw
///    (`draw_order`). Every order an earlier round kept for a finding then
///    takes the place of the one built at its position. No order may add a
///    node world A holds, and the corpus ZWR text must parse back to exactly
///    the generated nodes. Every CP case then runs once on world B. A halt
///    at an implemented branch keeps that order, its record and inputs, for
///    every later round, where it must halt at that branch again. A halt at
///    a not-implemented branch is attributed to the first single-dimension
///    reversion that lets the session end (`attribute`), and that value
///    leaves its list; while any CP case halts at a not-implemented branch
///    the next round starts, up to five rounds.
/// 3. On the final world B each CE and CR case runs once, and its outcome
///    keeps, moves or records it. The final round's outcomes, the kept
///    orders' included, give the CP findings.
/// 4. Every order must meet the fixture profile ([`profile_violations`])
///    and the paging bound ([`order_worst_y`]), the `C` values the non-prefix
///    rule ([`c_value_violations`]), and the corpus every minimum
///    ([`minimum_violations`]).
///
/// A failed session, in a round or in an attribution, a CR case at a
/// not-implemented branch, a kept order that no longer halts at its
/// finding's branch, an unmet rule, an attribution that finds no dimension
/// or whose candidate changes more than the reverted dimension, or a fifth
/// round that still drops a variant is a [`CorpusError`] naming the case
/// and the rule.
pub fn generate(world_a: &Store, spec: &str) -> Result<Corpus, CorpusError> {
    let not_implemented = ne_branches(spec);
    if not_implemented.is_empty() {
        return Err(CorpusError::new(
            None,
            "specification",
            "SPEC.md \"### Branch inventory\" lists no not-implemented branch",
        ));
    }
    // Without these file attributes every lookup ends at PO-09.b15 before
    // any order is read, which no reversion could attribute.
    if let Some(violation) = file_header_violation(world_a) {
        return Err(CorpusError::new(None, "profile violation", violation));
    }
    let refs = References::read(world_a)?;
    let Settled {
        built,
        world,
        outcomes,
        drops,
        rounds,
    } = settle_cp(world_a, &refs, Lists::initial(&refs), &not_implemented)?;

    let mut cases = Vec::with_capacity(corpus_case_ids().len());
    let mut moves = Vec::new();
    let mut findings = Vec::new();
    for (order, outcome) in built.iter().zip(&outcomes) {
        let record = &order.record;
        let mut tags = order.tags.clone();
        let mut finding = None;
        if let Outcome::Implemented(id) = outcome {
            tags.push(format!("finding={id}"));
            finding = Some(id.to_string());
            findings.push(Finding {
                case: record.case_id.clone(),
                branch: id.to_string(),
                cause: finding_cause(id),
            });
        }
        cases.push(CaseEntry {
            id: record.case_id.clone(),
            class: Class::Cp,
            specified: Class::Cp,
            inputs: cp_inputs(&record.c_value),
            orders: vec![record.ien.clone()],
            c_values: vec![record.c_value.clone()],
            item_count: Some(record.item_count),
            tags,
            expected_branch: finding.clone(),
            finding,
        });
    }
    let mut specs = edge_specs(&built, &world)?;
    specs.extend(CR_INPUTS.iter().map(|(id, gc, inputs)| CaseSpec {
        id: id.to_string(),
        class: Class::Cr,
        inputs: owned(inputs),
        expected: None,
        tags: vec![format!("gc={gc}")],
    }));
    for spec in specs {
        let outcome = classify(&run_case(&world, &spec.inputs), &not_implemented);
        cases.push(resolve_case(
            spec,
            &outcome,
            &world,
            &mut moves,
            &mut findings,
        )?);
    }
    let ids: Vec<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    if ids != corpus_case_ids() {
        return Err(CorpusError::new(
            None,
            "case ids",
            "the generated cases are not the corpus case ids in order",
        ));
    }

    let records: Vec<OrderRecord> = built.into_iter().map(|order| order.record).collect();
    let mut worst = 0;
    for record in &records {
        let violations = profile_violations(world_a, &world, &record.ien);
        if !violations.is_empty() {
            return Err(CorpusError::new(
                Some(&record.case_id),
                "profile violation",
                violations.join("; "),
            ));
        }
        let y = order_worst_y(&world, &record.ien);
        if y > PAGING_BOUND {
            return Err(CorpusError::new(
                Some(&record.case_id),
                "paging bound",
                format!("worst $Y {y} exceeds {PAGING_BOUND}"),
            ));
        }
        worst = worst.max(y);
    }
    let violations = c_value_violations(world_a, &world);
    if !violations.is_empty() {
        return Err(CorpusError::new(
            None,
            "non-prefix rule",
            violations.join("; "),
        ));
    }
    let corpus = Corpus {
        cases,
        largest_item_count: records
            .iter()
            .map(|record| record.item_count)
            .max()
            .unwrap_or(0),
        records,
        drops,
        moves,
        findings,
        rounds,
        worst_y: worst,
    };
    let violations = minimum_violations_with_refs(&refs, world_a, &world, &corpus);
    if !violations.is_empty() {
        return Err(CorpusError::new(
            None,
            "unmet minimum",
            violations.join("; "),
        ));
    }
    Ok(corpus)
}

// ---------------------------------------------------------------------------
// Shared checks, applied by the generator and by the offline gate
// ---------------------------------------------------------------------------

/// What the display reads of one item of a world B order.
struct ItemFacts {
    desc: String,
    qty: String,
    unit: String,
    cost: String,
    ext: String,
    stk: bool,
    nsn: bool,
    food: bool,
    /// The Items per line prints: the order type is not `S`, and piece 12
    /// is not zero [PRCHDP2.m:59].
    per: bool,
    boc: String,
    purchase_card: bool,
}

/// What the display reads of one world B order.
struct OrderFacts {
    /// The order's nodes that hold a value, by node subscript.
    nodes: BTreeMap<String, String>,
    order_type: String,
    authorities: usize,
    items: Vec<ItemFacts>,
}

impl OrderFacts {
    /// The value of order field `field`, empty when its node holds none.
    fn field(&self, field: Field) -> &str {
        self.nodes
            .get(field.node)
            .map_or("", |node| crate::mumps::piece(node, "^", field.piece))
    }
}

/// Reads order `ien` of `store`.
fn order_facts(store: &Store, ien: &str) -> OrderFacts {
    let order = key("PRC", &["442", ien]);
    let order_type = field_value(store, &order, ORDER_TYPE).to_string();
    let purchase_card = order_type == "S";
    let item_root = under(&order, &[ORDER_ITEMS]);
    let items = numeric_children(store, &item_root)
        .iter()
        .map(|j| {
            let item = under(&item_root, &[j]);
            let value = |field: Field| field_value(store, &item, field);
            let present = |text: &str| !text.is_empty();
            ItemFacts {
                desc: store.get_or_empty(&under(&item, &[ITEM_DESCRIPTION, "1", "0"])),
                qty: value(ITEM_QUANTITY).to_string(),
                unit: value(ITEM_UNIT).to_string(),
                cost: value(ITEM_UNIT_COST).to_string(),
                ext: value(ITEM_EXTENDED_COST).to_string(),
                stk: present(value(ITEM_STK)),
                nsn: present(value(ITEM_NSN)),
                food: present(value(ITEM_FOOD_GROUP)),
                per: !purchase_card && crate::mumps::num(value(ITEM_PER)) != "0",
                boc: value(ITEM_BOC).to_string(),
                purchase_card,
            }
        })
        .collect();
    let nodes = child_texts(store, &order)
        .into_iter()
        .filter_map(|node| {
            let value = store.get(&under(&order, &[&node]))?.to_string();
            Some((node, value))
        })
        .collect();
    OrderFacts {
        nodes,
        authorities: numeric_children(store, &under(&order, &[ORDER_AUTHORITIES])).len(),
        order_type,
        items,
    }
}

/// Whether `value` passes the input transform of field 442.01/9.5 NSN
/// [DD.zwr:516208], a length of 16 or 17 and `X?4N1"-"2UN1"-"3UN1"-"4N.UN`:
/// four digits, `-`, two upper-case letters or digits, `-`, three more,
/// `-`, four digits, and at most one more upper-case letter or digit.
fn is_nsn(value: &str) -> bool {
    let digits_only = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let codes_only = |part: &str| {
        part.bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    };
    let parts: Vec<&str> = value.split('-').collect();
    let [group, first, second, last] = parts.as_slice() else {
        return false;
    };
    let Some((serial, extra)) = last.split_at_checked(4) else {
        return false;
    };
    group.len() == 4
        && digits_only(group)
        && first.len() == 2
        && codes_only(first)
        && second.len() == 3
        && codes_only(second)
        && digits_only(serial)
        && extra.len() <= 1
        && codes_only(extra)
}

/// Whether `value` is a FOOD GROUP code of field 442.01/41, `1` to
/// [`FOOD_GROUPS`] [DD.zwr:516400].
fn is_food_group(value: &str) -> bool {
    (1..=FOOD_GROUPS).any(|code| code.to_string() == value)
}

/// The IENs `iens` as a comma-separated list, or `no IEN` when there are
/// none.
fn ien_list(iens: &[String]) -> String {
    if iens.is_empty() {
        "no IEN".to_string()
    } else {
        iens.join(", ")
    }
}

/// What the file 442 index entry `^PRC(442,index,value,ien)` of `store`
/// holds when it is present but not exactly `=""`, the node the `.01`
/// cross-references set [DD.zwr:514316, 514319], or `None` when it has that
/// shape or is absent. The entry must hold a value, the value must be
/// empty, and it must have no descendants, so its `$D` is 1. An absent
/// entry is left to the IEN rules of the callers, which report it. One `$D`
/// and one read are made.
fn index_leaf_violation(store: &Store, index: &str, value: &str, ien: &str) -> Option<String> {
    let entry = key("PRC", &["442", index, value, ien]);
    let data = store.data(&entry);
    let held = store.get(&entry);
    if data == 0 || (data == 1 && held == Some("")) {
        return None;
    }
    let found = match held {
        None => "no value".to_string(),
        Some("") => "the empty value".to_string(),
        Some(text) => format!("the value {text}"),
    };
    Some(format!(
        "entry {ien} has $D {data} and {found} instead of $D 1 and the empty value"
    ))
}

/// The violations of the file 442 index `index` of `world_b` for order
/// `ien`, whose field `.01` `number` gives its value `own` there: an empty
/// `own`, which files the order under no value; an `own` whose IENs are not
/// `ien` alone; and an entry `^PRC(442,index,own,ien)` that is present but
/// not exactly `=""`, the empty value with no descendants that the `.01`
/// cross-references set [DD.zwr:514316, 514319]. Only the IENs below `own`
/// and that one entry are read, so the cost does not grow with the number
/// of index values. A value other than `own` that holds `ien` is not looked
/// for here; [`c_value_violations`] reports one in index `C`. Each
/// violation names the index and the value, with the IENs found or with the
/// entry's IEN, `$D` and value.
fn index_entry_violations(
    world_b: &Store,
    index: &str,
    number: &str,
    own: &str,
    ien: &str,
) -> Vec<String> {
    let root = key("PRC", &["442", index]);
    let mut out = Vec::new();
    if own.is_empty() {
        out.push(format!(
            "field .01 {} gives no index {index} value",
            tag_value(number)
        ));
    } else {
        let iens = child_texts(world_b, &under(&root, &[own]));
        if iens != [ien] {
            out.push(format!(
                "the file 442 index {index} value {own} holds {} instead of {ien} alone",
                ien_list(&iens)
            ));
        }
        if let Some(found) = index_leaf_violation(world_b, index, own, ien) {
            out.push(format!("the file 442 index {index} value {own} {found}"));
        }
    }
    out
}

/// The fixture profile of `SPEC.md` "Parity corpus" for order `ien` of
/// `world_b`, every reference resolved in `world_a`: each violation names
/// the order and the rule, and an order on the profile gives none. It
/// checks the `^PRC(442,0)` attributes `442I`, nodes 0 and 1, a method of
/// processing below 10, no node 11, 5, 3, 15, 4, 13, 6 or 24, field 5.3
/// empty, node 23 pieces 7, 8 and 22 empty and piece 11 `P`, `S` or `D`, no
/// `M`, `MCH` or `G` index, the order filed under its `.01` in index `B`
/// and under the second `-` piece of its `.01` in index `C`, each by an
/// entry `^PRC(442,index,value,ien)` that is exactly `=""` and each of those
/// two values holding this order alone (another value that holds the order
/// is not looked for), a non-EDI vendor, a status that resolves in
/// 442.3, `.08` ESTIMATED ORDER? (node 7 piece 3) empty, at least one item,
/// each with one description line of at most 32 characters and no `|`, a
/// node 2 with pieces 2, 8, 9 and 11 empty and no node 3, an NSN (node 0
/// piece 13) empty or in the shape field 442.01/9.5 accepts, a FOOD GROUP
/// (node 4 piece 12) empty or a code of field 442.01/41, no item `AC`
/// index, no 442.8 `B` index, every vendor, state, service, ship-to, unit,
/// source code, authority, method and status pointer resolved, and an agent
/// that is empty or `.5` resolved in file 200.
pub fn profile_violations(world_a: &Store, world_b: &Store, ien: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fail = |rule: String| out.push(format!("order {ien}: {rule}"));
    if let Some(violation) = file_header_violation(world_a) {
        fail(violation.to_string());
    }
    let order = key("PRC", &["442", ien]);
    let value = |field: Field| field_value(world_b, &order, field);
    if world_b.get(&under(&order, &["0"])).is_none() {
        fail("node 0 is absent".to_string());
    }
    if world_b.get(&under(&order, &["1"])).is_none() {
        fail("node 1 is absent".to_string());
    }
    let mop = value(ORDER_MOP);
    if !resolves(world_a, "PRCD", "442.5", mop)
        || crate::mumps::num_cmp(mop, "10") != Ordering::Less
    {
        fail(format!(
            "method of processing {} is not a 442.5 entry below 10",
            tag_value(mop)
        ));
    }
    for sub in ["11", "5", "3", "15", "4", "13", "6", "24"] {
        if world_b.data(&under(&order, &[sub])) != 0 {
            fail(format!("node {sub} is present"));
        }
    }
    if !value(ORDER_DIRECT_PATIENT).is_empty() {
        fail(format!(
            "field 5.3, node {} piece {}, is not empty",
            ORDER_DIRECT_PATIENT.node, ORDER_DIRECT_PATIENT.piece
        ));
    }
    if world_b.data(&under(&order, &[ORDER_TYPE.node])) != 0 {
        for field in [ORDER_SUBSTATION, ORDER_CARD_NUMBER, ORDER_CARD_HOLDER] {
            if !value(field).is_empty() {
                fail(format!(
                    "node {} piece {} is not empty",
                    field.node, field.piece
                ));
            }
        }
        if !["P", "S", "D"].contains(&value(ORDER_TYPE)) {
            fail(format!(
                "node {} piece {} is not P, S or D",
                ORDER_TYPE.node, ORDER_TYPE.piece
            ));
        }
    }
    for index in ["M", "MCH", "G"] {
        if world_b.data(&key("PRC", &["442", index])) != 0 {
            fail(format!("the file 442 index {index} holds entries"));
        }
    }
    // The `.01` cross-references file the number itself in `B` and its
    // second `-` piece in `C`, each under the order's IEN [DD.zwr:514316,
    // 514319].
    let number = value(ORDER_NUMBER);
    for (index, own) in [("B", number), ("C", crate::mumps::piece(number, "-", 2))] {
        for violation in index_entry_violations(world_b, index, number, own, ien) {
            fail(violation);
        }
    }
    let vendor = value(ORDER_VENDOR);
    let vendor_record = key("PRC", &["440", vendor]);
    if world_a.get(&under(&vendor_record, &["0"])).is_none() {
        fail(format!(
            "vendor {} does not resolve in file 440",
            tag_value(vendor)
        ));
    } else {
        if field_value(world_a, &vendor_record, VENDOR_EDI) == "Y" {
            fail(format!("vendor {vendor} is an EDI vendor"));
        }
        let state = field_value(world_a, &vendor_record, VENDOR_STATE);
        if !state.is_empty() && !resolves(world_a, "DIC", "5", &crate::mumps::num(state)) {
            fail(format!(
                "vendor {vendor} state {state} does not resolve in file 5"
            ));
        }
    }
    let status = value(ORDER_STATUS);
    if !resolves(world_a, "PRCD", "442.3", status) {
        fail(format!(
            "status {} does not resolve in file 442.3",
            tag_value(status)
        ));
    }
    if !value(ORDER_ESTIMATED_ORDER).is_empty() {
        fail(format!(
            "field .08, node {} piece {}, is not empty",
            ORDER_ESTIMATED_ORDER.node, ORDER_ESTIMATED_ORDER.piece
        ));
    }
    let items = under(&order, &[ORDER_ITEMS]);
    let entries = numeric_children(world_b, &items);
    if entries.is_empty() {
        fail("the order has no item".to_string());
    }
    if world_b.data(&under(&items, &["AC"])) != 0 {
        fail("the item index AC holds entries".to_string());
    }
    for j in &entries {
        let item = under(&items, &[j]);
        if world_b.get(&under(&item, &["0"])).is_none() {
            fail(format!("item {j} has no zero node"));
            continue;
        }
        let lines = numeric_children(world_b, &under(&item, &[ITEM_DESCRIPTION]));
        let desc = world_b.get_or_empty(&under(&item, &[ITEM_DESCRIPTION, "1", "0"]));
        let header_present = world_b
            .get(&under(&item, &[ITEM_DESCRIPTION, "0"]))
            .is_some();
        if !header_present || lines != ["1"] || desc.chars().count() > 32 || desc.contains('|') {
            fail(format!(
                "item {j} description is not one line of at most 32 characters without |"
            ));
        }
        if world_b.get(&under(&item, &["2"])).is_none() {
            fail(format!("item {j} node 2 is absent"));
        } else {
            for field in [
                ITEM_CONTRACT,
                ITEM_RECEIVED,
                ITEM_EDI_STATUS_1,
                ITEM_EDI_STATUS_2,
            ] {
                if !field_value(world_b, &item, field).is_empty() {
                    fail(format!(
                        "item {j} node {} piece {} is not empty",
                        field.node, field.piece
                    ));
                }
            }
        }
        if world_b.data(&under(&item, &["3"])) != 0 {
            fail(format!("item {j} node 3 is present"));
        }
        let unit = field_value(world_b, &item, ITEM_UNIT);
        if !unit.is_empty() && !resolves(world_a, "PRCD", "420.5", unit) {
            fail(format!(
                "item {j} unit {unit} does not resolve in file 420.5"
            ));
        }
        let nsn = field_value(world_b, &item, ITEM_NSN);
        if !nsn.is_empty() && !is_nsn(nsn) {
            fail(format!(
                "item {j} NSN {nsn} is not 4N-2UN-3UN-4N with at most one more UN character"
            ));
        }
        let food = field_value(world_b, &item, ITEM_FOOD_GROUP);
        if !food.is_empty() && !is_food_group(food) {
            fail(format!(
                "item {j} FOOD GROUP {food} is not a code 1 to {FOOD_GROUPS}"
            ));
        }
    }
    if world_b.data(&key("PRC", &["442.8", "B"])) != 0 {
        fail("the file 442.8 index B holds entries".to_string());
    }
    let service = value(ORDER_SERVICE);
    if !service.is_empty() && !resolves(world_a, "DIC", "49", service) {
        fail(format!("service {service} does not resolve in file 49"));
    }
    let ship_to = value(ORDER_SHIP_TO);
    if !ship_to.is_empty()
        && world_a
            .get(&key("PRC", &["411", STATION, "1", ship_to, "0"]))
            .is_none()
    {
        fail(format!(
            "ship-to {ship_to} does not resolve under station {STATION}"
        ));
    }
    let source = value(ORDER_SOURCE);
    if !source.is_empty() && !resolves(world_a, "PRCD", "420.8", source) {
        fail(format!(
            "source code {source} does not resolve in file 420.8"
        ));
    }
    let authorities = under(&order, &[ORDER_AUTHORITIES]);
    for j in numeric_children(world_b, &authorities) {
        let authority = world_b.get_or_empty(&under(&authorities, &[&j, "0"]));
        if !resolves(world_a, "PRC", "442.4", &authority) {
            fail(format!("authority {j} does not resolve in file 442.4"));
        }
    }
    let agent = value(ORDER_AGENT);
    if !agent.is_empty() {
        if agent != ".5" {
            fail(format!("agent {agent} is not .5 or empty"));
        } else if !resolves(world_a, "VA", "200", agent) {
            fail(format!("agent {agent} does not resolve in file 200"));
        }
    }
    out
}

/// The largest `$Y` order `ien` of `world_b` reaches at any `ASK^PRCHDP2`
/// or after its last item. `HDR` leaves `$Y` at 4 [PRCHDP2.m:42]. For item
/// k, `Y_ask(k) = 4 + sum over j<k of (3 + d_j + ip_j) + 1 + d_k + 2*ip_k`,
/// where `d_j` counts the item's present STK#, NSN and FOOD GROUP lines
/// [PRCHDP2.m:56] and `ip_j` is 1 when its Items per line prints
/// [PRCHDP2.m:59]; `Y_end = 4 + sum over all items of (3 + d_j + ip_j) + e`,
/// with `e` 1 when the estimated shipping is above 0 [PRCHDP2.m:13]. The
/// result is `max(Y_ask(k) for all k, Y_end)`. With positive estimated
/// shipping, it is 44 for 13 items with no optional line and 40 for 5
/// items with all optional lines on a non-S order.
pub fn order_worst_y(world_b: &Store, ien: &str) -> usize {
    let facts = order_facts(world_b, ien);
    let shapes: Vec<(usize, usize)> = facts
        .items
        .iter()
        .map(|item| {
            let details = [item.stk, item.nsn, item.food]
                .iter()
                .filter(|present| **present)
                .count();
            (details, usize::from(item.per))
        })
        .collect();
    let shipping = crate::mumps::num_cmp(facts.field(ORDER_SHIPPING), "0") == Ordering::Greater;
    worst_y(&shapes, shipping)
}

/// Every child subscript of `parent` that is a string, as text.
fn string_children(store: &Store, parent: &[Sub]) -> Vec<String> {
    let mut out = Vec::new();
    let mut after = Sub::default();
    while let Some(next) = store.order(parent, Some(&after)) {
        if !next.is_num() {
            out.push(next.text().to_string());
        }
        after = next;
    }
    out
}

/// The non-prefix rule over the `C` values of `world_b`, the GC values
/// included: each indexes exactly one order, an IEN with a zero node in
/// file 442 whose `.01` files it under that value, the second `-` piece of
/// the `.01` being the value [DD.zwr:514319], through an entry
/// `^PRC(442,"C",value,ien)` that is exactly `=""`, the empty value with no
/// descendants, so no two orders share a value and no order is filed under
/// a value other than its own; and none is a prefix of another. The
/// one-order rule reads each value once, with the `.01` of its order and
/// its one entry, so its cost grows linearly with the values.
/// Each corpus value, one world A's `C` index does not hold, must also be
/// neither a prefix nor an extension of, nor equal to, any subscript of a
/// world A file 442 index whose name collates at or after `C`. No `C` value
/// may begin with one of [`MISS_STRINGS`], as written or in upper case.
pub fn c_value_violations(world_a: &Store, world_b: &Store) -> Vec<String> {
    let mut out = Vec::new();
    let c_index = key("PRC", &["442", "C"]);
    let c_values = child_texts(world_b, &c_index);
    for value in &c_values {
        let iens = child_texts(world_b, &under(&c_index, &[value]));
        let orders = iens
            .iter()
            .filter(|ien| world_b.get(&key("PRC", &["442", ien, "0"])).is_some())
            .count();
        if iens.len() != 1 || orders != 1 {
            out.push(format!(
                "C value {value} holds {}, not exactly one file 442 order",
                ien_list(&iens)
            ));
        } else if let Some(ien) = iens.first() {
            let number = field_value(world_b, &key("PRC", &["442", ien]), ORDER_NUMBER);
            let own = crate::mumps::piece(number, "-", 2);
            if own != value.as_str() {
                out.push(format!(
                    "C value {value} holds {ien}, whose field .01 {} gives C value {}",
                    tag_value(number),
                    tag_value(own)
                ));
            }
            if let Some(found) = index_leaf_violation(world_b, "C", value, ien) {
                out.push(format!("C value {value} {found}"));
            }
        }
    }
    for (at, first) in c_values.iter().enumerate() {
        for second in c_values.iter().skip(at + 1) {
            if first.starts_with(second.as_str()) || second.starts_with(first.as_str()) {
                out.push(format!(
                    "C value {first} and {second}: one is a prefix of the other"
                ));
            }
        }
    }
    let world_a_c: BTreeSet<String> = child_texts(world_a, &key("PRC", &["442", "C"]))
        .into_iter()
        .collect();
    let file = key("PRC", &["442"]);
    for index in string_children(world_a, &file)
        .into_iter()
        .filter(|name| name.as_str() >= "C")
    {
        for subscript in child_texts(world_a, &under(&file, &[&index])) {
            for value in c_values.iter().filter(|value| !world_a_c.contains(*value)) {
                if value.starts_with(subscript.as_str()) || subscript.starts_with(value.as_str()) {
                    out.push(format!(
                        "C value {value} and world A index {index} value {subscript}: one is a \
                         prefix of the other"
                    ));
                }
            }
        }
    }
    for value in &c_values {
        let upper = value.to_ascii_uppercase();
        for miss in MISS_STRINGS {
            if value.starts_with(miss) || upper.starts_with(&miss.to_ascii_uppercase()) {
                out.push(format!(
                    "C value {value} begins with the miss string {miss}"
                ));
            }
        }
    }
    out
}

/// The stored form of a unit cost, as the `item.cost` row names it.
fn cost_form(cost: &str) -> String {
    if cost == "N/C" {
        return cost.to_string();
    }
    match cost.split_once('.') {
        None => "whole".to_string(),
        Some((_, fraction)) => format!("{}dp", fraction.chars().count()),
    }
}

fn presence(present: bool) -> String {
    if present { "present" } else { "absent" }.to_string()
}

/// The order row labels of one world B order, as the rotation lists name
/// them.
fn order_labels(facts: &OrderFacts) -> Vec<(&'static str, String)> {
    let value = |field: Field| facts.field(field).to_string();
    let verbal = match (facts.field(ORDER_VERBAL), facts.field(ORDER_CONFIRMATION)) {
        ("Y", "") => "verbal",
        ("Y", "Y") => "confirmation",
        ("", "") => "",
        _ => "other",
    };
    vec![
        ("status", value(ORDER_STATUS)),
        ("type", facts.order_type.clone()),
        ("source", value(ORDER_SOURCE)),
        ("mop", value(ORDER_MOP)),
        ("vendor", value(ORDER_VENDOR)),
        ("service", value(ORDER_SERVICE)),
        ("shipto", value(ORDER_SHIP_TO)),
        (
            "delivery",
            presence(!facts.field(ORDER_DELIVERY_LOCATION).is_empty()),
        ),
        ("verbal", verbal.to_string()),
        ("fob", value(ORDER_FOB)),
        (
            "proposal",
            facts.field(ORDER_PROPOSAL).chars().count().to_string(),
        ),
        ("authority", facts.authorities.to_string()),
        ("date", value(ORDER_DELIVERY_DATE)),
        ("app", facts.field(ORDER_APP).chars().count().to_string()),
        ("agent", value(ORDER_AGENT)),
        ("total", value(ORDER_TOTAL)),
        ("shipping", value(ORDER_SHIPPING)),
    ]
}

/// The item row labels of one world B item, as the rotation lists name
/// them; `item.per` is `present` only where the Items per line prints.
fn item_labels(item: &ItemFacts) -> Vec<(&'static str, String)> {
    let qty = match item.qty.split_once('.') {
        None => "whole",
        Some((_, fraction)) if fraction.chars().count() == 1 => "decimal",
        Some(_) => "other",
    };
    vec![
        ("item.desc", item.desc.chars().count().to_string()),
        ("item.qty", qty.to_string()),
        ("item.unit", item.unit.clone()),
        ("item.cost", cost_form(&item.cost)),
        (
            "item.ext",
            if item.ext.chars().count() > 7 {
                "long"
            } else {
                "short"
            }
            .to_string(),
        ),
        ("item.stk", presence(item.stk)),
        ("item.nsn", presence(item.nsn)),
        ("item.food", presence(item.food)),
        ("item.per", presence(item.per)),
        (
            "item.boc",
            if item.boc.contains(' ') {
                "several"
            } else {
                "one"
            }
            .to_string(),
        ),
    ]
}

/// A date shape the minimums require, with the test that recognises it.
type DateShape = (&'static str, fn(&str) -> bool);

/// The tallies of every dimension over the corpus orders, with the dropped
/// variants that exempt a value from its minimum, and the violations found.
struct Tally<'a> {
    counts: BTreeMap<&'static str, BTreeMap<String, usize>>,
    drops: &'a [DroppedVariant],
    out: Vec<String>,
}

impl Tally<'_> {
    fn count(&self, dimension: &str, value: &str) -> usize {
        self.counts
            .get(dimension)
            .and_then(|values| values.get(value))
            .copied()
            .unwrap_or(0)
    }

    fn dropped(&self, dimension: &str, value: &str) -> bool {
        self.drops
            .iter()
            .any(|drop| drop.dimension == dimension && drop.value == tag_value(value))
    }

    fn drops_in(&self, dimension: &str) -> usize {
        self.drops
            .iter()
            .filter(|drop| drop.dimension == dimension)
            .count()
    }

    /// Every value of `values` not dropped is counted at least `minimum`
    /// times, in `unit`.
    fn each(&mut self, dimension: &str, values: &[String], minimum: usize, unit: &str) {
        for value in values {
            let found = self.count(dimension, value);
            if !self.dropped(dimension, value) && found < minimum {
                self.out.push(format!(
                    "{dimension}={}: {found} {unit}, minimum {minimum}",
                    tag_value(value)
                ));
            }
        }
    }

    /// At least `minimum` distinct values that `keep` accepts, less one per
    /// distinct dropped value of the dimension that `keep` also accepts, so
    /// a dropped value the count never includes relaxes nothing.
    fn distinct(&mut self, dimension: &str, minimum: usize, keep: impl Fn(&str) -> bool) {
        let found = self.counts.get(dimension).map_or(0, |values| {
            values.keys().filter(|value| keep(value)).count()
        });
        let exempt: BTreeSet<&str> = self
            .drops
            .iter()
            .filter(|drop| drop.dimension == dimension)
            .map(|drop| tag_label(&drop.value))
            .filter(|value| keep(value))
            .collect();
        let needed = minimum.saturating_sub(exempt.len());
        if found < needed {
            self.out.push(format!(
                "{dimension}: {found} distinct values, minimum {needed}"
            ));
        }
    }
}

/// Every variation minimum of `SPEC.md` "Parity corpus" the CP orders of
/// `world_b` miss, counted from the world B nodes of `corpus.records`, with
/// the reference lists read from `world_a`. A value `corpus.drops` lists is
/// exempt from its minimum, and a minimum over distinct values falls by one
/// per dropped value of its dimension that the distinct count includes: the
/// empty service and unit, which their distinct counts exclude, relax only
/// their own minimums. The item counts must be exact. The date minimums
/// count the delivery date, node 0 piece 10. Both date fields take the same
/// date, so every order's P.O. date, node 1 piece 15, must equal its
/// delivery date, and the date minimums then hold for both fields: a P.O.
/// date that differs, an empty one included, is a violation naming the
/// order, both fields and both values.
pub fn minimum_violations(world_a: &Store, world_b: &Store, corpus: &Corpus) -> Vec<String> {
    match References::read(world_a) {
        Ok(refs) => minimum_violations_with_refs(&refs, world_a, world_b, corpus),
        Err(error) => vec![error.to_string()],
    }
}

/// The checks of [`minimum_violations`], with the reference lists `refs`
/// already read from `world_a`, so that [`generate`] reads them once.
fn minimum_violations_with_refs(
    refs: &References,
    world_a: &Store,
    world_b: &Store,
    corpus: &Corpus,
) -> Vec<String> {
    let facts: Vec<OrderFacts> = corpus
        .records
        .iter()
        .map(|record| order_facts(world_b, &record.ien))
        .collect();
    let mut counts: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
    let mut add = |dimension: &'static str, value: String| {
        *counts
            .entry(dimension)
            .or_default()
            .entry(value)
            .or_default() += 1;
    };
    let mut cost_off_card: BTreeSet<String> = BTreeSet::new();
    for order in &facts {
        for (dimension, value) in order_labels(order) {
            add(dimension, value);
        }
        add("items", order.items.len().to_string());
        for item in &order.items {
            for (dimension, value) in item_labels(item) {
                add(dimension, value);
            }
            if !item.purchase_card {
                cost_off_card.insert(cost_form(&item.cost));
            }
        }
    }
    let mut tally = Tally {
        counts,
        drops: &corpus.drops,
        out: Vec::new(),
    };

    tally.each("status", &refs.statuses, 1, "orders");
    tally.each("status", &refs.low_statuses, 2, "orders");
    let long_kept = refs
        .long_statuses
        .iter()
        .filter(|ien| !tally.dropped("status", ien))
        .count();
    let long_used = refs
        .long_statuses
        .iter()
        .filter(|ien| tally.count("status", ien) > 0)
        .count();
    if long_used < long_kept.min(3) {
        tally.out.push(format!(
            "status: {long_used} statuses named over 34 characters, minimum 3"
        ));
    }
    tally.each("type", &owned(&["", "P", "S", "D"]), 10, "orders");
    tally.each("source", &with_empty(&refs.sources), 1, "orders");
    tally.each("mop", &refs.mops, 5, "orders");
    tally.distinct("vendor", VENDOR_COUNT, |_| true);
    // A dropped vendor can take a property's only holder with it, so the 16
    // properties are required only while no vendor has been dropped.
    if tally.drops_in("vendor") == 0 {
        let held: Vec<Vec<bool>> = facts
            .iter()
            .map(|order| vendor_properties(world_a, order.field(ORDER_VENDOR)))
            .collect();
        for (index, property) in vendor_property_names().into_iter().enumerate() {
            let found = held
                .iter()
                .any(|properties| properties.get(index).copied().unwrap_or(false));
            if !found {
                tally.out.push(format!(
                    "vendor: no order's vendor has the property {property}"
                ));
            }
        }
    }
    tally.distinct("service", 5, |value| !value.is_empty());
    tally.each("service", &owned(&[""]), 3, "orders");
    tally.each("shipto", &with_empty(&refs.ship_tos), 8, "orders");
    tally.each("delivery", &owned(&["present", "absent"]), 8, "orders");
    tally.each(
        "verbal",
        &owned(&["verbal", "confirmation", ""]),
        6,
        "orders",
    );
    tally.each("fob", &owned(&["O", "D", ""]), 1, "orders");
    tally.each("proposal", &owned(&PROPOSAL_LENGTHS), 1, "orders");
    tally.each("authority", &owned(&["0", "1", "3"]), 5, "orders");
    tally.distinct("date", DATES.len(), |_| true);
    let shapes: [DateShape; 5] = [
        ("a year end", |date| date.ends_with("1231")),
        ("a leap day", |date| date.ends_with("0229")),
        ("a 1999 date", |date| date.starts_with("299")),
        ("a 2026 date", |date| date.starts_with("326")),
        ("an empty date", str::is_empty),
    ];
    for (shape, matches) in shapes {
        let listed = DATES
            .iter()
            .any(|date| matches(date) && !tally.dropped("date", date));
        let used = DATES
            .iter()
            .any(|date| matches(date) && tally.count("date", date) > 0);
        if listed && !used {
            tally.out.push(format!("date: no order has {shape}"));
        }
    }
    // Both date fields take the plan's one date, so the tally of the
    // delivery date holds for the P.O. date once every order's two agree.
    for (record, order) in corpus.records.iter().zip(&facts) {
        let po_date = order.field(ORDER_DATE);
        let delivery_date = order.field(ORDER_DELIVERY_DATE);
        if po_date != delivery_date {
            tally.out.push(format!(
                "date: order {} P.O. date {};{} {} differs from its delivery date {};{} {}",
                record.ien,
                ORDER_DATE.node,
                ORDER_DATE.piece,
                tag_value(po_date),
                ORDER_DELIVERY_DATE.node,
                ORDER_DELIVERY_DATE.piece,
                tag_value(delivery_date)
            ));
        }
    }
    tally.distinct("app", 4, |_| true);
    tally.each("agent", &owned(&[".5", ""]), 5, "orders");
    tally.distinct("total", TOTALS.len(), |_| true);
    let positive: Vec<&str> = SHIPPING
        .iter()
        .copied()
        .filter(|value| crate::mumps::num_cmp(value, "0") == Ordering::Greater)
        .collect();
    let shipped: usize = positive
        .iter()
        .map(|value| tally.count("shipping", value))
        .sum();
    let shipping_listed = positive
        .iter()
        .any(|value| !tally.dropped("shipping", value));
    if shipping_listed && shipped < 10 {
        tally.out.push(format!(
            "shipping: {shipped} orders with estimated shipping above 0, minimum 10"
        ));
    }
    let mut expected_counts: BTreeMap<String, usize> = BTreeMap::new();
    expected_counts.insert("1".to_string(), CP_COUNT - MULTI_ORDERS);
    for (count, orders) in MULTI_QUOTAS {
        expected_counts.insert(count.to_string(), orders);
    }
    let actual_counts = tally.counts.get("items").cloned().unwrap_or_default();
    if actual_counts != expected_counts {
        tally.out.push(format!(
            "items: the item counts {actual_counts:?} are not exactly {expected_counts:?}"
        ));
    }
    let lengths: Vec<String> = (1..=32).map(|length: usize| length.to_string()).collect();
    tally.each("item.desc", &lengths, 1, "items");
    tally.each("item.qty", &owned(&["whole", "decimal"]), 1, "items");
    tally.distinct("item.unit", 10, |value| !value.is_empty());
    tally.each("item.unit", &owned(&[""]), 1, "items");
    for form in COST_FORMS {
        if !tally.dropped("item.cost", form) && !cost_off_card.contains(form) {
            tally.out.push(format!(
                "item.cost={form}: on no item of an order whose type is not S"
            ));
        }
    }
    tally.each("item.ext", &owned(&["short", "long"]), 1, "items");
    for field in ["item.stk", "item.nsn", "item.food", "item.per"] {
        tally.each(field, &owned(&["present", "absent"]), 1, "items");
    }
    tally.each("item.boc", &owned(&["one", "several"]), 1, "items");
    tally.out
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const MANIFEST_HEADER: &str =
    "case\tclass\tseed\torder_ien\tc_value\titem_count\ttags\texpected_branch";

impl Corpus {
    /// The corpus ZWR text: the header lines
    /// `OSEHRA ZGO Export: IFCAP PO INQUIRY PARITY CORPUS` and `ZWR`, then
    /// every node of every order in collation order as
    /// `<reference>=<value>`, the value bare when it is a canonical number
    /// and otherwise quoted with each `"` doubled, each line ending `\n`.
    pub fn render_zwr(&self) -> String {
        render_nodes(self.records.iter().flat_map(|record| record.nodes.iter()))
    }

    /// The manifest: the header row, then one tab-separated row per case in
    /// case order, each line ending `\n`. A CP row gives its order's IEN,
    /// `C` value and item count. A CE or CR row lists the order references
    /// in its inputs, including unused boundary tails, comma-separated, and
    /// has no item count. Tags are comma-separated, and the expected branch
    /// is `none` or a branch ID.
    pub fn render_manifest(&self) -> String {
        let seed = SEED.to_string();
        let mut text = String::from(MANIFEST_HEADER);
        text.push('\n');
        for case in &self.cases {
            let item_count = case
                .item_count
                .map(|count| count.to_string())
                .unwrap_or_default();
            let row = [
                case.id.as_str(),
                case.class.label(),
                &seed,
                &case.orders.join(","),
                &case.c_values.join(","),
                &item_count,
                &case.tags.join(","),
                case.expected_branch.as_deref().unwrap_or("none"),
            ];
            text.push_str(&row.join("\t"));
            text.push('\n');
        }
        text
    }

    /// The text of `case`'s input file: each input followed by `\n`.
    pub fn input_text(case: &CaseEntry) -> String {
        case.inputs
            .iter()
            .map(|input| format!("{input}\n"))
            .collect()
    }

    /// The 233 generated files and their contents: the corpus ZWR file, the
    /// manifest in the corpus folder, then each case's input file in case
    /// order.
    pub fn files(&self) -> Vec<(PathBuf, String)> {
        let folder = corpus_dir();
        let mut files = Vec::with_capacity(self.cases.len() + 2);
        files.push((corpus_zwr_path(), self.render_zwr()));
        files.push((folder.join("manifest.tsv"), self.render_manifest()));
        for case in &self.cases {
            files.push((
                corpus_input_path(&folder, &case.id),
                Corpus::input_text(case),
            ));
        }
        files
    }

    /// The seven summary lines `corpus_gen` prints: the specified and the
    /// generated class counts, the class moves, the dropped variants, the
    /// findings, the rounds, and the largest item count with the worst `$Y`.
    pub fn summary(&self) -> Vec<String> {
        let count = |class: Class, final_class: bool| {
            self.cases
                .iter()
                .filter(|case| {
                    if final_class {
                        case.class == class
                    } else {
                        case.specified == class
                    }
                })
                .count()
        };
        let multi = self
            .records
            .iter()
            .filter(|record| record.item_count > 1)
            .count();
        let line = |label: &str, final_class: bool| {
            let cp = count(Class::Cp, final_class);
            let inside = count(Class::In, final_class);
            let boundary = count(Class::Boundary, final_class);
            let cr = count(Class::Cr, final_class);
            format!(
                "{label}: {cp} CP, {} CE ({inside} IN, {boundary} BOUNDARY), {cr} CR, {multi} \
                 multi-item orders",
                inside + boundary
            )
        };
        let moved = if self.moves.is_empty() {
            "none".to_string()
        } else {
            self.moves
                .iter()
                .map(|change| {
                    format!(
                        "{} {}->{} {}",
                        change.case,
                        change.from.label(),
                        change.to.label(),
                        change.branch
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        vec![
            line("specified", false),
            line("generated", true),
            format!("moved: {moved}"),
            format!("dropped variants: {}", self.drops.len()),
            format!("findings: {}", self.findings.len()),
            format!("regeneration rounds: {}", self.rounds),
            format!(
                "largest item count: {}; worst $Y: {} (bound {PAGING_BOUND})",
                self.largest_item_count, self.worst_y
            ),
        ]
    }
}
