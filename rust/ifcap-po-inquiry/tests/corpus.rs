//! The offline gate of the parity corpus, `SPEC.md` "Parity corpus": every
//! committed case input runs on world B both through [`inquire_traced`]
//! directly and through `corpus::run_case`, the two results must agree, and
//! the case must end as its class says; every generated order must meet the
//! fixture profile, the non-prefix rule, the variation minimums and the
//! paging bound; and every committed generated file must equal a fresh
//! generation.
//!
//! The gate reads world A through `store::load_fixtures`, the committed
//! `corpus.zwr` and `manifest.tsv` whole, `SPEC.md` compiled in, and each
//! case's input only through `golden::read_corpus_input`. It never reads a
//! recording (a case's `transcript.txt`, a run's `meta.txt`, `SHA256SUMS` or
//! `run-summary.txt`), a GC capture or an export, and it writes nothing; nor
//! does the `corpus_gen --check` run it starts.
//!
//! The class, profile, `C` value, variation and paging tests call checkers
//! that `ifcap_po_inquiry::corpus` shares with the generator. The profile,
//! `C` value, variation and paging tests also recheck selected rules on
//! their own from the world B nodes. The variation test recounts only some
//! dimensions; for the others (source, delivery location, verbal or
//! confirmation, FOB, proposal, APP, the vendor properties, the P.O. date
//! and every item dimension but the unit cost) it builds malformed copies
//! of the corpus in memory, each breaking one of those minimums, and
//! requires `corpus::minimum_violations` to report that minimum and only
//! the violations listed for that copy; the one-vendor copy also breaks,
//! and lists, the distinct-vendor minimum. A test reports every violation
//! it finds, not only the first.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use ifcap_po_inquiry::inquiry::inquire_traced;
use ifcap_po_inquiry::store::{Store, Sub};
use ifcap_po_inquiry::{corpus, golden, store};

/// The specification: its "### Branch inventory" gives the not-implemented
/// branch set, and the generator takes it whole.
const SPEC: &str = include_str!("../SPEC.md");

/// The manifest's file name in the corpus folder.
const MANIFEST_NAME: &str = "manifest.tsv";

/// The manifest's header row.
const MANIFEST_HEADER: &str =
    "case\tclass\tseed\torder_ien\tc_value\titem_count\ttags\texpected_branch";

/// `corpus::SEED` in decimal, the `seed` column of every manifest row.
const SEED_DECIMAL: &str = "11400714819323198485";

/// The manifest's class labels.
const CLASSES: [&str; 4] = ["CP", "IN", "BOUNDARY", "CR"];

/// The session output limit of the inquiry, 1 MiB. The crate's own constant
/// is private to it, so the gate states the value.
const SESSION_OUTPUT_LIMIT: usize = 1_048_576;

/// The end of a session that runs to its end: the order prompt, then the
/// `\n` of the process exit (`SPEC.md` K14).
const EXIT_TAIL: &str = "P.O./REQ.NO.: \n";

/// The two header lines of `corpus.zwr`.
const ZWR_HEADER: [&str; 2] = ["OSEHRA ZGO Export: IFCAP PO INQUIRY PARITY CORPUS", "ZWR"];

/// The number of CP cases; CP-n displays order `IEN_BASE + n`.
const CP_COUNT: usize = 200;
/// The number of CE cases.
const CE_COUNT: usize = 26;
/// The number of CR cases.
const CR_COUNT: usize = 5;
/// The specified class counts of the CE cases: CE-01 to CE-16 are IN and
/// CE-17 to CE-26 BOUNDARY.
const CE_SPECIFIED: (usize, usize) = (16, 10);
/// CP-n is order IEN `IEN_BASE + n`.
const IEN_BASE: usize = 991_000;
/// The station every corpus order belongs to.
const STATION: &str = "500";

/// The `C` values of the GC orders, which no corpus value may extend or
/// prefix.
const GC_C_VALUES: [&str; 3] = ["GC1001", "GC2002", "GC5005"];

/// The edge cases the specification lists as IN whose lookup answer, a
/// canonical number or a lowercase value with no candidate, halts at the
/// not-implemented `PO-09.b11`, so they are BOUNDARY (`SPEC.md` K17).
const MOVED_TO_BOUNDARY: [&str; 3] = ["CE-04", "CE-06", "CE-08"];
/// The branch those three halt at.
const LOOKUP_BRANCH: &str = "PO-09.b11";

/// The methods of processing below 10 that 442.5 holds, the `mop` row.
const MOPS: [&str; 7] = ["1", "2", "3", "4", "7", "8", "9"];

/// The 12 dates of the `date` row, which both date fields of an order take.
const DATES: [&str; 12] = [
    "2990101", "2991231", "3000229", "3111109", "3240229", "3260101", "3260115", "3260630",
    "3261001", "3261015", "3261231", "",
];

/// The estimated shipping values above 0 of the `shipping` row.
const POSITIVE_SHIPPING: [&str; 3] = [".5", "12.345", "1000"];

/// The six stored unit cost forms of the `item.cost` row.
const COST_FORMS: [&str; 6] = ["N/C", "whole", "2dp", "3dp", "4dp", "5dp"];

/// The exact multi-item counts, each with the number of orders that take
/// it; the other 150 orders have one item.
const MULTI_COUNTS: [(usize, usize); 7] =
    [(2, 14), (3, 12), (4, 9), (5, 6), (6, 4), (7, 3), (8, 2)];

/// The largest item count a corpus order has.
const LARGEST_ITEM_COUNT: usize = 8;

// ---------------------------------------------------------------------------
// The shared gate state
// ---------------------------------------------------------------------------

/// What every test reads, built once.
struct Gate {
    /// The store of the fixtures, which the corpus never changes.
    world_a: Store,
    /// World A plus the committed corpus nodes, as `corpus::world_b` builds
    /// it from the committed `corpus.zwr`.
    world_b: Store,
    /// The committed `corpus.zwr`, as stored.
    corpus_text: String,
    /// The committed `corpus.zwr` parsed alone.
    corpus_nodes: Store,
    /// The committed `manifest.tsv`, as stored.
    manifest_text: String,
    /// The committed manifest's rows, in file order.
    manifest: Vec<Row>,
    /// The not-implemented branch IDs of `SPEC.md` "Branch inventory".
    not_implemented: Vec<String>,
    /// A fresh generation of the corpus.
    model: corpus::Corpus,
}

static GATE: OnceLock<Result<Gate, String>> = OnceLock::new();

/// The gate state, built on first use; a failure to build it fails the
/// calling test with its cause.
fn gate() -> &'static Gate {
    match GATE.get_or_init(build_gate) {
        Ok(gate) => gate,
        Err(cause) => panic!("the corpus gate cannot start: {cause}"),
    }
}

fn build_gate() -> Result<Gate, String> {
    let world_a = store::load_fixtures(&corpus::world_a_dir())
        .map_err(|error| format!("world A fixtures: {error}"))?;
    let zwr_path = corpus::corpus_zwr_path();
    let corpus_text = fs::read_to_string(&zwr_path)
        .map_err(|error| format!("{}: {error}", zwr_path.display()))?;
    let corpus_nodes = Store::from_zwr(&corpus_text)
        .map_err(|error| format!("{}: {error}", zwr_path.display()))?;
    let world_b = corpus::world_b(&world_a, &corpus_text)
        .map_err(|error| format!("world B from {}: {error}", zwr_path.display()))?;
    let manifest_path = golden::corpus_dir().join(MANIFEST_NAME);
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
    let manifest = parse_manifest(&manifest_text)
        .map_err(|problems| format!("{}: {}", manifest_path.display(), problems.join("; ")))?;
    let not_implemented = corpus::ne_branches(SPEC);
    if not_implemented.is_empty() {
        return Err("SPEC.md \"### Branch inventory\" lists no not-implemented branch".to_string());
    }
    let model =
        corpus::generate(&world_a, SPEC).map_err(|error| format!("corpus::generate: {error}"))?;
    Ok(Gate {
        world_a,
        world_b,
        corpus_text,
        corpus_nodes,
        manifest_text,
        manifest,
        not_implemented,
        model,
    })
}

// ---------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------

/// One `manifest.tsv` row, its eight columns as written; `tags` is the
/// comma-separated column split, empty when the column is.
struct Row {
    case: String,
    class: String,
    seed: String,
    order_ien: String,
    c_value: String,
    item_count: String,
    tags: Vec<String>,
    expected_branch: String,
}

impl Row {
    /// The value of the row's first tag `name=<value>`.
    fn tag(&self, name: &str) -> Option<&str> {
        self.tags.iter().find_map(|tag| {
            tag.split_once('=')
                .and_then(|(key, value)| (key == name).then_some(value))
        })
    }

    /// The tags column as written.
    fn tags_text(&self) -> String {
        self.tags.join(",")
    }

    /// The row's column rule violations: the seed, the class label, and an
    /// expected branch of `none` or a branch ID.
    fn violations(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.seed != SEED_DECIMAL {
            out.push(format!(
                "{}: seed {} is not {SEED_DECIMAL}",
                self.case, self.seed
            ));
        }
        if !CLASSES.contains(&self.class.as_str()) {
            out.push(format!(
                "{}: class {} is not CP, IN, BOUNDARY or CR",
                self.case, self.class
            ));
        }
        if self.expected_branch != "none" && !is_branch_id(&self.expected_branch) {
            out.push(format!(
                "{}: expected branch {} is neither none nor a PO-NN.bM branch ID",
                self.case, self.expected_branch
            ));
        }
        out
    }
}

/// Whether `text` is all ASCII digits and not empty.
fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether `text` is a `SPEC.md` branch ID, `PO-` with two digits, `.b` and
/// a number, such as `PO-09.b11`.
fn is_branch_id(text: &str) -> bool {
    text.strip_prefix("PO-")
        .and_then(|rest| rest.split_once(".b"))
        .is_some_and(|(file, branch)| file.len() == 2 && all_digits(file) && all_digits(branch))
}

/// The rows of the manifest `text`, or every problem found: the header row,
/// `\n` line ends with a final `\n` and no `\r`, eight tab-separated fields
/// per row, each row's column rules, and the case column equal to
/// `golden::corpus_case_ids()` in order.
fn parse_manifest(text: &str) -> Result<Vec<Row>, Vec<String>> {
    let mut problems = Vec::new();
    if corpus::SEED.to_string() != SEED_DECIMAL {
        problems.push(format!(
            "corpus::SEED is {}, not {SEED_DECIMAL}",
            corpus::SEED
        ));
    }
    if text.contains('\r') {
        problems.push("a carriage return".to_string());
    }
    let Some(body) = text.strip_suffix('\n') else {
        return Err(vec!["no final \\n".to_string()]);
    };
    let mut lines = body.split('\n');
    if lines.next() != Some(MANIFEST_HEADER) {
        problems.push(format!(
            "the first line is not the header {MANIFEST_HEADER:?}"
        ));
    }
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [
            case,
            class,
            seed,
            order_ien,
            c_value,
            item_count,
            tags,
            expected_branch,
        ] = fields.as_slice()
        else {
            problems.push(format!(
                "row {} has {} tab-separated fields, not 8",
                index + 1,
                fields.len()
            ));
            continue;
        };
        let row = Row {
            case: case.to_string(),
            class: class.to_string(),
            seed: seed.to_string(),
            order_ien: order_ien.to_string(),
            c_value: c_value.to_string(),
            item_count: item_count.to_string(),
            tags: tags
                .split(',')
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
            expected_branch: expected_branch.to_string(),
        };
        problems.extend(row.violations());
        rows.push(row);
    }
    let ids: Vec<&str> = rows.iter().map(|row| row.case.as_str()).collect();
    if ids != golden::corpus_case_ids() {
        problems.push(format!(
            "the {} case ids are not the {} ids of golden::corpus_case_ids() in order",
            ids.len(),
            golden::corpus_case_ids().len()
        ));
    }
    if problems.is_empty() {
        Ok(rows)
    } else {
        Err(problems)
    }
}

// ---------------------------------------------------------------------------
// Reading the stores
// ---------------------------------------------------------------------------

/// The key of `^global(subs...)`, each subscript classified by its text.
fn key(global: &str, subs: &[&str]) -> Vec<Sub> {
    std::iter::once(Sub::Str(global.to_string()))
        .chain(subs.iter().map(|sub| Sub::from_text(sub)))
        .collect()
}

/// `parent` extended by the subscripts `subs`.
fn under(parent: &[Sub], subs: &[&str]) -> Vec<Sub> {
    parent
        .iter()
        .cloned()
        .chain(subs.iter().map(|sub| Sub::from_text(sub)))
        .collect()
}

/// Every child subscript of `parent`, in collation order, walked with
/// `Store::order`.
fn child_subs(store: &Store, parent: &[Sub]) -> Vec<Sub> {
    let mut out = Vec::new();
    let mut after = Sub::default();
    while let Some(next) = store.order(parent, Some(&after)) {
        out.push(next.clone());
        after = next;
    }
    out
}

/// Every child subscript of `parent` as text.
fn children(store: &Store, parent: &[Sub]) -> Vec<String> {
    child_subs(store, parent)
        .iter()
        .map(|sub| sub.text().to_string())
        .collect()
}

/// The numeric child subscripts of `parent` other than 0: the entries of a
/// multiple or the IENs of a file.
fn entries(store: &Store, parent: &[Sub]) -> Vec<String> {
    child_subs(store, parent)
        .iter()
        .filter(|sub| sub.is_num() && sub.text() != "0")
        .map(|sub| sub.text().to_string())
        .collect()
}

/// The 1-based `^`-piece `position` of `text`, empty when there is none.
fn piece(text: &str, position: usize) -> &str {
    position
        .checked_sub(1)
        .and_then(|at| text.split('^').nth(at))
        .unwrap_or("")
}

/// The 1-based `^`-piece `position` of the node at `node` in `store`,
/// empty when the node holds no value.
fn field<'s>(store: &'s Store, node: &[Sub], position: usize) -> &'s str {
    store.get(node).map_or("", |value| piece(value, position))
}

/// Whether `^global(file,ien,0)` holds a value in `store`.
fn resolves(store: &Store, global: &str, file: &str, ien: &str) -> bool {
    !ien.is_empty() && store.get(&key(global, &[file, ien, "0"])).is_some()
}

/// `text` as an M number: its longest leading numeric prefix, an optional
/// sign, digits and at most one `.`, and 0 when there is none.
fn m_number(text: &str) -> f64 {
    let mut end = 0;
    let mut point = false;
    for (at, ch) in text.char_indices() {
        let numeric =
            ch.is_ascii_digit() || (at == 0 && (ch == '-' || ch == '+')) || (ch == '.' && !point);
        if !numeric {
            break;
        }
        point |= ch == '.';
        end = at + ch.len_utf8();
    }
    text.get(..end)
        .and_then(|prefix| prefix.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// The IENs of the 200 corpus orders, CP-001's first.
fn corpus_iens() -> Vec<String> {
    (1..=CP_COUNT).map(|n| (IEN_BASE + n).to_string()).collect()
}

/// The case number n of a CP case's `<prefix><nnn>` value, such as the `C`
/// value `CP001`, when it is three digits from 1 to 200.
fn cp_number(text: &str, prefix: &str) -> Option<usize> {
    let digits = text.strip_prefix(prefix)?;
    if digits.len() != 3 || !all_digits(digits) {
        return None;
    }
    digits
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=CP_COUNT).contains(n))
}

/// The tag form of a dimension value, as a dropped variant records it:
/// `empty` for the empty value.
fn tag_form(value: &str) -> &str {
    if value.is_empty() { "empty" } else { value }
}

/// What the gate reads of one item of a world B order.
struct ItemView {
    /// Node 0 piece 9, the unit cost.
    cost: String,
    /// Node 0 piece 6, the STK#.
    stk: String,
    /// Node 0 piece 13, the NSN.
    nsn: String,
    /// Node 4 piece 12, the FOOD GROUP.
    food: String,
    /// Node 0 piece 12, the Items per count.
    per: String,
}

/// What the gate reads of one world B order.
struct OrderView {
    /// Node 0 piece 2.
    mop: String,
    /// Node 0 piece 10, the delivery date.
    date: String,
    /// Node 0 piece 13.
    shipping: String,
    /// Node 0 piece 15.
    total: String,
    /// Node 1 piece 1.
    vendor: String,
    /// Node 1 piece 2.
    service: String,
    /// Node 1 piece 3.
    ship_to: String,
    /// Node 1 piece 10.
    agent: String,
    /// Node 7 piece 1.
    status: String,
    /// Node 23 piece 11, empty without node 23.
    order_type: String,
    /// The number of node 14 authority entries.
    authorities: usize,
    /// The items, in item order.
    items: Vec<ItemView>,
}

/// Reads order `ien` of `store`.
fn read_order(store: &Store, ien: &str) -> OrderView {
    let order = key("PRC", &["442", ien]);
    let at = |node: &str, position: usize| field(store, &under(&order, &[node]), position);
    let items_root = under(&order, &["2"]);
    let items = entries(store, &items_root)
        .iter()
        .map(|j| {
            let item = under(&items_root, &[j]);
            let zero = under(&item, &["0"]);
            ItemView {
                cost: field(store, &zero, 9).to_string(),
                stk: field(store, &zero, 6).to_string(),
                nsn: field(store, &zero, 13).to_string(),
                food: field(store, &under(&item, &["4"]), 12).to_string(),
                per: field(store, &zero, 12).to_string(),
            }
        })
        .collect();
    OrderView {
        mop: at("0", 2).to_string(),
        date: at("0", 10).to_string(),
        shipping: at("0", 13).to_string(),
        total: at("0", 15).to_string(),
        vendor: at("1", 1).to_string(),
        service: at("1", 2).to_string(),
        ship_to: at("1", 3).to_string(),
        agent: at("1", 10).to_string(),
        status: at("7", 1).to_string(),
        order_type: at("23", 11).to_string(),
        authorities: entries(store, &under(&order, &["14"])).len(),
        items,
    }
}

/// The paging `$Y` of `order` (`SPEC.md` "Parity corpus", PRCHDP2.m lines
/// 42 to 65): `HDR` leaves `$Y` at 4; item k's `ASK` sees
/// `Y_ask(k) = 4 + sum over j<k of (3 + d_j + ip_j) + 1 + d_k + 2*ip_k`,
/// and the end is `Y_end = 4 + sum over all items of (3 + d_j + ip_j) + e`.
/// `d_j` counts the item's present STK#, NSN and FOOD GROUP, `ip_j` is 1
/// when its Items per line prints (type not `S`, piece 12 not 0), and `e`
/// is 1 when the estimated shipping is above 0. The result is the largest.
fn paging_y(order: &OrderView) -> usize {
    let shapes: Vec<(usize, usize)> = order
        .items
        .iter()
        .map(|item| {
            let details = [&item.stk, &item.nsn, &item.food]
                .iter()
                .filter(|value| !value.is_empty())
                .count();
            let per = order.order_type != "S" && m_number(&item.per).abs() > 0.0;
            (details, usize::from(per))
        })
        .collect();
    let line = |(details, per): &(usize, usize)| 3 + details + per;
    let y_ask = shapes.iter().enumerate().map(|(k, (details, per))| {
        let before: usize = shapes.iter().take(k).map(line).sum();
        4 + before + 1 + details + 2 * per
    });
    let y_end =
        4 + shapes.iter().map(line).sum::<usize>() + usize::from(m_number(&order.shipping) > 0.0);
    y_ask.fold(y_end, usize::max)
}

/// The stored form of a unit cost, as the `item.cost` row names it: `N/C`,
/// `whole`, or the number of decimals followed by `dp`.
fn cost_form(cost: &str) -> String {
    if cost == "N/C" {
        return cost.to_string();
    }
    match cost.split_once('.') {
        None => "whole".to_string(),
        Some((_, fraction)) => format!("{}dp", fraction.chars().count()),
    }
}

/// How often each value of `values` occurs.
fn tally<'a>(values: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

/// The failure message of a test: the count, then one violation per line.
fn report(what: &str, failures: &[String]) -> String {
    format!(
        "{what}: {} violation(s)\n{}",
        failures.len(),
        failures.join("\n")
    )
}

// ---------------------------------------------------------------------------
// Case outcomes
// ---------------------------------------------------------------------------

/// The differences between manifest row `row` and the model's `case`, each
/// naming the column.
fn row_differences(row: &Row, case: &corpus::CaseEntry) -> Vec<String> {
    let item_count = case
        .item_count
        .map(|count| count.to_string())
        .unwrap_or_default();
    let columns: [(&str, &str, String); 7] = [
        ("case", &row.case, case.id.clone()),
        ("class", &row.class, case.class.label().to_string()),
        ("order_ien", &row.order_ien, case.orders.join(",")),
        ("c_value", &row.c_value, case.c_values.join(",")),
        ("item_count", &row.item_count, item_count),
        ("tags", &row.tags_text(), case.tags.join(",")),
        (
            "expected_branch",
            &row.expected_branch,
            case.expected_branch
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
    ];
    let mut out: Vec<String> = columns
        .iter()
        .filter(|(_, committed, generated)| *committed != generated.as_str())
        .map(|(column, committed, generated)| {
            format!("manifest column {column} is {committed:?}, the model gives {generated:?}")
        })
        .collect();
    if row.tag("finding") != case.finding.as_deref() {
        out.push(format!(
            "manifest finding tag {:?} differs from the model's finding {:?}",
            row.tag("finding"),
            case.finding
        ));
    }
    out
}

/// Whether the session of manifest row `row`, classified as `outcome`, meets
/// its class. `error`, `branch` and `transcript` are what `inquire_traced`
/// itself returned, which the End and BOUNDARY rules check again. A row
/// tagged `finding=<id>` passes only as `Implemented(id)` at its expected
/// branch; otherwise CP, IN and CR rows must end and BOUNDARY rows must halt
/// at their expected not-implemented branch. Besides the committed rows, it
/// judges the synthetic rows of `classifier_probes` against their in-memory
/// sessions, which reach the finding and failure rules no committed case
/// reaches.
fn judge(
    row: &Row,
    outcome: &corpus::Outcome,
    error: Option<&str>,
    branch: Option<&str>,
    transcript: &str,
    not_implemented: &[String],
) -> Result<(), String> {
    if let Some(id) = row.tag("finding") {
        return match outcome {
            corpus::Outcome::Implemented(found) if *found == id && row.expected_branch == id => {
                Ok(())
            }
            other => Err(format!(
                "tagged finding={id} with expected branch {}, but the outcome is {other:?}",
                row.expected_branch
            )),
        };
    }
    let mut causes = Vec::new();
    if let Some(error) = error {
        causes.push(format!("the session failed with {error}"));
    }
    match row.class.as_str() {
        "CP" | "IN" | "CR" => {
            if !matches!(outcome, corpus::Outcome::End) {
                causes.push(format!("the outcome is {outcome:?}, not End"));
            }
            if row.expected_branch != "none" {
                causes.push(format!(
                    "the expected branch is {}, not none",
                    row.expected_branch
                ));
            }
            if let Some(branch) = branch {
                causes.push(format!("the session halted at {branch}"));
            }
            if !transcript.ends_with(EXIT_TAIL) {
                causes.push(
                    "the transcript does not end with the order prompt and the exit newline \
                     (input ran out or the output limit cut it)"
                        .to_string(),
                );
            }
            if transcript.len() >= SESSION_OUTPUT_LIMIT {
                causes.push(format!(
                    "the transcript of {} bytes reaches the {SESSION_OUTPUT_LIMIT}-byte limit",
                    transcript.len()
                ));
            }
        }
        "BOUNDARY" => {
            let expected = row.expected_branch.as_str();
            if !matches!(outcome, corpus::Outcome::Unimplemented(found) if *found == expected) {
                causes.push(format!(
                    "the outcome is {outcome:?}, not Unimplemented({expected:?})"
                ));
            }
            if branch != Some(expected) {
                causes.push(format!(
                    "inquire_traced gave branch {branch:?}, not {expected}"
                ));
            }
            if !not_implemented.iter().any(|known| known == expected) {
                causes.push(format!(
                    "{expected} is not a not-implemented branch of SPEC.md \"Branch inventory\""
                ));
            }
        }
        other => causes.push(format!("{other} is not a case class")),
    }
    if causes.is_empty() {
        Ok(())
    } else {
        Err(causes.join("; "))
    }
}

/// A branch that `SPEC.md` "Branch inventory" lists as exercised, so
/// implemented: the halt of the in-memory finding probes.
const IMPLEMENTED_BRANCH: &str = "PO-09.b8";

/// A second exercised branch, whose finding tag must not accept a halt at
/// `IMPLEMENTED_BRANCH`.
const OTHER_IMPLEMENTED_BRANCH: &str = "PO-09.b3";

/// A not-implemented branch other than `LOOKUP_BRANCH`: the wrong expected
/// branch of a BOUNDARY probe.
const OTHER_LOOKUP_BRANCH: &str = "PO-09.b6";

/// What a probe requires of `corpus::classify`.
#[derive(Debug)]
enum Expect {
    /// Exactly this outcome.
    Exactly(corpus::Outcome),
    /// A `Failure` with any cause text, so the gate does not pin the
    /// classifier's wording.
    AnyFailure,
}

impl Expect {
    /// Whether `outcome` meets the expectation.
    fn admits(&self, outcome: &corpus::Outcome) -> bool {
        match self {
            Expect::Exactly(expected) => outcome == expected,
            Expect::AnyFailure => matches!(outcome, corpus::Outcome::Failure(_)),
        }
    }
}

/// One in-memory session of the classifier probes: its label, the session,
/// the exact transcript length it needs (checked before it is classified),
/// what `corpus::classify` must give, and synthetic rows, each with whether
/// `judge` must accept it.
struct Probe {
    label: &'static str,
    run: corpus::CaseRun,
    bytes: Option<usize>,
    expect: Expect,
    rows: Vec<(Row, bool)>,
}

/// A synthetic manifest row of `class`, tagged `tags` and expecting
/// `expected_branch`, with the manifest's seed and no order.
fn probe_row(class: &str, tags: &[&str], expected_branch: &str) -> Row {
    Row {
        case: "probe".to_string(),
        class: class.to_string(),
        seed: SEED_DECIMAL.to_string(),
        order_ien: String::new(),
        c_value: String::new(),
        item_count: String::new(),
        tags: tags.iter().copied().map(str::to_string).collect(),
        expected_branch: expected_branch.to_string(),
    }
}

/// A session that wrote `transcript` and halted at `branch`, with `error`.
fn session(
    transcript: String,
    branch: Option<&'static str>,
    error: Option<&str>,
) -> corpus::CaseRun {
    corpus::CaseRun {
        transcript,
        branch,
        error: error.map(str::to_string),
    }
}

/// A transcript of `bytes` bytes, `x` repeated and then `EXIT_TAIL`; below
/// the tail's length it is the tail alone, which the probe's length check
/// reports.
fn sized_transcript(bytes: usize) -> String {
    let mut text = "x".repeat(bytes.saturating_sub(EXIT_TAIL.len()));
    text.push_str(EXIT_TAIL);
    text
}

/// Whether `SPEC.md` "### Branch inventory" has a row for branch `id`, read
/// as `corpus::ne_branches` reads the table: the lines from that heading to
/// the next, each row a line beginning `| PO-` whose first cell is its ID.
fn inventory_lists(id: &str) -> bool {
    SPEC.lines()
        .skip_while(|line| line.trim_end() != "### Branch inventory")
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .filter(|line| line.starts_with("| PO-"))
        .filter_map(|line| line.strip_prefix('|'))
        .filter_map(|cells| cells.split_once('|'))
        .any(|(first, _)| first.trim() == id)
}

/// In-memory sessions for `gate_every_case_meets_its_class`, covering normal
/// completion and outcomes absent from committed cases (`SPEC.md`
/// "Parity corpus", offline gate): a session
/// that ends; halts at a not-implemented and at an implemented branch; an
/// error, alone and with either kind of branch; a transcript without the
/// K14 exit newline; transcripts one byte under, at and one byte over the
/// 1,048,576-byte session output limit; and one at the limit with either
/// kind of branch. An error and the limit come before a branch, and a row
/// tagged `finding=<id>` is accepted only for `Implemented(id)` at that
/// expected branch, whatever its class.
fn classifier_probes() -> Vec<Probe> {
    let error =
        ifcap_po_inquiry::InquiryError::CorruptStore("^PRC(442,991001,0)".to_string()).to_string();
    let finding = format!("finding={IMPLEMENTED_BRANCH}");
    let other_finding = format!("finding={OTHER_IMPLEMENTED_BRANCH}");
    let lookup_finding = format!("finding={LOOKUP_BRANCH}");
    let untagged = |class: &str| probe_row(class, &[], "none");
    let boundary = |expected: &str| probe_row("BOUNDARY", &[], expected);
    let found = || probe_row("CP", &[finding.as_str()], IMPLEMENTED_BRANCH);
    let unterminated = EXIT_TAIL.trim_end_matches('\n').to_string();
    let limit = SESSION_OUTPUT_LIMIT;
    vec![
        Probe {
            label: "a session that ends",
            run: session(EXIT_TAIL.to_string(), None, None),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::End),
            rows: vec![
                (untagged("CP"), true),
                (untagged("IN"), true),
                (untagged("CR"), true),
                (boundary(LOOKUP_BRANCH), false),
                (found(), false),
            ],
        },
        Probe {
            label: "a halt at a not-implemented branch",
            run: session(String::new(), Some(LOOKUP_BRANCH), None),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::Unimplemented(LOOKUP_BRANCH)),
            rows: vec![
                (boundary(LOOKUP_BRANCH), true),
                (boundary(OTHER_LOOKUP_BRANCH), false),
                (untagged("CP"), false),
                (
                    probe_row("BOUNDARY", &[lookup_finding.as_str()], LOOKUP_BRANCH),
                    false,
                ),
            ],
        },
        Probe {
            label: "a halt at an implemented branch",
            run: session(String::new(), Some(IMPLEMENTED_BRANCH), None),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::Implemented(IMPLEMENTED_BRANCH)),
            rows: CLASSES
                .iter()
                .map(|class| {
                    (
                        probe_row(class, &[finding.as_str()], IMPLEMENTED_BRANCH),
                        true,
                    )
                })
                .chain([
                    (
                        probe_row("CP", &[other_finding.as_str()], OTHER_IMPLEMENTED_BRANCH),
                        false,
                    ),
                    (probe_row("CP", &[finding.as_str()], "none"), false),
                    (untagged("CP"), false),
                    (boundary(IMPLEMENTED_BRANCH), false),
                ])
                .collect(),
        },
        Probe {
            label: "an error",
            run: session(String::new(), None, Some(&error)),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::Failure(error.clone())),
            rows: vec![
                (untagged("CP"), false),
                (untagged("IN"), false),
                (untagged("CR"), false),
                (boundary(LOOKUP_BRANCH), false),
                (found(), false),
            ],
        },
        Probe {
            label: "an error with a halt at an implemented branch",
            run: session(String::new(), Some(IMPLEMENTED_BRANCH), Some(&error)),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::Failure(error.clone())),
            rows: vec![(found(), false)],
        },
        Probe {
            label: "an error with a halt at a not-implemented branch",
            run: session(String::new(), Some(LOOKUP_BRANCH), Some(&error)),
            bytes: None,
            expect: Expect::Exactly(corpus::Outcome::Failure(error.clone())),
            rows: vec![(boundary(LOOKUP_BRANCH), false)],
        },
        Probe {
            label: "no K14 exit newline",
            run: session(unterminated, None, None),
            bytes: None,
            expect: Expect::AnyFailure,
            rows: vec![(untagged("CP"), false), (found(), false)],
        },
        Probe {
            label: "one byte under the output limit",
            run: session(sized_transcript(limit - 1), None, None),
            bytes: Some(limit - 1),
            expect: Expect::Exactly(corpus::Outcome::End),
            rows: vec![(untagged("CP"), true)],
        },
        Probe {
            label: "at the output limit",
            run: session(sized_transcript(limit), None, None),
            bytes: Some(limit),
            expect: Expect::AnyFailure,
            rows: vec![(untagged("CP"), false)],
        },
        Probe {
            label: "one byte over the output limit",
            run: session(sized_transcript(limit + 1), None, None),
            bytes: Some(limit + 1),
            expect: Expect::AnyFailure,
            rows: vec![(untagged("CP"), false)],
        },
        Probe {
            label: "at the output limit with a halt at an implemented branch",
            run: session(sized_transcript(limit), Some(IMPLEMENTED_BRANCH), None),
            bytes: Some(limit),
            expect: Expect::AnyFailure,
            rows: vec![(found(), false)],
        },
        Probe {
            label: "at the output limit with a halt at a not-implemented branch",
            run: session(sized_transcript(limit), Some(LOOKUP_BRANCH), None),
            bytes: Some(limit),
            expect: Expect::AnyFailure,
            rows: vec![(boundary(LOOKUP_BRANCH), false)],
        },
    ]
}

/// Every committed case input is the model's, runs on world B through
/// `inquire_traced` exactly as `corpus::run_case` runs it, and ends as its
/// class says (`SPEC.md` "Parity corpus", offline gate); the class counts,
/// the class moves, the K17 moves and the CR inputs are as specified. The
/// classifier and `judge` are also probed with the in-memory sessions of
/// `classifier_probes`, so that `Implemented`, every `Failure` cause, the
/// finding tag, the K14 exit newline and the exact output limit are
/// exercised although no committed case reaches them.
#[test]
fn gate_every_case_meets_its_class() {
    let gate = gate();
    let folder = golden::corpus_dir();
    let mut failures = Vec::new();
    if gate.manifest.len() != gate.model.cases.len() {
        failures.push(format!(
            "the manifest has {} rows, the model {} cases",
            gate.manifest.len(),
            gate.model.cases.len()
        ));
    }
    let mut inputs_by_case: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (row, case) in gate.manifest.iter().zip(&gate.model.cases) {
        let id = row.case.as_str();
        for difference in row_differences(row, case) {
            failures.push(format!("{id}: {difference}"));
        }
        let text = match golden::read_corpus_input(&folder, id) {
            Ok(text) => text,
            Err(cause) => {
                failures.push(format!("{id}: committed input: {cause}"));
                continue;
            }
        };
        let inputs = golden::split_input_lines(&text);
        if inputs != case.inputs {
            failures.push(format!(
                "{id}: the committed input lines {inputs:?} are not the model's {:?}",
                case.inputs
            ));
        }
        let (result, branch) = inquire_traced(&gate.world_b, &inputs);
        let run = corpus::run_case(&gate.world_b, &inputs);
        let error = result.as_ref().err().map(ToString::to_string);
        let transcript = result
            .as_ref()
            .map_or("", |output| output.transcript.as_str());
        let agrees = run.branch == branch
            && run.error == error
            && run.transcript == transcript
            && (error.is_none() || run.transcript.is_empty());
        if !agrees {
            failures.push(format!(
                "{id}: corpus::run_case (branch {:?}, error {:?}, {} bytes) disagrees with \
                 inquire_traced (branch {branch:?}, error {error:?}, {} bytes)",
                run.branch,
                run.error,
                run.transcript.len(),
                transcript.len()
            ));
        }
        let outcome = corpus::classify(&run, &gate.not_implemented);
        if let Err(cause) = judge(
            row,
            &outcome,
            error.as_deref(),
            branch,
            transcript,
            &gate.not_implemented,
        ) {
            failures.push(format!(
                "{id} class {} tags [{}]: {cause}",
                row.class,
                row.tags_text()
            ));
        }
        inputs_by_case.insert(id, inputs);
    }

    let classes = tally(gate.manifest.iter().map(|row| row.class.as_str()));
    let count = |class: &str| classes.get(class).copied().unwrap_or(0);
    let ce_rows: Vec<&Row> = gate
        .manifest
        .iter()
        .filter(|row| row.case.starts_with("CE-"))
        .collect();
    if count("CP") != CP_COUNT || count("CR") != CR_COUNT {
        failures.push(format!(
            "class counts {classes:?}: {CP_COUNT} CP and {CR_COUNT} CR are required"
        ));
    }
    if ce_rows.len() != CE_COUNT || count("IN") + count("BOUNDARY") != CE_COUNT {
        failures.push(format!(
            "{} CE rows and {} IN plus BOUNDARY rows, {CE_COUNT} of each required",
            ce_rows.len(),
            count("IN") + count("BOUNDARY")
        ));
    }
    for row in &gate.manifest {
        let allowed: &[&str] = match row.case.get(..3) {
            Some("CP-") => &["CP"],
            Some("CE-") => &["IN", "BOUNDARY"],
            Some("CR-") => &["CR"],
            _ => &[],
        };
        if !allowed.contains(&row.class.as_str()) {
            failures.push(format!(
                "{}: class {} does not fit its id",
                row.case, row.class
            ));
        }
    }

    let specified = |class: corpus::Class| {
        gate.model
            .cases
            .iter()
            .filter(|case| case.id.starts_with("CE-") && case.specified == class)
            .count()
    };
    let specified_counts = (
        specified(corpus::Class::In),
        specified(corpus::Class::Boundary),
    );
    if specified_counts != CE_SPECIFIED {
        failures.push(format!(
            "the model specifies {specified_counts:?} CE cases as (IN, BOUNDARY), not \
             {CE_SPECIFIED:?}"
        ));
    }
    for (row, case) in gate.manifest.iter().zip(&gate.model.cases) {
        if !case.id.starts_with("CE-") {
            continue;
        }
        let moved: Vec<&String> = row
            .tags
            .iter()
            .filter(|tag| tag.starts_with("moved="))
            .collect();
        if case.class == case.specified {
            if !moved.is_empty() {
                failures.push(format!("{}: unmoved, but tagged {moved:?}", row.case));
            }
            continue;
        }
        let tag = format!("moved={}>{}", case.specified.label(), case.class.label());
        if moved != [&tag] {
            failures.push(format!("{}: tagged {moved:?}, not [{tag}]", row.case));
        }
        let branch = case.expected_branch.as_deref().unwrap_or("none");
        let listed = gate.model.moves.iter().any(|change| {
            change.case == case.id
                && change.from == case.specified
                && change.to == case.class
                && change.branch == branch
        });
        if !listed {
            failures.push(format!(
                "{}: moved {} to {} at {branch}, which the model's moves do not list",
                row.case,
                case.specified.label(),
                case.class.label()
            ));
        }
    }
    for change in &gate.model.moves {
        let consistent = gate.model.cases.iter().any(|case| {
            case.id == change.case
                && case.specified == change.from
                && case.class == change.to
                && case.expected_branch.as_deref().unwrap_or("none") == change.branch
        });
        if !consistent {
            failures.push(format!(
                "the model's move {change:?} does not match its case"
            ));
        }
    }
    for id in MOVED_TO_BOUNDARY {
        let row = gate.manifest.iter().find(|row| row.case == id);
        let at_lookup = row.is_some_and(|row| {
            row.class == "BOUNDARY"
                && row.expected_branch == LOOKUP_BRANCH
                && row.tags.iter().any(|tag| tag == "moved=IN>BOUNDARY")
        });
        if !at_lookup {
            failures.push(format!(
                "{id}: not BOUNDARY at {LOOKUP_BRANCH} with moved=IN>BOUNDARY (SPEC.md K17)"
            ));
        }
    }
    for (id, gc, inputs) in corpus::CR_INPUTS {
        let gc_tag = format!("gc={gc}");
        let tagged = gate
            .manifest
            .iter()
            .any(|row| row.case == id && row.class == "CR" && row.tags.contains(&gc_tag));
        if !tagged {
            failures.push(format!("{id}: no CR row tagged {gc_tag}"));
        }
        let committed = inputs_by_case.get(id);
        if committed.is_none_or(|lines| lines.iter().map(String::as_str).ne(inputs.iter().copied()))
        {
            failures.push(format!(
                "{id}: the committed inputs {committed:?} are not corpus::CR_INPUTS {inputs:?}"
            ));
        }
    }

    for id in [LOOKUP_BRANCH, OTHER_LOOKUP_BRANCH] {
        if !gate.not_implemented.iter().any(|known| known == id) {
            failures.push(format!(
                "probe precondition: {id} is not a not-implemented branch of SPEC.md \
                 \"Branch inventory\""
            ));
        }
    }
    for id in [IMPLEMENTED_BRANCH, OTHER_IMPLEMENTED_BRANCH] {
        if !inventory_lists(id) || gate.not_implemented.iter().any(|known| known == id) {
            failures.push(format!(
                "probe precondition: {id} is not an implemented branch of SPEC.md \
                 \"Branch inventory\""
            ));
        }
    }
    for probe in classifier_probes() {
        let run = &probe.run;
        let label = format!(
            "probe {} (branch {:?}, error {:?}, {} bytes)",
            probe.label,
            run.branch,
            run.error,
            run.transcript.len()
        );
        if let Some(bytes) = probe.bytes
            && (run.transcript.len() != bytes || !run.transcript.ends_with(EXIT_TAIL))
        {
            failures.push(format!(
                "{label}: the transcript is not {bytes} bytes ending with {EXIT_TAIL:?}"
            ));
        }
        let outcome = corpus::classify(run, &gate.not_implemented);
        if !probe.expect.admits(&outcome) {
            failures.push(format!(
                "{label}: corpus::classify gave {outcome:?}, not {:?}",
                probe.expect
            ));
        }
        for (row, accept) in &probe.rows {
            let judged = judge(
                row,
                &outcome,
                run.error.as_deref(),
                run.branch,
                &run.transcript,
                &gate.not_implemented,
            );
            if judged.is_ok() != *accept {
                failures.push(format!(
                    "{label}: judge of the {} row tagged [{}] expecting {} gave {judged:?}, not {}",
                    row.class,
                    row.tags_text(),
                    row.expected_branch,
                    if *accept { "Ok" } else { "Err" }
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", report("case classes", &failures));
}

// ---------------------------------------------------------------------------
// The fixture profile and the references
// ---------------------------------------------------------------------------

/// The fixture profile and reference violations of order `ien` of
/// `world_b`, each reference resolved in `world_a`, read node by node.
/// Empty is allowed only for the service, ship-to, source code, unit and
/// agent, the references whose rotation lists hold an empty value.
fn profile_recheck(world_a: &Store, world_b: &Store, ien: &str) -> Vec<String> {
    let mut out = Vec::new();
    let order = key("PRC", &["442", ien]);
    let node = |sub: &str| under(&order, &[sub]);
    let at = |sub: &str, position: usize| field(world_b, &node(sub), position);
    let mut fail = |rule: String| out.push(format!("order {ien}: {rule}"));

    for sub in ["0", "1"] {
        if world_b.get(&node(sub)).is_none() {
            fail(format!("node {sub} is absent"));
        }
    }
    for sub in ["11", "5", "3", "15", "4", "13", "6", "24"] {
        if world_b.data(&node(sub)) != 0 {
            fail(format!("node {sub} is present"));
        }
    }
    let mop = at("0", 2);
    if !MOPS.contains(&mop) || !resolves(world_a, "PRCD", "442.5", mop) {
        fail(format!(
            "node 0 piece 2, the method of processing {mop:?}, is not a 442.5 entry below 10"
        ));
    }
    if !at("1", 12).is_empty() {
        fail("node 1 piece 12, field 5.3, is not empty".to_string());
    }
    if world_b.data(&node("23")) != 0 {
        for position in [7, 8, 22] {
            if !at("23", position).is_empty() {
                fail(format!("node 23 piece {position} is not empty"));
            }
        }
        if !["P", "S", "D"].contains(&at("23", 11)) {
            fail(format!(
                "node 23 piece 11 {:?} is not P, S or D",
                at("23", 11)
            ));
        }
    }
    if !at("7", 3).is_empty() {
        fail("node 7 piece 3, field .08, is not empty".to_string());
    }
    let status = at("7", 1);
    if !resolves(world_a, "PRCD", "442.3", status) {
        fail(format!("status {status:?} does not resolve in 442.3"));
    }

    let items = node("2");
    let item_entries = entries(world_b, &items);
    if item_entries.is_empty() {
        fail("the order has no item".to_string());
    }
    if world_b.data(&under(&items, &["AC"])) != 0 {
        fail("the item index AC is present".to_string());
    }
    for j in &item_entries {
        let item = under(&items, &[j]);
        if world_b.get(&under(&item, &["0"])).is_none() {
            fail(format!("item {j} has no zero node"));
        }
        let description = under(&item, &["1"]);
        let line = world_b.get(&under(&description, &["1", "0"]));
        let one_line = children(world_b, &description) == ["0", "1"]
            && world_b.get(&under(&description, &["0"])).is_some()
            && children(world_b, &under(&description, &["1"])) == ["0"];
        match line {
            Some(text) if one_line && text.chars().count() <= 32 && !text.contains('|') => {}
            _ => fail(format!(
                "item {j}: the description is not one header and one line of at most 32 \
                 characters without |"
            )),
        }
        let node_2 = under(&item, &["2"]);
        if world_b.get(&node_2).is_none() {
            fail(format!("item {j} node 2 is absent"));
        }
        for position in [2, 8, 9, 11] {
            if !field(world_b, &node_2, position).is_empty() {
                fail(format!("item {j} node 2 piece {position} is not empty"));
            }
        }
        if world_b.data(&under(&item, &["3"])) != 0 {
            fail(format!("item {j} node 3 is present"));
        }
        let unit = field(world_b, &under(&item, &["0"]), 3);
        if !unit.is_empty() && !resolves(world_a, "PRCD", "420.5", unit) {
            fail(format!("item {j} unit {unit} does not resolve in 420.5"));
        }
    }

    let vendor = at("1", 1);
    if !resolves(world_a, "PRC", "440", vendor) {
        fail(format!("vendor {vendor:?} does not resolve in 440"));
    } else {
        if field(world_a, &key("PRC", &["440", vendor, "3"]), 2) == "Y" {
            fail(format!("vendor {vendor} is EDI (node 3 piece 2 is Y)"));
        }
        let state = field(world_a, &key("PRC", &["440", vendor, "0"]), 7);
        if !state.is_empty() && !resolves(world_a, "DIC", "5", state) {
            fail(format!(
                "vendor {vendor} state {state} does not resolve in 5"
            ));
        }
    }
    let service = at("1", 2);
    if !service.is_empty() && !resolves(world_a, "DIC", "49", service) {
        fail(format!("service {service} does not resolve in 49"));
    }
    let ship_to = at("1", 3);
    if !ship_to.is_empty()
        && world_a
            .get(&key("PRC", &["411", STATION, "1", ship_to, "0"]))
            .is_none()
    {
        fail(format!(
            "ship-to {ship_to} does not resolve under station {STATION}"
        ));
    }
    let source = at("1", 7);
    if !source.is_empty() && !resolves(world_a, "PRCD", "420.8", source) {
        fail(format!("source code {source} does not resolve in 420.8"));
    }
    let authorities = node("14");
    for j in entries(world_b, &authorities) {
        let authority = world_b.get_or_empty(&under(&authorities, &[&j, "0"]));
        if !resolves(world_a, "PRC", "442.4", &authority) {
            fail(format!(
                "authority {j} ({authority:?}) does not resolve in 442.4"
            ));
        }
    }
    let agent = at("1", 10);
    if !agent.is_empty() && agent != ".5" {
        fail(format!(
            "node 1 piece 10, the agent {agent:?}, is neither .5 nor empty"
        ));
    }
    out
}

/// Every corpus order meets the fixture profile of `SPEC.md` "Parity
/// corpus", by `corpus::profile_violations` and node by node, and every
/// reference it holds resolves in world A.
#[test]
fn gate_profile_and_references() {
    let gate = gate();
    let (world_a, world_b) = (&gate.world_a, &gate.world_b);
    let iens = corpus_iens();
    let mut failures = Vec::new();
    for ien in &iens {
        for violation in corpus::profile_violations(world_a, world_b, ien) {
            failures.push(format!("corpus::profile_violations: {violation}"));
        }
        failures.extend(profile_recheck(world_a, world_b, ien));
    }

    for (name, store) in [("world A", world_a), ("world B", world_b)] {
        let header = field(store, &key("PRC", &["442", "0"]), 2);
        if header != "442I" {
            failures.push(format!(
                "{name}: ^PRC(442,0) piece 2 is {header:?}, not 442I"
            ));
        }
        if store.data(&key("PRC", &["442.8", "B"])) != 0 {
            failures.push(format!("{name}: ^PRC(442.8,\"B\") is present"));
        }
    }
    if !resolves(world_a, "VA", "200", ".5") {
        failures.push("world A: ^VA(200,.5,0) is absent".to_string());
    }
    let corpus_ien_set: BTreeSet<&str> = iens.iter().map(String::as_str).collect();
    let file = key("PRC", &["442"]);
    for index in ["M", "MCH", "G"] {
        let root = under(&file, &[index]);
        for (node, _) in world_b.iter() {
            let Some(rest) = node.strip_prefix(root.as_slice()) else {
                continue;
            };
            if rest.iter().any(|sub| corpus_ien_set.contains(sub.text())) {
                failures.push(format!("{} names a corpus order", Store::format_ref(node)));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        report("fixture profile and references", &failures)
    );
}

// ---------------------------------------------------------------------------
// C values and miss strings
// ---------------------------------------------------------------------------

/// The second input line of case `id`'s committed input, if it has one.
fn second_input(id: &str) -> Result<String, String> {
    let text = golden::read_corpus_input(&golden::corpus_dir(), id)?;
    golden::split_input_lines(&text)
        .into_iter()
        .nth(1)
        .ok_or_else(|| format!("{id}: the input has no second line"))
}

/// The CP `C` values are `CP` and three digits, distinct, and filed in
/// `corpus.zwr` under their orders; none is a prefix of another or of a GC
/// or world A `C` value, or the reverse; no `C` value of world B begins
/// with a miss string; and the long edge answers have their lengths.
#[test]
fn gate_c_values_and_miss_strings() {
    let gate = gate();
    let mut failures = Vec::new();
    let mut manifest_c: BTreeMap<&str, &str> = BTreeMap::new();
    for row in gate.manifest.iter().filter(|row| row.class == "CP") {
        let n = cp_number(&row.case, "CP-");
        let expected_c = n.map(|n| format!("CP{n:03}"));
        let expected_ien = n.map(|n| (IEN_BASE + n).to_string());
        if cp_number(&row.c_value, "CP").is_none()
            || expected_c.as_deref() != Some(row.c_value.as_str())
        {
            failures.push(format!(
                "{}: C value {:?} is not CP and the case's three digits",
                row.case, row.c_value
            ));
        }
        if expected_ien.as_deref() != Some(row.order_ien.as_str()) {
            failures.push(format!(
                "{}: order IEN {:?} is not {IEN_BASE} plus the case number",
                row.case, row.order_ien
            ));
        }
        if manifest_c.insert(&row.c_value, &row.order_ien).is_some() {
            failures.push(format!("{}: C value {} is repeated", row.case, row.c_value));
        }
    }
    if manifest_c.len() != CP_COUNT {
        failures.push(format!(
            "{} distinct CP C values, not {CP_COUNT}",
            manifest_c.len()
        ));
    }

    let c_index = key("PRC", &["442", "C"]);
    let mut filed: BTreeMap<&str, &str> = BTreeMap::new();
    for (node, value) in gate.corpus_nodes.iter() {
        let Some(rest) = node.strip_prefix(c_index.as_slice()) else {
            continue;
        };
        match rest {
            [c_value, ien] if value.is_empty() => {
                if filed.insert(c_value.text(), ien.text()).is_some() {
                    failures.push(format!("C value {} is filed twice", c_value.text()));
                }
            }
            _ => failures.push(format!(
                "{}={value:?} is not an index C entry ^PRC(442,\"C\",value,ien)=\"\"",
                Store::format_ref(node)
            )),
        }
    }
    if filed != manifest_c {
        failures.push(
            "the C entries of corpus.zwr are not the manifest's CP C values and orders".to_string(),
        );
    }

    let mut others: BTreeSet<String> = GC_C_VALUES.iter().map(|value| value.to_string()).collect();
    others.extend(children(&gate.world_a, &c_index));
    let values: Vec<&str> = manifest_c.keys().copied().collect();
    for (at, first) in values.iter().enumerate() {
        for second in values.iter().skip(at + 1) {
            if first.starts_with(second) || second.starts_with(first) {
                failures.push(format!(
                    "C values {first} and {second}: one prefixes the other"
                ));
            }
        }
        for other in &others {
            if first.starts_with(other.as_str()) || other.starts_with(first) {
                failures.push(format!(
                    "C value {first} and the GC or world A C value {other}: one prefixes the \
                     other"
                ));
            }
        }
    }
    for violation in corpus::c_value_violations(&gate.world_a, &gate.world_b) {
        failures.push(format!("corpus::c_value_violations: {violation}"));
    }

    let thirty_q = "Q".repeat(30);
    let overlong = "Z".repeat(251);
    let miss_strings: [&str; 6] = [
        "ZZ9",
        "NO-SUCH.ORDER",
        thirty_q.as_str(),
        "12345",
        "gc9999",
        "991001",
    ];
    if corpus::MISS_STRINGS != miss_strings {
        failures.push(format!(
            "corpus::MISS_STRINGS {:?} is not {miss_strings:?}",
            corpus::MISS_STRINGS
        ));
    }
    for value in children(&gate.world_b, &c_index) {
        let upper = value.to_ascii_uppercase();
        for miss in miss_strings {
            if value.starts_with(miss) || upper.starts_with(&miss.to_ascii_uppercase()) {
                failures.push(format!(
                    "world B C value {value} begins with the miss string {miss}"
                ));
            }
        }
    }
    for (id, expected, length) in [("CE-07", &thirty_q, 30), ("CE-26", &overlong, 251)] {
        match second_input(id) {
            Ok(line) if line == *expected && line.chars().count() == length => {}
            Ok(line) => failures.push(format!(
                "{id}: the second input is {} characters {:?}, not {length} of {:?}",
                line.chars().count(),
                line.get(..40).unwrap_or(line.as_str()),
                expected.get(..1).unwrap_or("")
            )),
            Err(cause) => failures.push(cause),
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        report("C values and miss strings", &failures)
    );
}

// ---------------------------------------------------------------------------
// Variation minimums
// ---------------------------------------------------------------------------

/// The minimum checks of the variation matrix, with the model's dropped
/// variants: a dropped value has left its list, so no minimum counts it,
/// and a minimum over distinct values falls by one per value dropped from
/// its dimension.
struct Minimums<'a> {
    drops: &'a [corpus::DroppedVariant],
    failures: Vec<String>,
}

impl Minimums<'_> {
    /// Whether `value` was dropped from `dimension`.
    fn dropped(&self, dimension: &str, value: &str) -> bool {
        self.drops
            .iter()
            .any(|drop| drop.dimension == dimension && drop.value == tag_form(value))
    }

    /// Each value of `values` not dropped occurs at least `minimum` times in
    /// `counts`.
    fn each(
        &mut self,
        dimension: &str,
        counts: &BTreeMap<&str, usize>,
        values: &[&str],
        minimum: usize,
        unit: &str,
    ) {
        for value in values {
            let found = counts.get(value).copied().unwrap_or(0);
            if !self.dropped(dimension, value) && found < minimum {
                self.failures.push(format!(
                    "{dimension}={}: {found} {unit}, minimum {minimum}",
                    tag_form(value)
                ));
            }
        }
    }

    /// `found` distinct values of `dimension` are at least `minimum`, less
    /// one per value dropped from it.
    fn distinct(&mut self, dimension: &str, found: usize, minimum: usize) {
        let dropped = self
            .drops
            .iter()
            .filter(|drop| drop.dimension == dimension)
            .count();
        let needed = minimum.saturating_sub(dropped);
        if found < needed {
            self.failures.push(format!(
                "{dimension}: {found} distinct values, minimum {needed}"
            ));
        }
    }

    /// Records `rule` as failed unless `held`.
    fn require(&mut self, held: bool, rule: String) {
        if !held {
            self.failures.push(rule);
        }
    }
}

/// The new value a malformed copy of the corpus gives one node of an order,
/// from the order IEN, the node's subscripts below `^PRC(442,ien)` and its
/// committed value; `None` keeps the committed value.
type NodeMutation = Box<dyn Fn(&str, &[&str], &str) -> Option<String>>;

/// A malformed copy of the committed corpus that breaks one variation
/// minimum, which `corpus::minimum_violations` must then report.
struct MinimumProbe {
    /// The minimum the copy breaks, named in the probe's failures.
    name: &'static str,
    /// How the copy differs from the committed corpus.
    mutate: NodeMutation,
    /// The starts of the violations the copy must give: each starts at
    /// least one violation, and every violation starts with one of them.
    violations: Vec<String>,
    /// A dropped variant, its dimension and its value in tag form, that
    /// exempts the broken minimum: once the model's drops list it, the copy
    /// gives no violation.
    exempt: Option<(&'static str, &'static str)>,
}

impl MinimumProbe {
    fn new(
        name: &'static str,
        violations: &[&str],
        mutate: impl Fn(&str, &[&str], &str) -> Option<String> + 'static,
    ) -> MinimumProbe {
        MinimumProbe {
            name,
            mutate: Box::new(mutate),
            violations: violations.iter().map(|start| start.to_string()).collect(),
            exempt: None,
        }
    }
}

/// The order IEN of a corpus node `^PRC(442,ien,...)` and the node's
/// subscripts below the order, or `None` for an index node such as
/// `^PRC(442,"C",value,ien)`.
fn order_node(key: &[Sub]) -> Option<(&str, Vec<&str>)> {
    let [global, file, ien, below @ ..] = key else {
        return None;
    };
    (global.text() == "PRC" && file.text() == "442" && ien.is_num())
        .then(|| (ien.text(), below.iter().map(Sub::text).collect()))
}

/// `node` with its 1-based `^`-piece `position` set to `value`, padded with
/// empty pieces when it has fewer.
fn with_piece(node: &str, position: usize, value: &str) -> String {
    let mut pieces: Vec<&str> = node.split('^').collect();
    if pieces.len() < position {
        pieces.resize(position, "");
    }
    if let Some(slot) = position.checked_sub(1).and_then(|at| pieces.get_mut(at)) {
        *slot = value;
    }
    pieces.join("^")
}

/// The committed corpus nodes changed by `mutate`, parsed by
/// `Store::from_zwr` from ZWR text whose every value is quoted with each
/// `"` doubled, as `corpus::world_b` writes values; with the number of
/// nodes `mutate` changed.
///
/// The store stands for world B: `corpus::minimum_violations` reads world
/// B only below the corpus orders `^PRC(442,ien)`, which world A never
/// holds, and reads the reference files and the vendors from world A. So
/// the corpus nodes alone give it the same facts, without re-parsing world
/// A's nodes for every probe.
fn malformed_corpus(gate: &Gate, mutate: &NodeMutation) -> Result<(Store, usize), String> {
    let mut text = String::new();
    for header in ZWR_HEADER {
        text.push_str(header);
        text.push('\n');
    }
    let mut changed = 0;
    for (key, value) in gate.corpus_nodes.iter() {
        let new = order_node(key)
            .and_then(|(ien, below)| mutate(ien, &below, value))
            .filter(|new| new != value);
        changed += usize::from(new.is_some());
        text.push_str(&Store::format_ref(key));
        text.push_str("=\"");
        text.push_str(&new.as_deref().unwrap_or(value).replace('"', "\"\""));
        text.push_str("\"\n");
    }
    let store = Store::from_zwr(&text).map_err(|error| error.to_string())?;
    Ok((store, changed))
}

/// The malformed copies of the corpus, one for each variation minimum that
/// `gate_variation_minimums` does not recount from the world B nodes, each
/// breaking that minimum alone (`SPEC.md` "Parity corpus" names the nodes
/// and pieces). The one-vendor copy leaves unheld the 8 properties its
/// vendor lacks, and so also leaves 1 distinct vendor of the 40 required.
/// The date copy gives order `first` (CP-001), whose delivery date is
/// `delivery_date`, a different P.O. date. A copy changes values only:
/// every changed value is printable ASCII without `|`, and no index node
/// changes.
fn minimum_probes(first: &str, vendor: &str, delivery_date: &str) -> Vec<MinimumProbe> {
    let first = first.to_string();
    let vendor = vendor.to_string();
    let po_date = if delivery_date == "2990101" {
        "2991231"
    } else {
        "2990101"
    };
    let date_violation = format!(
        "date: order {first} P.O. date 1;15 {} differs from its delivery date 0;10 {}",
        tag_form(po_date),
        tag_form(delivery_date)
    );
    let truncated = |text: &str, length: usize| text.chars().take(length).collect::<String>();
    vec![
        MinimumProbe::new(
            "source 1 replaced by 2",
            &["source=1: 0 orders, minimum 1"],
            |_, below, value| match below {
                ["1"] if piece(value, 7) == "1" => Some(with_piece(value, 7, "2")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "one vendor for every order",
            &[
                "vendor: no order's vendor has the property ",
                "vendor: 1 distinct values, minimum 40",
            ],
            move |_, below, value| match below {
                ["1"] => Some(with_piece(value, 1, &vendor)),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no delivery location",
            &["delivery=present: 0 orders, minimum 8"],
            |_, below, value| match below {
                ["1"] => Some(with_piece(value, 11, "")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no confirmation of a verbal order",
            &["verbal=confirmation: 0 orders, minimum 6"],
            |_, below, value| match below {
                ["1"] if piece(value, 4) == "Y" && piece(value, 5) == "Y" => {
                    Some(with_piece(value, 5, ""))
                }
                _ => None,
            },
        ),
        MinimumProbe {
            exempt: Some(("fob", "D")),
            ..MinimumProbe::new(
                "FOB D replaced by O",
                &["fob=D: 0 orders, minimum 1"],
                |_, below, value| match below {
                    ["1"] if piece(value, 6) == "D" => Some(with_piece(value, 6, "O")),
                    _ => None,
                },
            )
        },
        MinimumProbe::new(
            "proposals of 33 characters cut to 32",
            &["proposal=33: 0 orders, minimum 1"],
            move |_, below, value| match below {
                ["1"] if piece(value, 8).chars().count() == 33 => {
                    Some(with_piece(value, 8, &truncated(piece(value, 8), 32)))
                }
                _ => None,
            },
        ),
        MinimumProbe::new(
            "a P.O. date that differs from the delivery date",
            &[&date_violation],
            move |ien, below, value| match below {
                ["1"] if ien == first => Some(with_piece(value, 15, po_date)),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "APPs of 10 digits cut to 7",
            &["app: 3 distinct values, minimum 4"],
            move |_, below, value| match below {
                ["0"] if piece(value, 4).chars().count() == 10 => {
                    Some(with_piece(value, 4, &truncated(piece(value, 4), 7)))
                }
                _ => None,
            },
        ),
        MinimumProbe::new(
            "descriptions of 32 characters cut to 31",
            &["item.desc=32: 0 items, minimum 1"],
            move |_, below, value| match below {
                ["2", _, "1", "1", "0"] if value.chars().count() == 32 => {
                    Some(truncated(value, 31))
                }
                _ => None,
            },
        ),
        MinimumProbe::new(
            "decimal quantities replaced by 1",
            &["item.qty=decimal: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] if piece(value, 2).contains('.') => Some(with_piece(value, 2, "1")),
                _ => None,
            },
        ),
        MinimumProbe {
            exempt: Some(("item.unit", "1")),
            ..MinimumProbe::new(
                "unit 1 replaced by 2",
                &["item.unit: 9 distinct values, minimum 10"],
                |_, below, value| match below {
                    ["2", _, "0"] if piece(value, 3) == "1" => Some(with_piece(value, 3, "2")),
                    _ => None,
                },
            )
        },
        MinimumProbe::new(
            "empty units replaced by 1",
            &["item.unit=empty: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] if piece(value, 3).is_empty() => Some(with_piece(value, 3, "1")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "extended costs over 7 characters replaced by 1",
            &["item.ext=long: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "2"] if piece(value, 1).chars().count() > 7 => {
                    Some(with_piece(value, 1, "1"))
                }
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no STK#",
            &["item.stk=present: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] => Some(with_piece(value, 6, "")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no NSN",
            &["item.nsn=present: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] => Some(with_piece(value, 13, "")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no FOOD GROUP",
            &["item.food=present: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "4"] => Some(with_piece(value, 12, "")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "no Items per",
            &["item.per=present: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] => Some(with_piece(value, 12, "")),
                _ => None,
            },
        ),
        MinimumProbe::new(
            "BOCs of several words cut to the first",
            &["item.boc=several: 0 items, minimum 1"],
            |_, below, value| match below {
                ["2", _, "0"] => piece(value, 4)
                    .split_once(' ')
                    .map(|(word, _)| with_piece(value, 4, word)),
                _ => None,
            },
        ),
    ]
}

/// Every minimum of the variation matrix (`SPEC.md` "Parity corpus") holds
/// by `corpus::minimum_violations`, and the item counts are exact. From the
/// world B nodes the test itself checks only the item counts and the
/// multi-item positions; the type, method of processing, status, ship-to,
/// agent and authority minimums; the distinct vendors, services, totals and
/// dates; the empty service; the year-end, leap-day, empty, 1999 and 2026
/// dates; positive estimated shipping; and the unit cost forms on orders
/// whose type is not `S`. Each other minimum is proved by a malformed copy
/// of the corpus from `minimum_probes` that breaks it: the shared checker
/// must report that minimum and only the violations the probe lists (the
/// one-vendor copy also breaks the distinct-vendor minimum), and for the
/// FOB and unit copies a dropped variant of the removed value must exempt
/// that minimum.
#[test]
fn gate_variation_minimums() {
    let gate = gate();
    let (world_a, world_b) = (&gate.world_a, &gate.world_b);
    let mut check = Minimums {
        drops: &gate.model.drops,
        failures: corpus::minimum_violations(world_a, world_b, &gate.model)
            .into_iter()
            .map(|violation| format!("corpus::minimum_violations: {violation}"))
            .collect(),
    };
    let orders: Vec<(usize, OrderView)> = (1..=CP_COUNT)
        .map(|n| (n, read_order(world_b, &(IEN_BASE + n).to_string())))
        .collect();
    let views = || orders.iter().map(|(_, order)| order);

    let mut item_counts: BTreeMap<usize, usize> = BTreeMap::new();
    for order in views() {
        *item_counts.entry(order.items.len()).or_insert(0) += 1;
    }
    let multi_orders: usize = MULTI_COUNTS.iter().map(|(_, orders)| orders).sum();
    let mut expected_counts: BTreeMap<usize, usize> = MULTI_COUNTS.into_iter().collect();
    expected_counts.insert(1, CP_COUNT - multi_orders);
    check.require(
        item_counts == expected_counts,
        format!("items: the item counts {item_counts:?} are not exactly {expected_counts:?}"),
    );
    let multi_positions: BTreeSet<usize> = (0..multi_orders).map(|k| 4 * k + k % 4 + 1).collect();
    let multi_found: BTreeSet<usize> = orders
        .iter()
        .filter(|(_, order)| order.items.len() > 1)
        .map(|(n, _)| *n)
        .collect();
    check.require(
        multi_found == multi_positions,
        format!("items: the multi-item orders are CP {multi_found:?}, not n = 4k + (k mod 4) + 1"),
    );

    let types = tally(views().map(|order| order.order_type.as_str()));
    check.each("type", &types, &["", "P", "S", "D"], 10, "orders");
    let mops = tally(views().map(|order| order.mop.as_str()));
    check.each("mop", &mops, &MOPS, 5, "orders");

    let status_root = key("PRCD", &["442.3"]);
    let statuses: Vec<String> = entries(world_a, &status_root)
        .into_iter()
        .filter(|ien| world_a.get(&under(&status_root, &[ien, "0"])).is_some())
        .collect();
    let status_zero = |ien: &str| world_a.get_or_empty(&under(&status_root, &[ien, "0"]));
    let low: Vec<&str> = statuses
        .iter()
        .map(String::as_str)
        .filter(|ien| {
            let level = piece(&status_zero(ien), 2).to_string();
            level.is_empty() || m_number(&level) < 10.0
        })
        .collect();
    let long: Vec<&str> = statuses
        .iter()
        .map(String::as_str)
        .filter(|ien| piece(&status_zero(ien), 1).chars().count() > 34)
        .collect();
    let used = tally(views().map(|order| order.status.as_str()));
    let all: Vec<&str> = statuses.iter().map(String::as_str).collect();
    check.each("status", &used, &all, 1, "orders");
    check.each("status", &used, &low, 2, "orders");
    let long_kept = long
        .iter()
        .filter(|ien| !check.dropped("status", ien))
        .count();
    let long_used = long.iter().filter(|ien| used.contains_key(*ien)).count();
    check.require(
        long_used >= long_kept.min(3),
        format!("status: {long_used} statuses named over 34 characters used, minimum 3"),
    );

    let ship_tos = tally(views().map(|order| order.ship_to.as_str()));
    check.each("shipto", &ship_tos, &["1", "2", ""], 8, "orders");
    let agents = tally(views().map(|order| order.agent.as_str()));
    check.each("agent", &agents, &[".5", ""], 5, "orders");
    let authority_counts: Vec<String> =
        views().map(|order| order.authorities.to_string()).collect();
    let authorities = tally(authority_counts.iter().map(String::as_str));
    check.each("authority", &authorities, &["0", "1", "3"], 5, "orders");

    let vendors = tally(views().map(|order| order.vendor.as_str()));
    let named_vendors = vendors.keys().filter(|vendor| !vendor.is_empty()).count();
    check.distinct("vendor", named_vendors, 40);
    let services = tally(views().map(|order| order.service.as_str()));
    let named_services = services
        .keys()
        .filter(|service| !service.is_empty())
        .count();
    check.distinct("service", named_services, 5);
    check.each("service", &services, &[""], 3, "orders");
    let totals = tally(views().map(|order| order.total.as_str()));
    check.distinct("total", totals.len(), 12);

    let dates = tally(views().map(|order| order.date.as_str()));
    check.distinct("date", dates.len(), DATES.len());
    check.each(
        "date",
        &dates,
        &["2991231", "3000229", "3240229", ""],
        1,
        "orders",
    );
    for (shape, prefix) in [("a 1999 date", "299"), ("a 2026 date", "326")] {
        let listed = DATES
            .iter()
            .any(|date| date.starts_with(prefix) && !check.dropped("date", date));
        let found = dates.keys().any(|date| date.starts_with(prefix));
        check.require(!listed || found, format!("date: no order has {shape}"));
    }

    let shipped = views()
        .filter(|order| m_number(&order.shipping) > 0.0)
        .count();
    let shipping_listed = POSITIVE_SHIPPING
        .iter()
        .any(|value| !check.dropped("shipping", value));
    check.require(
        !shipping_listed || shipped >= 10,
        format!("shipping: {shipped} orders with estimated shipping above 0, minimum 10"),
    );

    let off_card_forms: BTreeSet<String> = views()
        .filter(|order| order.order_type != "S")
        .flat_map(|order| order.items.iter().map(|item| cost_form(&item.cost)))
        .collect();
    for form in COST_FORMS {
        let held = check.dropped("item.cost", form) || off_card_forms.contains(form);
        check.require(
            held,
            format!("item.cost={form}: on no item of an order whose type is not S"),
        );
    }

    let first_ien = (IEN_BASE + 1).to_string();
    let first = read_order(world_b, &first_ien);
    for probe in minimum_probes(&first_ien, &first.vendor, &first.date) {
        let name = probe.name;
        let malformed = match malformed_corpus(gate, &probe.mutate) {
            Ok((store, changed)) if changed > 0 => store,
            Ok(_) => {
                check
                    .failures
                    .push(format!("probe {name}: no committed corpus node changes"));
                continue;
            }
            Err(cause) => {
                check.failures.push(format!(
                    "probe {name}: the changed corpus does not parse: {cause}"
                ));
                continue;
            }
        };
        let found = corpus::minimum_violations(world_a, &malformed, &gate.model);
        for start in &probe.violations {
            check.require(
                found.iter().any(|violation| violation.starts_with(start)),
                format!(
                    "probe {name}: corpus::minimum_violations reports no {start:?}, only {found:?}"
                ),
            );
        }
        for violation in &found {
            check.require(
                probe
                    .violations
                    .iter()
                    .any(|start| violation.starts_with(start.as_str())),
                format!(
                    "probe {name}: corpus::minimum_violations also reports {violation:?}, a rule the probe does not break"
                ),
            );
        }
        if let Some((dimension, value)) = probe.exempt {
            // Only the dimension and the value exempt a minimum; the round,
            // case and branch only record where a drop came from.
            let mut model = gate.model.clone();
            model.drops.push(corpus::DroppedVariant {
                round: 1,
                case: "CP-001".to_string(),
                dimension: dimension.to_string(),
                value: value.to_string(),
                branch: gate.not_implemented.first().cloned().unwrap_or_default(),
            });
            let exempted = corpus::minimum_violations(world_a, &malformed, &model);
            check.require(
                exempted.is_empty(),
                format!(
                    "probe {name} with {dimension}={value} dropped: corpus::minimum_violations still reports {exempted:?}"
                ),
            );
        }
    }
    assert!(
        check.failures.is_empty(),
        "{}",
        report("variation minimums", &check.failures)
    );
}

// ---------------------------------------------------------------------------
// The paging bound
// ---------------------------------------------------------------------------

/// No corpus order reaches the paging prompt: its largest `$Y`, computed
/// from its world B nodes, is within `corpus::PAGING_BOUND` (45) and equals
/// `corpus::order_worst_y`; the largest over the corpus is the model's.
#[test]
fn gate_no_paging_by_computation() {
    let gate = gate();
    assert_eq!(
        corpus::PAGING_BOUND,
        45,
        "corpus::PAGING_BOUND is not the bound 45 of SPEC.md \"Parity corpus\""
    );
    let mut failures = Vec::new();
    let mut worst = 0;
    let mut largest = 0;
    for ien in corpus_iens() {
        let order = read_order(&gate.world_b, &ien);
        let y = paging_y(&order);
        if y > corpus::PAGING_BOUND {
            failures.push(format!(
                "order {ien}: $Y reaches {y}, above the bound {}",
                corpus::PAGING_BOUND
            ));
        }
        let shared = corpus::order_worst_y(&gate.world_b, &ien);
        if shared != y {
            failures.push(format!(
                "order {ien}: corpus::order_worst_y gives {shared}, the computation {y}"
            ));
        }
        worst = worst.max(y);
        largest = largest.max(order.items.len());
    }
    if worst != gate.model.worst_y {
        failures.push(format!(
            "the largest $Y is {worst}, the model's worst_y {}",
            gate.model.worst_y
        ));
    }
    if gate.model.largest_item_count != LARGEST_ITEM_COUNT || largest != LARGEST_ITEM_COUNT {
        failures.push(format!(
            "the largest item count is {largest} in world B and {} in the model, not \
             {LARGEST_ITEM_COUNT}",
            gate.model.largest_item_count
        ));
    }
    assert!(failures.is_empty(), "{}", report("paging bound", &failures));
}

// ---------------------------------------------------------------------------
// Determinism and the committed files
// ---------------------------------------------------------------------------

/// Two generations give the same 233 files, and each equals its committed
/// counterpart: `corpus.zwr` and the manifest read whole, each case input
/// read through `golden::read_corpus_input`.
#[test]
fn generator_is_deterministic() {
    let gate = gate();
    let again = match corpus::generate(&gate.world_a, SPEC) {
        Ok(again) => again,
        Err(error) => panic!("the second generation fails: {error}"),
    };
    let first = gate.model.files();
    let second = again.files();
    let expected_files = golden::corpus_case_ids().len() + 2;
    assert_eq!(
        first.len(),
        expected_files,
        "the generation gives {} files, not {expected_files}",
        first.len()
    );
    let unequal: Vec<String> = first
        .iter()
        .zip(&second)
        .filter(|(one, other)| one != other)
        .map(|((path, _), _)| path.display().to_string())
        .collect();
    assert!(
        first.len() == second.len() && unequal.is_empty(),
        "two generations differ: {} and {} files, unequal at {unequal:?}",
        first.len(),
        second.len()
    );
    assert!(gate.model == again, "two generations give different models");

    let mut failures = Vec::new();
    let by_path: BTreeMap<&Path, &str> = first
        .iter()
        .map(|(path, text)| (path.as_path(), text.as_str()))
        .collect();
    if by_path.len() != first.len() {
        failures.push(format!(
            "the {} generated files have {} distinct paths",
            first.len(),
            by_path.len()
        ));
    }
    let zwr_path = corpus::corpus_zwr_path();
    if by_path.get(zwr_path.as_path()) != Some(&gate.corpus_text.as_str()) {
        failures.push(format!(
            "{}: the committed file differs from the generated one",
            zwr_path.display()
        ));
    }
    let folder = golden::corpus_dir();
    let manifest_path = folder.join(MANIFEST_NAME);
    if by_path.get(manifest_path.as_path()) != Some(&gate.manifest_text.as_str()) {
        failures.push(format!(
            "{}: the committed file differs from the generated one",
            manifest_path.display()
        ));
    }
    for id in golden::corpus_case_ids() {
        let path = golden::corpus_input_path(&folder, &id);
        let Some(generated) = by_path.get(path.as_path()) else {
            failures.push(format!("{}: not among the generated files", path.display()));
            continue;
        };
        match golden::read_corpus_input(&folder, &id) {
            Ok(committed) if committed == *generated => {}
            Ok(_) => failures.push(format!(
                "{id}: {} differs from the generated input",
                path.display()
            )),
            Err(cause) => failures.push(format!("{id}: {cause}")),
        }
    }
    if gate.model.render_zwr() != gate.corpus_text {
        failures.push("Corpus::render_zwr differs from the committed corpus.zwr".to_string());
    }
    if gate.model.render_manifest() != gate.manifest_text {
        failures.push("Corpus::render_manifest differs from the committed manifest".to_string());
    }
    assert!(
        failures.is_empty(),
        "{}",
        report("committed generated files", &failures)
    );
}

/// `corpus_gen --check` exits 0, prints the model's summary with the
/// criterion 2 line, and ends with `check: 233 files equal`.
#[test]
fn corpus_gen_check_exits_zero() {
    const SPECIFIED_LINE: &str =
        "specified: 200 CP, 26 CE (16 IN, 10 BOUNDARY), 5 CR, 50 multi-item orders";
    const CHECK_LINE: &str = "check: 233 files equal";
    let gate = gate();
    let output = match Command::new(env!("CARGO_BIN_EXE_corpus_gen"))
        .arg("--check")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("corpus_gen --check cannot run: {error}"),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "corpus_gen --check exited with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stderr.is_empty(),
        "corpus_gen --check wrote to standard error:\n{stderr}"
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.contains(&SPECIFIED_LINE),
        "corpus_gen --check did not print {SPECIFIED_LINE:?}:\n{stdout}"
    );
    assert_eq!(
        lines.last().copied(),
        Some(CHECK_LINE),
        "corpus_gen --check did not end with {CHECK_LINE:?}:\n{stdout}"
    );
    let summary = gate.model.summary();
    let expected: Vec<&str> = summary
        .iter()
        .map(String::as_str)
        .chain([CHECK_LINE])
        .collect();
    assert_eq!(
        lines, expected,
        "corpus_gen --check printed other lines than the model's summary and the check line"
    );
}

// ---------------------------------------------------------------------------
// The two worlds
// ---------------------------------------------------------------------------

/// The violation of corpus node `node` holding `value`, if it is not under
/// `^PRC(442,…)` as an order node of IEN 991001 to 991200, or a `B` or `C`
/// index entry `=""` of a CP order's `500-CPnnn` or `CPnnn` value and IEN.
fn corpus_key_violation(node: &[Sub], value: &str) -> Option<String> {
    let shown = Store::format_ref(node);
    let order_ien = |sub: &Sub| {
        sub.is_num()
            && sub
                .text()
                .parse::<usize>()
                .ok()
                .and_then(|ien| ien.checked_sub(IEN_BASE))
                .is_some_and(|n| (1..=CP_COUNT).contains(&n))
    };
    let [global, file, third, rest @ ..] = node else {
        return Some(format!("{shown}: not under ^PRC(442)"));
    };
    if global.is_num() || global.text() != "PRC" || !file.is_num() || file.text() != "442" {
        return Some(format!("{shown}: not under ^PRC(442)"));
    }
    if third.is_num() {
        return (!order_ien(third) || rest.is_empty())
            .then(|| format!("{shown}: not a node of a corpus order"));
    }
    let entry = match (third.text(), rest) {
        ("B", [number, ien]) => {
            cp_number(number.text(), &format!("{STATION}-CP")).map(|n| (n, ien))
        }
        ("C", [c_value, ien]) => cp_number(c_value.text(), "CP").map(|n| (n, ien)),
        _ => None,
    };
    let filed = entry.is_some_and(|(n, ien)| {
        ien.is_num() && ien.text() == (IEN_BASE + n).to_string() && value.is_empty()
    });
    (!filed).then(|| format!("{shown}: not a B or C index entry of a corpus order"))
}

/// `corpus.zwr` has its header, line ends and characters, and only corpus
/// order and index nodes; world B is world A unchanged plus exactly the
/// corpus nodes; `corpus::world_b` refuses, at the line and with the fixed
/// text it documents, a text that names a key world A holds (quoted
/// aliases included), lacks the corpus header or has no data line; and
/// world A holds no corpus node.
#[test]
fn world_b_keeps_world_a_and_world_a_has_no_corpus() {
    let gate = gate();
    let (world_a, world_b) = (&gate.world_a, &gate.world_b);
    let text = gate.corpus_text.as_str();
    let mut failures = Vec::new();

    let mut lines = text.lines();
    for (number, header) in ZWR_HEADER.iter().enumerate() {
        let line = lines.next();
        if line != Some(*header) {
            failures.push(format!(
                "corpus.zwr line {} is {line:?}, not {header:?}",
                number + 1
            ));
        }
    }
    let data_lines: Vec<&str> = lines.collect();
    if let Some(line) = data_lines.iter().find(|line| !line.starts_with('^')) {
        failures.push(format!("corpus.zwr holds the non-data line {line:?}"));
    }
    if !text.ends_with('\n') || text.ends_with("\n\n") {
        failures.push("corpus.zwr does not end with exactly one \\n".to_string());
    }
    if text.contains("$C(") || text.contains('\r') {
        failures.push("corpus.zwr holds $C( or a carriage return".to_string());
    }
    if !text
        .bytes()
        .all(|byte| byte == b'\n' || (b' '..=b'~').contains(&byte))
    {
        failures.push("corpus.zwr holds a byte that is not printable ASCII".to_string());
    }
    if data_lines.len() != gate.corpus_nodes.len() {
        failures.push(format!(
            "corpus.zwr has {} data lines for {} distinct nodes",
            data_lines.len(),
            gate.corpus_nodes.len()
        ));
    }

    for (node, value) in gate.corpus_nodes.iter() {
        if let Some(violation) = corpus_key_violation(node, value) {
            failures.push(violation);
        }
        if world_a.data(node) != 0 {
            failures.push(format!(
                "{}: the corpus node exists in world A",
                Store::format_ref(node)
            ));
        }
        if world_b.get(node) != Some(value) {
            failures.push(format!(
                "{}: world B does not hold the corpus value",
                Store::format_ref(node)
            ));
        }
    }
    if world_b.len() != world_a.len() + gate.corpus_nodes.len() {
        failures.push(format!(
            "world B holds {} nodes, not world A's {} plus the corpus's {}",
            world_b.len(),
            world_a.len(),
            gate.corpus_nodes.len()
        ));
    }
    let changed: Vec<String> = world_a
        .iter()
        .filter(|(node, value)| world_b.get(node) != Some(*value))
        .map(|(node, _)| Store::format_ref(node))
        .collect();
    if !changed.is_empty() {
        failures.push(format!(
            "world B changes or lacks {} world A nodes, the first {:?}",
            changed.len(),
            changed.first()
        ));
    }

    for node in [
        key("VA", &["200", ".5", "0"]),
        key("PRC", &["442", "0"]),
        key("PRC", &["442", "990001", "0"]),
    ] {
        if world_a.get(&node).is_none() {
            failures.push(format!(
                "world A lacks {}, which the world B probes name",
                Store::format_ref(&node)
            ));
        }
    }
    const HELD: &str = "the key already exists in world A";
    const NO_HEADER: &str = "expected the corpus header line";
    const NO_DATA: &str = "expected a data line after the corpus header";
    let [first_header, _] = ZWR_HEADER;
    let header: String = ZWR_HEADER.iter().map(|line| format!("{line}\n")).collect();
    let data: String = text
        .lines()
        .skip(ZWR_HEADER.len())
        .map(|line| format!("{line}\n"))
        .collect();
    let appended = text.lines().count() + 1;
    let refused: [(&str, String, usize, &str); 17] = [
        (
            "the POSTMASTER node",
            format!("{header}^VA(200,.5,0)=\"EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "the POSTMASTER alias with a quoted IEN",
            format!("{header}^VA(200,\".5\",0)=\"EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "the POSTMASTER alias with every subscript quoted",
            format!("{header}^VA(\"200\",\".5\",\"0\")=\"EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "the file 442 header node",
            format!("{header}^PRC(442,0)=\"EVIL^442I^1^1\"\n"),
            3,
            HELD,
        ),
        (
            "the GC-01 order node",
            format!("{header}^PRC(442,990001,0)=\"500-GC1001^1^EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "the GC-01 order alias with a quoted IEN",
            format!("{header}^PRC(442,\"990001\",0)=\"500-GC1001^1^EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "a key above world A nodes",
            format!("{header}^PRC(442)=\"EVIL\"\n"),
            3,
            HELD,
        ),
        (
            "the committed corpus with the POSTMASTER node appended",
            format!("{text}^VA(200,.5,0)=\"EVIL\"\n"),
            appended,
            HELD,
        ),
        (
            "the committed corpus with the GC-01 order alias appended",
            format!("{text}^PRC(442,\"990001\",0)=\"500-GC1001^1^EVIL\"\n"),
            appended,
            HELD,
        ),
        ("an empty text", String::new(), 1, NO_HEADER),
        (
            "a text with no line beginning ^",
            "EVIL GARBAGE\nMORE EVIL\n".to_string(),
            1,
            NO_HEADER,
        ),
        (
            "the corpus lines without the header",
            data.clone(),
            1,
            NO_HEADER,
        ),
        (
            "another first header line",
            format!("OSEHRA ZGO Export: EVIL\nZWR\n{data}"),
            1,
            NO_HEADER,
        ),
        (
            "another second header line",
            format!("{first_header}\nEVIL\n{data}"),
            2,
            NO_HEADER,
        ),
        (
            "the first header line alone before the data",
            format!("{first_header}\n{data}"),
            2,
            NO_HEADER,
        ),
        ("the header alone", header.clone(), 3, NO_DATA),
        (
            "a third header line",
            format!("{header}EVIL\n{data}"),
            3,
            NO_DATA,
        ),
    ];
    for (what, probe, line, message) in &refused {
        let expected = format!("MALFORMED ZWR LINE {line}: {message}");
        match corpus::world_b(world_a, probe) {
            Ok(_) => failures.push(format!("world B accepts {what}")),
            Err(error) if error.to_string() != expected => failures.push(format!(
                "world B refuses {what} with {:?}, not {expected:?}",
                error.to_string()
            )),
            Err(_) => {}
        }
    }

    for ien in corpus_iens() {
        if world_a.data(&key("PRC", &["442", &ien])) != 0 {
            failures.push(format!("world A holds ^PRC(442,{ien})"));
        }
    }
    for (index, prefix) in [("B", "500-CP"), ("C", "CP")] {
        for value in children(world_a, &key("PRC", &["442", index])) {
            if value.starts_with(prefix) {
                failures.push(format!(
                    "world A holds the index {index} value {value}, which begins {prefix}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        report("the two worlds", &failures)
    );
}
