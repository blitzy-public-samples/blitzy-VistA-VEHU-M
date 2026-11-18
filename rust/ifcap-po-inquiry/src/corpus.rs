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
use crate::mumps::{SESSION_OUTPUT_LIMIT, is_canonical, num, num_cmp, piece};
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

/// The end of a session that `EN3^PRCHRPT` completes: the order prompt, the
/// `^` that selects nothing, and the `\n` of the process exit (`SPEC.md`
/// K14).
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

/// The number of CP cases.
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

/// The stored unit cost forms of the `item.cost` row, each printed through
/// its own arm of [PRCHDP2.m:52].
const COST_FORMS: [&str; 6] = ["N/C", "whole", "2dp", "3dp", "4dp", "5dp"];

/// The description word-processing header of every corpus item, as the GC
/// items carry it.
const DESCRIPTION_HEADER: &str = "^^1^1^3261001^";

/// The characters of stream text.
const ALPHABET: &[u8; 36] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

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
/// the first data line does not parse. A corpus key equal to a world A key
/// would replace it; [`generate`] refuses such a key.
pub fn world_b(world_a: &Store, corpus_zwr: &str) -> Result<Store, StoreError> {
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
    for line in corpus_zwr.lines().skip_while(|line| !line.starts_with('^')) {
        text.push_str(line);
        text.push('\n');
    }
    Store::from_zwr(&text)
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
    /// The order IENs the case opens, without repeats, in first-use order.
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
    /// attribution`, `session failure` or `CR unimplemented`.
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

/// The node 0 pieces of a vendor that the order display prints when present
/// [PRCHDP1.m:26-28]: address lines 3 to 5, city, state, zip and phone.
const VENDOR_PIECES: [usize; 7] = [3, 4, 5, 6, 7, 8, 10];

/// The names of the 16 vendor properties of the variation matrix, in order:
/// each of [`VENDOR_PIECES`] present then absent, then ACCT # (node 2 piece 1
/// [PRCHDP1.m:29]) present then absent.
fn vendor_property_names() -> Vec<String> {
    let mut names: Vec<String> = VENDOR_PIECES
        .iter()
        .flat_map(|field| {
            [
                format!("piece {field} present"),
                format!("piece {field} absent"),
            ]
        })
        .collect();
    names.push("ACCT # present".to_string());
    names.push("ACCT # absent".to_string());
    names
}

/// Whether vendor `ien` has each of the 16 properties of
/// [`vendor_property_names`], in the same order.
fn vendor_properties(store: &Store, ien: &str) -> Vec<bool> {
    let node0 = store.get_or_empty(&key("PRC", &["440", ien, "0"]));
    let node2 = store.get_or_empty(&key("PRC", &["440", ien, "2"]));
    let mut out = Vec::with_capacity(16);
    for field in VENDOR_PIECES {
        let present = !piece(&node0, "^", field).is_empty();
        out.push(present);
        out.push(!present);
    }
    let present = !piece(&node2, "^", 1).is_empty();
    out.push(present);
    out.push(!present);
    out
}

/// Whether vendor `ien` may be used: it has a zero node, it is not EDI
/// (node 3 piece 2 is not `Y`, which leads to the not-implemented PO-10.b6
/// [PRCHDP1.m:32]), its state pointer is empty or resolves in file 5, and
/// neither node 2 nor node 3 holds only descendants, which the display reads
/// after a bare `$D` [PRCHDP1.m:29-30].
fn vendor_eligible(store: &Store, ien: &str) -> bool {
    let Some(node0) = store.get(&key("PRC", &["440", ien, "0"])) else {
        return false;
    };
    let node3 = store.get_or_empty(&key("PRC", &["440", ien, "3"]));
    let state = piece(node0, "^", 7);
    piece(&node3, "^", 2) != "Y"
        && (state.is_empty() || resolves(store, "DIC", "5", &num(state)))
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

/// The number of vendors the corpus uses.
const VENDOR_COUNT: usize = 40;

impl References {
    fn read(world_a: &Store) -> Result<References, CorpusError> {
        let statuses = file_entries(world_a, "PRCD", "442.3");
        let status_name = |ien: &String| world_a.get_or_empty(&key("PRCD", &["442.3", ien, "0"]));
        let low_statuses: Vec<String> = statuses
            .iter()
            .filter(|ien| {
                let level = piece(&status_name(ien), "^", 2).to_string();
                level.is_empty() || num_cmp(&level, "10") == Ordering::Less
            })
            .cloned()
            .collect();
        let long_statuses: Vec<String> = statuses
            .iter()
            .filter(|ien| piece(&status_name(ien), "^", 1).chars().count() > 34)
            .cloned()
            .collect();
        let mops: Vec<String> = file_entries(world_a, "PRCD", "442.5")
            .into_iter()
            .filter(|ien| num_cmp(ien, "10") == Ordering::Less)
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

/// `values` as owned strings.
fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// `values` followed by the empty label.
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

/// One item of an order as drawn, before its nodes are written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemDraft {
    desc: String,
    qty: String,
    unit: String,
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
    /// which it does only off a purchase-card order [PRCHDP2.m:59].
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
    /// The stream state the order's draws started from, so that a reverted
    /// order can be rebuilt with every other draw unchanged.
    start: Xorshift64,
    record: OrderRecord,
    tags: Vec<String>,
}

/// Builds CP-n of `plan` with the stream `rng`, drawn in this order: the
/// delivery location (a draw for its length, `1 + r mod 20`, then its
/// text, when present), the proposal text, the APP digits, the fund control
/// point's three digits and word, the cost center's four digits; then per
/// item the description text, the quantity, the unit cost (none for
/// `N/C`), the extended cost, the STK#, NSN and FOOD GROUP words and the
/// Items per count when present, and the BOC with its words. Orders of
/// five or more items are then thinned until [`worst_y`] is within
/// [`PAGING_BOUND`]: from the last item back, Items per, FOOD GROUP, NSN and
/// STK#, one printed line at a time.
fn build_record(
    plan: &Plan,
    refs: &References,
    rng: &mut Xorshift64,
) -> Result<Built, CorpusError> {
    let start = rng.clone();
    let n = plan.n;
    let case_id = format!("CP-{n:03}");
    let ien = (IEN_BASE + n).to_string();
    let c_value = format!("CP{n:03}");
    let order_type = plan.value("type").to_string();
    let purchase_card = order_type == "S";

    let delivery = if plan.value("delivery") == "present" {
        let length = 1 + usize::try_from(rng.next_u64() % 20).unwrap_or(0);
        stream_text(rng, length)
    } else {
        String::new()
    };
    let proposal_length = label_number(&case_id, "proposal", plan.value("proposal"))?;
    let proposal = stream_text(rng, proposal_length);
    let app_length = label_number(&case_id, "app", plan.value("app"))?;
    let app = digits(rng, app_length);
    let fund_control_point = format!("{} {}", digits(rng, 3), word(rng));
    let cost_center = digits(rng, 4);
    let (verbal, confirmation) = match plan.value("verbal") {
        "verbal" => ("Y", ""),
        "confirmation" => ("Y", "Y"),
        _ => ("", ""),
    };
    let authorities = label_number(&case_id, "authority", plan.value("authority"))?;

    let mut drafts = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let field = |name: &str| Plan::item_value(item, &format!("item.{name}")).to_string();
        let desc_length = label_number(&case_id, "item.desc", &field("desc"))?;
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
        let mut optional = |name: &str| (field(name) == "present").then(|| word(rng));
        let stk = optional("stk");
        let nsn = optional("nsn");
        let food = optional("food");
        let per = (field("per") == "present").then(|| (1 + pick(rng, 99)).to_string());
        let boc = if field("boc") == "several" {
            format!("{} {} {}", digits(rng, 4), word(rng), word(rng))
        } else {
            digits(rng, 4)
        };
        drafts.push(ItemDraft {
            desc,
            qty,
            unit: field("unit"),
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

    let shipping = num_cmp(plan.value("shipping"), "0") == Ordering::Greater;
    if drafts.len() >= 5 {
        let shapes = |drafts: &[ItemDraft]| -> Vec<(usize, usize)> {
            drafts
                .iter()
                .map(|draft| draft.shape(purchase_card))
                .collect()
        };
        while worst_y(&shapes(&drafts), shipping) > PAGING_BOUND {
            if !drafts
                .iter_mut()
                .rev()
                .any(|draft| draft.thin_one(purchase_card))
            {
                break;
            }
        }
    }

    let date = plan.value("date");
    let node0 = [
        format!("{STATION}-{c_value}").as_str(),
        plan.value("mop"),
        &fund_control_point,
        &app,
        &cost_center,
        "",
        "",
        "",
        "",
        date,
        "",
        "",
        plan.value("shipping"),
        "",
        plan.value("total"),
    ]
    .join("^");
    let node1 = [
        plan.value("vendor"),
        plan.value("service"),
        plan.value("shipto"),
        verbal,
        confirmation,
        plan.value("fob"),
        plan.value("source"),
        &proposal,
        "",
        plan.value("agent"),
        &delivery,
        "",
        "",
        "",
        date,
    ]
    .join("^");

    let order = key("PRC", &["442", &ien]);
    let mut nodes: BTreeMap<Vec<Sub>, String> = BTreeMap::new();
    nodes.insert(under(&order, &["0"]), node0);
    nodes.insert(under(&order, &["1"]), node1);
    let count = drafts.len().to_string();
    nodes.insert(
        under(&order, &["2", "0"]),
        format!("^442.01^{count}^{count}"),
    );
    for (index, draft) in drafts.iter().enumerate() {
        let j = (index + 1).to_string();
        let item0 = [
            j.as_str(),
            &draft.qty,
            &draft.unit,
            &draft.boc,
            "",
            draft.stk.as_deref().unwrap_or(""),
            "",
            "",
            &draft.cost,
            "",
            "",
            draft.per.as_deref().unwrap_or(""),
            draft.nsn.as_deref().unwrap_or(""),
        ]
        .join("^");
        nodes.insert(under(&order, &["2", &j, "0"]), item0);
        nodes.insert(
            under(&order, &["2", &j, "1", "0"]),
            DESCRIPTION_HEADER.to_string(),
        );
        nodes.insert(under(&order, &["2", &j, "1", "1", "0"]), draft.desc.clone());
        nodes.insert(under(&order, &["2", &j, "2"]), draft.ext.clone());
        if let Some(food) = &draft.food {
            nodes.insert(
                under(&order, &["2", &j, "4"]),
                format!("{}{food}", "^".repeat(11)),
            );
        }
        nodes.insert(under(&order, &["2", "B", &j, &j]), String::new());
    }
    nodes.insert(under(&order, &["7"]), format!("{}^^", plan.value("status")));
    if authorities > 0 {
        let size = refs.authorities.len();
        nodes.insert(
            under(&order, &["14", "0"]),
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
                under(&order, &["14", &j.to_string(), "0"]),
                authority.clone(),
            );
        }
    }
    if !order_type.is_empty() {
        nodes.insert(
            under(&order, &["23"]),
            format!("{}{order_type}", "^".repeat(10)),
        );
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
    for (index, (item, draft)) in plan.items.iter().zip(&drafts).enumerate() {
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

    Ok(Built {
        plan: plan.clone(),
        start,
        record: OrderRecord {
            case_id,
            ien,
            c_value,
            item_count: drafts.len(),
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

/// One generation round under `lists`: the 200 orders built from one
/// stream started at [`SEED`], checked to add no node world A holds and to
/// survive their own ZWR text unchanged, and world B built from them.
fn run_round(
    world_a: &Store,
    refs: &References,
    lists: &Lists,
) -> Result<(Vec<Built>, Store), CorpusError> {
    let mut rng = Xorshift64::new(SEED);
    let built = plans(lists)?
        .iter()
        .map(|plan| build_record(plan, refs, &mut rng))
        .collect::<Result<Vec<_>, CorpusError>>()?;
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

/// Every single-dimension reversion of `plan` under `lists`, each as the
/// dimension, the value it replaces and the reverted plan: the order rows
/// in matrix order, then the item rows, each over the items in order. A
/// value reverts to its list's first value, or to the second when it is the
/// first. The item count is not a dimension and never reverts.
fn reversions(plan: &Plan, lists: &Lists) -> Vec<(String, String, Plan)> {
    let target = |dimension: &str, current: &str| -> Option<String> {
        let list = lists.list(dimension);
        let first = list.first()?;
        if first != current {
            Some(first.clone())
        } else {
            list.get(1).filter(|second| *second != current).cloned()
        }
    };
    let mut out = Vec::new();
    for (at, (dimension, current)) in plan.order.iter().enumerate() {
        if let Some(value) = target(dimension, current) {
            let mut reverted = plan.clone();
            if let Some(slot) = reverted.order.get_mut(at) {
                slot.1 = value;
            }
            out.push((dimension.clone(), current.clone(), reverted));
        }
    }
    for field in ITEM_ROWS {
        let dimension = format!("item.{field}");
        for (at, item) in plan.items.iter().enumerate() {
            let current = Plan::item_value(item, &dimension);
            if let Some(value) = target(&dimension, current) {
                let mut reverted = plan.clone();
                let slot = reverted
                    .items
                    .get_mut(at)
                    .and_then(|item| item.iter_mut().find(|(name, _)| *name == dimension));
                if let Some(slot) = slot {
                    slot.1 = value;
                }
                out.push((dimension.clone(), current.to_string(), reverted));
            }
        }
    }
    out
}

/// The dimension and value that make CP order `index` of `built` halt at a
/// not-implemented branch: the first of its [`reversions`] after which its
/// session runs to its end. Each reverted order is rebuilt from the stream
/// state its own draws started from, so its other text and amounts and the
/// round's stream stay as they were, and world B is rebuilt around it.
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
    for (dimension, current, plan) in reversions(&target.plan, lists) {
        let mut rng = target.start.clone();
        let rebuilt = build_record(&plan, refs, &mut rng)?;
        let text = render_nodes(built.iter().enumerate().flat_map(|(at, order)| {
            if at == index {
                rebuilt.record.nodes.iter()
            } else {
                order.record.nodes.iter()
            }
        }));
        let world = world_b(world_a, &text)
            .map_err(|error| CorpusError::new(Some(case), "world B", error.to_string()))?;
        let run = run_case(&world, &cp_inputs(&rebuilt.record.c_value));
        if classify(&run, not_implemented) == Outcome::End {
            return Ok((dimension, current));
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

/// Generates the corpus from `world_a` and the `SPEC.md` text `spec`.
///
/// 1. The not-implemented branch IDs come from `spec` ([`ne_branches`]); the
///    file 442 attributes `442I` and the reference lists from `world_a`.
/// 2. Each round restarts the stream at [`SEED`] and builds the 200 orders
///    under the current lists. Orders are built in case order, and each
///    draws its delivery location, proposal, APP, fund control point and
///    cost center, then per item its description, quantity, unit cost,
///    extended cost, STK#, NSN, FOOD GROUP, Items per and BOC, each only
///    when the rotation gives it a value to draw. No order may add a node
///    world A holds, and the corpus ZWR text must parse back to exactly the
///    generated nodes. Every CP case then runs once on world B. A halt at a
///    not-implemented branch is attributed to the first single-dimension
///    reversion that lets the session end, and that value leaves its list;
///    if any was dropped the next round starts, up to five rounds.
/// 3. On the final world B each CE and CR case runs once, and its outcome
///    keeps, moves or records it.
/// 4. Every order must meet the fixture profile ([`profile_violations`])
///    and the paging bound ([`order_worst_y`]), the `C` values the non-prefix
///    rule ([`c_value_violations`]), and the corpus every minimum
///    ([`minimum_violations`]).
///
/// A failed session, a CR case at a not-implemented branch, an unmet rule,
/// an attribution that finds no dimension, or a fifth round that still
/// drops a variant is a [`CorpusError`] naming the case and the rule.
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
    let header = world_a.get_or_empty(&key("PRC", &["442", "0"]));
    if piece(&header, "^", 2) != "442I" {
        return Err(CorpusError::new(
            None,
            "profile violation",
            "the world A file 442 header piece 2 is not 442I",
        ));
    }
    let refs = References::read(world_a)?;
    let mut lists = Lists::initial(&refs);
    let mut drops: Vec<DroppedVariant> = Vec::new();
    let mut round = 0;
    let (built, world, outcomes) = loop {
        round += 1;
        let (built, world) = run_round(world_a, &refs, &lists)?;
        let mut outcomes = Vec::with_capacity(built.len());
        let mut unresolved = Vec::new();
        for (index, order) in built.iter().enumerate() {
            let run = run_case(&world, &cp_inputs(&order.record.c_value));
            let outcome = classify(&run, &not_implemented);
            match &outcome {
                Outcome::Failure(cause) => {
                    return Err(CorpusError::new(
                        Some(&order.record.case_id),
                        "session failure",
                        cause.clone(),
                    ));
                }
                Outcome::Unimplemented(id) => unresolved.push((index, *id)),
                Outcome::End | Outcome::Implemented(_) => {}
            }
            outcomes.push(outcome);
        }
        if unresolved.is_empty() {
            break (built, world, outcomes);
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
            let (dimension, value) = attribute(
                world_a,
                &refs,
                &round_lists,
                &built,
                index,
                &not_implemented,
            )?;
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
    };

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
        rounds: round,
        worst_y: worst,
    };
    let violations = minimum_violations(world_a, &world, &corpus);
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
    /// The Items per line prints: not a purchase-card order, and piece 12
    /// is not zero [PRCHDP2.m:59].
    per: bool,
    boc: String,
    purchase_card: bool,
}

/// What the display reads of one world B order.
struct OrderFacts {
    node0: String,
    node1: String,
    node7: String,
    order_type: String,
    authorities: usize,
    items: Vec<ItemFacts>,
}

/// Reads order `ien` of `store`.
fn order_facts(store: &Store, ien: &str) -> OrderFacts {
    let order = key("PRC", &["442", ien]);
    let read = |subs: &[&str]| store.get_or_empty(&under(&order, subs));
    let order_type = piece(&read(&["23"]), "^", 11).to_string();
    let purchase_card = order_type == "S";
    let items = numeric_children(store, &under(&order, &["2"]))
        .iter()
        .map(|j| {
            let item0 = read(&["2", j, "0"]);
            let node2 = read(&["2", j, "2"]);
            let node4 = read(&["2", j, "4"]);
            let present = |text: &str| !text.is_empty();
            ItemFacts {
                desc: read(&["2", j, "1", "1", "0"]),
                qty: piece(&item0, "^", 2).to_string(),
                unit: piece(&item0, "^", 3).to_string(),
                cost: piece(&item0, "^", 9).to_string(),
                ext: piece(&node2, "^", 1).to_string(),
                stk: present(piece(&item0, "^", 6)),
                nsn: present(piece(&item0, "^", 13)),
                food: present(piece(&node4, "^", 12)),
                per: !purchase_card && num(piece(&item0, "^", 12)) != "0",
                boc: piece(&item0, "^", 4).to_string(),
                purchase_card,
            }
        })
        .collect();
    OrderFacts {
        node0: read(&["0"]),
        node1: read(&["1"]),
        node7: read(&["7"]),
        authorities: numeric_children(store, &under(&order, &["14"])).len(),
        order_type,
        items,
    }
}

/// The fixture profile of `SPEC.md` "Parity corpus" for order `ien` of
/// `world_b`, every reference resolved in `world_a`: each violation names
/// the order and the rule, and an order on the profile gives none. It
/// checks the `^PRC(442,0)` attributes `442I`, nodes 0 and 1, a method of
/// processing below 10, no node 11, 5, 3, 15, 4, 13, 6 or 24, field 5.3
/// empty, node 23 pieces 7, 8 and 22 empty and piece 11 `P`, `S` or `D`, no
/// `M`, `MCH` or `G` index, a non-EDI vendor, status and `.08` ESTIMATED
/// ORDER? (node 7 piece 3) empty, at least one item, each with one
/// description line of at most 32 characters and no `|`, a node 2 with
/// pieces 2, 8, 9 and 11 empty and no node 3, no item `AC` index, no 442.8
/// `B` index, every vendor, state, service, ship-to, unit, source code,
/// authority, method and status pointer resolved, and an agent of `.5` or
/// empty.
pub fn profile_violations(world_a: &Store, world_b: &Store, ien: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fail = |rule: String| out.push(format!("order {ien}: {rule}"));
    let header = world_a.get_or_empty(&key("PRC", &["442", "0"]));
    if piece(&header, "^", 2) != "442I" {
        fail("the world A file 442 header piece 2 is not 442I".to_string());
    }
    let order = key("PRC", &["442", ien]);
    let node0 = world_b.get(&under(&order, &["0"]));
    let node1 = world_b.get(&under(&order, &["1"]));
    if node0.is_none() {
        fail("node 0 is absent".to_string());
    }
    if node1.is_none() {
        fail("node 1 is absent".to_string());
    }
    let node0 = node0.unwrap_or("");
    let node1 = node1.unwrap_or("");
    let mop = piece(node0, "^", 2);
    if !resolves(world_a, "PRCD", "442.5", mop) || num_cmp(mop, "10") != Ordering::Less {
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
    if !piece(node1, "^", 12).is_empty() {
        fail("field 5.3, node 1 piece 12, is not empty".to_string());
    }
    let node23 = under(&order, &["23"]);
    if world_b.data(&node23) != 0 {
        let node23 = world_b.get_or_empty(&node23);
        for field in [7, 8, 22] {
            if !piece(&node23, "^", field).is_empty() {
                fail(format!("node 23 piece {field} is not empty"));
            }
        }
        if !["P", "S", "D"].contains(&piece(&node23, "^", 11)) {
            fail("node 23 piece 11 is not P, S or D".to_string());
        }
    }
    for index in ["M", "MCH", "G"] {
        if world_b.data(&key("PRC", &["442", index])) != 0 {
            fail(format!("the file 442 index {index} holds entries"));
        }
    }
    let vendor = piece(node1, "^", 1);
    match world_a.get(&key("PRC", &["440", vendor, "0"])) {
        None => fail(format!(
            "vendor {} does not resolve in file 440",
            tag_value(vendor)
        )),
        Some(vendor0) => {
            let node3 = world_a.get_or_empty(&key("PRC", &["440", vendor, "3"]));
            if piece(&node3, "^", 2) == "Y" {
                fail(format!("vendor {vendor} is an EDI vendor"));
            }
            let state = piece(vendor0, "^", 7);
            if !state.is_empty() && !resolves(world_a, "DIC", "5", &num(state)) {
                fail(format!(
                    "vendor {vendor} state {state} does not resolve in file 5"
                ));
            }
        }
    }
    let node7 = world_b.get_or_empty(&under(&order, &["7"]));
    let status = piece(&node7, "^", 1);
    if !resolves(world_a, "PRCD", "442.3", status) {
        fail(format!(
            "status {} does not resolve in file 442.3",
            tag_value(status)
        ));
    }
    if !piece(&node7, "^", 3).is_empty() {
        fail("field .08, node 7 piece 3, is not empty".to_string());
    }
    let items = under(&order, &["2"]);
    let entries = numeric_children(world_b, &items);
    if entries.is_empty() {
        fail("the order has no item".to_string());
    }
    if world_b.data(&under(&items, &["AC"])) != 0 {
        fail("the item index AC holds entries".to_string());
    }
    for j in &entries {
        let item = under(&items, &[j]);
        let Some(item0) = world_b.get(&under(&item, &["0"])) else {
            fail(format!("item {j} has no zero node"));
            continue;
        };
        let lines = numeric_children(world_b, &under(&item, &["1"]));
        let desc = world_b.get_or_empty(&under(&item, &["1", "1", "0"]));
        let header_present = world_b.get(&under(&item, &["1", "0"])).is_some();
        if !header_present || lines != ["1"] || desc.chars().count() > 32 || desc.contains('|') {
            fail(format!(
                "item {j} description is not one line of at most 32 characters without |"
            ));
        }
        match world_b.get(&under(&item, &["2"])) {
            None => fail(format!("item {j} node 2 is absent")),
            Some(node2) => {
                for field in [2, 8, 9, 11] {
                    if !piece(node2, "^", field).is_empty() {
                        fail(format!("item {j} node 2 piece {field} is not empty"));
                    }
                }
            }
        }
        if world_b.data(&under(&item, &["3"])) != 0 {
            fail(format!("item {j} node 3 is present"));
        }
        let unit = piece(item0, "^", 3);
        if !unit.is_empty() && !resolves(world_a, "PRCD", "420.5", unit) {
            fail(format!(
                "item {j} unit {unit} does not resolve in file 420.5"
            ));
        }
    }
    if world_b.data(&key("PRC", &["442.8", "B"])) != 0 {
        fail("the file 442.8 index B holds entries".to_string());
    }
    let service = piece(node1, "^", 2);
    if !service.is_empty() && !resolves(world_a, "DIC", "49", service) {
        fail(format!("service {service} does not resolve in file 49"));
    }
    let ship_to = piece(node1, "^", 3);
    if !ship_to.is_empty()
        && world_a
            .get(&key("PRC", &["411", STATION, "1", ship_to, "0"]))
            .is_none()
    {
        fail(format!(
            "ship-to {ship_to} does not resolve under station {STATION}"
        ));
    }
    let source = piece(node1, "^", 7);
    if !source.is_empty() && !resolves(world_a, "PRCD", "420.8", source) {
        fail(format!(
            "source code {source} does not resolve in file 420.8"
        ));
    }
    let authorities = under(&order, &["14"]);
    for j in numeric_children(world_b, &authorities) {
        let authority = world_b.get_or_empty(&under(&authorities, &[&j, "0"]));
        if !resolves(world_a, "PRC", "442.4", &authority) {
            fail(format!("authority {j} does not resolve in file 442.4"));
        }
    }
    let agent = piece(node1, "^", 10);
    if !(agent.is_empty() || (agent == ".5" && resolves(world_a, "VA", "200", ".5"))) {
        fail(format!("agent {agent} is not .5 or empty"));
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
/// result is `max(Y_ask(k) for all k, Y_end)`: 44 for 13 items with no
/// optional line, 40 for 5 items with every one.
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
    let shipping = num_cmp(piece(&facts.node0, "^", 13), "0") == Ordering::Greater;
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
/// included: all are distinct and none is a prefix of another. Each corpus
/// value, one world A's `C` index does not hold, must also be neither a
/// prefix nor an extension of, nor equal to, any subscript of a world A
/// file 442 index whose name collates at or after `C`. No `C` value may
/// begin with one of [`MISS_STRINGS`], as written or in upper case.
pub fn c_value_violations(world_a: &Store, world_b: &Store) -> Vec<String> {
    let mut out = Vec::new();
    let c_values = child_texts(world_b, &key("PRC", &["442", "C"]));
    let mut seen = BTreeSet::new();
    for value in &c_values {
        if !seen.insert(value.as_str()) {
            out.push(format!("C value {value} appears twice"));
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

/// `present` or `absent`.
fn presence(present: bool) -> String {
    if present { "present" } else { "absent" }.to_string()
}

/// The order row labels of one world B order, as the rotation lists name
/// them.
fn order_labels(facts: &OrderFacts) -> Vec<(&'static str, String)> {
    let p0 = |field: usize| piece(&facts.node0, "^", field).to_string();
    let p1 = |field: usize| piece(&facts.node1, "^", field).to_string();
    let verbal = match (p1(4).as_str(), p1(5).as_str()) {
        ("Y", "") => "verbal",
        ("Y", "Y") => "confirmation",
        ("", "") => "",
        _ => "other",
    };
    vec![
        ("status", piece(&facts.node7, "^", 1).to_string()),
        ("type", facts.order_type.clone()),
        ("source", p1(7)),
        ("mop", p0(2)),
        ("vendor", p1(1)),
        ("service", p1(2)),
        ("shipto", p1(3)),
        ("delivery", presence(!p1(11).is_empty())),
        ("verbal", verbal.to_string()),
        ("fob", p1(6)),
        ("proposal", p1(8).chars().count().to_string()),
        ("authority", facts.authorities.to_string()),
        ("date", p0(10)),
        ("app", p0(4).chars().count().to_string()),
        ("agent", p1(10)),
        ("total", p0(15)),
        ("shipping", p0(13)),
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
    /// dropped value of the dimension.
    fn distinct(&mut self, dimension: &str, minimum: usize, keep: impl Fn(&str) -> bool) {
        let found = self.counts.get(dimension).map_or(0, |values| {
            values.keys().filter(|value| keep(value)).count()
        });
        let needed = minimum.saturating_sub(self.drops_in(dimension));
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
/// per dropped value of its dimension. The item counts must be exact.
pub fn minimum_violations(world_a: &Store, world_b: &Store, corpus: &Corpus) -> Vec<String> {
    let refs = match References::read(world_a) {
        Ok(refs) => refs,
        Err(error) => return vec![error.to_string()],
    };
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
            .map(|order| vendor_properties(world_a, piece(&order.node1, "^", 1)))
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
    tally.distinct("app", 4, |_| true);
    tally.each("agent", &owned(&[".5", ""]), 5, "orders");
    tally.distinct("total", TOTALS.len(), |_| true);
    let positive: Vec<&str> = SHIPPING
        .iter()
        .copied()
        .filter(|value| num_cmp(value, "0") == Ordering::Greater)
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

/// The manifest's header row.
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
    /// `C` value and item count; a CE or CR row the orders it opens,
    /// comma-separated, and no item count. Tags are comma-separated, and
    /// the expected branch is `none` or a branch ID.
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
