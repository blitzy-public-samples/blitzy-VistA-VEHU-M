//! The M (MUMPS) commands and functions that the ported `PRCH DISPLAY PO`
//! routines use, and nothing more.
//!
//! Each item models one M built-in as YottaDB evaluates it under default
//! collation: `READ` and `WRITE` through [`Session`], plus `$PIECE`,
//! `$EXTRACT`, `$GET`, `$DATA`, `$ORDER`, `$JUSTIFY`, unary `+`, `\`, `#`,
//! the follows operator `]` and the contains operator. No M code is parsed
//! or interpreted; callers in `inquiry.rs` and `store.rs` call these
//! functions where the original routine uses the built-in.
//!
//! Numbers are handled as decimal digit text, never as floating point, so
//! amounts, 11-digit IENs and FileMan dates keep their exact value.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::iter::{repeat, repeat_n};
use std::ops::Bound::{Excluded, Unbounded};

// ---------------------------------------------------------------------------
// Canonical numbers and collation
// ---------------------------------------------------------------------------

/// True when `text` is an M canonical number: an optional `-`, then either
/// an integer with no leading zero (`0` alone included), or an optional
/// integer part with no leading zero, `.`, and a fraction with no trailing
/// zero. `.5` and `-.5` are canonical; `0.5`, `01`, `1.0`, `1.`, `+1`, `-0`,
/// the empty string and `1E5` are not.
///
/// This is M's rule for whether a subscript is a number or a string, and
/// FileMan's `+X=X` test before trying a lookup value as an IEN
/// (`BYIEN2^DIC5`, reached from DIC.m:48; PO-09). Works on the digit text
/// only, because fixture subscripts such as `10000000044` must not pass
/// through a machine integer or float.
pub(crate) fn is_canonical(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let negative = body.len() != text.len();
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    match body.split_once('.') {
        None => {
            !body.is_empty()
                && all_digits(body)
                && ((body == "0" && !negative) || !body.starts_with('0'))
        }
        Some((int, frac)) => {
            all_digits(int)
                && !int.starts_with('0')
                && !frac.is_empty()
                && all_digits(frac)
                && !frac.ends_with('0')
        }
    }
}

/// Splits number text into its sign, integer digits and fraction digits,
/// with leading integer zeros and trailing fraction zeros removed. A zero
/// value is never negative.
fn parts(text: &str) -> (bool, &str, &str) {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let int = int.trim_start_matches('0');
    let frac = frac.trim_end_matches('0');
    let is_zero = int.is_empty() && frac.is_empty();
    (negative && !is_zero, int, frac)
}

/// Compares two magnitudes given as integer and fraction digit strings
/// without leading or trailing zeros.
fn cmp_magnitude(a_int: &str, a_frac: &str, b_int: &str, b_frac: &str) -> Ordering {
    let width = a_frac.len().max(b_frac.len());
    let a_fraction = a_frac.bytes().chain(repeat(b'0')).take(width);
    let b_fraction = b_frac.bytes().chain(repeat(b'0')).take(width);
    a_int
        .len()
        .cmp(&b_int.len())
        .then_with(|| a_int.as_bytes().cmp(b_int.as_bytes()))
        .then_with(|| a_fraction.cmp(b_fraction))
}

/// Compares two canonical numbers by exact decimal value: sign first, then
/// the length of the integer part, then its digits, then the fraction digit
/// by digit with missing digits read as `0`. No integer is built, so no
/// length of digit text can overflow.
///
/// Used for numeric subscripts in M collation (`store.rs` `Sub::Num`) and,
/// through [`num_cmp`], for M's `<` and `>` on strings.
pub(crate) fn cmp_numbers(a: &str, b: &str) -> Ordering {
    let (a_negative, a_int, a_frac) = parts(a);
    let (b_negative, b_int, b_frac) = parts(b);
    match (a_negative, b_negative) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (false, false) => cmp_magnitude(a_int, a_frac, b_int, b_frac),
        (true, true) => cmp_magnitude(b_int, b_frac, a_int, a_frac),
    }
}

/// M default collation of two subscript texts, as YottaDB orders global
/// nodes: every canonical number sorts before every string, numbers compare
/// by value ([`cmp_numbers`]) and strings by bytes. Quoted numeric-looking
/// strings such as `"008202226"`, `"001"` or `"04"` are not canonical and
/// collate as strings. The 411 export lists the numeric IENs before `"AC"`
/// and `"B"` in exactly this order, and the item loop at PRCHDP2.m:11
/// (`Q:PRCHDI'>0`) relies on numeric items preceding the `"B"` subscript.
///
/// The empty string, which no exported subscript holds, falls among the
/// strings, first.
pub(crate) fn collate(a: &str, b: &str) -> Ordering {
    match (is_canonical(a), is_canonical(b)) {
        (true, true) => cmp_numbers(a, b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

// ---------------------------------------------------------------------------
// Numeric coercion and arithmetic
// ---------------------------------------------------------------------------

/// Builds the canonical form of a sign, integer digits and fraction digits.
fn canonical(negative: bool, int: &str, frac: &str) -> String {
    let int = int.trim_start_matches('0');
    let frac = frac.trim_end_matches('0');
    if int.is_empty() && frac.is_empty() {
        return "0".to_string();
    }
    let mut out = String::with_capacity(int.len() + frac.len() + 2);
    if negative {
        out.push('-');
    }
    out.push_str(int);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// Length of the run of ASCII digits that starts `text`.
fn digit_run(text: &str) -> usize {
    text.bytes().take_while(u8::is_ascii_digit).count()
}

/// M unary `+`: the numeric value of the leading numeric prefix of `text`,
/// in canonical form. The prefix is any run of `+` and `-` signs (an odd
/// number of `-` negates), then digits, then an optional `.` and digits.
/// Text with no such prefix is `0`. Exponent notation is not modelled
/// (FileMan guards it with `$P(X,"E")` at DIC.m:48), so `1E5` gives `1`.
///
/// Uses: `+$G(^PRC(442,D0,7))` at PRCHDP1.m:15 (PO-10), `+$P(PRCHDI0,U,1)`
/// at PRCHDP2.m:49 (PO-27), `+Y` and `+$G(^PRC(442,DA,7))` at PRCHPAT.m:83
/// (PO-08).
pub(crate) fn num(text: &str) -> String {
    let unsigned = text.trim_start_matches(['+', '-']);
    let sign_count = text.len() - unsigned.len();
    let minus_signs = text.bytes().take(sign_count).filter(|&b| b == b'-').count();
    let negative = minus_signs % 2 == 1;
    let (int, rest) = unsigned
        .split_at_checked(digit_run(unsigned))
        .unwrap_or((unsigned, ""));
    let frac = match rest.strip_prefix('.') {
        Some(after_point) => after_point
            .split_at_checked(digit_run(after_point))
            .map_or(after_point, |(digits, _)| digits),
        None => "",
    };
    canonical(negative, int, frac)
}

/// M numeric comparison of two values: both sides are coerced with
/// [`num`] and compared by exact value, so `""` counts as `0`.
///
/// Uses: `X<10` at PRCHRPT.m:73 (PO-06), the lookup screen
/// `$P(^(0),U,2)<10` at PRCHRPT.m:9 (PO-07, PO-09), `PRCHDI'>0` at
/// PRCHDP2.m:11 and `$P(PRCHD0,U,13)>0` at PRCHDP2.m:13 (PO-25), and
/// `PRCHDCNT>1` at PRCHDP2.m:54 (PO-27). String equality such as `S:X=0`
/// at PRCHPAT.m:83 compares the text directly and needs no helper.
pub(crate) fn num_cmp(a: &str, b: &str) -> Ordering {
    cmp_numbers(&num(a), &num(b))
}

/// An exact decimal: `mantissa / 10^scale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    /// The exact value of `text` after [`num`] coercion, or `None` when its
    /// digits do not fit an `i128`.
    fn parse(text: &str) -> Option<Decimal> {
        let canon = num(text);
        let (negative, int, frac) = parts(&canon);
        let scale = u32::try_from(frac.len()).ok()?;
        let magnitude = int.bytes().chain(frac.bytes()).try_fold(0_i128, |acc, b| {
            let digit = char::from(b).to_digit(10)?;
            acc.checked_mul(10)?.checked_add(i128::from(digit))
        })?;
        let mantissa = if negative {
            magnitude.checked_neg()?
        } else {
            magnitude
        };
        Some(Decimal { mantissa, scale })
    }

    /// The mantissa expressed at `scale`, which must not be below the
    /// value's own scale.
    fn mantissa_at(self, scale: u32) -> Option<i128> {
        let factor = 10_i128.checked_pow(scale.checked_sub(self.scale)?)?;
        self.mantissa.checked_mul(factor)
    }
}

/// Both operands as mantissas at their common scale.
fn aligned(a: &str, b: &str) -> Option<(i128, i128, u32)> {
    let a = Decimal::parse(a)?;
    let b = Decimal::parse(b)?;
    let scale = a.scale.max(b.scale);
    Some((a.mantissa_at(scale)?, b.mantissa_at(scale)?, scale))
}

/// Canonical text of `mantissa / 10^scale`.
fn render(mantissa: i128, scale: u32) -> Option<String> {
    let fraction_width = usize::try_from(scale).ok()?;
    let digits = mantissa.unsigned_abs().to_string();
    let padded = format!("{digits:0>width$}", width = fraction_width.checked_add(1)?);
    let (int, frac) = padded.split_at_checked(padded.len().checked_sub(fraction_width)?)?;
    Some(canonical(mantissa < 0, int, frac))
}

/// The value these arithmetic models give where M would raise an error.
/// M stops with a division-by-zero or numeric-overflow error there; no
/// exercised path divides by zero, and FileMan dates are far below the
/// 38-digit `i128` range, so this value is never observable on the slice.
const ARITHMETIC_ERROR_VALUE: &str = "0";

/// M integer division `a\b`: the quotient of the numeric values, truncated
/// toward zero, in canonical form. Exact on decimals, because FileMan dates
/// may carry a time fraction. Returns `"0"` on division by zero or overflow,
/// where M raises an error.
///
/// Uses: `Y\100#100`, `Y#100\1` and `Y\10000+1700` in `DT^PRCHDP1` at
/// PRCHDP1.m:69 (PO-19).
pub(crate) fn int_div(a: &str, b: &str) -> String {
    aligned(a, b)
        .and_then(|(dividend, divisor, _)| dividend.checked_div(divisor))
        .map_or_else(|| ARITHMETIC_ERROR_VALUE.to_string(), |q| q.to_string())
}

/// M modulo `a#b`: floored modulo, so a non-zero result takes the sign of
/// the divisor (`-7#5` is `3`). Exact on decimals, so `3261028.143#100` is
/// `28.143`. Returns `"0"` on division by zero or overflow, where M raises
/// an error.
///
/// Uses: `Y\100#100` and `Y#100\1` in `DT^PRCHDP1` at PRCHDP1.m:69 (PO-19).
pub(crate) fn modulo(a: &str, b: &str) -> String {
    aligned(a, b)
        .and_then(|(dividend, divisor, scale)| {
            let remainder = dividend.checked_rem(divisor)?;
            let floored = if remainder != 0 && ((remainder < 0) != (divisor < 0)) {
                remainder.checked_add(divisor)?
            } else {
                remainder
            };
            render(floored, scale)
        })
        .unwrap_or_else(|| ARITHMETIC_ERROR_VALUE.to_string())
}

/// M addition `a+b`, exact, in canonical form. Returns `"0"` on overflow,
/// where M raises an error.
///
/// Uses: `Y\10000+1700` in `DT^PRCHDP1` at PRCHDP1.m:69 (PO-19).
pub(crate) fn add(a: &str, b: &str) -> String {
    aligned(a, b)
        .and_then(|(left, right, scale)| render(left.checked_add(right)?, scale))
        .unwrap_or_else(|| ARITHMETIC_ERROR_VALUE.to_string())
}

// ---------------------------------------------------------------------------
// String functions
// ---------------------------------------------------------------------------

/// M `$PIECE(s,delim,n)` with a 1-based `n`: the `n`th field of `s`
/// between occurrences of `delim`. `n` of 0, an empty `delim` or a piece
/// past the end gives `""`.
///
/// Uses: caret pieces of every node the display reads, for example
/// `$P(PRCHD0,"-",1)` at PRCHDP1.m:6 (PO-10) and `$P(PRCHDI0,U,1)` at
/// PRCHDP2.m:49 (PO-27).
pub(crate) fn piece<'a>(s: &'a str, delim: &str, n: usize) -> &'a str {
    if delim.is_empty() {
        return "";
    }
    n.checked_sub(1)
        .and_then(|index| s.split(delim).nth(index))
        .unwrap_or("")
}

/// M `$PIECE(s,delim,from,to)`: pieces `from` through `to` of `s`, rejoined
/// with `delim`. A `from` of 0 counts as 1; `to` below `from`, an empty
/// `delim` or a range past the end gives `""` or the pieces that exist.
///
/// Uses: `$P(^(2),"^",9,12)` at PRCHUTL.m:73 (PO-29) and `$P(X," ",2,9999)`
/// in the `^DIWP` reproduction (PO-15).
pub(crate) fn piece_range(s: &str, delim: &str, from: usize, to: usize) -> String {
    let from = from.max(1);
    if delim.is_empty() || to < from {
        return String::new();
    }
    let count = (to - from).saturating_add(1);
    s.split(delim)
        .skip(from - 1)
        .take(count)
        .collect::<Vec<&str>>()
        .join(delim)
}

/// M `$EXTRACT(s,from,to)`: characters `from` through `to` of `s`,
/// 1-based and inclusive, counted in characters. A `from` below 1 counts as
/// 1, and `to` below `from` or a range past the end gives `""` or the
/// characters that exist. `$E(s)` is `extract(s,1,1)` and `$E(s,n)` is
/// `extract(s,n,n)`.
///
/// Uses: `$E(...,1,34)` on the status name at PRCHDP1.m:15 (PO-10),
/// `$E(...,1,12)` at PRCHDP1.m:37, `$E($P(PRCHD1,U,6))` at PRCHDP1.m:53
/// (PO-13), `$E(FMSLN,$L(FMSLN)-2,99)` at PRCHDP2.m:63 (PO-27), and the
/// `MM-DD-YY` date parts of identifier `Z.01` (PO-09).
pub(crate) fn extract(s: &str, from: i64, to: i64) -> String {
    let from = from.max(1);
    if to < from {
        return String::new();
    }
    let skip = usize::try_from(from - 1).unwrap_or(usize::MAX);
    let take = usize::try_from(to.saturating_sub(from).saturating_add(1)).unwrap_or(usize::MAX);
    s.chars().skip(skip).take(take).collect()
}

/// M follows operator `a]b`: true when `a` sorts after `b` in byte order,
/// so `X]""` holds for every non-empty `X`.
///
/// Uses: `Q:PRCHD0']""` at PRCHDP1.m:8 and the non-empty tests at
/// PRCHDP1.m:7, 10-11 and 26-38 (PO-10), PRCHDP2.m:56-57 and 64 (PO-27).
pub(crate) fn follows(a: &str, b: &str) -> bool {
    a.as_bytes() > b.as_bytes()
}

/// M contains operator (`a` contains `b`): true when `b` is empty or
/// occurs in `a`.
///
/// Uses: the halt test on the answer containing `"^"` at PRCHDP2.m:10
/// (PO-25), and the description test for `"| IN "` at PRCHDP2.m:100
/// (PO-28).
pub(crate) fn contains(a: &str, b: &str) -> bool {
    a.contains(b)
}

/// Left-pads `text` with spaces to `width` characters, never truncating.
fn pad_left(text: &str, width: usize) -> String {
    let length = text.chars().count();
    let mut out = String::with_capacity(width.max(text.len()));
    out.extend(repeat_n(' ', width.saturating_sub(length)));
    out.push_str(text);
    out
}

/// Adds one to the last digit of an ASCII digit sequence, carrying left and
/// growing the sequence when every digit is `9`.
fn increment_digits(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            return;
        }
    }
    digits.insert(0, b'1');
}

/// The value of `text` as fixed-point text with exactly `decimals` fraction
/// digits: coerced with [`num`], rounded half away from zero by adding 5 at
/// the first discarded digit and truncating, with a leading `0` below 1.
fn fixed_point(text: &str, decimals: usize) -> String {
    let canon = num(text);
    let (negative, int, frac) = parts(&canon);
    let mut digits: Vec<u8> = if int.is_empty() {
        vec![b'0']
    } else {
        int.bytes().collect()
    };
    digits.extend(frac.bytes().chain(repeat(b'0')).take(decimals));
    let first_dropped = frac.as_bytes().get(decimals).copied();
    if first_dropped.is_some_and(|digit| digit >= b'5') {
        increment_digits(&mut digits);
    }
    let int_length = digits.len().saturating_sub(decimals);
    let mut out = String::with_capacity(digits.len() + 2);
    // Negative amounts are not exercised: the DD input transforms reject
    // them (DD.zwr:516287). A negative value is written with `-` before its
    // digits, including one that rounds to zero.
    if negative {
        out.push('-');
    }
    out.extend(digits.iter().take(int_length).map(|&b| char::from(b)));
    if decimals > 0 {
        out.push('.');
        out.extend(digits.iter().skip(int_length).map(|&b| char::from(b)));
    }
    out
}

/// M `$JUSTIFY`. With `decimals` of `None` (`$J(s,width)`), `s` is
/// right-justified unchanged, with no numeric conversion. With
/// `Some(d)` (`$J(s,width,d)`), `s` is coerced with [`num`] and rounded to
/// exactly `d` fraction digits, half away from zero by adding 5 at the
/// first discarded digit and truncating, with a leading `0` when the
/// magnitude is below 1; `$J(1.005,0,2)` is `1.01`. Either way the result
/// is left-padded with spaces to `width` and never truncated.
///
/// Uses: `$J(x,8,2)` for the order total at PRCHDP1.m:67 (PO-24);
/// `$J(x,7,2)` for estimated shipping at PRCHDP2.m:13 (PO-25) and the item
/// total at PRCHDP2.m:53; `$J(x,5,4)`, `$J(x,6,3)` and `$J(x,7,2)` for the
/// unit cost at PRCHDP2.m:52; `$J(N,3)` and `$J(q,5)` at PRCHDP2.m:49, 51
/// and 57 (PO-27); and `$J(total,0,2)` in identifier `Z.03` at
/// DD.zwr:514204 (PO-09).
pub(crate) fn justify(s: &str, width: usize, decimals: Option<usize>) -> String {
    match decimals {
        None => pad_left(s, width),
        Some(d) => pad_left(&fixed_point(s, d), width),
    }
}

// ---------------------------------------------------------------------------
// Global-style accessors over an ordered map of subscript vectors
// ---------------------------------------------------------------------------

/// The value stored at `key`, or `None` when the node has no value. The
/// `Option` form of [`get`], for reads where a missing node must be told
/// apart from an empty value (an unguarded M reference).
pub(crate) fn get_opt<'m, S: Ord>(map: &'m BTreeMap<Vec<S>, String>, key: &[S]) -> Option<&'m str> {
    map.get(key).map(String::as_str)
}

/// M `$GET`: the value stored at `key`, or `""` when the node has no value.
///
/// Uses: `$G(^PRC(442,D0,0))` and `$G(^(1))` at PRCHDP1.m:4 (PO-10), and
/// every other `$G` on the display path.
pub(crate) fn get<S: Ord>(map: &BTreeMap<Vec<S>, String>, key: &[S]) -> String {
    get_opt(map, key).unwrap_or("").to_string()
}

/// M `$DATA`: 0 when `key` has neither a value nor descendants, 1 for a
/// value only, 10 for descendants only and 11 for both. Descendants are
/// found from the first key after `key` alone, because every key that
/// extends `key` sorts directly after it.
///
/// Uses: `$D(^PRC(442,D0,4,1,0))` at PRCHDP1.m:9 (PO-10),
/// `$D(^PRC(442,D0,6,0))` at PRCHDP2.m:9 (PO-25), `$D(^(2))` at
/// PRCHDP2.m:11, and FileMan's cross-reference test (`$D` of 10) in the
/// lookup (PO-09).
pub(crate) fn data<S: Ord>(map: &BTreeMap<Vec<S>, String>, key: &[S]) -> u8 {
    let value = u8::from(map.contains_key(key));
    let descendants = map
        .range::<[S], _>((Excluded(key), Unbounded))
        .next()
        .is_some_and(|(next, _)| next.len() > key.len() && next.starts_with(key));
    value + if descendants { 10 } else { 0 }
}

/// M forward `$ORDER(parent(after))`: the next child subscript of `parent`
/// strictly after `after`, at depth `parent.len() + 1`, in collation order.
/// `after` of `None` stands for the `""` seed and gives the first child;
/// `None` as the result stands for M's `""` (no further child). Keys under
/// the current child are skipped, and the walk stops at the first key that
/// no longer starts with `parent`. Reverse `$ORDER` is not used on the
/// path.
///
/// Uses: the item loop `$O(^PRC(442,D0,2,PRCHDI))` at PRCHDP2.m:11
/// (PO-25), the authority loop over node 14 at PRCHDP1.m:56 (PO-13), the
/// item index `AC` at PRCHDP1.m:61 (PO-20), and the index walks of the
/// lookup (PO-09).
pub(crate) fn order<S: Ord + Clone>(
    map: &BTreeMap<Vec<S>, String>,
    parent: &[S],
    after: Option<&S>,
) -> Option<S> {
    let mut start = parent.to_vec();
    if let Some(current) = after {
        start.push(current.clone());
    }
    let depth = parent.len();
    map.range::<[S], _>((Excluded(start.as_slice()), Unbounded))
        .map(|(key, _)| key)
        .take_while(|key| key.starts_with(parent))
        .filter_map(|key| key.get(depth))
        .find(|child| after.is_none_or(|current| *child != current))
        .cloned()
}

// ---------------------------------------------------------------------------
// READ and WRITE
// ---------------------------------------------------------------------------

/// The principal device of one session: typed lines in, transcript out,
/// with M's `$X` (column) and `$Y` (line) tracked as the original's output
/// moves them.
///
/// The original is captured without a terminal, so typed input is never
/// echoed: a READ leaves the transcript, `$X` and `$Y` unchanged. Form feed
/// (`W @IOF`) is not modelled, because the HOME device's `IOF` is unresolved
/// (open item O-01); `inquiry.rs` returns the BLOCKED branch ID there.
#[derive(Debug)]
pub(crate) struct Session<'a> {
    inputs: &'a [String],
    next: usize,
    out: String,
    x: usize,
    y: usize,
}

impl<'a> Session<'a> {
    /// A session that answers READs from `inputs` in order, each one typed
    /// line without its terminator, with an empty transcript at column 0,
    /// line 0.
    pub(crate) fn new(inputs: &'a [String]) -> Session<'a> {
        Session {
            inputs,
            next: 0,
            out: String::new(),
            x: 0,
            y: 0,
        }
    }

    /// M `READ X:DTIME`: the next input line, unchanged and not echoed, or
    /// `None` when the inputs are exhausted (the caller ends the session
    /// there). A READ never times out, so the timeout branches stay
    /// unexercised.
    ///
    /// Uses: `R X:DTIME` at PRCHDP2.m:10 (PO-25) and PRCHDP2.m:41 (PO-38),
    /// and FileMan's prompt reads at DIC11.m:17 (PO-05, PO-09).
    pub(crate) fn read(&mut self) -> Option<String> {
        let line = self.inputs.get(self.next)?.clone();
        self.next = self.next.saturating_add(1);
        Some(line)
    }

    /// M `WRITE "text"`: appends `text` and advances `$X` by its character
    /// count.
    pub(crate) fn write(&mut self, text: &str) {
        self.out.push_str(text);
        self.x = self.x.saturating_add(text.chars().count());
    }

    /// M `WRITE !`: appends a newline, sets `$X` to 0 and adds 1 to `$Y`.
    pub(crate) fn nl(&mut self) {
        self.out.push('\n');
        self.x = 0;
        self.y = self.y.saturating_add(1);
    }

    /// M `WRITE ?col`: when `$X` is below `col`, appends spaces up to column
    /// `col` and sets `$X` to `col`; otherwise writes nothing.
    ///
    /// Uses: every column position of the display, for example `?37` at
    /// PRCHDP1.m:15 (PO-10) and `?67` at PRCHDP2.m:53 (PO-27).
    pub(crate) fn tab(&mut self, col: usize) {
        if self.x < col {
            self.out.extend(repeat_n(' ', col - self.x));
            self.x = col;
        }
    }

    /// M `WRITE $C(7)`: appends the BEL byte and counts it in `$X` like any
    /// other character. No exercised `?n` follows a BEL on the same line,
    /// so whether the device counts it cannot be observed.
    ///
    /// Uses: PRCHRPT.m:73 (PO-06), PRCHDP2.m:41 (PO-38), and FileMan's
    /// no-match response `DING^DIC` at DIC.m:63-65 (PO-09).
    pub(crate) fn bell(&mut self) {
        self.write("\u{7}");
    }

    /// M `$X`: the current column.
    pub(crate) fn x(&self) -> usize {
        self.x
    }

    /// M `$Y`: the current line.
    pub(crate) fn y(&self) -> usize {
        self.y
    }

    /// Everything written so far.
    pub(crate) fn transcript(&self) -> &str {
        &self.out
    }

    /// Ends the session and returns everything written.
    pub(crate) fn into_transcript(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test subscript whose order is M collation.
    #[derive(Clone, PartialEq, Eq, Debug)]
    struct T(String);

    impl Ord for T {
        fn cmp(&self, other: &Self) -> Ordering {
            collate(&self.0, &other.0)
        }
    }

    impl PartialOrd for T {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    fn key(subs: &[&str]) -> Vec<T> {
        subs.iter().map(|sub| T((*sub).to_string())).collect()
    }

    fn map(nodes: &[(&[&str], &str)]) -> BTreeMap<Vec<T>, String> {
        nodes
            .iter()
            .map(|(subs, value)| (key(subs), (*value).to_string()))
            .collect()
    }

    /// `DT^PRCHDP1` at PRCHDP1.m:69: `Y\100#100,"/",Y#100\1,"/",Y\10000+1700`.
    fn fileman_date(y: &str) -> String {
        format!(
            "{}/{}/{}",
            modulo(&int_div(y, "100"), "100"),
            int_div(&modulo(y, "100"), "1"),
            add(&int_div(y, "10000"), "1700")
        )
    }

    #[test]
    fn collation_numbers_before_strings() {
        let cases = [
            ("999", "A", Ordering::Less),
            ("10", "9", Ordering::Greater),
            (".5", "1", Ordering::Less),
            ("-1", "0", Ordering::Less),
            ("-12.25", "-12.2", Ordering::Less),
            ("001", "1", Ordering::Greater),
            ("008202226", "9", Ordering::Greater),
            ("10000000044", "9999999999", Ordering::Greater),
            ("500", "500", Ordering::Equal),
        ];
        for (a, b, expected) in cases {
            assert_eq!(collate(a, b), expected, "collate({a:?}, {b:?})");
        }
    }

    #[test]
    fn collation_strings_byte_order() {
        let cases = [
            ("AC", "B"),
            ("B", "C"),
            ("A IFCAP-Wide Parameters", "AC"),
            ("Z", "a"),
            ("001", "04"),
        ];
        for (a, b) in cases {
            assert_eq!(collate(a, b), Ordering::Less, "collate({a:?}, {b:?})");
            assert_eq!(collate(b, a), Ordering::Greater, "collate({b:?}, {a:?})");
        }
    }

    #[test]
    fn canonical_number_forms() {
        for text in ["0", "5", ".5", "-.5", "1.1", "10000000044", "-12.25"] {
            assert!(is_canonical(text), "{text:?} should be canonical");
        }
        for text in [
            "01", "1.0", "+1", "-0", "0.5", "1.", "", "1E5", "-", ".", "1.1.1", " 1",
        ] {
            assert!(!is_canonical(text), "{text:?} should not be canonical");
        }
    }

    #[test]
    fn piece_extract_get_data() {
        assert_eq!(piece("a^b^c", "^", 2), "b");
        assert_eq!(piece("a", "^", 3), "");
        assert_eq!(piece("a^b", "^", 0), "");
        assert_eq!(piece("a^b", "", 1), "");
        assert_eq!(piece("500-A12345^2", "-", 1), "500");
        assert_eq!(piece_range("1^2^3^4", "^", 2, 3), "2^3");
        assert_eq!(piece_range("a b c", " ", 2, 9999), "b c");
        assert_eq!(piece_range("a^b", "^", 3, 1), "");
        assert_eq!(extract("3261028", 4, 5), "10");
        assert_eq!(extract("abc", 5, 9), "");
        assert_eq!(extract("abc", 0, 2), "ab");
        assert_eq!(extract("abc", 3, 1), "");
        assert_eq!(extract("ORIGIN", 1, 1), "O");
        assert_eq!(extract("\u{e9}tat", 1, 2), "\u{e9}t");
        assert_eq!(extract("00123", 3, 99), "123");
        assert!(follows("A", ""));
        assert!(!follows("", ""));
        assert!(follows("B", "AC"));
        assert!(contains("A^B", "^"));
        assert!(contains("text", ""));
        assert!(contains("PART | IN STOCK", "| IN "));
        assert!(!contains("text", "^"));

        let nodes = map(&[
            (&["A", "1"], "v"),
            (&["A", "1", "2"], "w"),
            (&["A", "3", "1"], "x"),
        ]);
        assert_eq!(data(&nodes, &key(&["A", "4"])), 0);
        assert_eq!(data(&nodes, &key(&["A", "2"])), 0);
        assert_eq!(data(&nodes, &key(&["A", "1", "2"])), 1);
        assert_eq!(data(&nodes, &key(&["A", "3"])), 10);
        assert_eq!(data(&nodes, &key(&["A", "1"])), 11);
        assert_eq!(get(&nodes, &key(&["A", "1"])), "v");
        assert_eq!(get(&nodes, &key(&["A", "3"])), "");
        assert_eq!(get(&nodes, &key(&["A", "9"])), "");
        assert_eq!(get_opt(&nodes, &key(&["A", "1", "2"])), Some("w"));
        assert_eq!(get_opt(&nodes, &key(&["A", "3"])), None);
    }

    #[test]
    fn order_over_mixed_subscripts() {
        let nodes = map(&[
            (&["PRC", "442", "1", "0"], "500-A12345"),
            (&["PRC", "442", "1", "2", "1", "0"], "1^2"),
            (&["PRC", "442", "1", "2", "1", "1", "1", "0"], "TEXT"),
            (&["PRC", "442", "1", "2", "2", "0"], "2^3"),
            (&["PRC", "442", "1", "2", "10", "0"], "10^1"),
            (&["PRC", "442", "1", "2", "AC", "X", "1"], ""),
            (&["PRC", "442", "1", "2", "B", "1", "1"], ""),
            (&["PRC", "442", "1", "2", "B", "2", "2"], ""),
            (&["PRC", "442", "1", "3", "1", "0"], "after the parent"),
            (&["PRC", "442", "2", "0"], "500-B1"),
        ]);
        let parent = key(&["PRC", "442", "1", "2"]);
        let mut walked = Vec::new();
        let mut current = order(&nodes, &parent, None);
        while let Some(child) = current {
            walked.push(child.0.clone());
            current = order(&nodes, &parent, Some(&child));
        }
        assert_eq!(walked, vec!["1", "2", "10", "AC", "B"]);

        let top = key(&["PRC", "442"]);
        assert_eq!(order(&nodes, &top, None), Some(T("1".to_string())));
        assert_eq!(
            order(&nodes, &top, Some(&T("1".to_string()))),
            Some(T("2".to_string()))
        );
        assert_eq!(order(&nodes, &top, Some(&T("2".to_string()))), None);
        assert_eq!(order(&nodes, &key(&["PRC", "442", "9"]), None), None);
        assert_eq!(order(&nodes, &parent, Some(&T("C".to_string()))), None);
    }

    #[test]
    fn num_leading_prefix_coercion() {
        let cases = [
            ("500-A12345", "500"),
            ("", "0"),
            (".50", ".5"),
            ("007", "7"),
            ("12abc", "12"),
            ("-0", "0"),
            ("3261028.1430", "3261028.143"),
            ("1E5", "1"),
            ("0.5", ".5"),
            ("1.", "1"),
            ("-", "0"),
            ("--5", "5"),
            ("+-5", "-5"),
            ("-.250", "-.25"),
            (" 5", "0"),
            ("N/C", "0"),
        ];
        for (text, expected) in cases {
            assert_eq!(num(text), expected, "num({text:?})");
        }
        assert_eq!(num_cmp("", "10"), Ordering::Less);
        assert_eq!(num_cmp("12", "10"), Ordering::Greater);
        assert_eq!(num_cmp("25", "25.0"), Ordering::Equal);
        assert_eq!(num_cmp("B", "0"), Ordering::Equal);
    }

    #[test]
    fn int_div_and_modulo_on_fileman_dates() {
        assert_eq!(int_div("3261028", "100"), "32610");
        assert_eq!(modulo("32610", "100"), "10");
        assert_eq!(modulo("3261028", "100"), "28");
        assert_eq!(int_div("3261028", "10000"), "326");
        assert_eq!(fileman_date("3261028"), "10/28/2026");
        assert_eq!(modulo("3261028.143", "100"), "28.143");
        assert_eq!(fileman_date("3261028.143"), "10/28/2026");
        assert_eq!(fileman_date("3260105"), "1/5/2026");
        assert_eq!(modulo("-7", "5"), "3");
        assert_eq!(modulo("7", "-5"), "-3");
        assert_eq!(modulo("10", "5"), "0");
        assert_eq!(int_div("-7", "2"), "-3");
        assert_eq!(int_div("7.9", "1"), "7");
        assert_eq!(add("326", "1700"), "2026");
        assert_eq!(add("1.5", ".25"), "1.75");
        assert_eq!(add("-1.5", "1.5"), "0");
        assert_eq!(int_div("1", "0"), ARITHMETIC_ERROR_VALUE);
        assert_eq!(modulo("1", "0"), ARITHMETIC_ERROR_VALUE);
    }

    #[test]
    fn justify_two_and_three_argument() {
        let cases = [
            ("251", 8, Some(2), "  251.00"),
            (".99", 7, Some(2), "   0.99"),
            ("125.5", 7, Some(2), " 125.50"),
            ("311.84", 8, Some(2), "  311.84"),
            ("1.005", 0, Some(2), "1.01"),
            ("2", 5, None, "    2"),
            ("59.85", 7, Some(2), "  59.85"),
            ("19.95", 7, Some(2), "  19.95"),
            (".996", 7, Some(2), "   1.00"),
            ("9.995", 0, Some(2), "10.00"),
            ("1.23456", 5, Some(4), "1.2346"),
            ("19.995", 6, Some(3), "19.995"),
            ("2.5", 3, Some(0), "  3"),
            (".4", 3, Some(0), "  0"),
            ("", 7, Some(2), "   0.00"),
            ("007", 4, None, " 007"),
            ("12345678", 3, None, "12345678"),
        ];
        for (value, width, decimals, expected) in cases {
            assert_eq!(
                justify(value, width, decimals),
                expected,
                "justify({value:?}, {width}, {decimals:?})"
            );
        }
    }

    #[test]
    fn write_newline_and_tab_track_x_y() {
        let inputs: Vec<String> = Vec::new();
        let mut session = Session::new(&inputs);
        session.write("ABC");
        assert_eq!(session.x(), 3);
        session.tab(8);
        assert_eq!(session.x(), 8);
        session.tab(4);
        assert_eq!(session.x(), 8);
        session.nl();
        assert_eq!(session.x(), 0);
        assert_eq!(session.y(), 1);
        assert_eq!(session.transcript(), "ABC     \n");
        session.bell();
        session.write(" ??");
        assert_eq!(session.x(), 4);
        assert_eq!(session.y(), 1);
        assert_eq!(session.into_transcript(), "ABC     \n\u{7} ??");
    }

    #[test]
    fn read_returns_none_when_exhausted() {
        let inputs = vec![String::new(), "^".to_string()];
        let mut session = Session::new(&inputs);
        assert_eq!(session.read(), Some(String::new()));
        assert_eq!(session.read(), Some("^".to_string()));
        assert_eq!(session.read(), None);
        assert_eq!(session.read(), None);
        assert_eq!(session.transcript(), "");
        assert_eq!((session.x(), session.y()), (0, 0));
    }
}
