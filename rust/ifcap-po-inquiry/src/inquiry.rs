//! The port of OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`):
//! one read-only purchase-order display session, from the station prompt
//! through the order prompt, the display and the closing prompt.
//!
//! [`inquire`] is contract PO-01 and the only public operation. Every other
//! contract PO-02 to PO-41 of `SPEC.md` is one private function here, named
//! after its `TAG^ROUTINE` and called in the original's order: `DO` is a
//! call, and fall-through labels and local `GOTO`s are loops or tail calls.
//! MUMPS locals that one tag sets and another reads live in `Locals`;
//! globals are only read, through the borrowed [`Store`]; output goes to the
//! transcript of a `Session`, which never echoes input.
//!
//! A branch that no case exercises, or whose source cannot be settled
//! (BLOCKED), returns `UnsupportedBranch` with its `SPEC.md` branch ID at the
//! point the original decides it, before that branch writes anything. A read
//! of a missing node where M raises `UNDEF` returns `CorruptStore`. No M
//! code is interpreted: the screens and identifiers FileMan XECUTEs are
//! literal source and DD text, reproduced as fixed logic.
//!
//! The original's non-business writes are not reproduced: FileMan's
//! `^DISV(DUZ,...)` after each selection [DIC2.m:65], the word-wrap scratch
//! `^UTILITY($J,"W")` and `^TMP($J,"W")` (held in `Locals` instead), and the
//! kills of `^TMP($J,"PRCHDP6")` and `^TMP($J,"MDEL")`, whose only builders
//! lie on unexercised branches.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use crate::mumps::{
    Session, add, contains, extract, follows, int_div, justify, modulo, num, num_cmp, piece,
    piece_range,
};
use crate::store::{Store, Sub};

// ---------------------------------------------------------------------------
// Public items
// ---------------------------------------------------------------------------

/// Everything one session wrote, and what its entry point returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InquiryOutput {
    /// Everything the original writes, in order, with `\n` line endings and
    /// no echo of input.
    pub transcript: String,
    /// The entry point's return value. Always `None`: the option reaches
    /// `EN3^PRCHRPT` by `DO`, not as an extrinsic function.
    pub returned: Option<String>,
}

/// Why a session could not be reproduced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InquiryError {
    /// A read of a missing node on an exercised path, where the original
    /// raises an M `UNDEF` error. Carries the global reference.
    CorruptStore(String),
    /// A branch that is not exercised (n/e) or is BLOCKED, and so is not
    /// implemented. Carries its `SPEC.md` branch ID, `PO-NN.bM`.
    UnsupportedBranch(String),
}

impl fmt::Display for InquiryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InquiryError::CorruptStore(reference) => write!(f, "CORRUPT STORE {reference}"),
            InquiryError::UnsupportedBranch(id) => write!(f, "UNSUPPORTED BRANCH {id}"),
        }
    }
}

impl std::error::Error for InquiryError {}

/// The fixed context of every session, and the only identity of the
/// inquiry layer: no caller can supply or override it.
///
/// `DUZ` is POSTMASTER, `.5`, as the Kernel test code signs on with
/// `D DUZ^XUP(.5)` [ZOSVGUT1.m:13]. `STATION` is the snapshot's primary
/// station, `$O(^PRC(411,"AC","Y",0))` = 500 [PRCFSITE.m:14]. `IOF` and
/// `IOSL` come from the HOME device that `^%ZIS` opens [ZIS1.m:103-113] and
/// are unresolved (`None`) while open item O-01 stands. Capture contexts are
/// validated against this table key by key.
pub const STUB_CONTEXT: [(&str, Option<&str>); 4] = [
    ("DUZ", Some(".5")),
    ("IOF", None),
    ("IOSL", None),
    ("STATION", Some("500")),
];

/// PO-01 `EN3^PRCHRPT` [PRCHRPT.m:70-71]: runs one session of OPTION
/// `PRCH DISPLAY PO` against `store`, answering its READs from `inputs`,
/// each one typed line without its terminator.
///
/// Input that runs out at a prompt ends the session after the last prompt
/// written, as `Ok`. Lines left over when `EN3` returns are ignored. An
/// unexercised or BLOCKED branch, or a read of a missing node where M raises
/// `UNDEF`, gives `Err` and no transcript. `store` is never written.
pub fn inquire(store: &Store, inputs: &[String]) -> Result<InquiryOutput, InquiryError> {
    let mut s = Session::new(inputs);
    let mut v = Locals::default();
    let result =
        st_prchrpt(&mut s, store, &mut v).and_then(|()| en30_prchrpt(&mut s, store, &mut v));
    match result {
        Ok(()) | Err(Halt::InputExhausted) => Ok(InquiryOutput {
            transcript: s.into_transcript(),
            returned: None,
        }),
        Err(Halt::Error(error)) => Err(error),
    }
}

// ---------------------------------------------------------------------------
// Private infrastructure
// ---------------------------------------------------------------------------

/// How a contract function stops short of returning normally.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Halt {
    /// A READ found no input left. The session ends after the last prompt
    /// written, and `inquire` returns the transcript so far.
    InputExhausted,
    /// The session cannot be reproduced; `inquire` returns the error.
    Error(InquiryError),
}

fn unsupported(id: &str) -> Halt {
    Halt::Error(InquiryError::UnsupportedBranch(id.to_string()))
}

fn corrupt(key: &[Sub]) -> Halt {
    Halt::Error(InquiryError::CorruptStore(Store::format_ref(key)))
}

/// M `READ X:DTIME`: the next input line, or `InputExhausted` when none is
/// left. The Rust READ never times out.
fn read(s: &mut Session) -> Result<String, Halt> {
    s.read().ok_or(Halt::InputExhausted)
}

/// The value of `name` in [`STUB_CONTEXT`]; `None` for an unresolved value.
fn stub_value(name: &str) -> Option<&'static str> {
    STUB_CONTEXT
        .iter()
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| *value)
}

/// The key of `^global(subs...)`, each subscript classified by its text as M
/// does (canonical number text is a number, anything else a string).
fn key(global: &str, subs: &[&str]) -> Vec<Sub> {
    let mut key = Vec::with_capacity(subs.len() + 1);
    key.push(Sub::Str(global.to_string()));
    key.extend(subs.iter().map(|sub| Sub::from_text(sub)));
    key
}

fn child(parent: &[Sub], sub: &Sub) -> Vec<Sub> {
    let mut key = parent.to_vec();
    key.push(sub.clone());
    key
}

/// M truth value: true when the numeric interpretation is not zero, as in
/// `Q:'PRCH` or `I $P(PRCHDI0,U,12)`.
fn truthy(text: &str) -> bool {
    num(text) != "0"
}

/// M `text>0`, as in `Q:PRCHDI'>0`.
fn positive(text: &str) -> bool {
    num_cmp(text, "0") == Ordering::Greater
}

/// The first subscript of `parent` after `0`, the seed every numeric walk of
/// the path starts from.
fn first_after_zero(st: &Store, parent: &[Sub]) -> Option<Sub> {
    st.order(parent, Some(&Sub::Num("0".to_string())))
}

/// A `$D` test followed by a read of the same node, as in
/// `$S($D(^(2)):^(2),1:"")`: `None` when `$D` is 0, the value otherwise. A
/// node holding only descendants passes `$D` but has no value, and the read
/// raises `UNDEF` in M, so that case is `CorruptStore`.
fn defined(st: &Store, key: &[Sub]) -> Result<Option<String>, Halt> {
    if st.data(key) == 0 {
        return Ok(None);
    }
    st.get(key)
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| corrupt(key))
}

/// M `S $P(text,U,1)=value`: `text` with its first `^` piece replaced.
fn set_first_piece(text: &str, value: &str) -> String {
    match text.split_once('^') {
        Some((_, rest)) => format!("{value}^{rest}"),
        None => value.to_string(),
    }
}

/// M `W:$Y>0 @IOF` [PRCHDP1.m:12], and as the postcondition of the whole
/// column-header WRITE [PRCHDP2.m:42]. With `$Y` at 0 nothing is written.
/// Otherwise the form feed runs: the HOME device's `IOF`
/// ([`STUB_CONTEXT`]), the bytes `W @IOF` writes for it and `$Y` after it
/// are all open item O-01, and `Session` models no form feed, so the
/// outcome is the BLOCKED branch `id` whatever value `IOF` is given.
fn form_feed(s: &Session, id: &str) -> Result<(), Halt> {
    if s.y() > 0 {
        return Err(unsupported(id));
    }
    Ok(())
}

/// `DIC("A")` of the station lookup [PRCFSITE.m:18].
const STATION_PROMPT: &str = "Select STATION NUMBER ('^' TO EXIT): ";

/// What the station lookup writes when RETURN accepts the default 500
/// (`SPEC.md` R1): `WO^DIC2` writes two spaces and the untyped rest of the
/// `.01` value, which is empty [DIC2.m:10, 25, 32]; `DIC("W")` writes two
/// spaces [DIC1.m:52-56]; the identifier `^DD(411,0,"ID",.1)` writes two
/// spaces and the file 4 name of `^PRC(411,500,0)` piece 10, `CAMP MASTER`
/// [DD.zwr:488724] [4+INSTITUTION.zwr:494]. No newline follows.
const STATION_ECHO: &str = "      CAMP MASTER";

/// `PRCHP("A")`, the order prompt text [PRCHRPT.m:12].
const ORDER_PROMPT: &str = "P.O./REQ.NO.: ";

/// `DIC("A")` when `PRCHP("A")` is undefined [PRCHPAT.m:81].
const DEFAULT_ORDER_PROMPT: &str = "PURCHASE ORDER: ";

/// `PRCHP("S")`, the default method-of-purchase screen [PRCHRPT.m:9]. The
/// `PRCHPC` and `PRCHDELV` variants [PRCHRPT.m:10-11] need locals that
/// OPTION 1901's empty entry action never sets.
const MOP_SCREEN: &str = "$P(^(0),U,2)<10!($P(^(0),U,2)=25)";

/// Piece 2 of `^PRC(442,0)`, the file attributes the order lookup's
/// identifiers and prompt assume [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3].
const FILE_442_ATTRIBUTES: &str = "442I";

/// The length of index `C`; a longer value skips the index search
/// [DIC3.m:13] [DICUIX.m:96-97].
const LOOKUP_LENGTH: usize = 30;

/// The tab stops `^DIWP` sets when `DIWT` is undefined [DIWP.m:31].
const DEFAULT_TAB_STOPS: &str = "5,10,15,20,25";

/// The pointer-field cross-references of file 442 that collate after `C`,
/// in `^DD(442,0,"IX")` order [DD.zwr:514219-514231], each the tuple (index
/// name, global root of the file it points to, that file's number). Every
/// other index after `C` is on a free-text or numeric field and is searched
/// on the raw value.
const POINTER_INDEXES: [(&str, &str, &str); 6] = [
    // field 5 VENDOR
    ("D", "PRC", "440"),
    // field .02 METHOD OF PROCESSING
    ("F", "PRCD", "442.5"),
    // subfile 442.14 field 11
    ("G", "PRCP", "445"),
    // field 46 PURCHASE CARD NUMBER
    ("M", "PRC", "440.5"),
    // field 61
    ("MCH", "VA", "200"),
    // field .5 SUPPLY STATUS
    ("ST", "PRCD", "442.3"),
];

/// `$G(^(DIWL,n,0))` of the word-wrap scratch lines `lines`
/// ([`Locals`] `utility_w_lines` or `tmp_w_lines`).
fn wrap_line(lines: &BTreeMap<(i64, i64), String>, diwl: i64, n: i64) -> String {
    lines.get(&(diwl, n)).cloned().unwrap_or_default()
}

/// The MUMPS locals of one session that one tag sets and another reads,
/// each named after its M variable. `None` is an undefined local, which is
/// what `KILL` restores. Locals the path sets but never reads are not kept:
/// `PRCHDHSP` [PRCHDP1.m:12], `PRCHAMCT` [PRCHDP2.m:9], `PRCHONL` and
/// `PRCHQUIT` [PRCHDP4.m:5, 8], whose only readers are unexercised; and the
/// `^DIWP` locals `DIWI`, `DIWX`, `DIWTC` and `DIWP`, which every call
/// assigns before reading [DIWP.m:29-32, 43]. Locals a tag NEWs and reads
/// itself are Rust locals of that function.
#[derive(Debug, Default)]
struct Locals {
    /// `PRC("SITE")`.
    prc_site: Option<String>,
    /// `PRCF("X")`, the `^PRCFSITE` flags.
    prcf_x: Option<String>,
    /// `DIC("A")`, the prompt of the lookup about to run.
    dic_a: Option<String>,
    /// `DIC("B")`, the station lookup's default.
    dic_b: Option<String>,
    /// `DIC("S")` of the order lookup, as the fixed rule its literal text
    /// resolves to: `I +$P(^(0),U,1)=PRC("SITE")`, followed by `PRCHP("S")`
    /// when that is defined [PRCHPAT.m:81]. `Some(false)` is the station test
    /// only, `Some(true)` the station test, then [`MOP_SCREEN`]; with `None`
    /// (undefined) every entry passes.
    dic_s: Option<bool>,
    /// `D`, the index the order lookup starts on.
    d: Option<String>,
    /// `PRCHP("A")`.
    prchp_a: Option<String>,
    /// `PRCHP("S")`.
    prchp_s: Option<String>,
    /// `PRCHPO`, the selected order.
    prchpo: Option<String>,
    /// `DA`.
    da: Option<String>,
    /// `D0`, the order on display.
    d0: Option<String>,
    /// `PRCHNRQ`, set for a requisition (MOP 8).
    prchnrq: Option<String>,
    /// `X`.
    x: Option<String>,
    /// `Y`.
    y: Option<String>,
    /// `PRCHD0`, `^PRC(442,D0,0)`.
    prchd0: Option<String>,
    /// `PRCHD1`, `^PRC(442,D0,1)`.
    prchd1: Option<String>,
    /// `PRCHDSIT`, the station part of the order number.
    prchdsit: Option<String>,
    /// `PRCHDST`, `^PRC(411,PRCHDSIT,0)`.
    prchdst: Option<String>,
    /// `PRCHDSHP`, the ship-to node.
    prchdshp: Option<String>,
    /// `PRCHDS`, 1 for a direct-delivery patient address.
    prchds: Option<String>,
    /// `PRCHDV`, the vendor node `^PRC(440,vendor,0)`.
    prchdv: Option<String>,
    /// `PRCHTYPE`, the order type code.
    prchtype: Option<String>,
    /// `PRCHFTYP`, the facility type name.
    prchftyp: Option<String>,
    /// The vendor rows `V(n)`.
    v_rows: BTreeMap<i64, String>,
    /// The ship-to rows `S(n)`.
    s_rows: BTreeMap<i64, String>,
    /// `PRCHAMNT`, NEWed for the display [PRCHDP2.m:9].
    prchamnt: Option<String>,
    /// `PRCHDQ`, 1 once the user halts the display.
    prchdq: Option<String>,
    /// `PRCHDI`, the item on display.
    prchdi: Option<String>,
    /// `PRCHDI0`, the item's zero node.
    prchdi0: Option<String>,
    /// `PRCHDI2`, the item's node 2.
    prchdi2: Option<String>,
    /// `PRCHDCNT`, the item description's wrapped line count.
    prchdcnt: Option<String>,
    /// `N`, the last item's line number.
    n: Option<String>,
    /// `PURPIPE`, NEWed by `ITEM` and set by `PIPECK`.
    purpipe: Option<String>,
    /// `DIWL`, the word-wrap left margin.
    diwl: Option<i64>,
    /// `DIWR`, the word-wrap right margin.
    diwr: Option<i64>,
    /// `DIWF`, the word-wrap flags.
    diwf: Option<String>,
    /// `DIWT`, the tab stops `^DIWP` leaves defined.
    diwt: Option<String>,
    /// `DN`, which `^DIWP` leaves defined.
    dn: Option<String>,
    /// `^UTILITY($J,"W",DIWL)`, the word-wrap scratch's line count per left
    /// margin `DIWL`.
    utility_w_counts: BTreeMap<i64, i64>,
    /// `^UTILITY($J,"W",DIWL,n,0)`, the word-wrap scratch's lines.
    utility_w_lines: BTreeMap<(i64, i64), String>,
    /// `^TMP($J,"W",DIWL)`, the copy `%XY^%RCR` makes of the line counts.
    tmp_w_counts: BTreeMap<i64, i64>,
    /// `^TMP($J,"W",DIWL,n,0)`, the copy `%XY^%RCR` makes of the lines.
    tmp_w_lines: BTreeMap<(i64, i64), String>,
}

// ---------------------------------------------------------------------------
// Session start: PO-02 to PO-05 and PO-41
// ---------------------------------------------------------------------------

/// PO-02 `ST^PRCHRPT` [PRCHRPT.m:5-7]: sets the `^PRCFSITE` flags to `SP`
/// (station prompt) and calls `^PRCFSITE`.
fn st_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    v.prcf_x = Some("SP".to_string());
    prcfsite(s, st, v)
}

/// PO-03 `^PRCFSITE` [PRCFSITE.m:4-29], stub: the snapshot's site context,
/// fixed, with nothing read from `Store`. The site file holds six stations
/// [411+ADMIN. ACTIVITY SITE PARAMETER.zwr:3], so the station prompt is
/// written. The flags `SP` hold no `A`, `F`, `Q` or `B`, so the `A`
/// callbacks [PRCFSITE.m:16, 19], the fiscal-year and quarter prompts
/// [PRCFSITE.m:20, 23-25] and the budget line [PRCFSITE.m:28] are skipped.
/// `PRC("FY")` and `PRC("QTR")`, computed from the `^%DT` date through `A`
/// and `B` [PRCFSITE.m:15], are not modelled because they reach no output:
/// `FY` copies `PRC("FY")` into `X` [PRCFSITE.m:21], `SE1` overwrites `X`
/// [PRCFSITE.m:26], `OUT` kills `A`, `B`, `X` and `Y` [PRCFSITE.m:29], and
/// no later tag on the path reads `PRC("FY")` or `PRC("QTR")`.
fn prcfsite(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // DIVFY  D DUZ G:'% Q   [PRCFSITE.m:5]; PO-04's exercised outcome leaves % at 1
    duz_prcfsite(s, st, v)?;
    //  W ! I '$D(^PRC(411,0)) W "SITE PARAMETERS ..." G Q   [PRCFSITE.m:8]; the file exists
    s.nl();
    //  I '$D(PRC("SITE")),PRCF("X") contains S or B: S PRC("SITE")=$O(^PRC(411,"AC","Y",0))   [PRCFSITE.m:14]
    let flags = v.prcf_x.clone().unwrap_or_default();
    if v.prc_site.is_none() && (contains(&flags, "S") || contains(&flags, "B")) {
        v.prc_site = stub_value("STATION").map(str::to_string);
    }
    //  I '$D(PRC("FY")) S %DT="",X="T" D ^%DT S A=...,PRC("FY")=...,PRC("QTR")=... S X=""   [PRCFSITE.m:15]
    pct_dt(s, st, v)?;
    v.x = Some(String::new());
    // SI ... W ! S DIC("A")="Select STATION NUMBER ('^' TO EXIT): ",DIC("B")=PRC("SITE"),
    //    DIC=411,DIC(0)="AEQMZ",DIC("S")="I +Y<1000000" D ^DIC K DIC   [PRCFSITE.m:18]
    s.nl();
    v.dic_a = Some(STATION_PROMPT.to_string());
    v.dic_b = v.prc_site.clone();
    dic(s, st, v)?;
    v.dic_a = None;
    v.dic_b = None;
    //  G:+Y<0 Q S PRC("SITE")=+Y
    let y = num(v.y.as_deref().unwrap_or(""));
    if num_cmp(&y, "0") == Ordering::Less {
        return Err(unsupported("PO-03.b7"));
    }
    v.prc_site = Some(y);
    // OUT S %=1 K %DT,DIC,PRC("SP"),PRC("MDIV"),PRC("L"),PRC("I"),PRCF("X"),%F,A,B,X,Y Q   [PRCFSITE.m:29]
    v.prcf_x = None;
    v.x = None;
    v.y = None;
    Ok(())
}

/// PO-04 `DUZ^PRCFSITE` [PRCFSITE.m:35-40], stub: the user context of
/// [`STUB_CONTEXT`] `DUZ`, read from nothing in `Store`. POSTMASTER's
/// `^VA(200,.5,20)` piece 2 is set in the snapshot [200+NEW PERSON.zwr:12],
/// so `D A` never writes (PO-04.b3 n/e). `PRC("PER")` is not modelled: no
/// output on the path reads it.
fn duz_prcfsite(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt> {
    // DUZ K PRCFDEL,PRC("PER") S %=1 I $D(DUZ)#2,+DUZ>0 S PRC("PER")=+DUZ
    //  I '$D(PRC("PER")) S %=0 W !,$C(7),"YOU ARE NOT IN THE 'NEW PERSON' FILE. ..." Q
    if !stub_value("DUZ").is_some_and(positive) {
        return Err(unsupported("PO-04.b2"));
    }
    Ok(())
}

/// PO-41 `^%DT` [PRCFSITE.m:15] [DIDT.m:8-25], stub: called with `%DT=""`
/// and `X="T"`. With neither the `A` nor the `E` flag, `^%DT` neither
/// prompts nor writes [DIDT.m:11-12, 63, 67, 92]. The date it returns in
/// `Y` is read only into `A` and `B`, from which `^PRCFSITE` computes
/// `PRC("FY")` and `PRC("QTR")` [PRCFSITE.m:15]; none of them reaches any
/// output (see [`prcfsite`]), and `OUT^PRCFSITE` kills `A`, `B` and `Y`
/// [PRCFSITE.m:29], so the reproduction writes nothing and sets nothing.
fn pct_dt(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt> {
    Ok(())
}

/// PO-05 `^DIC` on file 411 [PRCFSITE.m:18] [DIC.m:8-69] [DIC11.m:15-32]
/// [DIC5.m:21-32] [DIC2.m:8-66], stub: the station lookup with
/// `DIC(0)="AEQMZ"`, resolved byte for byte for the snapshot (`SPEC.md` R1),
/// with nothing read from `Store`. RETURN accepts the default and selects
/// it; any other answer is classified inside the lookup, so it is PO-05.b2.
fn dic(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    s.nl();
    // PR1 S DIY=DIPRMT(DISUB) ... I $G(DIY(DISUB))]"" S DIY=DIY_DIY(DISUB)_"// "
    //  W DIY R X:$S($G(DTIME):DTIME,1:300)   [DIC11.m:15-17]
    s.write(v.dic_a.as_deref().unwrap_or(""));
    let default = v.dic_b.clone().filter(|station| follows(station, ""));
    if let Some(station) = &default {
        s.write(station);
        s.write("// ");
    }
    let answer = read(s)?;
    if !answer.is_empty() {
        return Err(unsupported("PO-05.b2"));
    }
    //  I X="",$G(DIY(DISUB))]"" S DIVAL(DISUB)=DIY(DISUB)   [DIC11.m:30]
    let Some(station) = default else {
        // With no default an empty answer looks nothing up [DIC11.m:31] [DIC.m:45].
        v.y = Some("-1".to_string());
        return Ok(());
    };
    // The exact B match is selected at once [DIC3.m:28-31] and echoed by WO^DIC2.
    s.write(STATION_ECHO);
    // GOT ... S Y=Y_"^"_DIY   [DIC2.m:53]: the entry and its .01 value
    v.y = Some(format!("{station}^{station}"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Order selection: PO-06 to PO-09
// ---------------------------------------------------------------------------

/// PO-06 `EN30^PRCHRPT` [PRCHRPT.m:72-75]: the selection loop. Each pass
/// prompts for an order; a selection is displayed and the prompt repeats
/// (`G EN30`), and a lookup that ends without a selection returns.
fn en30_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    loop {
        po_prchrpt(s, st, v)?;
        let Some(order) = v.prchpo.clone() else {
            return Ok(());
        };
        //  I X<10 W $C(7)," >>> Status makes this record non-specifiable here."
        if num_cmp(v.x.as_deref().unwrap_or(""), "10") == Ordering::Less {
            s.bell();
            s.write(" >>> Status makes this record non-specifiable here.");
        }
        v.d0 = Some(order);
        prchdp1(s, st, v)?;
        // The NEW of PRCHAMNT [PRCHDP2.m:9] ends when the DO of ^PRCHDP1 returns.
        v.prchamnt = None;
        k_prchrpt(s, st, v)?;
    }
}

/// PO-07 `PO^PRCHRPT` [PRCHRPT.m:9-15]: sets the order prompt and screen,
/// runs the lookup, and flags a requisition (MOP 8) for the display label.
fn po_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // PO S PRCHP("S")="$P(^(0),U,2)<10!($P(^(0),U,2)=25)"
    //  (the PRCHPC and PRCHDELV SETs [PRCHRPT.m:10-11] need locals this option never sets)
    v.prchp_s = Some(MOP_SCREEN.to_string());
    v.prchp_a = Some(ORDER_PROMPT.to_string());
    en3_prchpat(s, st, v)?;
    //  I $D(PRCHPO),$D(^PRC(442,+PRCHPO,0)),$P(^(0),U,2)=8 S PRCHNRQ=1
    if let Some(order) = v.prchpo.clone() {
        // ^(0) is ^PRC(442,+PRCHPO,0)
        let node0 = defined(st, &key("PRC", &["442", &num(&order), "0"]))?;
        if node0.is_some_and(|node| piece(&node, "^", 2) == "8") {
            v.prchnrq = Some("1".to_string());
        }
    }
    Ok(())
}

/// PO-08 `EN3^PRCHPAT` [PRCHPAT.m:78-84]: sets up the lookup of file 442 on
/// index `C` and, on a selection, the order and its supply status order.
fn en3_prchpat(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // EN3 K PRCHPO,PRCHNEW,DA,DIC,D0,DQ Q:'$D(PRC("SITE"))  S DIC="^PRC(442,",DIC(0)="QEAMZ"
    v.prchpo = None;
    v.da = None;
    v.dic_a = None;
    v.dic_s = None;
    v.d0 = None;
    if v.prc_site.is_none() {
        return Err(unsupported("PO-08.b1"));
    }
    //  S D=$S($G(PRCHPC)=1:"APCS",$G(PRCHPC)=2:"APCP",$G(PRCHDELV):"APCD",1:"C")
    v.d = Some("C".to_string());
    //  S DIC("A")=$S($D(PRCHP("A")):PRCHP("A"),1:"PURCHASE ORDER: "),
    //    DIC("S")="I +$P(^(0),U,1)=PRC(""SITE"")"_$S($D(PRCHP("S")):","_PRCHP("S"),1:"")
    v.dic_a = Some(
        v.prchp_a
            .clone()
            .unwrap_or_else(|| DEFAULT_ORDER_PROMPT.to_string()),
    );
    v.dic_s = Some(v.prchp_s.is_some());
    //  W !! D IX^DIC K DIC,PRCHP S X="" Q:+Y<0
    s.nl();
    s.nl();
    ix_dic(s, st, v)?;
    v.dic_a = None;
    v.dic_s = None;
    v.prchp_a = None;
    v.prchp_s = None;
    v.x = Some(String::new());
    let selected = num(v.y.as_deref().unwrap_or(""));
    if num_cmp(&selected, "0") == Ordering::Less {
        return Ok(());
    }
    //  S (PRCHPO,DA)=+Y,X=$P($G(^PRCD(442.3,+$G(^PRC(442,DA,7)),0)),U,2) S:X=0 X=""
    v.prchpo = Some(selected.clone());
    v.da = Some(selected.clone());
    let status = num(&st.get_or_empty(&key("PRC", &["442", &selected, "7"])));
    let order = st.get_or_empty(&key("PRCD", &["442.3", &status, "0"]));
    let x = piece(&order, "^", 2);
    v.x = Some(if x == "0" {
        String::new()
    } else {
        x.to_string()
    });
    Ok(())
}

/// The subscript `A` followed by an opening bracket, which FileMan orders
/// from to test whether file 442 has any index at all [DICM.m:71].
const AFTER_INDEX_NAMES: &str = "A\u{5b}";

/// PO-09 `IX^DIC` [DIC.m:84-86], stub over `Store`: the order lookup of
/// file 442, starting on the index the local `D` names (`C`), with
/// `DIC(0)="QEAMZ"`, following `SPEC.md` R2 to R6. It applies
/// FileMan's lookup rules to whatever the store holds, never a list of known
/// identifiers. On a selection `Y` is `ien^.01`; on `^` it is `-1`.
fn ix_dic(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // IX N DINDEX,DF S (DF,DINDEX,DINDEX("START"))=D G EN
    // EN ... D INIT^DIC0: GETFILE^DIC0 and FILENUM^DILIBF read ^PRC(442,0)
    // with $G [DIC0.m:21, 83-87] [DILIBF.m:141, 147], so a header that is
    // absent, empty or holding only descendants leaves DIFILEI empty and EN
    // sets Y=-1 before the prompt [DIC.m:14]: the n/e PO-09.b15, never
    // CorruptStore. A header with a value then gives the file attributes,
    // which decide the prompt and the identifiers [DIC0.m:23] [DIC1.m:41-49].
    let header = st.get_or_empty(&key("PRC", &["442", "0"]));
    if piece(&header, "^", 2) != FILE_442_ATTRIBUTES {
        return Err(unsupported("PO-09.b15"));
    }
    s.nl();
    s.write(v.dic_a.as_deref().unwrap_or(""));
    let x = read(s)?;
    // The read checks of PR1^DIC11, in source order [DIC11.m:19-33]; the
    // timeout [DIC11.m:18] (PO-09.b1) cannot occur.
    if x.chars().any(|c| !(' '..='~').contains(&c)) || overlong_number(&x) {
        return Err(unsupported("PO-09.b2"));
    }
    if x == "^" {
        v.y = Some("-1".to_string());
        return Ok(());
    }
    if x.chars().count() > 250 {
        return Err(unsupported("PO-09.b2"));
    }
    if !x.is_empty() && x.chars().all(|c| c == '?') {
        return Err(unsupported("PO-09.b4"));
    }
    if x == " " || backtick_value(&x) {
        return Err(unsupported("PO-09.b5"));
    }
    if x.is_empty() {
        return Err(unsupported("PO-09.b6"));
    }
    //  S:$E(X)="^" (DIOUT,DUOUT)=1   [DIC11.m:33]; RTN ... I $G(DUOUT) S Y=-1   [DIC.m:45]
    if x.starts_with('^') {
        v.y = Some("-1".to_string());
        return Ok(());
    }
    v.x = Some(x.clone());
    // RTN: no IEN attempt, because IX^DIC defines DF [DIC.m:48, 84-85] [DIC5.m:22]
    // F ... D SEARCH^DIC3   [DIC.m:56]
    let (candidates, exact_on_start) = search(st, v, &x)?;
    if let Some(ien) = candidates.first() {
        // I DS=1 S DS("DD")=1 D G^DIC2 Q  /  I DS D Y^DIC1 ...   [DIC3.m:71-72]
        if candidates.len() > 1 {
            return Err(unsupported("PO-09.b10"));
        }
        if exact_on_start != 1 {
            return Err(unsupported("PO-09.b9"));
        }
        let node0 = write_order_echo(s, st, ien)?;
        // GOT ... S Y=Y_"^"_DIY   [DIC2.m:53]. ACT's ^DISV write [DIC2.m:65] is not reproduced.
        v.y = Some(format!("{}^{}", ien.text(), piece(&node0, "^", 1)));
        return Ok(());
    }
    // No candidate: ^DICM's transforms and retries [DICM.m:13-66]
    if pointer_candidate(st, v, &x)? {
        return Err(unsupported("PO-09.b11"));
    }
    if x.chars().any(|c| c.is_ascii_lowercase()) {
        let upper = x.to_ascii_uppercase();
        let (upper_candidates, _) = search(st, v, &upper)?;
        if !upper_candidates.is_empty() || pointer_candidate(st, v, &upper)? {
            return Err(unsupported("PO-09.b13"));
        }
    }
    // With no subscript after AFTER_INDEX_NAMES FileMan scans the whole file [DICM.m:71-81]
    let file = key("PRC", &["442"]);
    if st
        .order(&file, Some(&Sub::Str(AFTER_INDEX_NAMES.to_string())))
        .is_none()
    {
        return Err(unsupported("PO-09.b16"));
    }
    // BAD^DIC1, then DING and the re-ask [DIC1.m:157-159] [DIC.m:63-65, 80-81, 25]
    // would write <BEL> ??, two newlines and the prompt (SPEC.md R3). Whether
    // the nested pointer lookups' own transforms write first is open item
    // O-03 (SPEC.md R4), so this outcome is BLOCKED for every value alike.
    Err(unsupported("PO-09.b14"))
}

/// `X?1.N.1"."1.N` with more than 25 integer or 24 fraction digits
/// [DIC11.m:21].
fn overlong_number(x: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let is_number = match x.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac),
        None => digits(x) && x.len() >= 2,
    };
    is_number && (piece(x, ".", 1).len() > 25 || piece(x, ".", 2).len() > 24)
}

/// M pattern code `P`: ASCII punctuation, space included.
fn m_punctuation(c: char) -> bool {
    c == ' ' || c.is_ascii_punctuation()
}

/// `X?1"`".NP`: a backtick followed only by digits and punctuation
/// [DIC11.m:27].
fn backtick_value(x: &str) -> bool {
    x.strip_prefix('`')
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || m_punctuation(c)))
}

/// `DIC("S")` of the order lookup applied to an entry's zero node, its naked
/// reference `^(0)` being `^PRC(442,ien,0)` [DIC3.m:122-124]: the entry's
/// station `+$P(^(0),U,1)` equals `PRC("SITE")`, and with the MOP rule its
/// method of purchase is numerically below 10 or is the text `25`.
fn screen_passes(v: &Locals, node0: &str) -> bool {
    let site = v
        .prc_site
        .as_deref()
        .is_some_and(|station| num(piece(node0, "^", 1)) == station);
    match v.dic_s {
        Some(true) => {
            let mop = piece(node0, "^", 2);
            site && (num_cmp(mop, "10") == Ordering::Less || mop == "25")
        }
        Some(false) => site,
        None => true,
    }
}

/// `MN` and `S^DIC3` [DIC3.m:89-126]: entry `ien` of file 442 counts only
/// when its zero node exists, it has no `-9` node, and it passes the screen.
fn qualifies(st: &Store, v: &Locals, ien: &Sub) -> Result<bool, Halt> {
    let entry = child(&key("PRC", &["442"]), ien);
    let zero = child(&entry, &Sub::from_text("0"));
    // I '$D(@(DIC_"Y,0)")) X "I 0" Q   [DIC3.m:94]
    if st.data(&zero) == 0 {
        return Ok(false);
    }
    // I $D(@(DIC_"Y,0)")),'$D(^(-9)) S DIY=$P(^(0),U)   [DIC3.m:110]
    if st.data(&child(&entry, &Sub::from_text("-9"))) != 0 {
        return Ok(false);
    }
    let Some(node0) = st.get(&zero) else {
        return Err(corrupt(&zero));
    };
    Ok(screen_passes(v, node0))
}

/// `MOREX` [DIC3.m:82-87]: the entries filed under one index value `entry`,
/// in collation order up to the first subscript whose numeric value is 0
/// (`I 'Y S Y=-1 Q`), each added once when it qualifies [DIC3.m:142].
/// The "added once" test runs over the entries found so far one by one, as
/// the loop of `K^DIC3` over `DS` does [DIC3.m:141-142].
fn collect_iens(st: &Store, v: &Locals, entry: &[Sub], found: &mut Vec<Sub>) -> Result<(), Halt> {
    let mut next = first_after_zero(st, entry);
    while let Some(ien) = next {
        if !truthy(ien.text()) {
            break;
        }
        if qualifies(st, v, &ien)? && !found.contains(&ien) {
            found.push(ien.clone());
        }
        next = st.order(entry, Some(&ien));
    }
    Ok(())
}

/// The index values the partial-match step visits under `root` after the
/// exact value `x` [DIC3.m:36-49]: each later subscript that begins with
/// `x`, in collation order, up to the first that does not. For a value of
/// digits, `-` and `.` the walk starts at the first subscript at or after
/// `x` followed by a space [DIC3.m:40], which passes over the numbers.
fn partial_keys(st: &Store, root: &[Sub], x: &str) -> Vec<Sub> {
    let mut next = if x
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
    {
        let probe = Sub::from_text(&format!("{x} "));
        if st.data(&child(root, &probe)) != 0 {
            Some(probe)
        } else {
            st.order(root, Some(&probe))
        }
    } else {
        st.order(root, Some(&Sub::from_text(x)))
    };
    let mut keys = Vec::new();
    while let Some(value) = next {
        if !value.text().starts_with(x) {
            break;
        }
        next = st.order(root, Some(&value));
        keys.push(value);
    }
    keys
}

/// `SEARCH^DIC3` [DIC3.m:8-79] for `x`: exact then partial matches on the
/// starting index `D`, then on every later subscript of `^PRC(442)` whose
/// `$D` is 10, searched on the raw value. A pointer-field index is skipped
/// when `x` is numeric (`+$P(X,"E")=X`) [DIC4.m:84-86]. A value longer than
/// the index skips the search [DIC3.m:13].
///
/// Returns every qualifying entry, in discovery order without repeats, and
/// how many of them came from the exact match on the starting index.
fn search(st: &Store, v: &Locals, x: &str) -> Result<(Vec<Sub>, usize), Halt> {
    let mut candidates = Vec::new();
    if x.chars().count() > LOOKUP_LENGTH {
        return Ok((candidates, 0));
    }
    let file = key("PRC", &["442"]);
    let start = Sub::from_text(v.d.as_deref().unwrap_or(""));
    let start_root = child(&file, &start);
    collect_iens(
        st,
        v,
        &child(&start_root, &Sub::from_text(x)),
        &mut candidates,
    )?;
    let exact_on_start = candidates.len();
    for value in partial_keys(st, &start_root, x) {
        collect_iens(st, v, &child(&start_root, &value), &mut candidates)?;
    }
    let numeric = num(piece(x, "E", 1)) == x;
    let mut index = start;
    // M ... S (D,DINDEX)=$O(@(DIC_"D)")) ... I $D(@(DIC_"D)"))-10 Q   [DIC3.m:61-66]
    while let Some(next) = st.order(&file, Some(&index)) {
        index = next;
        let root = child(&file, &index);
        if st.data(&root) != 10 {
            continue;
        }
        if numeric
            && POINTER_INDEXES
                .iter()
                .any(|(name, _, _)| *name == index.text())
        {
            continue;
        }
        collect_iens(st, v, &child(&root, &Sub::from_text(x)), &mut candidates)?;
        for value in partial_keys(st, &root, x) {
            collect_iens(st, v, &child(&root, &value), &mut candidates)?;
        }
    }
    Ok((candidates, exact_on_start))
}

/// The pointer-candidate rule (`SPEC.md` R6), from `P^DICM0` [DICM0.m:8-41]:
/// true when, for some pointer index of file 442 after `C`, the pointed-to
/// file has a header holding a value and holds an entry whose `B` value
/// equals `x` or begins with it and under which a qualifying order is filed.
/// A pointed-to header that is absent, empty or holding only descendants
/// ends that nested lookup before anything else is read, so its index has
/// no candidate. The nested lookup runs with `A` and `Q` stripped
/// [DICM.m:107], so a pointed-to entry with no qualifying order writes
/// nothing and is simply not a candidate.
fn pointer_candidate(st: &Store, v: &Locals, x: &str) -> Result<bool, Halt> {
    let file = key("PRC", &["442"]);
    // P1 ... I @("$O"_Y_"0))'>0") G L1   [DICM0.m:26]: no record, no pointer lookup
    if !first_after_zero(st, &file).is_some_and(|sub| positive(sub.text())) {
        return Ok(false);
    }
    for (name, global, number) in POINTER_INDEXES {
        let parents = child(&file, &Sub::from_text(name));
        //  I @("$O"_Y_%_",0))>0") ...   [DICM0.m:32]: nothing filed, no screen
        if !first_after_zero(st, &parents).is_some_and(|sub| positive(sub.text())) {
            continue;
        }
        let target = key(global, &[number]);
        // N DS,DINDEX,DIFILEI D X^DIC   [DICM0.m:41] [DIC.m:33-34]: INIT^DIC0's
        // GETFILE and FILENUM^DILIBF read the pointed-to header with $G
        // [DIC0.m:21, 83-87] [DILIBF.m:141, 147]; a header that is absent,
        // empty or holding only descendants leaves DIFILEI empty, and the
        // nested lookup ends with Y=-1 with nothing more read.
        if st
            .get_or_empty(&child(&target, &Sub::from_text("0")))
            .is_empty()
        {
            continue;
        }
        let b_index = child(&target, &Sub::from_text("B"));
        let mut values = vec![Sub::from_text(x)];
        values.extend(partial_keys(st, &b_index, x));
        for value in values {
            let entries = child(&b_index, &value);
            let mut next = first_after_zero(st, &entries);
            while let Some(entry) = next {
                if !truthy(entry.text()) {
                    break;
                }
                if pointed_entry_qualifies(st, v, &target, &parents, &entry)? {
                    return Ok(true);
                }
                next = st.order(&entries, Some(&entry));
            }
        }
    }
    Ok(false)
}

/// One pointed-to entry of the nested lookup: it must exist without a `-9`
/// node [DIC3.m:94, 110], and the screen `P^DICM0` builds must find a parent
/// order `p > 0` under `^PRC(442,index,entry,p)` whose zero node exists and
/// passes the order screen [DICM0.m:32, 36].
fn pointed_entry_qualifies(
    st: &Store,
    v: &Locals,
    target: &[Sub],
    parents: &[Sub],
    entry: &Sub,
) -> Result<bool, Halt> {
    let record = child(target, entry);
    let zero = child(&record, &Sub::from_text("0"));
    if st.data(&zero) == 0 || st.data(&child(&record, &Sub::from_text("-9"))) != 0 {
        return Ok(false);
    }
    if st.get(&zero).is_none() {
        return Err(corrupt(&zero));
    }
    let filed = child(parents, entry);
    let mut next = first_after_zero(st, &filed);
    while let Some(parent) = next {
        if !positive(parent.text()) {
            break;
        }
        let parent_zero = key("PRC", &["442", parent.text(), "0"]);
        if defined(st, &parent_zero)?.is_some_and(|node0| screen_passes(v, &node0)) {
            return Ok(true);
        }
        next = st.order(&filed, Some(&parent));
    }
    Ok(false)
}

/// The echo of a unique order match (`SPEC.md` R2), returning the order's
/// zero node. `WO^DIC2` writes two spaces and the full `.01` value
/// [DIC2.m:10, 25, 32]; `WOV^DICQ1` then writes two spaces before each
/// identifier of `^DD(442,0,"ID")` in subscript order, after
/// `$E(^PRC(442,ien,0),0)` points the naked reference at `^PRC(442,ien,0)`
/// [DICQ1.m:86-88] [DD.zwr:514202-514204]. Nothing follows: file 442 asks
/// no confirmation [DIC2.m:54-58].
fn write_order_echo(s: &mut Session, st: &Store, ien: &Sub) -> Result<String, Halt> {
    let entry = child(&key("PRC", &["442"]), ien);
    let zero = child(&entry, &Sub::from_text("0"));
    let Some(node0) = st.get(&zero).map(str::to_string) else {
        return Err(corrupt(&zero));
    };
    s.write("  ");
    s.write(piece(&node0, "^", 1));
    // Z.01: W:$D(^(1)) $E($P(^(1),U,15),4,5)_"-"_$E($P(^(1),U,15),6,7)_"-"_$E($P(^(1),U,15),2,3)
    s.write("  ");
    if let Some(node1) = defined(st, &child(&entry, &Sub::from_text("1")))? {
        let date = piece(&node1, "^", 15);
        s.write(&format!(
            "{}-{}-{}",
            extract(date, 4, 5),
            extract(date, 6, 7),
            extract(date, 2, 3)
        ));
    }
    // Z.02: W $S($D(^PRCD(442.5,+$P(^(0),U,2),0))#2:$P(^(0),U,2),1:"")
    //       I @("$D("_DIC_"Y,7))"),$D(^PRCD(442.3,+^(7),0)) W "   ",$E($P(^(0),U,1),1,34)
    s.write("  ");
    let mop = key("PRCD", &["442.5", &num(piece(&node0, "^", 2)), "0"]);
    if let Some(method) = st.get(&mop) {
        s.write(piece(method, "^", 2));
    }
    // ^(7) is ^PRC(442,ien,7); the second ^(0) is ^PRCD(442.3,status,0)
    if let Some(node7) = defined(st, &child(&entry, &Sub::from_text("7")))? {
        let status = key("PRCD", &["442.3", &num(&node7), "0"]);
        if let Some(name) = defined(st, &status)? {
            s.write("   ");
            s.write(&extract(piece(&name, "^", 1), 1, 34));
        }
    }
    // Z.03: W:$D(^(0)) !,?13,"FCP: ",$P($P(^(0),U,3)," ",1),"     $ ",$J($P(^(0),U,15),0,2)
    s.write("  ");
    s.nl();
    s.tab(13);
    s.write("FCP: ");
    s.write(piece(piece(&node0, "^", 3), " ", 1));
    s.write("     $ ");
    s.write(&justify(piece(&node0, "^", 15), 0, Some(2)));
    Ok(node0)
}

// ---------------------------------------------------------------------------
// Page 1 of the display: PO-10 to PO-24
// ---------------------------------------------------------------------------

/// PO-10 `^PRCHDP1` [PRCHDP1.m:4-52]: the page-1 header of order `D0`,
/// falling into the FOB block (PO-13) and from there through the rest of
/// page 1 into the line items. It stops at `^%ZIS` (PO-12, BLOCKED by open
/// item O-01); the header after it is ported as source gives it.
fn prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // S PRCHD0=$G(^PRC(442,D0,0)),PRCHD1=$G(^(1)),X=0   [PRCHDP1.m:4]; ^(1) is ^PRC(442,D0,1)
    let prchd0 = st.get_or_empty(&key("PRC", &["442", &d0, "0"]));
    let prchd1 = st.get_or_empty(&key("PRC", &["442", &d0, "1"]));
    v.prchd0 = Some(prchd0.clone());
    v.prchd1 = Some(prchd1.clone());
    v.x = Some("0".to_string());
    let prchdsit = piece(&prchd0, "-", 1).to_string();
    v.prchdsit = Some(prchdsit.clone());
    // S PRCHSIT1=$S($P($G(^PRC(442,D0,23)),U,7)]"":$P(^(23),U,7),1:$P(PRCHD0,"-"))   [PRCHDP1.m:7]
    let node23 = st.get_or_empty(&key("PRC", &["442", &d0, "23"]));
    let substation = piece(&node23, "^", 7);
    let prchsit1 = if follows(substation, "") {
        substation
    } else {
        piece(&prchd0, "-", 1)
    };
    // S PRCHDSHP="" Q:PRCHD0']""  S:'$D(PRCHPO) PRCHPO=D0   [PRCHDP1.m:8]
    v.prchdshp = Some(String::new());
    if !follows(&prchd0, "") {
        return Err(unsupported("PO-10.b1"));
    }
    if v.prchpo.is_none() {
        v.prchpo = Some(d0.clone());
    }
    // I PRCHD1']"",$D(^PRC(442,D0,4,1,0)) S DIC="^PRC(442,",DA=PRCHPO D EN^DIQ Q   [PRCHDP1.m:9]
    if !follows(&prchd1, "") && st.data(&key("PRC", &["442", &d0, "4", "1", "0"])) != 0 {
        return Err(unsupported("PO-10.b2"));
    }
    // Q:PRCHD1']""  S PRCHDS=0 I +$P(PRCHD0,U,2)'=4,$P(PRCHD1,U,12)=""
    //  S PRCHDSHP=$G(^PRC(411,PRCHSIT1,1,+$P(PRCHD1,U,3),0))   [PRCHDP1.m:10]
    if !follows(&prchd1, "") {
        return Err(unsupported("PO-10.b3"));
    }
    v.prchds = Some("0".to_string());
    let patient = piece(&prchd1, "^", 12);
    let site_ship_to = num(piece(&prchd0, "^", 2)) != "4" && patient.is_empty();
    if site_ship_to {
        let location = num(piece(&prchd1, "^", 3));
        v.prchdshp = Some(st.get_or_empty(&key("PRC", &["411", prchsit1, "1", &location, "0"])));
    }
    // I '$T,$P(PRCHD1,U,12)]"" S PRCHDSHP=$G(^PRC(440.2,$P(PRCHD1,U,12),0)),PRCHDS=1
    //  I +PRCHDSHP>0 S $P(PRCHDSHP,U,1)=$E($P($G(^DPT(+PRCHDSHP,0)),U,1),1,21)   [PRCHDP1.m:11]
    if !site_ship_to && follows(patient, "") {
        let mut ship_to = st.get_or_empty(&key("PRC", &["440.2", patient, "0"]));
        v.prchds = Some("1".to_string());
        if positive(&ship_to) {
            let name = st.get_or_empty(&key("DPT", &[&num(&ship_to), "0"]));
            ship_to = set_first_piece(&ship_to, &extract(piece(&name, "^", 1), 1, 21));
        }
        v.prchdshp = Some(ship_to);
    }
    // S IOP="HOME",%ZIS="",PRCHDST=$G(^PRC(411,PRCHDSIT,0)),PRCHDHSP=$G(^(3)) D ^%ZIS W:$Y>0 @IOF   [PRCHDP1.m:12]
    // (PRCHDHSP, ^PRC(411,PRCHDSIT,3), is read whole and never displayed.)
    v.prchdst = Some(st.get_or_empty(&key("PRC", &["411", &prchdsit, "0"])));
    pct_zis(s, st, v)?;
    form_feed(s, "PO-10.b4")?;
    // S PRCHTYPE=$P($G(^PRC(442,D0,23)),U,11)   [PRCHDP1.m:13]
    // I PRCHTYPE="P"!(PRCHTYPE="S") S PRCHTYPE="P"   [PRCHDP1.m:14]
    let mut prchtype = piece(&node23, "^", 11).to_string();
    if prchtype == "P" || prchtype == "S" {
        prchtype = "P".to_string();
    }
    // W !,$S($G(PRCHTYPE)="P":"PC ORDER: ",$G(PRCHTYPE)="D":"DELIVER ORDER: ",
    //  $D(PRCHNRQ):"REQUISITION: ",1:"PURCHASE ORDER: ")_$P(PRCHD0,U,1),?37,"STATUS: "   [PRCHDP1.m:15]
    let label = match prchtype.as_str() {
        "P" => "PC ORDER: ",
        "D" => "DELIVER ORDER: ",
        _ if v.prchnrq.is_some() => "REQUISITION: ",
        _ => "PURCHASE ORDER: ",
    };
    v.prchtype = Some(prchtype);
    s.nl();
    s.write(label);
    s.write(piece(&prchd0, "^", 1));
    s.tab(37);
    s.write("STATUS: ");
    //  I +$G(^PRC(442,D0,7))>0 W $E($P($G(^PRCD(442.3,+^(7),0)),U,1),1,34); ^(7) is ^PRC(442,D0,7)
    let status = num(&st.get_or_empty(&key("PRC", &["442", &d0, "7"])));
    if positive(&status) {
        let name = st.get_or_empty(&key("PRCD", &["442.3", &status, "0"]));
        s.write(&extract(piece(&name, "^", 1), 1, 34));
    }
    // W !,"M.O.P.: ",$P($G(^PRCD(442.5,+$P(PRCHD0,U,2),0)),U,1),?37,"LAST PARTIAL RECD.: "
    //  S X=0 I $D(^PRC(442,D0,11,0)) S I=0 F  S I=$O(^PRC(442,D0,11,I)) ...   [PRCHDP1.m:16]
    let method = st.get_or_empty(&key("PRCD", &["442.5", &num(piece(&prchd0, "^", 2)), "0"]));
    s.nl();
    s.write("M.O.P.: ");
    s.write(piece(&method, "^", 1));
    s.tab(37);
    s.write("LAST PARTIAL RECD.: ");
    v.x = Some("0".to_string());
    if st.data(&key("PRC", &["442", &d0, "11", "0"])) != 0 {
        return Err(unsupported("PO-10.b5"));
    }
    // S:X>0 ... W:X>0 X W !,?37,"REQUESTING SERVICE: ",$S($D(^DIC(49,+$P(PRCHD1,U,2),0)):$P(^(0),U,1),1:"")   [PRCHDP1.m:17]
    s.nl();
    s.tab(37);
    s.write("REQUESTING SERVICE: ");
    let service = key("DIC", &["49", &num(piece(&prchd1, "^", 2)), "0"]);
    if let Some(node) = defined(st, &service)? {
        s.write(piece(&node, "^", 1));
    }
    // N PRCHDV1 S PRCHDV=$G(^PRC(440,+PRCHD1,0)),PRCHDV1=$P(PRCHDV,U,1)   [PRCHDP1.m:18]
    let vendor = num(&prchd1);
    let prchdv = st.get_or_empty(&key("PRC", &["440", &vendor, "0"]));
    v.prchdv = Some(prchdv.clone());
    // I PRCHDV1="SIMPLIFIED",$P($G(^PRC(442,PRCHPO,24)),U,2)'="" S PRCHDV1=$P($G(^PRC(442,PRCHPO,24)),U,2)   [PRCHDP1.m:19]
    let prchpo = v.prchpo.clone().unwrap_or_default();
    let node24 = st.get_or_empty(&key("PRC", &["442", &prchpo, "24"]));
    let free_text_vendor = piece(&node24, "^", 2);
    let simplified = piece(&prchdv, "^", 1) == "SIMPLIFIED" && !free_text_vendor.is_empty();
    let prchdv1 = if simplified {
        free_text_vendor
    } else {
        piece(&prchdv, "^", 1)
    };
    // W !,"VENDOR:",?9,PRCHDV1   [PRCHDP1.m:20]
    // W ?48,"SHIP TO: ",$P(PRCHDSHP,U,1)   [PRCHDP1.m:21]
    let ship_to = v.prchdshp.clone().unwrap_or_default();
    s.nl();
    s.write("VENDOR:");
    s.tab(9);
    s.write(prchdv1);
    s.tab(48);
    s.write("SHIP TO: ");
    s.write(piece(&ship_to, "^", 1));
    v.prc_site = Some(prchdsit);
    // D FTYP^PRCHFPNT S V(1)=$P(PRCHDV,U,2),V=2,S=1 S:'PRCHDS S(S)=PRCHFTYP,S=S+1   [PRCHDP1.m:25]
    ftyp_prchfpnt(s, st, v)?;
    let direct_delivery = truthy(v.prchds.as_deref().unwrap_or(""));
    v.v_rows.insert(1, piece(&prchdv, "^", 2).to_string());
    let mut vn: i64 = 2;
    let mut sn: i64 = 1;
    if !direct_delivery {
        v.s_rows.insert(sn, v.prchftyp.clone().unwrap_or_default());
        sn += 1;
    }
    // I $P(PRCHDV,U,3)]"" S V(V)=$P(PRCHDV,U,3),V=V+1 S:$P(PRCHDV,U,4)]"" V(V)=$P(PRCHDV,U,4),V=V+1
    //  S:$P(PRCHDV,U,5)]"" V(V)=$P(PRCHDV,U,5),V=V+1   [PRCHDP1.m:26]
    if follows(piece(&prchdv, "^", 3), "") {
        for field in 3..=5 {
            let line = piece(&prchdv, "^", field);
            if follows(line, "") {
                v.v_rows.insert(vn, line.to_string());
                vn += 1;
            }
        }
    }
    // S V(V)=$S($P(PRCHDV,U,6)]"":($P(PRCHDV,U,6)_", "),1:"   ")_$P($G(^DIC(5,+$P(PRCHDV,U,7),0)),U,2)
    //  _"  "_$P(PRCHDV,U,8),V=V+1   [PRCHDP1.m:27]
    let city = piece(&prchdv, "^", 6);
    let state = st.get_or_empty(&key("DIC", &["5", &num(piece(&prchdv, "^", 7)), "0"]));
    let city_part = if follows(city, "") {
        format!("{city}, ")
    } else {
        "   ".to_string()
    };
    v.v_rows.insert(
        vn,
        format!(
            "{city_part}{}  {}",
            piece(&state, "^", 2),
            piece(&prchdv, "^", 8)
        ),
    );
    vn += 1;
    // S:$P(PRCHDV,U,10)]"" V(V)=$P(PRCHDV,U,10),V=V+1   [PRCHDP1.m:28]
    let phone = piece(&prchdv, "^", 10);
    if follows(phone, "") {
        v.v_rows.insert(vn, phone.to_string());
        vn += 1;
    }
    // I $D(^PRC(440,+PRCHD1,2)),$P(^(2),U,1)]"" S V(V)="ACCT # "_$P(^(2),U,1)   [PRCHDP1.m:29]
    if let Some(node2) = defined(st, &key("PRC", &["440", &vendor, "2"]))? {
        let account = piece(&node2, "^", 1);
        if follows(account, "") {
            v.v_rows.insert(vn, format!("ACCT # {account}"));
        }
    }
    // I $P(PRCHDST,U,19)="Y",$D(^PRC(440,+PRCHD1,3)),$P(^(3),U,4)'=""
    //  S V(6)="FMS Vendor Code: "_$P(^(3),U,4)_$P(^(3),U,5)   [PRCHDP1.m:30]
    let prchdst = v.prchdst.clone().unwrap_or_default();
    if piece(&prchdst, "^", 19) == "Y"
        && let Some(node3) = defined(st, &key("PRC", &["440", &vendor, "3"]))?
        && !piece(&node3, "^", 4).is_empty()
    {
        v.v_rows.insert(
            6,
            format!(
                "FMS Vendor Code: {}{}",
                piece(&node3, "^", 4),
                piece(&node3, "^", 5)
            ),
        );
    }
    // I $P(PRCHD1,U,4)="Y" S V(8)=" VERBAL PURCHASE ORDER" S:$P(PRCHD1,U,5)="Y" V(8)=" CONFIRMATION COPY"   [PRCHDP1.m:31]
    if piece(&prchd1, "^", 4) == "Y" {
        v.v_rows.insert(8, " VERBAL PURCHASE ORDER".to_string());
        if piece(&prchd1, "^", 5) == "Y" {
            v.v_rows.insert(8, " CONFIRMATION COPY".to_string());
        }
    }
    // S PRCHEDI=$G(^PRC(440,+PRCHD1,3)) I PRCHEDI]"",$P(PRCHEDI,U,2)="Y",
    //  $P($G(^PRC(442,D0,12)),U,16)'="n" D  S V(8)=PRCHEDIT_" DO NOT MAIL"   [PRCHDP1.m:32-35]
    let edi = st.get_or_empty(&key("PRC", &["440", &vendor, "3"]));
    let node12 = st.get_or_empty(&key("PRC", &["442", &d0, "12"]));
    if follows(&edi, "") && piece(&edi, "^", 2) == "Y" && piece(&node12, "^", 16) != "n" {
        return Err(unsupported("PO-10.b6"));
    }
    // S:$P(PRCHDSHP,U,2)]"" S(S)=$P(PRCHDSHP,U,2),S=S+1 S:$P(PRCHDSHP,U,3)]"" ... S:$P(PRCHDSHP,U,4)]"" ...   [PRCHDP1.m:36]
    for field in 2..=4 {
        let line = piece(&ship_to, "^", field);
        if follows(line, "") {
            v.s_rows.insert(sn, line.to_string());
            sn += 1;
        }
    }
    // S S(S)=$S($P(PRCHDSHP,U,5)]"":($E($P(PRCHDSHP,U,5),1,12)_", "),1:"   ")
    //  _$P($G(^DIC(5,+$P(PRCHDSHP,U,6),0)),U,2)_"  "_$P(PRCHDSHP,U,7),S=S+2   [PRCHDP1.m:37]
    let ship_city = piece(&ship_to, "^", 5);
    let ship_state = st.get_or_empty(&key("DIC", &["5", &num(piece(&ship_to, "^", 6)), "0"]));
    let ship_city_part = if follows(ship_city, "") {
        format!("{}, ", extract(ship_city, 1, 12))
    } else {
        "   ".to_string()
    };
    v.s_rows.insert(
        sn,
        format!(
            "{ship_city_part}{}  {}",
            piece(&ship_state, "^", 2),
            piece(&ship_to, "^", 7)
        ),
    );
    sn += 2;
    // I $P(PRCHDSHP,U,8)]"",'PRCHDS S S(S)="DELIVERY HOURS:",S=S+1,S(S)=$P(PRCHDSHP,U,8)   [PRCHDP1.m:38]
    let hours = piece(&ship_to, "^", 8);
    if follows(hours, "") && !direct_delivery {
        v.s_rows.insert(sn, "DELIVERY HOURS:".to_string());
        sn += 1;
        v.s_rows.insert(sn, hours.to_string());
    }
    // I $P(PRCHDV,U)="SIMPLIFIED",$P($G(^PRC(442,PRCHPO,24)),U,2)'="" S V(1)=""   [PRCHDP1.m:40]
    if simplified {
        v.v_rows.insert(1, String::new());
    }
    // F I=1:1:7 W ! W:$D(V(I)) ?9,V(I) W:$D(S(I)) ?57,S(I)   [PRCHDP1.m:41]
    for row in 1..=7 {
        s.nl();
        if let Some(text) = v.v_rows.get(&row) {
            s.tab(9);
            s.write(text);
        }
        if let Some(text) = v.s_rows.get(&row) {
            s.tab(57);
            s.write(text);
        }
    }
    // W ! W:$D(V(8)) V(8) W:$D(S(8)) ?57,S(8) W !   [PRCHDP1.m:42]
    s.nl();
    if let Some(text) = v.v_rows.get(&8) {
        s.write(text);
    }
    if let Some(text) = v.s_rows.get(&8) {
        s.tab(57);
        s.write(text);
    }
    s.nl();
    // I $P(PRCHD1,U,11)]"" W ?38,"DELIVERY LOCATION: ",$P(PRCHD1,U,11),!   [PRCHDP1.m:43]
    let location = piece(&prchd1, "^", 11);
    if follows(location, "") {
        s.tab(38);
        s.write("DELIVERY LOCATION: ");
        s.write(location);
        s.nl();
    }
    s.write(&"_".repeat(80));
    // I $P($G(^PRC(442,D0,24)),U,3)="RMPR" D  K RMPRPO,RMPR664,RMPRR3   [PRCHDP1.m:45-52]
    let prosthetics = st.get_or_empty(&key("PRC", &["442", &d0, "24"]));
    if piece(&prosthetics, "^", 3) == "RMPR" {
        return Err(unsupported("PO-10.b7"));
    }
    fob_prchdp1(s, st, v)
}

/// PO-11 `FTYP^PRCHFPNT` [PRCHFPNT.m:41-53]: the facility type name of the
/// station `PRC("SITE")`, in `PRCHFTYP`, through `^PRC(411.2)` piece 2.
/// `PRCHFTP2` is undefined on this path.
fn ftyp_prchfpnt(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // N PRCSUB   [PRCHFPNT.m:42]
    // S PRCHFTYP="V.A. *FACILITY TYPE UNDEFINED*"   [PRCHFPNT.m:44]
    v.prchftyp = Some("V.A. *FACILITY TYPE UNDEFINED*".to_string());
    // S X=$P($G(^PRC(411,PRC("SITE"),0)),"^",7) S:X="" X=1   [PRCHFPNT.m:45]
    let site = v.prc_site.clone().unwrap_or_default();
    let mut x = piece(&st.get_or_empty(&key("PRC", &["411", &site, "0"])), "^", 7).to_string();
    if x.is_empty() {
        x = "1".to_string();
    }
    // I $P($G(^PRC(442,D0,23)),U,7)]"" S PRCSUB=$P(^(23),U,7) D  ...   [PRCHFPNT.m:46-47]
    let node23 = st.get_or_empty(&key("PRC", &["442", &d0, "23"]));
    if follows(piece(&node23, "^", 7), "") {
        return Err(unsupported("PO-11.b2"));
    }
    // I '$D(PRCHFTP2) S PRCHFTYP=$P($G(^PRC(411.2,+X,0)),U,2)   [PRCHFPNT.m:49]
    let facility = st.get_or_empty(&key("PRC", &["411.2", &num(&x), "0"]));
    v.prchftyp = Some(piece(&facility, "^", 2).to_string());
    v.x = Some(x);
    // K PRCHFTP2   [PRCHFPNT.m:52]
    Ok(())
}

/// PO-12 `^%ZIS` [ZIS.m:28-37] [ZIS1.m:103-113], stub: opens the HOME
/// device (`IOP="HOME"`, `%ZIS=""`). What the HOME device set-up writes, and
/// the `IOF` and `IOSL` it sets, depend on the runtime device and are open
/// item O-01 ([`STUB_CONTEXT`] holds them unresolved), so the device open is
/// BLOCKED: requires a runtime shim, not converted.
fn pct_zis(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt> {
    Err(unsupported("PO-12.b1"))
}

/// PO-13 `FOB^PRCHDP1` [PRCHDP1.m:53-57]: the FOB point, the proposal
/// wrapped at width 16, the purchase authorities, the cost center, the order
/// type and the buyer or agent label; then falls into `DIS` (PO-18). Wrap
/// lines beyond the third are not displayed.
fn fob_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    let prchd0 = v.prchd0.clone().unwrap_or_default();
    let prchd1 = v.prchd1.clone().unwrap_or_default();
    // FOB W !,"FOB POINT: ",$S("O"=$E($P(PRCHD1,U,6)):"ORIGIN","D"=$E($P(PRCHD1,U,6)):"DESTINATION",1:""),
    //  ?29,"|","PROPOSAL: " S DIWL=1,DIWR=16,DIWF="",X=$P(PRCHD1,U,8) K ^UTILITY($J,"W") D DIWP^PRCUTL($G(DA))   [PRCHDP1.m:53]
    let point = match extract(piece(&prchd1, "^", 6), 1, 1).as_str() {
        "O" => "ORIGIN",
        "D" => "DESTINATION",
        _ => "",
    };
    s.nl();
    s.write("FOB POINT: ");
    s.write(point);
    s.tab(29);
    s.write("|");
    s.write("PROPOSAL: ");
    v.diwl = Some(1);
    v.diwr = Some(16);
    v.diwf = Some(String::new());
    v.x = Some(piece(&prchd1, "^", 8).to_string());
    v.utility_w_counts.clear();
    v.utility_w_lines.clear();
    let da = v.da.clone().unwrap_or_default();
    diwp_prcutl(s, st, v, &da)?;
    // K ^TMP($J,"W") S %X="^UTILITY($J,""W"",",%Y="^TMP($J,""W""," D %XY^%RCR   [PRCHDP1.m:54]
    v.tmp_w_counts.clear();
    v.tmp_w_lines.clear();
    pct_xy_pct_rcr(s, st, v)?;
    // W ?40,$G(^TMP($J,"W",1,1,0)),?57,"|AUTHORITY: ",!,"COST CENTER: ",$P(PRCHD0,U,5),
    //  ?29,"|",?40,$G(^TMP($J,"W",1,2,0)),?57,"|  "   [PRCHDP1.m:55]
    s.tab(40);
    s.write(&wrap_line(&v.tmp_w_lines, 1, 1));
    s.tab(57);
    s.write("|AUTHORITY: ");
    s.nl();
    s.write("COST CENTER: ");
    s.write(piece(&prchd0, "^", 5));
    s.tab(29);
    s.write("|");
    s.tab(40);
    s.write(&wrap_line(&v.tmp_w_lines, 1, 2));
    s.tab(57);
    s.write("|  ");
    // S Y=0 F I=1:1 S Y=$O(^PRC(442,D0,14,Y)) Q:'Y  W:I>1 "," W $P($G(^PRC(442.4,+^(Y,0),0)),U,2)   [PRCHDP1.m:56]
    // ^(Y,0) is ^PRC(442,D0,14,Y,0), read unguarded.
    let methods = key("PRC", &["442", &d0, "14"]);
    let mut y = Sub::from_text("0");
    let mut first = true;
    loop {
        match st.order(&methods, Some(&y)) {
            Some(entry) if truthy(entry.text()) => {
                if !first {
                    s.write(",");
                }
                let method = child(&child(&methods, &entry), &Sub::from_text("0"));
                let Some(authority) = st.get(&method) else {
                    return Err(corrupt(&method));
                };
                let abbreviation = st.get_or_empty(&key("PRC", &["442.4", &num(authority), "0"]));
                s.write(piece(&abbreviation, "^", 2));
                first = false;
                y = entry;
            }
            last => {
                v.y = Some(last.map(|sub| sub.text().to_string()).unwrap_or_default());
                break;
            }
        }
    }
    // W !,"TYPE: ",$P(PRCHD1,U,14) D TY W X,?29,"|",?40,$G(^TMP($J,"W",1,3,0)),
    //  ?57,"|"_$S($P($G(^PRC(442,D0,23)),U,11)'="":"BUYER",1:"AGENT")_":"   [PRCHDP1.m:57]
    s.nl();
    s.write("TYPE: ");
    s.write(piece(&prchd1, "^", 14));
    ty_prchdp1(s, st, v)?;
    s.write(v.x.as_deref().unwrap_or(""));
    s.tab(29);
    s.write("|");
    s.tab(40);
    s.write(&wrap_line(&v.tmp_w_lines, 1, 3));
    s.tab(57);
    let node23 = st.get_or_empty(&key("PRC", &["442", &d0, "23"]));
    let role = if piece(&node23, "^", 11).is_empty() {
        "AGENT"
    } else {
        "BUYER"
    };
    s.write(&format!("|{role}:"));
    dis_prchdp1(s, st, v)
}

/// PO-14 `DIWP^PRCUTL` [PRCUTL.m:33-35]: `^DIWP` with the formal `DA`, so
/// the caller's `DA` is the same after the call.
fn diwp_prcutl(s: &mut Session, st: &Store, v: &mut Locals, da: &str) -> Result<(), Halt> {
    let caller_da = v.da.replace(da.to_string());
    let result = diwp(s, st, v);
    v.da = caller_da;
    result
}

/// The length of `text` in characters, as M's `$L`.
fn char_len(text: &str) -> i64 {
    i64::try_from(text.chars().count()).unwrap_or(i64::MAX)
}

/// `PUT^DIWP` [DIWP.m:49]: `S I=^UTILITY($J,"W",DIWL),^(DIWL,I,0)=DIWI`.
fn wrap_put(v: &mut Locals, diwl: i64, diwi: &str) {
    let count = v.utility_w_counts.get(&diwl).copied().unwrap_or_default();
    v.utility_w_lines.insert((diwl, count), diwi.to_string());
}

/// `NEW^DIWP` [DIWP.m:67-70] with `DIWF` empty: `D DIWI` empties the line
/// in progress, and `PRE` adds an empty line,
/// `S I=^UTILITY($J,"W",DIWL),^(DIWL)=I+1,^(DIWL,I+1,0)=""`.
fn wrap_new(v: &mut Locals, diwl: i64, diwi: &mut String) {
    diwi.clear();
    wrap_pre(v, diwl);
}

/// `PRE^DIWP` [DIWP.m:68-70] with `DIWF` empty: adds an empty line to
/// `^UTILITY($J,"W",DIWL)`.
fn wrap_pre(v: &mut Locals, diwl: i64) {
    let count = v.utility_w_counts.get(&diwl).copied().unwrap_or_default() + 1;
    v.utility_w_counts.insert(diwl, count);
    v.utility_w_lines.insert((diwl, count), String::new());
}

/// `C` and `B^DIWP` [DIWP.m:58-62] with `DIWF` empty: appends the text `x`
/// to the line in progress `diwi`, storing each full line in
/// `^UTILITY($J,"W",DIWL)` and starting the next. In line mode (`DIWTC`)
/// the text is appended whole.
fn wrap_text(v: &mut Locals, diwl: i64, diwr: i64, diwi: &mut String, diwtc: bool, mut x: String) {
    // C K DIWP I DIWTC S DIWI=DIWI_X Q
    if diwtc {
        diwi.push_str(&x);
        return;
    }
    loop {
        // B S Z=DIWR-DIWL+1-$L(DIWI) G FULL:$F(X," ")-1>Z
        let room = diwr - diwl + 1 - char_len(diwi);
        let first_space = x
            .chars()
            .position(|c| c == ' ')
            .map_or(-1, |at| i64::try_from(at).map_or(i64::MAX, |at| at + 1));
        if first_space <= room {
            //  F %=Z:-1 I " " contains $E(X,%) S:$E(X,%+1)=" " %=%+1 Q
            let mut at = room;
            while !contains(" ", &extract(&x, at, at)) {
                at -= 1;
            }
            if extract(&x, at + 1, at + 1) == " " {
                at += 1;
            }
            // S Z=$E(X,1,%-1),X=$E(X,%+1,9999) I Z]"" S DIWI=DIWI_Z G S:X]""
            //  S %=$E(Z,$L(Z)) S:%'=" " DIWI=DIWI_$J("",%="."+1),DIWP=1 Q
            let chunk = extract(&x, 1, at - 1);
            x = extract(&x, at + 1, 9999);
            if follows(&chunk, "") {
                diwi.push_str(&chunk);
                if x.is_empty() {
                    match chunk.chars().last() {
                        Some('.') => diwi.push_str("  "),
                        Some(' ') => {}
                        _ => diwi.push(' '),
                    }
                    return;
                }
                // S D PUT,NEW G B:X]""
                wrap_put(v, diwl, diwi);
                wrap_new(v, diwl, diwi);
                continue;
            }
            // An empty Z ends the IF of that line, and control runs on into FULL.
        }
        // FULL I $P(DIWF,"I",2)'<$L(DIWI) S DIWI=DIWI_$P(X," ",1),X=$P(X," ",2,9999)
        if diwi.is_empty() {
            diwi.push_str(piece(&x, " ", 1));
            x = piece_range(&x, " ", 2, 9999);
        }
        // S D PUT,NEW G B:X]"" Q
        wrap_put(v, diwl, diwi);
        wrap_new(v, diwl, diwi);
        if x.is_empty() {
            return;
        }
    }
}

/// PO-15 `^DIWP` [DIWP.m:28-77], reproduced for plain text with `DIWF`
/// empty (PO-15.b1): adds the line `X` to the word-wrap scratch
/// `^UTILITY($J,"W",DIWL)` held in `Locals`, wrapping at `DIWR`, so that
/// successive calls continue one paragraph. A `|` window (PO-15.b2), a
/// `|TAB` or any `DIWF` letter (PO-15.b3) is not converted.
fn diwp(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let diwl = v.diwl.unwrap_or_default();
    let diwr = v.diwr.unwrap_or_default();
    // S:'$L(X) X=" "   [DIWP.m:28]
    let mut x = v.x.clone().unwrap_or_default();
    if x.is_empty() {
        x = " ".to_string();
    }
    // S DIWTC=X contains $C(124)_"TAB" S:'$D(DN) DN=1   [DIWP.m:29]
    if contains(&x, "|TAB") {
        return Err(unsupported("PO-15.b3"));
    }
    let mut diwtc = false;
    if v.dn.is_none() {
        v.dn = Some("1".to_string());
    }
    // LN S:'$D(DIWF) DIWF="" S:'DIWTC DIWTC=DIWF contains "N" S DIWX=X,DIW=$C(124),
    //  I=$P(DIWF,"C",2) I I S DIWR=DIWL+I-1   [DIWP.m:30]
    if !v.diwf.get_or_insert_with(String::new).is_empty() {
        return Err(unsupported("PO-15.b3"));
    }
    // A "|" in DIWX opens a window [DIWP.m:37-41]
    if contains(&x, "|") {
        return Err(unsupported("PO-15.b2"));
    }
    let diwx = x.clone();
    // I '$D(^UTILITY($J,"W",DIWL)) S ^(DIWL)=1 K DIWFU,DIWFWU,DIWLL D DIWI
    //  S:'$D(DIWT) DIWT="5,10,15,20,25" G DIW   [DIWP.m:31]
    let mut diwi = String::new();
    if let Some(&count) = v.utility_w_counts.get(&diwl) {
        // S I=^(DIWL),DIWI=^(DIWL,I,0) I DIWI="" D DIWI G Z   [DIWP.m:32]
        // D NEW:DIWTC   [DIWP.m:33]: DIWTC is 0 here
        diwi = wrap_line(&v.utility_w_lines, diwl, count);
        // Z S Z=X?.P!DIWTC I X?1" ".E!Z S DIWTC=1 D NEW:DIWI]"" S DIWTC=Z   [DIWP.m:34]
        let z = x.chars().all(m_punctuation) || diwtc;
        if x.starts_with(' ') || z {
            if follows(&diwi, "") {
                wrap_new(v, diwl, &mut diwi);
            }
            diwtc = z;
        }
    } else {
        v.utility_w_counts.insert(diwl, 1);
        if v.diwt.is_none() {
            v.diwt = Some(DEFAULT_TAB_STOPS.to_string());
        }
    }
    // DIW I DIWF contains "X" ...   [DIWP.m:36]: no X flag
    //  S X=$P(DIWX,DIW,1) D C:X]"" S X=$P(DIWX,DIW,1),DIWX=$P(DIWX,DIW,2,9999) G D:DIWX=""   [DIWP.m:37]
    wrap_text(v, diwl, diwr, &mut diwi, diwtc, x);
    v.x = Some(diwx);
    // D K DIWP D PUT,PRE:DIWTC S:DIWTC DIWI="" Q   [DIWP.m:43]
    wrap_put(v, diwl, &diwi);
    if diwtc {
        wrap_pre(v, diwl);
    }
    Ok(())
}

/// PO-16 `%XY^%RCR` [DIRCR.m:24-30] [ZTRCR.m:21-27], reproduced for the
/// literal `%X="^UTILITY($J,""W"","` and `%Y="^TMP($J,""W"","`
/// [PRCHDP1.m:54] [PRCHDP2.m:48]: copies every node of the `^UTILITY`
/// scratch to the same subscripts of the `^TMP` scratch.
fn pct_xy_pct_rcr(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // F  S %Z=$Q(@%Z) Q:$P(%Z,%A)]""!(%Z="")  S @(%Y_$E(%Z,%B,255))=@%Z   [DIRCR.m:29]
    v.tmp_w_counts.extend(&v.utility_w_counts);
    v.tmp_w_lines.extend(
        v.utility_w_lines
            .iter()
            .map(|(at, line)| (*at, line.clone())),
    );
    Ok(())
}

/// PO-17 `TY^PRCHDP1` [PRCHDP1.m:71-72]: the order-type label of the
/// source code, in `X`. M's `=` compares text, so `X=2` holds only for the
/// code text `2`.
fn ty_prchdp1(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // TY S X=+$P(PRCHD1,U,7),X=$P($G(^PRCD(420.8,X,0)),U,1),
    //  X=$S(X=2:"PURCHASE ORDER",X="B":"DELIVERY & PURCHASE ORDER",X="":"",1:"DELIVERY ORDER")
    let code = num(piece(v.prchd1.as_deref().unwrap_or(""), "^", 7));
    let source = st.get_or_empty(&key("PRCD", &["420.8", &code, "0"]));
    let label = match piece(&source, "^", 1) {
        "2" => "PURCHASE ORDER",
        "B" => "DELIVERY & PURCHASE ORDER",
        "" => "",
        _ => "DELIVERY ORDER",
    };
    v.x = Some(label.to_string());
    Ok(())
}

/// PO-18 `DIS^PRCHDP1` [PRCHDP1.m:58-60]: the delivery date, the buyer or
/// agent's name and the discount-terms label; then falls into `CON` (PO-20).
fn dis_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    let prchd0 = v.prchd0.clone().unwrap_or_default();
    let prchd1 = v.prchd1.clone().unwrap_or_default();
    // DIS W !,"DELIVER ON/BEFORE " S Y=$P(PRCHD0,U,10) D DT W ?29,"|","CONTRACT: "   [PRCHDP1.m:58]
    s.nl();
    s.write("DELIVER ON/BEFORE ");
    v.y = Some(piece(&prchd0, "^", 10).to_string());
    dt_prchdp1(s, st, v)?;
    s.tab(29);
    s.write("|");
    s.write("CONTRACT: ");
    //  S Y=+$P(PRCHD1,U,10),Y=$P($G(^VA(200,Y,0)),U,1) W ?57,"|  ",$P(Y,",",2)," ",$P(Y,",",1)   [PRCHDP1.m:59]
    let agent = num(piece(&prchd1, "^", 10));
    let person = st.get_or_empty(&key("VA", &["200", &agent, "0"]));
    let name = piece(&person, "^", 1).to_string();
    s.tab(57);
    s.write("|  ");
    s.write(piece(&name, ",", 2));
    s.write(" ");
    s.write(piece(&name, ",", 1));
    v.y = Some(name);
    //  W !,"DISCOUNT TERM: " S X=0 I $D(^PRC(442,D0,5,0)) F I=1:1:2 ...   [PRCHDP1.m:60]
    s.nl();
    s.write("DISCOUNT TERM: ");
    v.x = Some("0".to_string());
    if st.data(&key("PRC", &["442", &d0, "5", "0"])) != 0 {
        return Err(unsupported("PO-18.b2"));
    }
    con_prchdp1(s, st, v)
}

/// PO-19 `DT^PRCHDP1` [PRCHDP1.m:69-70]: writes the FileMan date `Y` as
/// `M/D/YYYY` with no zero padding, each operator applied left to right:
/// `(Y\100)#100`, `(Y#100)\1`, `(Y\10000)+1700`. Nothing for a zero `Y`.
fn dt_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // DT I Y W Y\100#100,"/",Y#100\1,"/",Y\10000+1700
    let y = v.y.clone().unwrap_or_default();
    if truthy(&y) {
        s.write(&format!(
            "{}/{}/{}",
            modulo(&int_div(&y, "100"), "100"),
            int_div(&modulo(&y, "100"), "1"),
            add(&int_div(&y, "10000"), "1700")
        ));
    }
    Ok(())
}

/// PO-20 `CON^PRCHDP1` [PRCHDP1.m:61-64]: the three rows of the FCP block,
/// with the order date (PO-22), the appropriation (PO-21), the estimated
/// flag (PO-23) and the total (PO-24), then 80 dashes and `G ^PRCHDP2`.
/// The first row continues the `DISCOUNT TERM: ` line.
fn con_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // CON K Y S X=0 F I=1:1:3 S:(X'="") X=$O(^PRC(442,D0,2,"AC",X)) D:X'=""  D:I=2 APP W ?29,"|"
    //  W:$D(Y) ?38,... W:X'="" ?41,X W ?57,"|" D DAT:I=1,EST:I=2,TOT:I=3 W ! K Y   [PRCHDP1.m:61]
    //  .S:$D(^(X)) Y=$O(^(X,0))   [PRCHDP1.m:62]
    v.y = None;
    let contracts = key("PRC", &["442", &d0, "2", "AC"]);
    let mut x = "0".to_string();
    v.x = Some(x.clone());
    for row in 1..=3 {
        if !x.is_empty() {
            x = st
                .order(&contracts, Some(&Sub::from_text(&x)))
                .map(|sub| sub.text().to_string())
                .unwrap_or_default();
            v.x = Some(x.clone());
        }
        if !x.is_empty() {
            return Err(unsupported("PO-20.b1"));
        }
        if row == 2 {
            app_prchdp1(s, st, v)?;
        }
        s.tab(29);
        s.write("|");
        // W:$D(Y) and W:X'="" write nothing: Y is killed before each row and
        // set again only by DAT and EST after these WRITEs, and X is empty.
        s.tab(57);
        s.write("|");
        match row {
            1 => dat_prchdp1(s, st, v)?,
            2 => est_prchdp1(s, st, v)?,
            _ => tot_prchdp1(s, st, v)?,
        }
        s.nl();
        v.y = None;
    }
    s.write(&"-".repeat(80));
    prchdp2(s, st, v)
}

/// PO-21 `APP^PRCHDP1` [PRCHDP1.m:65]: the appropriation and the first word
/// of the fund control point.
fn app_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // APP W "APP: ",$P(PRCHD0,U,4),"-",$P($P(PRCHD0,U,3)," ",1) Q
    let prchd0 = v.prchd0.as_deref().unwrap_or("");
    s.write("APP: ");
    s.write(piece(prchd0, "^", 4));
    s.write("-");
    s.write(piece(piece(prchd0, "^", 3), " ", 1));
    Ok(())
}

/// PO-22 `DAT^PRCHDP1` [PRCHDP1.m:66]: the order date (field .1).
fn dat_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // DAT W "DATE: " S Y=$P(PRCHD1,U,15) D DT Q
    s.write("DATE: ");
    v.y = Some(piece(v.prchd1.as_deref().unwrap_or(""), "^", 15).to_string());
    dt_prchdp1(s, st, v)
}

/// PO-23 `EST^PRCHDP1` [PRCHDP1.m:68]: the estimated-order flag (field
/// .08); `ESTIMATED` for `Y` is not exercised (PO-23.b2).
fn est_prchdp1(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // EST S Y=$S($D(^PRC(442,D0,7)):$P(^(7),U,3),1:"") Q:Y'="Y"  W "ESTIMATED" Q
    let d0 = v.d0.clone().unwrap_or_default();
    let flag = defined(st, &key("PRC", &["442", &d0, "7"]))?
        .map(|node7| piece(&node7, "^", 3).to_string())
        .unwrap_or_default();
    let estimated = flag == "Y";
    v.y = Some(flag);
    if estimated {
        return Err(unsupported("PO-23.b2"));
    }
    Ok(())
}

/// PO-24 `TOT^PRCHDP1` [PRCHDP1.m:67]: the order total (field 91) as
/// `$J(total,8,2)`.
fn tot_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    s.write("TOTAL: ");
    s.write(&justify(
        piece(v.prchd0.as_deref().unwrap_or(""), "^", 15),
        8,
        Some(2),
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Line items and closing: PO-25 to PO-40
// ---------------------------------------------------------------------------

/// PO-25 `^PRCHDP2` [PRCHDP2.m:9-18]: the halt prompt, the column header,
/// each line item and the estimated-shipping line; then `G COM` (PO-31).
fn prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // N PRCHAMNT,PRCHAMCT S PRCHAMNT=0 I $D(^PRC(442,D0,6,0)) S PRCHAMCT=$P(^PRC(442,D0,6,0),U,3),PRCHAMNT=1   [PRCHDP2.m:9]
    // (PRCHAMCT is read only by the amendment code, PO-33.b2.)
    v.prchamnt = Some("0".to_string());
    if defined(st, &key("PRC", &["442", &d0, "6", "0"]))?.is_some() {
        v.prchamnt = Some("1".to_string());
    }
    // W !?8,"ENTER '^' TO HALT: " S PRCHDQ=0 R X:DTIME S:X contains "^" PRCHDQ=1 G ASK2:PRCHDQ D HDR   [PRCHDP2.m:10]
    s.nl();
    s.tab(8);
    s.write("ENTER '^' TO HALT: ");
    v.prchdq = Some("0".to_string());
    let answer = read(s)?;
    let halted = contains(&answer, "^");
    v.x = Some(answer);
    if halted {
        return Err(unsupported("PO-25.b1"));
    }
    hdr_prchdp2(s, st, v)?;
    // S (N,PRCHDI)=0 F I=0:0 S PRCHDI=$O(^PRC(442,D0,2,PRCHDI)) Q:PRCHDI'>0  S PRCHDI0=^(PRCHDI,0),
    //  PRCHDI2=$S($D(^(2)):^(2),1:""),N=+PRCHDI0 D ITEM G:PRCHDQ ASK2   [PRCHDP2.m:11]
    // ^(PRCHDI,0) is ^PRC(442,D0,2,PRCHDI,0), read unguarded; ^(2) is ^PRC(442,D0,2,PRCHDI,2).
    v.n = Some("0".to_string());
    let items = key("PRC", &["442", &d0, "2"]);
    let mut item = Sub::from_text("0");
    let mut any_item = false;
    loop {
        let next = st.order(&items, Some(&item));
        v.prchdi = Some(
            next.as_ref()
                .map(|sub| sub.text().to_string())
                .unwrap_or_default(),
        );
        let Some(entry) = next.filter(|sub| positive(sub.text())) else {
            break;
        };
        any_item = true;
        let item_key = child(&items, &entry);
        let zero = child(&item_key, &Sub::from_text("0"));
        let Some(item0) = st.get(&zero).map(str::to_string) else {
            return Err(corrupt(&zero));
        };
        let item2 = defined(st, &child(&item_key, &Sub::from_text("2")))?.unwrap_or_default();
        v.n = Some(num(&item0));
        v.prchdi0 = Some(item0);
        v.prchdi2 = Some(item2);
        item_prchdp2(s, st, v)?;
        if truthy(v.prchdq.as_deref().unwrap_or("")) {
            return Err(unsupported("PO-25.b4"));
        }
        item = entry;
    }
    if !any_item {
        return Err(unsupported("PO-25.b3"));
    }
    // S PRCHDI=0 F I=0:0 S PRCHDI=$O(^PRC(442,D0,3,PRCHDI)) Q:PRCHDI'>0  ... D DIS   [PRCHDP2.m:12]
    if first_after_zero(st, &key("PRC", &["442", &d0, "3"])).is_some_and(|sub| positive(sub.text()))
    {
        return Err(unsupported("PO-25.b5"));
    }
    // I $P(PRCHD0,U,13)>0 W !?2,$J(N+1,3),?7,"EST. SHIPPING AND/OR HANDLING",?58,$J($P(PRCHD0,U,13),7,2)   [PRCHDP2.m:13]
    let shipping = piece(v.prchd0.as_deref().unwrap_or(""), "^", 13).to_string();
    if positive(&shipping) {
        let line = add(v.n.as_deref().unwrap_or(""), "1");
        s.nl();
        s.tab(2);
        s.write(&justify(&line, 3, None));
        s.tab(7);
        s.write("EST. SHIPPING AND/OR HANDLING");
        s.tab(58);
        s.write(&justify(&shipping, 7, Some(2)));
    }
    // G:'$D(^PRC(442,D0,15,0)) COM ...   [PRCHDP2.m:14-18]
    if st.data(&key("PRC", &["442", &d0, "15", "0"])) != 0 {
        return Err(unsupported("PO-25.b7"));
    }
    com_prchdp2(s, st, v)
}

/// PO-26 `HDR^PRCHDP2` [PRCHDP2.m:42-43]: the form feed and the column
/// header, all one WRITE under `$Y>0`, then 80 dashes. The form feed is
/// BLOCKED by open item O-01 (see [`form_feed`]).
fn hdr_prchdp2(s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt> {
    // HDR W:$Y>0 @IOF,!!?55,"UNIT",?70,"TOTAL",!,"ITEM",?15,"DESCRIPTION",?42,"QTY",?46,"UNIT",
    //  ?55,"COST",?70,"COST",! F I=1:1:80 W "-"
    form_feed(s, "PO-26.b1")?;
    s.write(&"-".repeat(80));
    Ok(())
}

/// PO-27 `ITEM^PRCHDP2` [PRCHDP2.m:44-66]: one line item `PRCHDI` of order
/// `D0`: its line number (piece 1 of its zero node, not its IEN), first
/// description line wrapped at width 33, quantity, unit, unit cost and total
/// cost, then its detail lines. A purchase-card order (node 23 piece 11 `S`)
/// omits the number, quantity, unit and unit cost.
fn item_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    let item = v.prchdi.clone().unwrap_or_default();
    let item0 = v.prchdi0.clone().unwrap_or_default();
    let item2 = v.prchdi2.clone().unwrap_or_default();
    // ITEM S DIWL=1,DIWR=33,DIWF="",PRCHDIW=0 K ^UTILITY($J,"W")   [PRCHDP2.m:44]
    v.diwl = Some(1);
    v.diwr = Some(33);
    v.diwf = Some(String::new());
    v.utility_w_counts.clear();
    v.utility_w_lines.clear();
    //  N PURCTYPE,PURPIPE,PRCHI,PRCHJ S:$P($G(^PRC(442,D0,23)),"^",11)="S" PURCTYPE=1   [PRCHDP2.m:45]
    let caller_purpipe = v.purpipe.take();
    let node23 = st.get_or_empty(&key("PRC", &["442", &d0, "23"]));
    let purchase_card = piece(&node23, "^", 11) == "S";
    pipeck_prchdp2(s, st, v)?;
    //  F PRCHJ=1:1 S PRCHDIW=$O(^PRC(442,D0,2,PRCHDI,1,PRCHDIW)) Q:PRCHDIW'>0  S X=$S($D(^(PRCHDIW,0)):^(0),1:"")
    //   S:PURPIPE DIWF=$G(DIWF)_"|"  D DIWP^PRCUTL($G(DA))   [PRCHDP2.m:47]
    // ^(PRCHDIW,0) and ^(0) are ^PRC(442,D0,2,PRCHDI,1,PRCHDIW,0).
    let description = key("PRC", &["442", &d0, "2", &item, "1"]);
    let mut line = Sub::from_text("0");
    let mut any_line = false;
    while let Some(next) = st
        .order(&description, Some(&line))
        .filter(|sub| positive(sub.text()))
    {
        any_line = true;
        let text = child(&child(&description, &next), &Sub::from_text("0"));
        v.x = Some(defined(st, &text)?.unwrap_or_default());
        if truthy(v.purpipe.as_deref().unwrap_or("")) {
            let flags = v.diwf.clone().unwrap_or_default();
            v.diwf = Some(format!("{flags}|"));
        }
        let da = v.da.clone().unwrap_or_default();
        diwp_prcutl(s, st, v, &da)?;
        line = next;
    }
    if !any_line {
        return Err(unsupported("PO-27.b2"));
    }
    //  K ^TMP($J,"W") S %X="^UTILITY($J,""W"",",%Y="^TMP($J,""W""," D %XY^%RCR   [PRCHDP2.m:48]
    v.tmp_w_counts.clear();
    v.tmp_w_lines.clear();
    pct_xy_pct_rcr(s, st, v)?;
    //  S PRCHDCNT=$S($D(^TMP($J,"W",1)):^(1),1:"") W ! I $G(PURCTYPE)="" W ?2,$J(+$P(PRCHDI0,U,1),3)   [PRCHDP2.m:49]
    let count = v
        .tmp_w_counts
        .get(&1)
        .map(i64::to_string)
        .unwrap_or_default();
    v.prchdcnt = Some(count.clone());
    s.nl();
    if !purchase_card {
        s.tab(2);
        s.write(&justify(&num(piece(&item0, "^", 1)), 3, None));
    }
    //  W ?7,$S($D(^(1,1,0)):^(0),1:"")   [PRCHDP2.m:50]; ^(1,1,0) is ^TMP($J,"W",1,1,0)
    s.tab(7);
    s.write(&wrap_line(&v.tmp_w_lines, 1, 1));
    //  I $G(PURCTYPE)="" W ?40,$J($P(PRCHDI0,U,2),5),?47,$S($D(^PRCD(420.5,+$P(PRCHDI0,U,3),0)):$P(^(0),U,1),1:"")   [PRCHDP2.m:51]
    let unit = key("PRCD", &["420.5", &num(piece(&item0, "^", 3)), "0"]);
    if !purchase_card {
        s.tab(40);
        s.write(&justify(piece(&item0, "^", 2), 5, None));
        s.tab(47);
        if let Some(node) = defined(st, &unit)? {
            s.write(piece(&node, "^", 1));
        }
    }
    //  S X=$P($P(PRCHDI0,U,9),".",2) I $G(PURCTYPE)="" W ?52,$S($L(X)>3:$J($P(PRCHDI0,U,9),5,4),
    //   $L(X)>2:$J($P(PRCHDI0,U,9),6,3),$P(PRCHDI0,U,9)="N/C":"    N/C",1:$J($P(PRCHDI0,U,9),7,2))   [PRCHDP2.m:52]
    let cost = piece(&item0, "^", 9);
    let fraction = piece(cost, ".", 2).to_string();
    let decimals = fraction.chars().count();
    v.x = Some(fraction);
    if !purchase_card {
        let shown = if decimals > 3 {
            justify(cost, 5, Some(4))
        } else if decimals > 2 {
            justify(cost, 6, Some(3))
        } else if cost == "N/C" {
            "    N/C".to_string()
        } else {
            justify(cost, 7, Some(2))
        };
        s.tab(52);
        s.write(&shown);
    }
    //  W ?67,$J($P(PRCHDI2,U,1),7,2)   [PRCHDP2.m:53]
    s.tab(67);
    s.write(&justify(piece(&item2, "^", 1), 7, Some(2)));
    //  I PRCHDCNT>1 S K=1 F  S K=$O(^TMP($J,"W",1,K)) Q:K=""!(K'>0)  D:$Y+5>IOSL ASK Q:PRCHDQ  W !?8,^(K,0)   [PRCHDP2.m:54]
    if num_cmp(&count, "1") == Ordering::Greater {
        return Err(unsupported("PO-27.b3"));
    }
    //  Q:PRCHDQ   [PRCHDP2.m:55]
    if truthy(v.prchdq.as_deref().unwrap_or("")) {
        return Err(unsupported("PO-27.b4"));
    }
    //  W:$P(PRCHDI0,U,6)]"" !?8,"STK#: ",$P(PRCHDI0,U,6) W:$P(PRCHDI0,U,13)]"" !,?8,"NSN:  ",$P(PRCHDI0,U,13)
    //  W:$P($G(^PRC(442,D0,2,PRCHDI,4)),U,12)]"" !,?8,"FOOD GROUP: ",$P(^(4),U,12)   [PRCHDP2.m:56]
    let node4 = st.get_or_empty(&key("PRC", &["442", &d0, "2", &item, "4"]));
    for (label, value) in [
        ("STK#: ", piece(&item0, "^", 6)),
        ("NSN:  ", piece(&item0, "^", 13)),
        ("FOOD GROUP: ", piece(&node4, "^", 12)),
    ] {
        if follows(value, "") {
            s.nl();
            s.tab(8);
            s.write(label);
            s.write(value);
        }
    }
    //  W:$P(PRCHDI2,U,8)]"" !,?8,"QTY PREV RCVD: ",$J($P(PRCHDI2,U,8),5)
    //  I $D(^PRC(442,D0,2,PRCHDI,3,"AC")) W !,?8,"PARTIAL NO.: " ...   [PRCHDP2.m:57]
    let received = piece(&item2, "^", 8);
    if follows(received, "") {
        s.nl();
        s.tab(8);
        s.write("QTY PREV RCVD: ");
        s.write(&justify(received, 5, None));
    }
    if st.data(&key("PRC", &["442", &d0, "2", &item, "3", "AC"])) != 0 {
        return Err(unsupported("PO-27.b5"));
    }
    let mut zz = "0".to_string();
    edistat_prchutl(s, st, v, &d0, &item, &mut zz)?;
    //  I $G(PURCTYPE)="",$P(PRCHDI0,U,12) W:'ZZ ! W ?8,"Items per ",
    //   $S($D(^PRCD(420.5,+$P(PRCHDI0,U,3),0)):$P(^(0),U,1),1:""),": ",$P(PRCHDI0,U,12),!   [PRCHDP2.m:59]
    let multiple = piece(&item0, "^", 12);
    if !purchase_card && truthy(multiple) {
        if !truthy(&zz) {
            s.nl();
        }
        s.tab(8);
        s.write("Items per ");
        if let Some(node) = defined(st, &unit)? {
            s.write(piece(&node, "^", 1));
        }
        s.write(": ");
        s.write(multiple);
        s.nl();
    }
    ask_prchdp2(s, st, v)?;
    if s.x() > 1 {
        s.nl();
    }
    //  W ?8,"BOC: ",$P($P(PRCHDI0,U,4)," ",1) S FMSLN=$O(^PRC(442,D0,22,"B",+$P(PRCHDI0,U,4),0))   [PRCHDP2.m:62]
    let boc = piece(&item0, "^", 4);
    s.tab(8);
    s.write("BOC: ");
    s.write(piece(boc, " ", 1));
    let fms_index = key("PRC", &["442", &d0, "22", "B", &num(boc)]);
    let fms_line = first_after_zero(st, &fms_index)
        .map(|sub| sub.text().to_string())
        .unwrap_or_default();
    //  I FMSLN>0,'$P($G(^PRC(442,D0,23)),U,8) S FMSLN="00"_$P($G(^PRC(442,D0,22,FMSLN,0)),U,3),
    //   FMSLN=$E(FMSLN,$L(FMSLN)-2,99) W ?22,"FMS LINE: ",FMSLN   [PRCHDP2.m:63]
    if positive(&fms_line) && !truthy(piece(&node23, "^", 8)) {
        let entry = st.get_or_empty(&key("PRC", &["442", &d0, "22", &fms_line, "0"]));
        let padded = format!("00{}", piece(&entry, "^", 3));
        s.tab(22);
        s.write("FMS LINE: ");
        s.write(&extract(&padded, char_len(&padded) - 2, 99));
    }
    //  W:$P(PRCHDI2,U,2)]"" ?40,"CONTRACT: ",$P(PRCHDI2,U,2)   [PRCHDP2.m:64]
    let contract = piece(&item2, "^", 2);
    if follows(contract, "") {
        s.tab(40);
        s.write("CONTRACT: ");
        s.write(contract);
    }
    s.nl();
    //  Q   [PRCHDP2.m:66]: the NEW [PRCHDP2.m:45] ends
    v.purpipe = caller_purpipe;
    Ok(())
}

/// PO-28 `PIPECK^PRCHDP2` [PRCHDP2.m:96-102]: scans the description lines
/// of every item of the order, not only the current one, for the literal
/// `| IN ` (pipe, space, `IN`, space). `ITEM` runs it for every item, so each
/// item's call scans the whole order again, as in the original [PRCHDP2.m:46].
fn pipeck_prchdp2(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // PIPECK S PURPIPE=0,PRCH=0
    v.purpipe = Some("0".to_string());
    let items = key("PRC", &["442", &d0, "2"]);
    let mut item = Sub::from_text("0");
    //  F PRCHI=1:1 S PRCH=$O(^PRC(442,D0,2,PRCH)),PRCHDIW=0 Q:'PRCH  D  Q:PURPIPE
    while let Some(next) = st
        .order(&items, Some(&item))
        .filter(|sub| truthy(sub.text()))
    {
        //  . F PRCHJ=1:1 S PRCHDIW=$O(^PRC(442,D0,2,PRCH,1,PRCHDIW)) Q:PRCHDIW'>0
        //  .  S X=$S($D(^(PRCHDIW,0)):^(0),1:"") D  Q:PURPIPE
        //  . . I X contains "| IN " S PURPIPE=1
        let description = child(&child(&items, &next), &Sub::from_text("1"));
        let mut line = Sub::from_text("0");
        let mut any_line = false;
        while let Some(entry) = st
            .order(&description, Some(&line))
            .filter(|sub| positive(sub.text()))
        {
            any_line = true;
            let text = child(&child(&description, &entry), &Sub::from_text("0"));
            let x = defined(st, &text)?.unwrap_or_default();
            let pipe_in = contains(&x, "| IN ");
            v.x = Some(x);
            if pipe_in {
                v.purpipe = Some("1".to_string());
                return Err(unsupported("PO-28.b2"));
            }
            line = entry;
        }
        if !any_line {
            return Err(unsupported("PO-28.b3"));
        }
        item = next;
    }
    Ok(())
}

/// PO-29 `EDISTAT^PRCHUTL` [PRCHUTL.m:65-80], called as
/// `D EDISTAT^PRCHUTL(D0,PRCHDI,.ZZ)` [PRCHDP2.m:58]: the item's EDI status
/// lines, of which none are written when both status codes are empty. Its
/// `X`, `Y` and `C` are NEWed, so the caller's are untouched; `_linecnt` is
/// the caller's `ZZ` by reference, which only the unexercised status lines
/// (PO-29.b3) count into.
fn edistat_prchutl(
    _s: &mut Session,
    st: &Store,
    _v: &mut Locals,
    d0: &str,
    d1: &str,
    _linecnt: &mut String,
) -> Result<(), Halt> {
    // EDISTAT(D0,D1,LINECNT) N X,Y,C S:'$D(LINECNT) LINECNT=0   [PRCHUTL.m:65-72]
    //  I $D(^PRC(442,D0,2,D1,2)) S X=$P(^(2),"^",9,12) D   [PRCHUTL.m:73]
    //  .I $P(X,"^",1)=""&($P(X,"^",3)="") Q   [PRCHUTL.m:74]
    let Some(node2) = defined(st, &key("PRC", &["442", d0, "2", d1, "2"]))? else {
        return Err(unsupported("PO-29.b1"));
    };
    let x = piece_range(&node2, "^", 9, 12);
    if piece(&x, "^", 1).is_empty() && piece(&x, "^", 3).is_empty() {
        return Ok(());
    }
    Err(unsupported("PO-29.b3"))
}

/// PO-30 `ASK^PRCHDP2` [PRCHDP2.m:31-32]: the paging test `$Y+5>IOSL`. The
/// HOME device's `IOSL` ([`STUB_CONTEXT`]) is open item O-01, so the test
/// cannot be decided and its exercised outcome, no prompt, is BLOCKED.
fn ask_prchdp2(s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt> {
    // ASK I $Y+5>IOSL W !?8,"ENTER '^' TO HALT: " R X:DTIME S:X contains "^" PRCHDQ=1 D:'PRCHDQ HDR Q
    let Some(iosl) = stub_value("IOSL") else {
        return Err(unsupported("PO-30.b1"));
    };
    let line = add(&s.y().to_string(), "5");
    if num_cmp(&line, iosl) == Ordering::Greater {
        return Err(unsupported("PO-30.b2"));
    }
    Ok(())
}

/// PO-31 `COM^PRCHDP2` [PRCHDP2.m:19-21]: the order comments; with none,
/// `G PT` (PO-32).
fn com_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // COM G:'$D(^PRC(442,D0,4,0)) PT ...
    let d0 = v.d0.clone().unwrap_or_default();
    if st.data(&key("PRC", &["442", &d0, "4", "0"])) != 0 {
        return Err(unsupported("PO-31.b2"));
    }
    pt_prchdp2(s, st, v)
}

/// PO-32 `PT^PRCHDP2` [PRCHDP2.m:22-25]: the 2237 transaction numbers and
/// the amendment lines, of which an order with neither has none; then falls
/// into `ASK2` (PO-34).
fn pt_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    let d0 = v.d0.clone().unwrap_or_default();
    // PT I $O(^PRC(442,D0,13,0)) W !!?8,"V.A. TRANSACTION NUMBERS: " ...   [PRCHDP2.m:22]
    if first_after_zero(st, &key("PRC", &["442", &d0, "13"])).is_some_and(|sub| truthy(sub.text()))
    {
        return Err(unsupported("PO-32.b2"));
    }
    amends_prchdp6(s, st, v)?;
    //  I $D(^PRC(442,D0,6,0)) F PRCHI=0:0 ...   [PRCHDP2.m:24]
    if st.data(&key("PRC", &["442", &d0, "6", "0"])) != 0 {
        return Err(unsupported("PO-32.b3"));
    }
    //  K ^TMP($J,"PRCHDP6")   [PRCHDP2.m:25]: only the amendment history (PO-33.b2) builds that scratch.
    // ASK2 ... (fall-through)
    ask2_prchdp2(s, st, v)
}

/// PO-33 `AMENDS^PRCHDP6` [PRCHDP6.m:126-152]: the amendment history, which
/// an order without node 6 does not have.
fn amends_prchdp6(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // AMENDS ;SET UP AMENDMENT HISTORY
    //  Q:'$D(^PRC(442,D0,6))
    let d0 = v.d0.clone().unwrap_or_default();
    if st.data(&key("PRC", &["442", &d0, "6"])) != 0 {
        return Err(unsupported("PO-33.b2"));
    }
    Ok(())
}

/// PO-34 `ASK2^PRCHDP2` [PRCHDP2.m:26]: the delivery schedule (PO-35) and,
/// for an order with receiving reports, the review prompt; with none,
/// `G ASK1` (PO-38).
fn ask2_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // ASK2 D:'PRCHDQ EN^PRCHDP4 G:'$O(^PRC(442,D0,11,0)) ASK1 W ! S %A="   Review a Receiving Report " ...
    if truthy(v.prchdq.as_deref().unwrap_or("")) {
        return Err(unsupported("PO-34.b3"));
    }
    en_prchdp4(s, st, v)?;
    let d0 = v.d0.clone().unwrap_or_default();
    if first_after_zero(st, &key("PRC", &["442", &d0, "11"])).is_some_and(|sub| truthy(sub.text()))
    {
        return Err(unsupported("PO-34.b2"));
    }
    ask1_prchdp2(s, st, v)
}

/// PO-35 `EN^PRCHDP4` [PRCHDP4.m:4-5]: the online delivery-schedule display;
/// falls into `EN2` (PO-36). The flags it sets, `PRCHONL=1` and
/// `PRCHQUIT=0`, are read only by the schedule print, which no order without
/// a schedule reaches.
fn en_prchdp4(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    en2_prchdp4(s, st, v)
}

/// PO-36 `EN2^PRCHDP4` [PRCHDP4.m:7-11]: the delivery schedule filed under
/// the order number in `^PRC(442.8,"B")`; with none, `G EXDELV` (PO-37).
fn en2_prchdp4(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // EN2 ... Q:'$D(D0)  S PRCHPONO=$P(^PRC(442,D0,0),U,1),PRCHQUIT="" G:PRCHPONO="" EXDELV K ^TMP($J,"MDEL")   [PRCHDP4.m:8]
    // ^PRC(442,D0,0) is read unguarded.
    let d0 = v.d0.clone().unwrap_or_default();
    let zero = key("PRC", &["442", &d0, "0"]);
    let Some(node0) = st.get(&zero) else {
        return Err(corrupt(&zero));
    };
    let number = piece(node0, "^", 1);
    if number.is_empty() {
        return Err(unsupported("PO-36.b2"));
    }
    //  F PRCHI=0:0 S PRCHI=$O(^PRC(442.8,"B",PRCHPONO,PRCHI)) Q:'PRCHI  I $D(^PRC(442.8,PRCHI,0)) S Y(0)=^(0) D BLD   [PRCHDP4.m:9]
    let schedule = key("PRC", &["442.8", "B", number]);
    if first_after_zero(st, &schedule).is_some_and(|sub| truthy(sub.text())) {
        return Err(unsupported("PO-36.b3"));
    }
    //  G:'$D(^TMP($J,"MDEL")) EXDELV   [PRCHDP4.m:10]: only BLD fills that scratch
    exdelv_prchdp4(s, st, v)
}

/// PO-37 `EXDELV^PRCHDP4` [PRCHDP4.m:13-14]: a newline, then the kills.
fn exdelv_prchdp4(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    s.nl();
    v.x = None;
    v.y = None;
    Ok(())
}

/// PO-38 `ASK1^PRCHDP2` [PRCHDP2.m:33-41]: the closing prompt, whose answer
/// is not used; then `G Q` (PO-39).
fn ask1_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt> {
    // ASK1 I $G(PRCHAMNT)=2 D  ...   [PRCHDP2.m:33-40]
    if v.prchamnt.as_deref() == Some("2") {
        return Err(unsupported("PO-38.b1"));
    }
    //  W !,$C(7) G:PRCHDQ Q W "END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: " R X:DTIME G Q   [PRCHDP2.m:41]
    s.nl();
    s.bell();
    if truthy(v.prchdq.as_deref().unwrap_or("")) {
        return Err(unsupported("PO-38.b2"));
    }
    s.write("END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ");
    v.x = Some(read(s)?);
    q_prchdp2(s, st, v)
}

/// PO-39 `Q^PRCHDP2` [PRCHDP2.m:94-95]: kills the display's locals and
/// scratch. Its `W @IOF` is commented out, so it writes nothing.
fn q_prchdp2(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    v.n = None;
    v.dic_a = None;
    v.dic_b = None;
    v.dic_s = None;
    v.diwf = None;
    v.diwl = None;
    v.diwr = None;
    v.prchdi = None;
    v.prchd0 = None;
    v.prchd1 = None;
    v.prchftyp = None;
    v.prchdsit = None;
    v.prchdshp = None;
    v.prchdst = None;
    v.prchds = None;
    v.prchdv = None;
    v.prchdq = None;
    v.prchdi0 = None;
    v.prchdi2 = None;
    v.prchdcnt = None;
    v.s_rows.clear();
    v.v_rows.clear();
    v.tmp_w_counts.clear();
    v.tmp_w_lines.clear();
    v.utility_w_counts.clear();
    v.utility_w_lines.clear();
    Ok(())
}

/// PO-40 `K^PRCHRPT` [PRCHRPT.m:106-108]: the option's cleanup after each
/// display, including `PRCHNRQ`, so the next order's label is decided
/// afresh.
fn k_prchrpt(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt> {
    v.prchnrq = None;
    v.x = None;
    v.y = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SPEC.md` R1: the station prompt, written before its READ.
    const STATION_PROMPT_BYTES: &str = "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// ";

    /// `SPEC.md` R1 and "Consequences": the station echo, then the order
    /// prompt [PRCHPAT.m:83] [DIC.m:25] [DIC11.m:17].
    const ORDER_PROMPT_BYTES: &str =
        "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500//       CAMP MASTER\n\n\nP.O./REQ.NO.: ";

    /// `SPEC.md` R4 and "Consequences": the exhausted search is BLOCKED by
    /// open item O-03. Once O-03 closes, every assertion on this constant
    /// becomes an `Ok` holding the R3 bytes after the order prompt.
    const NO_MATCH: &str = "PO-09.b14";

    /// `SPEC.md` "Consequences": a found order stops at `^%ZIS` while open
    /// item O-01 stands.
    const DEVICE_GATE: &str = "PO-12.b1";

    /// File 442 with one order, `500-A12345`, that passes the lookup screen,
    /// filed under index `C` as `A12345`, with its status and MOP entries.
    const ORDER_FILE: &str = r#"^PRC(442,0)="PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^1^1"
^PRC(442,1,0)="500-A12345^2^101 GENERAL^3660160^2100^^^^^3261115^^^^^251"
^PRC(442,1,1)="7^1^1^^^D^2^QUOTE 99^^12^^^^SIMPLIFIED ACQ^3261028"
^PRC(442,1,7)="20^^N"
^PRC(442,"C","A12345",1)=""
^PRCD(442.3,20,0)="Ordered (No Fiscal Action Required)^20"
^PRCD(442.5,2,0)="REQUISITION^RQ"
"#;

    /// Orders 2 and 3, both filed under `B22222` and both rejected by the
    /// screen: order 2 by its MOP of 12, order 3 by its station 600.
    const SCREENED_ORDERS: &str = r#"^PRC(442,2,0)="500-B22222^12^101 GENERAL^3660160^2100"
^PRC(442,3,0)="600-B22222^2^101 GENERAL^3660160^2100"
^PRC(442,"C","B22222",2)=""
^PRC(442,"C","B22222",3)=""
"#;

    /// The vendor file header in the fixture's form, without whose value the
    /// nested lookup of R6 reads nothing more of file 440; vendor 7,
    /// `ACME SUPPLY CO`; and vendor 8, which order 1 references through
    /// index `D`.
    const VENDORS: &str = r#"^PRC(440,0)="VENDOR^440OIs^8^2"
^PRC(440,7,0)="ACME SUPPLY CO"
^PRC(440,8,0)="OTHER VENDOR"
^PRC(440,"B","ACME SUPPLY CO",7)=""
^PRC(440,"B","OTHER VENDOR",8)=""
^PRC(442,"D",8,1)=""
"#;

    fn load(text: &str) -> Result<Store, String> {
        Store::from_zwr(text).map_err(|error| error.to_string())
    }

    fn order_store(extra: &str) -> Result<Store, String> {
        load(&format!("{ORDER_FILE}{extra}"))
    }

    fn inputs(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_string()).collect()
    }

    fn unsupported_branch(id: &str) -> Result<InquiryOutput, InquiryError> {
        Err(InquiryError::UnsupportedBranch(id.to_string()))
    }

    fn ok_transcript(transcript: &str) -> Result<InquiryOutput, InquiryError> {
        Ok(InquiryOutput {
            transcript: transcript.to_string(),
            returned: None,
        })
    }

    /// `Locals` for order 1 of [`ORDER_FILE`], for tests that call a display contract
    /// directly: `PRC("SITE")` from the station prompt [PRCFSITE.m:18], `PRCHPO` and `DA`
    /// from the order lookup [PRCHPAT.m:83], `D0` from `EN30^PRCHRPT` [PRCHRPT.m:74], and
    /// the `PRCHD0` and `PRCHD1` nodes `^PRCHDP1` loads [PRCHDP1.m:4].
    fn order_locals(store: &Store) -> Locals {
        Locals {
            prc_site: Some("500".to_string()),
            d0: Some("1".to_string()),
            da: Some("1".to_string()),
            prchpo: Some("1".to_string()),
            prchd0: Some(store.get_or_empty(&key("PRC", &["442", "1", "0"]))),
            prchd1: Some(store.get_or_empty(&key("PRC", &["442", "1", "1"]))),
            ..Locals::default()
        }
    }

    #[test]
    fn input_exhaustion_returns_partial_transcript() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(inquire(&store, &[]), ok_transcript(STATION_PROMPT_BYTES));
        assert_eq!(
            inquire(&store, &inputs(&[""])),
            ok_transcript(ORDER_PROMPT_BYTES)
        );
        Ok(())
    }

    #[test]
    fn leftover_input_ignored() -> Result<(), String> {
        let store = order_store("")?;
        let exact = inquire(&store, &inputs(&["", "^"]));
        assert_eq!(exact, ok_transcript(ORDER_PROMPT_BYTES));
        assert_eq!(inquire(&store, &inputs(&["", "^", "A12345", "x"])), exact);
        Ok(())
    }

    #[test]
    fn non_empty_station_answer_is_unsupported_branch() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["500"])),
            unsupported_branch("PO-05.b2")
        );
        Ok(())
    }

    #[test]
    fn help_at_order_prompt_is_unsupported_branch() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "?"])),
            unsupported_branch("PO-09.b4")
        );
        Ok(())
    }

    #[test]
    fn order_prompt_read_checks() -> Result<(), String> {
        let store = order_store("")?;
        let long = "A".repeat(251);
        let long_number = "1".repeat(26);
        let cases: [(&str, Result<InquiryOutput, InquiryError>); 8] = [
            ("\u{1}A", unsupported_branch("PO-09.b2")),
            (&long_number, unsupported_branch("PO-09.b2")),
            (&long, unsupported_branch("PO-09.b2")),
            ("??", unsupported_branch("PO-09.b4")),
            ("`12", unsupported_branch("PO-09.b5")),
            (" ", unsupported_branch("PO-09.b5")),
            ("", unsupported_branch("PO-09.b6")),
            ("^ABC", ok_transcript(ORDER_PROMPT_BYTES)),
        ];
        for (answer, expected) in cases {
            assert_eq!(
                inquire(&store, &inputs(&["", answer])),
                expected,
                "answer {answer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn missing_file_header_is_unsupported_branch() -> Result<(), String> {
        // The header is read with $G [DIC0.m:83-87] [DILIBF.m:141, 147]: absent,
        // or holding only a descendant, it ends the lookup with Y=-1 before
        // the prompt [DIC.m:14], never CorruptStore; piece 2 `442` gives other
        // identifiers [DIC1.m:46-49].
        for header in [
            "",
            "^PRC(442,0,1)=\"x\"\n",
            "^PRC(442,0)=\"PROCUREMENT & ACCOUNTING TRANSACTIONS^442^1^1\"\n",
        ] {
            let store = load(&format!("{header}^PRC(442,\"C\",\"A12345\",1)=\"\"\n"))?;
            assert_eq!(
                inquire(&store, &inputs(&["", "^"])),
                unsupported_branch("PO-09.b15"),
                "header {header:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn absent_identifiers_follow_no_match_path() -> Result<(), String> {
        let store = order_store("")?;
        let first = inquire(&store, &inputs(&["", "ABC123", "^"]));
        let second = inquire(&store, &inputs(&["", "XYZ999", "^"]));
        assert_eq!(first, second);
        assert_eq!(first, unsupported_branch(NO_MATCH));
        Ok(())
    }

    #[test]
    fn numeric_value_naming_no_ien_follows_no_match_path() -> Result<(), String> {
        let store = order_store("")?;
        // 999 names no entry; 1 names order 1, which IX^DIC never tries as an IEN.
        for value in ["999", "1"] {
            assert_eq!(
                inquire(&store, &inputs(&["", value, "^"])),
                unsupported_branch(NO_MATCH),
                "value {value:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn lowercase_value_whose_retry_fails_follows_no_match_path() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "abc123", "^"])),
            unsupported_branch(NO_MATCH)
        );
        Ok(())
    }

    #[test]
    fn lowercase_value_whose_retry_matches_is_unsupported_branch() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "a12345", "^"])),
            unsupported_branch("PO-09.b13")
        );
        Ok(())
    }

    #[test]
    fn candidates_all_rejected_by_screen_follow_no_match_path() -> Result<(), String> {
        let store = order_store(SCREENED_ORDERS)?;
        assert_eq!(
            inquire(&store, &inputs(&["", "B22222", "^"])),
            unsupported_branch(NO_MATCH)
        );
        Ok(())
    }

    #[test]
    fn several_candidates_are_unsupported_branch() -> Result<(), String> {
        let store = order_store(
            r#"^PRC(442,4,0)="500-A123456^2^101 GENERAL"
^PRC(442,"C","A123456",4)=""
"#,
        )?;
        assert_eq!(
            inquire(&store, &inputs(&["", "A12345", "^"])),
            unsupported_branch("PO-09.b10")
        );
        assert_eq!(
            inquire(&store, &inputs(&["", "A1234", "^"])),
            unsupported_branch("PO-09.b10")
        );
        // The exact match alone is selected; order 4 has no node 1 (PO-10.b3).
        assert_eq!(
            inquire(&store, &inputs(&["", "A123456", "^"])),
            unsupported_branch("PO-10.b3")
        );
        Ok(())
    }

    #[test]
    fn single_partial_match_is_unsupported_branch() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "A123", "^"])),
            unsupported_branch("PO-09.b9")
        );
        Ok(())
    }

    #[test]
    fn order_filed_under_two_indexes_is_one_candidate() -> Result<(), String> {
        // Order 1 is the exact match on `C` and is filed again under `E`, a
        // later index that is not a pointer index: it stays one candidate,
        // selected (PO-09.b8), so the session reaches the device gate rather
        // than the choose list (PO-09.b10).
        let store = order_store("^PRC(442,\"E\",\"A12345\",1)=\"\"\n")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "A12345", "", "", "^"])),
            unsupported_branch(DEVICE_GATE)
        );
        Ok(())
    }

    #[test]
    fn later_index_candidate_is_counted() -> Result<(), String> {
        // `SPEC.md` R5 step 5: after `C`, every later subscript of `^PRC(442)`
        // whose `$D` is 10 is searched, exact then partial [DIC3.m:61-69].
        // Order 5 passes the screen and is filed only under `E`, the FCP
        // index. Found there beside order 1, the exact `C` match, it makes two
        // candidates (PO-09.b10); found there alone, by its exact or a partial
        // value, it is one candidate not from `C` (PO-09.b9).
        for (filed, value, expected) in [
            ("A12345", "A12345", "PO-09.b10"),
            ("Z55555", "Z55555", "PO-09.b9"),
            ("Z55555", "Z555", "PO-09.b9"),
        ] {
            let store = order_store(&format!(
                r#"^PRC(442,5,0)="500-Z55555^2^101 GENERAL"
^PRC(442,"E","{filed}",5)=""
"#
            ))?;
            assert_eq!(
                inquire(&store, &inputs(&["", value, "^"])),
                unsupported_branch(expected),
                "order 5 under E as {filed:?}, value {value:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn pointer_index_is_skipped_for_numeric_value() -> Result<(), String> {
        // `SPEC.md` R5 step 5: a later index on a pointer field (D, F, G, M,
        // MCH, ST) is skipped when the value is numeric, `+$P(X,"E")=X`
        // [DIC4.m:84-86]; every other later index, and a pointer index for any
        // other value, is searched on the raw value. Order 5 passes the screen
        // and is filed only under the index node given. Skipped, it leaves no
        // candidate, and with no pointed-to `B` entry the pointer-candidate
        // rule (R6) finds none either: the no-match path. Searched, it is one
        // candidate not from `C` (PO-09.b9). `G` files the order before its
        // 442.14 subrecord.
        for (node, value, expected) in [
            (r#""D","77",5"#, "77", NO_MATCH),
            (r#""F","77",5"#, "77", NO_MATCH),
            (r#""G","77",5,1"#, "77", NO_MATCH),
            (r#""M","77",5"#, "77", NO_MATCH),
            (r#""MCH","77",5"#, "77", NO_MATCH),
            (r#""ST","77",5"#, "77", NO_MATCH),
            // Not numeric: text, and digits that are not a canonical number.
            (r#""D","Z55555",5"#, "Z55555", "PO-09.b9"),
            (r#""D","077",5"#, "077", "PO-09.b9"),
            // Not pointer indexes: E (free text) and MAPP (numeric field).
            (r#""E","77",5"#, "77", "PO-09.b9"),
            (r#""MAPP","77",5"#, "77", "PO-09.b9"),
        ] {
            let store = order_store(&format!(
                r#"^PRC(442,5,0)="500-Z55555^2^101 GENERAL"
^PRC(442,{node})=""
"#
            ))?;
            assert_eq!(
                inquire(&store, &inputs(&["", value, "^"])),
                unsupported_branch(expected),
                "order 5 filed as ^PRC(442,{node}), value {value:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn unreferenced_vendor_with_matching_name_is_not_a_candidate() -> Result<(), String> {
        let store = order_store(VENDORS)?;
        assert_eq!(
            inquire(&store, &inputs(&["", "ACME", "^"])),
            unsupported_branch(NO_MATCH)
        );
        Ok(())
    }

    #[test]
    fn referenced_vendor_whose_orders_fail_screen_is_not_a_candidate() -> Result<(), String> {
        let store = order_store(&format!(
            "{VENDORS}{SCREENED_ORDERS}^PRC(442,\"D\",7,2)=\"\"\n^PRC(442,\"D\",7,3)=\"\"\n"
        ))?;
        assert_eq!(
            inquire(&store, &inputs(&["", "ACME", "^"])),
            unsupported_branch(NO_MATCH)
        );
        Ok(())
    }

    #[test]
    fn referenced_vendor_with_qualifying_order_is_unsupported_branch() -> Result<(), String> {
        let store = order_store(&format!("{VENDORS}^PRC(442,\"D\",7,1)=\"\"\n"))?;
        assert_eq!(
            inquire(&store, &inputs(&["", "ACME", "^"])),
            unsupported_branch("PO-09.b11")
        );
        Ok(())
    }

    #[test]
    fn pointed_to_file_without_header_value_is_not_searched() -> Result<(), String> {
        // `SPEC.md` R6: the nested lookup reads the pointed-to header with $G
        // [DICM0.m:41] [DIC.m:33-34] [DIC0.m:21, 83-87] [DILIBF.m:141, 147].
        // Absent, holding only a descendant, or empty, that header ends the
        // nested lookup with Y=-1 before the `B` index is read. Vendor 7
        // matches `ACME` and the qualifying order 1 is filed under it, yet it
        // is no candidate (not PO-09.b11), and a vendor zero node holding only
        // a descendant is never read (not CorruptStore): the no-match path.
        const REFERENCED_VENDOR: &str = r#"^PRC(440,"B","ACME SUPPLY CO",7)=""
^PRC(442,"D",7,1)=""
"#;
        for (label, records) in [
            ("header absent", "^PRC(440,7,0)=\"ACME SUPPLY CO\"\n"),
            (
                "header holding only a descendant",
                "^PRC(440,0,1)=\"x\"\n^PRC(440,7,0)=\"ACME SUPPLY CO\"\n",
            ),
            (
                "header empty",
                "^PRC(440,0)=\"\"\n^PRC(440,7,0)=\"ACME SUPPLY CO\"\n",
            ),
            (
                "header absent, vendor zero node holding only a descendant",
                "^PRC(440,7,0,1)=\"x\"\n",
            ),
        ] {
            let store = order_store(&format!("{records}{REFERENCED_VENDOR}"))?;
            assert_eq!(
                inquire(&store, &inputs(&["", "ACME", "^"])),
                unsupported_branch(NO_MATCH),
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn found_order_stops_at_device_gate() -> Result<(), String> {
        let store = order_store("")?;
        assert_eq!(
            inquire(&store, &inputs(&["", "A12345", "", "", "^"])),
            unsupported_branch(DEVICE_GATE)
        );
        Ok(())
    }

    #[test]
    fn unique_order_match_writes_identifier_echo() -> Result<(), String> {
        let store = order_store("")?;
        let answers = inputs(&["A12345"]);
        let mut s = Session::new(&answers);
        let mut v = Locals {
            prc_site: Some("500".to_string()),
            prchp_a: Some(ORDER_PROMPT.to_string()),
            prchp_s: Some(MOP_SCREEN.to_string()),
            ..Locals::default()
        };
        assert_eq!(en3_prchpat(&mut s, &store, &mut v), Ok(()));
        // SPEC.md R2: "  <.01>  <Z.01>  <Z.02>  \n" + 13 spaces + "FCP: <fcp>     $ <total>"
        let expected = format!(
            "\n\n\nP.O./REQ.NO.:   500-A12345  10-28-26  RQ   {}  \n{}FCP: 101     $ 251.00",
            "Ordered (No Fiscal Action Required",
            " ".repeat(13)
        );
        assert_eq!(s.transcript(), expected);
        assert_eq!(v.prchpo.as_deref(), Some("1"));
        assert_eq!(v.da.as_deref(), Some("1"));
        assert_eq!(v.x.as_deref(), Some("20"));
        assert_eq!(v.dic_a, None);
        Ok(())
    }

    #[test]
    fn estimated_flag_decides_est_branch() -> Result<(), String> {
        let cases = [
            ("^PRC(442,1,7)=\"20^^Y\"\n", Err(unsupported("PO-23.b2"))),
            ("^PRC(442,1,7)=\"20^^N\"\n", Ok(())),
            ("", Ok(())),
        ];
        for (node7, expected) in cases {
            let store = load(&format!("^PRC(442,1,0)=\"500-A12345^2\"\n{node7}"))?;
            let no_input: Vec<String> = Vec::new();
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                d0: Some("1".to_string()),
                ..Locals::default()
            };
            assert_eq!(
                est_prchdp1(&mut s, &store, &mut v),
                expected,
                "node 7 {node7:?}"
            );
            assert_eq!(s.transcript(), "");
        }
        Ok(())
    }

    #[test]
    fn store_unchanged_after_session() -> Result<(), String> {
        let store = order_store(VENDORS)?;
        let before = store.clone();
        for answers in [
            inputs(&["", "A12345", "", "", "^"]),
            inputs(&["", "ACME", "^"]),
            inputs(&["", "^"]),
        ] {
            let _outcome = inquire(&store, &answers);
            assert_eq!(before, store);
        }
        Ok(())
    }

    #[test]
    fn word_wrap_reproduces_plain_text_rules() {
        let cases: [(i64, &[&str], &[&str]); 7] = [
            (33, &["WIDGET, BLUE"], &["WIDGET, BLUE "]),
            (
                33,
                &["WIDGET, BLUE", "SIZE 4."],
                &["WIDGET, BLUE SIZE 4.  "],
            ),
            (
                16,
                &["QUOTE NUMBER 2026-1234 DATED NOV 1"],
                &["QUOTE NUMBER", "2026-1234 DATED", "NOV 1 "],
            ),
            (
                16,
                &["ABCDEFGHIJKLMNOPQRST UV"],
                &["ABCDEFGHIJKLMNOPQRST", "UV "],
            ),
            (16, &[""], &[" "]),
            (33, &["FIRST", " SECOND"], &["FIRST ", " SECOND "]),
            (33, &["FIRST", "---"], &["FIRST ", "---", ""]),
        ];
        let store = Store::default();
        let no_input: Vec<String> = Vec::new();
        for (width, lines, expected) in cases {
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                diwl: Some(1),
                diwr: Some(width),
                diwf: Some(String::new()),
                ..Locals::default()
            };
            for line in lines {
                v.x = Some((*line).to_string());
                assert_eq!(diwp_prcutl(&mut s, &store, &mut v, "1"), Ok(()));
            }
            let count = v.utility_w_counts.get(&1).copied().unwrap_or_default();
            let wrapped: Vec<String> = (1..=count)
                .map(|n| wrap_line(&v.utility_w_lines, 1, n))
                .collect();
            assert_eq!(wrapped, expected, "width {width}, lines {lines:?}");
            assert_eq!(v.da, None, "DIWP^PRCUTL restores the caller's DA");
            assert_eq!(s.transcript(), "");
        }
    }

    #[test]
    fn word_wrap_unconverted_forms_are_unsupported_branches() {
        let cases = [
            ("A |WINDOW| B", "", "PO-15.b2"),
            ("A |TAB| B", "", "PO-15.b3"),
            ("TEXT", "N", "PO-15.b3"),
        ];
        let store = Store::default();
        let no_input: Vec<String> = Vec::new();
        for (text, flags, id) in cases {
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                diwl: Some(1),
                diwr: Some(33),
                diwf: Some(flags.to_string()),
                x: Some(text.to_string()),
                ..Locals::default()
            };
            assert_eq!(
                diwp(&mut s, &store, &mut v),
                Err(unsupported(id)),
                "text {text:?}"
            );
        }
    }

    #[test]
    fn word_wrap_wrapper_restores_caller_da() {
        let cases = [
            (None, "WIDGET, BLUE", Ok(())),
            (None, "A |TAB| B", Err(unsupported("PO-15.b3"))),
            (None, "A |WINDOW| B", Err(unsupported("PO-15.b2"))),
            (Some("42"), "WIDGET, BLUE", Ok(())),
            (Some("42"), "A |TAB| B", Err(unsupported("PO-15.b3"))),
            (Some("42"), "A |WINDOW| B", Err(unsupported("PO-15.b2"))),
        ];
        let store = Store::default();
        let no_input: Vec<String> = Vec::new();
        for (caller_da, text, outcome) in cases {
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                diwl: Some(1),
                diwr: Some(33),
                diwf: Some(String::new()),
                da: caller_da.map(str::to_string),
                x: Some(text.to_string()),
                ..Locals::default()
            };
            assert_eq!(
                diwp_prcutl(&mut s, &store, &mut v, "1"),
                outcome,
                "caller DA {caller_da:?}, text {text:?}"
            );
            assert_eq!(
                v.da.as_deref(),
                caller_da,
                "DIWP^PRCUTL restores the caller's DA {caller_da:?}, text {text:?}"
            );
        }
    }

    #[test]
    fn date_prints_month_day_year_unpadded() {
        let store = Store::default();
        let no_input: Vec<String> = Vec::new();
        for (date, expected) in [
            ("3261028", "10/28/2026"),
            ("3260105", "1/5/2026"),
            ("3260105.143", "1/5/2026"),
            ("0", ""),
            ("", ""),
        ] {
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                y: Some(date.to_string()),
                ..Locals::default()
            };
            assert_eq!(dt_prchdp1(&mut s, &store, &mut v), Ok(()));
            assert_eq!(s.transcript(), expected, "date {date:?}");
        }
    }

    #[test]
    fn page_one_block_runs_to_column_header_gate() -> Result<(), String> {
        let store = order_store(
            r#"^PRC(442,1,14,0)="^442.12P^1^1"
^PRC(442,1,14,1,0)="3"
^PRC(442.4,3,0)="OPEN MARKET^OM"
^PRCD(420.8,2,0)="2"
^VA(200,12,0)="BUYER,BETTY"
"#,
        )?;
        let answers = inputs(&[""]);
        let mut s = Session::new(&answers);
        let mut v = order_locals(&store);
        assert_eq!(
            fob_prchdp1(&mut s, &store, &mut v),
            Err(unsupported("PO-26.b1"))
        );
        let sp = |n: usize| " ".repeat(n);
        let expected = [
            String::new(),
            format!(
                "FOB POINT: DESTINATION{}|PROPOSAL: QUOTE 99 {}|AUTHORITY: ",
                sp(7),
                sp(8)
            ),
            format!("COST CENTER: 2100{}|{}{}|  OM", sp(12), sp(10), sp(17)),
            format!(
                "TYPE: SIMPLIFIED ACQPURCHASE ORDER|{}{}|AGENT:",
                sp(5),
                sp(17)
            ),
            format!(
                "DELIVER ON/BEFORE 11/15/2026 |CONTRACT: {}|  BETTY BUYER",
                sp(17)
            ),
            format!("DISCOUNT TERM: {}|{}|DATE: 10/28/2026", sp(14), sp(27)),
            format!("APP: 3660160-101{}|{}|", sp(13), sp(27)),
            format!("{}|{}|TOTAL:   251.00", sp(29), sp(27)),
            "-".repeat(80),
            format!("{}ENTER '^' TO HALT: ", sp(8)),
        ]
        .join("\n");
        assert_eq!(s.transcript(), expected);
        Ok(())
    }

    #[test]
    fn item_line_runs_to_paging_gate() -> Result<(), String> {
        // Item IEN 7 holds line number 3 in piece 1 of its zero node, so the
        // printed number shows which of the two the line was built from.
        let store = order_store(
            r#"^PRC(442,1,2,0)="^442.01IA^7^1"
^PRC(442,1,2,7,0)="3^2^1^2696 SUPPLIES^^STK-9^^^125.5"
^PRC(442,1,2,7,1,0)="^^1^1^3261028^"
^PRC(442,1,2,7,1,1,0)="WIDGET, BLUE"
^PRC(442,1,2,7,2)="251"
^PRCD(420.5,1,0)="EACH"
"#,
        )?;
        let no_input: Vec<String> = Vec::new();
        let mut s = Session::new(&no_input);
        let mut v = Locals {
            prchdi: Some("7".to_string()),
            prchdi0: Some(store.get_or_empty(&key("PRC", &["442", "1", "2", "7", "0"]))),
            prchdi2: Some("251".to_string()),
            prchdq: Some("0".to_string()),
            ..order_locals(&store)
        };
        assert_eq!(
            item_prchdp2(&mut s, &store, &mut v),
            Err(unsupported("PO-30.b1"))
        );
        let sp = |n: usize| " ".repeat(n);
        let expected = format!(
            "\n    3  WIDGET, BLUE {}    2  EACH  125.50{} 251.00\n{}STK#: STK-9",
            sp(20),
            sp(8),
            sp(8)
        );
        assert_eq!(s.transcript(), expected);
        let line = s
            .transcript()
            .lines()
            .nth(1)
            .unwrap_or_default()
            .to_string();
        // $J(+$P(PRCHDI0,U,1),3) at column 2 fills columns 3-5.
        assert_eq!(extract(&line, 3, 5), "  3", "line number, not IEN 7");
        assert_eq!(v.prchdcnt.as_deref(), Some("1"));
        Ok(())
    }

    #[test]
    fn unit_cost_format_follows_stored_decimals() -> Result<(), String> {
        let cases = [
            ("19.95", "  19.95"),
            (".99", "   0.99"),
            // $J(x,6,3) is one column narrower; the seventh is the pad before column 67.
            ("1.125", " 1.125 "),
            // $J(x,5,4) pads to five columns only: a six-character value gets no
            // leading pad, and the seventh column is the pad before column 67.
            ("1.2345", "1.2345 "),
            ("12.3456", "12.3456"),
            ("N/C", "    N/C"),
        ];
        for (cost, shown) in cases {
            let store = order_store(&format!(
                "^PRC(442,1,2,1,0)=\"1^1^^^^^^^{cost}\"\n^PRC(442,1,2,1,1,1,0)=\"X\"\n^PRC(442,1,2,1,2)=\"1\"\n"
            ))?;
            let no_input: Vec<String> = Vec::new();
            let mut s = Session::new(&no_input);
            let mut v = Locals {
                prchdi: Some("1".to_string()),
                prchdi0: Some(store.get_or_empty(&key("PRC", &["442", "1", "2", "1", "0"]))),
                prchdi2: Some("1".to_string()),
                prchdq: Some("0".to_string()),
                ..order_locals(&store)
            };
            assert_eq!(
                item_prchdp2(&mut s, &store, &mut v),
                Err(unsupported("PO-30.b1"))
            );
            let line = s
                .transcript()
                .lines()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            assert_eq!(extract(&line, 53, 59), shown, "cost {cost:?}");
        }
        Ok(())
    }
}
