//! Integration tests of the loopback bridge, driven through its public test
//! hook `serve_until` over real TCP connections on `127.0.0.1:0`.
//!
//! The `synthetic_*` tests check routing only: static paths, status codes,
//! error texts, JSON framing and the capture-file rules. Their store and
//! capture directories are invented. Each directory lives under
//! `std::env::temp_dir()`, carries `synthetic` in its name and a
//! `SYNTHETIC.txt` marker, and is removed when its test ends. Synthetic
//! fixtures are never parity evidence. Every expected Rust-side answer comes
//! from calling `inquire` directly on the same store, never from a literal.
//!
//! `gc_02_end_to_end_compare` is the one check against a real capture,
//! `tests/golden/ifcap/GC-02/` with the real fixtures, and is reported apart
//! from the synthetic tests. Without GC-02's prerequisites it prints NOT
//! COMPARED and passes.
//!
//! Two answers cannot be reached end to end, so the unit tests in
//! `src/bridge.rs` cover them instead:
//! - HTTP 500: every `CorruptStore` site on the Rust path lies after the
//!   BLOCKED branch `PO-12.b1`, the device gate a found order reaches first
//!   (`inquiry_error_to_response_corrupt_store_500`).
//! - `match: null`: it arises only in live mode, which cannot obtain the
//!   original's return value (open item O-06), and no live session runs here
//!   (`compare_match_null_when_mumps_returned_unavailable`).
//!
//! No test sets an environment variable: configuration reaches the bridge
//! only as `serve_until` arguments. Every test binds its own port and uses
//! its own directory, so the tests also run in parallel.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ifcap_po_inquiry::bridge::{
    MumpsMode, normalise, parse_normalisation, serve_until, split_input_lines,
};
use ifcap_po_inquiry::store::load_fixtures;
use ifcap_po_inquiry::{InquiryError, Store, StoreError, inquire};

/// Read and write timeout of every client connection.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long [`with_server`] waits, once it has set the shutdown flag, for
/// the server thread to finish.
const STOP_BOUND: Duration = Duration::from_secs(5);
/// The bound `synthetic_shutdown_joins_within_bound` holds the server to.
const SHUTDOWN_BOUND: Duration = Duration::from_secs(2);
/// A session the capture-file tests send: RETURN at the station prompt and
/// `^` at the order prompt. The replay side never runs it, so any sequence
/// that round-trips through `split_input_lines` would do.
const SESSION_INPUTS: [&str; 2] = ["", "^"];

// ---------------------------------------------------------------------------
// Synthetic directories and store
// ---------------------------------------------------------------------------

/// Text of `SYNTHETIC.txt` in every synthetic directory.
const SYNTHETIC_MARKER: &str = "\
SYNTHETIC ROUTING FIXTURE
Created by tests/bridge.rs of the ifcap-po-inquiry crate for one routing
test, and removed when that test ends. It is not a capture of the original
system and is never parity evidence.
";

/// Distinguishes directories created in the same nanosecond.
static SYNTHETIC_SERIAL: AtomicUsize = AtomicUsize::new(0);

/// Runs `body` with one test's own capture directory under
/// `std::env::temp_dir()`, named
/// `ifcap-po-inquiry-synthetic-<test>-<pid>-<nanos>-<serial>` and marked by
/// `SYNTHETIC.txt`. The directory is the `golden_dir` of the test's bridge,
/// and its one case is `GC-01/`.
///
/// The directory is removed on every exit of `body`: `Ok`, `Err` or a
/// panic, which is resumed once the directory is gone. The result is
/// `body`'s, or the removal failure when `body` succeeded. A directory
/// `body` has already removed is not an error.
fn with_synthetic_dir<T>(
    test: &str,
    body: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let serial = SYNTHETIC_SERIAL.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "ifcap-po-inquiry-synthetic-{test}-{}-{nanos}-{serial}",
        std::process::id()
    ));
    fs::create_dir_all(&root)
        .map_err(|e| format!("{test}: creating the synthetic directory: {e}"))?;
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        write_file(&root, "SYNTHETIC.txt", SYNTHETIC_MARKER.as_bytes())?;
        body(&root)
    }));
    let removed = match fs::remove_dir_all(&root) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(format!("{test}: removing the synthetic directory: {e}"))
        }
        _ => Ok(()),
    };
    match outcome {
        Ok(result) => result.and_then(|value| removed.map(|()| value)),
        Err(payload) => panic::resume_unwind(payload),
    }
}

/// The path of `name` inside the `GC-01/` case directory under `root`, the
/// first case the bridge scans.
fn case_file(root: &Path, name: &str) -> PathBuf {
    root.join("GC-01").join(name)
}

/// Writes `bytes` to `rel` under `root`, creating its parents. Errors name
/// `rel`, never the temporary path.
fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("creating the parent of synthetic {rel}: {e}"))?;
    }
    fs::write(&path, bytes).map_err(|e| format!("writing synthetic {rel}: {e}"))?;
    Ok(path)
}

/// Replay mode over the synthetic directory `root`.
fn replay(root: &Path) -> MumpsMode {
    MumpsMode::Replay {
        golden_dir: root.to_path_buf(),
    }
}

/// Live mode over the synthetic directory `root`, in a container that does
/// not exist.
fn live(root: &Path) -> MumpsMode {
    MumpsMode::Live {
        container: "synthetic-no-such-container".to_string(),
        golden_dir: root.to_path_buf(),
    }
}

/// An invented store for the routing tests, taken from no snapshot: the
/// site file with two stations, 500 the primary one; the institution entry
/// 500; and the purchase-order file header with no order.
///
/// The `^PRC(442,0)` header is the only node of file 442 here, and is kept
/// although the store is meant to hold no purchase-order data: the order
/// lookup tests it before prompting, and without it every session reaching
/// the order prompt stops at `UnsupportedBranch("PO-09.b15")`, so neither
/// `^` nor `?` would be read there. No order record or index entry is
/// added, as the repository snapshot holds no order. The site-context stubs
/// read none of this, which is why every expectation comes from `inquire`.
const SYNTHETIC_ZWR: &str = r#"SYNTHETIC routing store of tests/bridge.rs: invented, not a capture, never parity evidence
ZWR
^DIC(4,500,0)="SYNTHETIC INSTITUTION 500"
^PRC(411,0)="SYNTHETIC SITE PARAMETER^411I^501^2"
^PRC(411,500,0)="500^Y"
^PRC(411,501,0)="501^N"
^PRC(411,"AC","Y",500)=""
^PRC(411,"B",500,500)=""
^PRC(411,"B",501,501)=""
^PRC(442,0)="PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^^0"
"#;

fn synthetic_store() -> Result<Store, String> {
    Store::from_zwr(SYNTHETIC_ZWR)
        .map_err(|e| format!("SYNTHETIC_ZWR does not load: {}", store_error_cause(&e)))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

/// The captured form of `inputs`: each input followed by one `\n`.
fn capture_form(inputs: &[String]) -> String {
    let mut text = String::new();
    for input in inputs {
        text.push_str(input);
        text.push('\n');
    }
    text
}

/// The first input of `read_back` that differs from `inputs`, 1-based, or
/// the count past the shorter list; `None` when the lists are equal.
fn first_differing_input(read_back: &[String], inputs: &[String]) -> Option<usize> {
    if read_back == inputs {
        return None;
    }
    let same = read_back
        .iter()
        .zip(inputs)
        .take_while(|(a, b)| a == b)
        .count();
    Some(same.saturating_add(1))
}

/// A synthetic session that `inquire` answers with `Ok`, as its inputs, its
/// transcript and its return value: the first candidate sequence that
/// `inquire` answers with `Ok` on `store` and whose captured form reads back
/// through `split_input_lines` unchanged. The candidates shrink because
/// station acceptance or the order prompt may be BLOCKED by `SPEC.md`;
/// `Err` is the NOT RUN reason, naming each candidate by its number.
fn ok_case(store: &Store) -> Result<(Vec<String>, String, Option<String>), String> {
    let candidates: [&[&str]; 3] = [&["", "^"], &[""], &[]];
    let mut refusals: Vec<String> = Vec::new();
    for (number, candidate) in (1usize..).zip(candidates) {
        let inputs = strings(candidate);
        let read_back = split_input_lines(&capture_form(&inputs));
        if let Some(input) = first_differing_input(&read_back, &inputs) {
            refusals.push(format!(
                "candidate {number}: input {input} does not round-trip split_input_lines"
            ));
            continue;
        }
        match inquire(store, &inputs) {
            Ok(output) => return Ok((inputs, output.transcript, output.returned)),
            Err(InquiryError::UnsupportedBranch(id)) => {
                refusals.push(format!("candidate {number}: UnsupportedBranch({id})"));
            }
            Err(InquiryError::CorruptStore(_)) => {
                refusals.push(format!("candidate {number}: CorruptStore"));
            }
        }
    }
    Err(format!(
        "NOT RUN: inquire refuses every candidate input ({})",
        refusals.join(", ")
    ))
}

/// [`ok_case`], or `None` after printing the NOT RUN reason for `test`.
fn ok_case_or_not_run(test: &str, store: &Store) -> Option<(Vec<String>, String, Option<String>)> {
    match ok_case(store) {
        Ok(case) => Some(case),
        Err(reason) => {
            println!("{test}: {reason}");
            None
        }
    }
}

/// Writes `GC-01/input.txt` in captured form, after confirming it reads
/// back as `inputs` through `split_input_lines`, so that replay can match.
fn write_inputs(root: &Path, inputs: &[String]) -> Result<(), String> {
    let text = capture_form(inputs);
    if let Some(input) = first_differing_input(&split_input_lines(&text), inputs) {
        return Err(format!(
            "synthetic GC-01/input.txt: input {input} does not read back through split_input_lines"
        ));
    }
    write_file(root, "GC-01/input.txt", text.as_bytes())?;
    Ok(())
}

/// Writes a whole synthetic capture into `GC-01/`. `returned.txt` is zero
/// bytes for `None`, and otherwise the value with one trailing newline,
/// which the bridge removes.
fn write_case(
    root: &Path,
    inputs: &[String],
    transcript: &str,
    returned: Option<&str>,
    normalisation: &str,
) -> Result<(), String> {
    write_inputs(root, inputs)?;
    write_file(root, "GC-01/transcript.txt", transcript.as_bytes())?;
    let returned = returned
        .map(|value| format!("{value}\n"))
        .unwrap_or_default();
    write_file(root, "GC-01/returned.txt", returned.as_bytes())?;
    write_file(root, "GC-01/normalisation.txt", normalisation.as_bytes())?;
    Ok(())
}

/// A well-formed capture context whose `IOF` and `IOSL` carry values,
/// which `STUB_CONTEXT` holds unresolved while open item O-01 stands.
const SYNTHETIC_CONTEXT: &str = r##"{"DUZ":".5","IOF":"#","IOSL":"24","STATION":"500"}"##;

/// The seven keys of a synthetic `meta.txt`, with `context` and
/// `docker_exec_args` as given.
fn meta_text(context: &str, docker_exec_args: &str) -> String {
    format!(
        "image_digest=SYNTHETIC\nsource_revision=SYNTHETIC\nsnapshot_id=SYNTHETIC\n\
         record_ids=SYNTHETIC\nentry_point=EN3^PRCHRPT\ncontext={context}\n\
         docker_exec_args={docker_exec_args}\n"
    )
}

// ---------------------------------------------------------------------------
// Server and client
// ---------------------------------------------------------------------------

/// Sets `shutdown` and waits up to `bound` for the server thread, then
/// joins it. `Ok` only when `serve_until` returned `Ok(())` within the
/// bound; a thread still running then is left behind and reported.
fn stop_server(
    shutdown: &AtomicBool,
    handle: JoinHandle<io::Result<()>>,
    bound: Duration,
) -> Result<(), String> {
    shutdown.store(true, Ordering::SeqCst);
    let started = Instant::now();
    while !handle.is_finished() {
        if started.elapsed() >= bound {
            return Err(format!(
                "server stop: serve_until still running {} ms after the shutdown flag was set",
                bound.as_millis()
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    match handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("server stop: serve_until returned an error: {e}")),
        Err(_) => Err("server stop: the server thread panicked".to_string()),
    }
}

/// [`with_server_within`] with the stop bound [`STOP_BOUND`].
fn with_server<T>(
    store: Store,
    mode: MumpsMode,
    client: impl FnOnce(SocketAddr) -> Result<T, String>,
) -> Result<T, String> {
    with_server_within(STOP_BOUND, store, mode, client)
}

/// Runs `client` against a bridge serving `store` in `mode` on its own
/// `127.0.0.1:0` port, in its own thread, then stops the bridge on every
/// exit of `client`: `Ok`, `Err` or a panic, which is resumed once the
/// bridge is stopped. Stopping sets the shutdown flag and joins the server
/// thread within `bound`. The result is `client`'s, or the stop failure
/// when `client` succeeded.
fn with_server_within<T>(
    bound: Duration,
    store: Store,
    mode: MumpsMode,
    client: impl FnOnce(SocketAddr) -> Result<T, String>,
) -> Result<T, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&shutdown);
    let handle = thread::Builder::new()
        .name("synthetic-bridge".to_string())
        .spawn(move || serve_until(listener, store, mode, flag))
        .map_err(|e| format!("spawn the server thread: {e}"))?;
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| client(addr)));
    let stopped = stop_server(&shutdown, handle, bound);
    match outcome {
        Ok(result) => result.and_then(|value| stopped.map(|()| value)),
        Err(payload) => panic::resume_unwind(payload),
    }
}

/// A fresh client connection to `addr` with the 5-second read and write
/// timeouts.
fn connect(addr: SocketAddr) -> Result<TcpStream, String> {
    let stream = TcpStream::connect(addr).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| format!("set read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| format!("set write timeout: {e}"))?;
    Ok(stream)
}

/// Sends one request over a fresh connection and reads its response, as
/// its status and body. The target is sent literally, so `/../Cargo.toml`
/// reaches the server as typed; `None` sends an empty body.
fn http(
    addr: SocketAddr,
    method: &str,
    target: &str,
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    let fail = |what: &str, e: io::Error| format!("{method} {target}: {what}: {e}");
    let mut stream = connect(addr).map_err(|e| format!("{method} {target}: {e}"))?;
    let body = body.unwrap_or_default();
    let mut request = format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\n");
    let _ = write!(
        request,
        "Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(body);
    stream.write_all(&bytes).map_err(|e| fail("send", e))?;
    read_response(&mut stream).map_err(|e| format!("{method} {target}: {e}"))
}

/// POSTs `body` to `/api/{backend}/session`.
fn post_session(addr: SocketAddr, backend: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
    http(addr, "POST", &format!("/api/{backend}/session"), Some(body))
}

/// One `read`, retried on `Interrupted`.
fn read_some(stream: &mut TcpStream, buf: &mut [u8]) -> Result<usize, String> {
    loop {
        match stream.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("read: {e}")),
        }
    }
}

/// Reads the status line and headers, then exactly `Content-Length` body
/// bytes, or everything up to end of stream when that header is absent.
/// Errors name the part of the response at fault, never its content.
fn read_response(stream: &mut TcpStream) -> Result<(u16, Vec<u8>), String> {
    let mut received: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    let head_end = loop {
        if let Some(start) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            break start.saturating_add(4);
        }
        let n = read_some(stream, &mut buf)?;
        if n == 0 {
            return Err(format!(
                "connection closed inside the response head, after {} bytes",
                received.len()
            ));
        }
        received.extend_from_slice(buf.get(..n).unwrap_or_default());
    };
    let (head, rest) = received
        .split_at_checked(head_end)
        .ok_or_else(|| "response head offset out of range".to_string())?;
    let head = std::str::from_utf8(head)
        .map_err(|e| format!("response head is not UTF-8 at byte {}", e.valid_up_to()))?;
    let mut lines = head.trim_end_matches("\r\n").split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let (Some(version), Some(code)) = (parts.next(), parts.next()) else {
        return Err("malformed status line: fewer than two fields".to_string());
    };
    if !version.starts_with("HTTP/1.") {
        return Err("malformed status line: not HTTP/1.x".to_string());
    }
    let status: u16 = code
        .parse()
        .map_err(|_| "malformed status line: the status code is not a number".to_string())?;
    let mut content_length: Option<usize> = None;
    for (number, line) in (2usize..).zip(lines) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("malformed header on head line {number}: no ':'"));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let length = value
                .trim()
                .parse()
                .map_err(|_| format!("malformed Content-Length on head line {number}"))?;
            content_length = Some(length);
        }
    }
    let mut body = rest.to_vec();
    loop {
        if content_length.is_some_and(|length| body.len() >= length) {
            break;
        }
        let n = read_some(stream, &mut buf)?;
        if n == 0 {
            if let Some(length) = content_length {
                return Err(format!(
                    "connection closed after {} of {length} body bytes",
                    body.len()
                ));
            }
            break;
        }
        body.extend_from_slice(buf.get(..n).unwrap_or_default());
    }
    if let Some(length) = content_length {
        body.truncate(length);
    }
    Ok((status, body))
}

// ---------------------------------------------------------------------------
// Test-local JSON: an oracle independent of the bridge's private reader and
// writer
// ---------------------------------------------------------------------------

// A JSON text is checked once by `json_body` and then kept as that text.
// Every scanning function takes the bytes and a position, and returns the
// position just past what it scanned; errors name the cause and its byte
// offset, never the text.

/// `body` as text, once it is checked to be UTF-8 holding exactly one JSON
/// value.
fn json_body(body: &[u8]) -> Result<&str, String> {
    let text = std::str::from_utf8(body)
        .map_err(|e| format!("response body is not UTF-8 at byte {}", e.valid_up_to()))?;
    parse_json(text).map_err(|e| format!("response body is not JSON: {e}"))?;
    Ok(text)
}

/// Checks that `text` is exactly one JSON value with only whitespace around
/// it. Strings may hold four-hex-digit Unicode escapes, surrogate pairs
/// combined; an unpaired surrogate or an unescaped control character is an
/// error, so a response that leaves BEL or a tab raw fails here.
fn parse_json(text: &str) -> Result<(), String> {
    let bytes = text.as_bytes();
    let end = skip_whitespace(bytes, json_value(bytes, skip_whitespace(bytes, 0))?);
    if end == bytes.len() {
        Ok(())
    } else {
        Err(json_error("trailing text", end))
    }
}

fn json_error(what: &str, pos: usize) -> String {
    format!("{what} at byte {pos}")
}

fn skip_whitespace(bytes: &[u8], mut pos: usize) -> usize {
    while matches!(bytes.get(pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        pos += 1;
    }
    pos
}

/// The byte at `pos` and the position after it.
fn next_byte(bytes: &[u8], pos: usize) -> Result<(u8, usize), String> {
    bytes
        .get(pos)
        .map(|&byte| (byte, pos + 1))
        .ok_or_else(|| json_error("unexpected end", pos))
}

/// The position after `byte`, which must stand at `pos`.
fn eat(bytes: &[u8], pos: usize, byte: u8) -> Result<usize, String> {
    match next_byte(bytes, pos)? {
        (found, next) if found == byte => Ok(next),
        _ => Err(json_error(&format!("expected {:?}", char::from(byte)), pos)),
    }
}

/// Scans the one JSON value that starts at `pos`.
fn json_value(bytes: &[u8], pos: usize) -> Result<usize, String> {
    match bytes.get(pos) {
        Some(b'{') => json_object(bytes, pos, &mut |_, _, _| {}),
        Some(b'[') => json_array(bytes, pos),
        Some(b'"') => json_string(bytes, pos).map(|(_, next)| next),
        Some(b't') => json_literal(bytes, pos, "true"),
        Some(b'f') => json_literal(bytes, pos, "false"),
        Some(b'n') => json_literal(bytes, pos, "null"),
        Some(b'-' | b'0'..=b'9') => json_number(bytes, pos),
        _ => Err(json_error("expected a value", pos)),
    }
}

fn json_literal(bytes: &[u8], mut pos: usize, word: &str) -> Result<usize, String> {
    for byte in word.bytes() {
        pos = eat(bytes, pos, byte)?;
    }
    Ok(pos)
}

/// The position after one or more ASCII digits at `pos`; `None` when there
/// is none.
fn digits(bytes: &[u8], mut pos: usize) -> Option<usize> {
    let first = pos;
    while matches!(bytes.get(pos), Some(b'0'..=b'9')) {
        pos += 1;
    }
    (pos > first).then_some(pos)
}

fn json_number(bytes: &[u8], mut pos: usize) -> Result<usize, String> {
    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
    }
    pos = digits(bytes, pos).ok_or_else(|| json_error("expected digits", pos))?;
    if bytes.get(pos) == Some(&b'.') {
        let fraction = pos + 1;
        pos = digits(bytes, fraction)
            .ok_or_else(|| json_error("expected fraction digits", fraction))?;
    }
    if matches!(bytes.get(pos), Some(b'e' | b'E')) {
        pos += 1;
        if matches!(bytes.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        pos = digits(bytes, pos).ok_or_else(|| json_error("expected exponent digits", pos))?;
    }
    Ok(pos)
}

fn json_array(bytes: &[u8], pos: usize) -> Result<usize, String> {
    let mut pos = skip_whitespace(bytes, eat(bytes, pos, b'[')?);
    if bytes.get(pos) == Some(&b']') {
        return Ok(pos + 1);
    }
    loop {
        pos = skip_whitespace(bytes, json_value(bytes, skip_whitespace(bytes, pos))?);
        match next_byte(bytes, pos)? {
            (b',', next) => pos = next,
            (b']', next) => return Ok(next),
            _ => return Err(json_error("expected ',' or ']'", pos)),
        }
    }
}

/// Scans the object that starts at `pos`, calling `visit` with the decoded
/// name and the value's start and end of each member, in order.
fn json_object(
    bytes: &[u8],
    pos: usize,
    visit: &mut dyn FnMut(&str, usize, usize),
) -> Result<usize, String> {
    let mut pos = skip_whitespace(bytes, eat(bytes, pos, b'{')?);
    if bytes.get(pos) == Some(&b'}') {
        return Ok(pos + 1);
    }
    loop {
        let (name, after_name) = json_string(bytes, skip_whitespace(bytes, pos))?;
        let colon = skip_whitespace(bytes, after_name);
        let start = skip_whitespace(bytes, eat(bytes, colon, b':')?);
        let end = json_value(bytes, start)?;
        visit(&name, start, end);
        pos = skip_whitespace(bytes, end);
        match next_byte(bytes, pos)? {
            (b',', next) => pos = next,
            (b'}', next) => return Ok(next),
            _ => return Err(json_error("expected ',' or '}'", pos)),
        }
    }
}

/// Decodes the string that starts at `pos`, returning it with the position
/// after its closing quote. Raw bytes are copied as they stand, and every
/// delimiter is ASCII, so the UTF-8 of the input survives intact.
fn json_string(bytes: &[u8], pos: usize) -> Result<(String, usize), String> {
    let start = pos;
    let mut pos = eat(bytes, pos, b'"')?;
    let mut out: Vec<u8> = Vec::new();
    loop {
        let (byte, next) = next_byte(bytes, pos)?;
        match byte {
            b'"' => {
                let text =
                    String::from_utf8(out).map_err(|_| json_error("string is not UTF-8", start))?;
                return Ok((text, next));
            }
            b'\\' => {
                let (escape, after) = next_byte(bytes, next)?;
                let (c, after) = match escape {
                    b'"' => ('"', after),
                    b'\\' => ('\\', after),
                    b'/' => ('/', after),
                    b'b' => ('\u{8}', after),
                    b'f' => ('\u{c}', after),
                    b'n' => ('\n', after),
                    b'r' => ('\r', after),
                    b't' => ('\t', after),
                    b'u' => unicode_escape(bytes, after)?,
                    _ => return Err(json_error("unknown escape", next)),
                };
                let mut utf8 = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                pos = after;
            }
            byte if byte < 0x20 => return Err(json_error("unescaped control character", pos)),
            byte => {
                out.push(byte);
                pos = next;
            }
        }
    }
}

/// The character of a four-hex-digit Unicode escape whose backslash and `u`
/// end just before `pos`, reading the low half of a surrogate pair too, with
/// the position after it.
fn unicode_escape(bytes: &[u8], pos: usize) -> Result<(char, usize), String> {
    let (first, mut next) = hex4(bytes, pos)?;
    let code = match first {
        0xD800..=0xDBFF => {
            let low = eat(bytes, eat(bytes, next, b'\\')?, b'u')?;
            let (second, after) = hex4(bytes, low)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(json_error("high surrogate without a low surrogate", low));
            }
            next = after;
            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
        }
        0xDC00..=0xDFFF => return Err(json_error("unpaired low surrogate", pos)),
        _ => first,
    };
    char::from_u32(code)
        .map(|c| (c, next))
        .ok_or_else(|| json_error("invalid code point", pos))
}

fn hex4(bytes: &[u8], mut pos: usize) -> Result<(u32, usize), String> {
    let mut code = 0u32;
    for _ in 0..4 {
        let (byte, next) = next_byte(bytes, pos)?;
        let digit = char::from(byte)
            .to_digit(16)
            .ok_or_else(|| json_error("expected a hex digit", pos))?;
        code = code * 16 + digit;
        pos = next;
    }
    Ok((code, pos))
}

/// `text` as a JSON string: `"` and `\` escaped, `\n`, `\r` and `\t` by
/// name, every other control character as `\u00XX`, and the rest as itself.
fn json_str(text: &str) -> String {
    encode_json_string(text, false)
}

/// [`json_str`] with every non-ASCII character also escaped, as a
/// four-hex-digit Unicode escape or, beyond the Basic Multilingual Plane, as
/// a surrogate pair of them.
fn json_str_ascii(text: &str) -> String {
    encode_json_string(text, true)
}

fn encode_json_string(text: &str, ascii_only: bool) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c if ascii_only && !c.is_ascii() => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `{"inputs":[...]}` with each input encoded by `encode`.
fn inputs_body(inputs: &[String], encode: fn(&str) -> String) -> Vec<u8> {
    let elements: Vec<String> = inputs.iter().map(|input| encode(input)).collect();
    format!("{{\"inputs\":[{}]}}", elements.join(",")).into_bytes()
}

/// The session request body for `inputs`.
fn body_for(inputs: &[String]) -> Vec<u8> {
    inputs_body(inputs, json_str)
}

/// [`body_for`] in pure ASCII, every non-ASCII character escaped.
fn body_for_ascii(inputs: &[String]) -> Vec<u8> {
    inputs_body(inputs, json_str_ascii)
}

/// The value of member `name` of the JSON object `json`, as the text it
/// stands as in `json`; the first one when `name` repeats.
fn member<'j>(json: &'j str, name: &str) -> Result<&'j str, String> {
    let bytes = json.as_bytes();
    let start = skip_whitespace(bytes, 0);
    if bytes.get(start) != Some(&b'{') {
        return Err(format!(
            "member {name:?}: the value at byte {start} is not an object"
        ));
    }
    let mut found: Option<(usize, usize)> = None;
    json_object(bytes, start, &mut |key, value_start, value_end| {
        if found.is_none() && key == name {
            found = Some((value_start, value_end));
        }
    })?;
    let (value_start, value_end) = found.ok_or_else(|| format!("no member {name:?}"))?;
    json.get(value_start..value_end)
        .ok_or_else(|| format!("member {name:?}: bytes {value_start}..{value_end} out of range"))
}

/// The member `name` of `json` that is a string or `null`, decoded.
fn optional_str_member(json: &str, name: &str) -> Result<Option<String>, String> {
    let value = member(json, name)?;
    if value == "null" {
        return Ok(None);
    }
    if !value.starts_with('"') {
        return Err(format!("member {name:?} is neither a string nor null"));
    }
    json_string(value.as_bytes(), 0)
        .map(|(text, _)| Some(text))
        .map_err(|e| format!("member {name:?}: {e}"))
}

/// The string member `name` of `json`, decoded.
fn str_member(json: &str, name: &str) -> Result<String, String> {
    optional_str_member(json, name)?.ok_or_else(|| format!("member {name:?} is null, not a string"))
}

/// The `transcript` and `returned` of one session object.
fn session_of(json: &str) -> Result<(String, Option<String>), String> {
    Ok((
        str_member(json, "transcript")?,
        optional_str_member(json, "returned")?,
    ))
}

/// Asserts that `resp` has `status` and a JSON body `{"error": string}`, and
/// returns the error text. Failures name `context`, never the body.
fn expect_error(resp: &(u16, Vec<u8>), status: u16, context: &str) -> Result<String, String> {
    let (actual, body) = resp;
    assert_eq!(*actual, status, "{context}: status");
    let json = json_body(body).map_err(|e| format!("{context}: {e}"))?;
    str_member(json, "error").map_err(|e| format!("{context}: {e}"))
}

/// Starts a bridge in `mode` over a synthetic directory, POSTs `inputs` to
/// the mumps backend, stops the bridge, and returns the error text of the
/// 502 answer.
fn mumps_502(mode: MumpsMode, inputs: &[String]) -> Result<String, String> {
    let resp = with_server(synthetic_store()?, mode, |addr| {
        post_session(addr, "mumps", &body_for(inputs))
    })?;
    expect_error(&resp, 502, "mumps backend over the synthetic directory")
}

/// Where `actual` first differs from `expected`: the 1-based line and the
/// 1-based byte within it, with the byte offset and both lengths. Where one
/// is a prefix of the other, the difference is just past the shorter one.
fn first_difference(actual: &[u8], expected: &[u8]) -> String {
    let offset = actual
        .iter()
        .zip(expected)
        .position(|(a, e)| a != e)
        .unwrap_or(actual.len().min(expected.len()));
    let before = actual.get(..offset).unwrap_or(actual);
    let line = before.iter().filter(|&&byte| byte == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |newline| newline + 1);
    format!(
        "line {line} byte {} (offset {offset}; {} bytes, {} expected)",
        offset - line_start + 1,
        actual.len(),
        expected.len()
    )
}

/// Asserts that `actual` equals `expected` without printing either: a
/// failure names `check` and the first differing position.
fn assert_same(check: &str, actual: impl AsRef<[u8]>, expected: impl AsRef<[u8]>) {
    let (actual, expected) = (actual.as_ref(), expected.as_ref());
    assert!(
        actual == expected,
        "{check}: first difference at {}",
        first_difference(actual, expected)
    );
}

/// [`assert_same`] for a value that may be `null`.
fn assert_same_optional(check: &str, actual: Option<&str>, expected: Option<&str>) {
    let kind = |value: Option<&str>| if value.is_some() { "a string" } else { "null" };
    assert!(
        actual.is_some() == expected.is_some(),
        "{check}: {} where {} is expected",
        kind(actual),
        kind(expected)
    );
    if let (Some(actual), Some(expected)) = (actual, expected) {
        assert_same(check, actual, expected);
    }
}

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `path` relative to the crate, or its file name when it lies outside.
fn crate_relative(path: &Path) -> String {
    match path.strip_prefix(crate_dir()) {
        Ok(relative) => relative.display().to_string(),
        Err(_) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "a path outside the crate".to_string()),
    }
}

/// The cause of a `StoreError`, from its fields: the crate-relative file and
/// line of a malformed line, or the file that cannot be read. Never its
/// message, which may quote fixture content.
fn store_error_cause(e: &StoreError) -> String {
    match e {
        StoreError::Parse {
            file: Some(file),
            line,
            ..
        } => format!("{} line {line}: malformed", crate_relative(file)),
        StoreError::Parse {
            file: None, line, ..
        } => format!("line {line}: malformed"),
        StoreError::Io { file, .. } => format!("{}: unreadable", crate_relative(file)),
    }
}

// ---------------------------------------------------------------------------
// 200
// ---------------------------------------------------------------------------

#[test]
fn synthetic_static_paths_exact_match() -> Result<(), String> {
    let frontend = crate_dir().join("frontend");
    let static_paths = [
        ("/", "index.html"),
        ("/index.html", "index.html"),
        ("/style.css", "style.css"),
        ("/app.js", "app.js"),
    ];
    with_synthetic_dir("static", |root| {
        with_server(synthetic_store()?, replay(root), |addr| {
            for (target, file) in static_paths {
                let expected = fs::read(frontend.join(file))
                    .map_err(|e| format!("reading frontend/{file}: {e}"))?;
                let (status, body) = http(addr, "GET", target, None)?;
                assert_eq!(status, 200, "GET {target}: status");
                assert_same(
                    &format!("GET {target} against frontend/{file}"),
                    &body,
                    &expected,
                );
            }
            for target in ["/../Cargo.toml", "/frontend/app.js"] {
                let resp = http(addr, "GET", target, None)?;
                expect_error(&resp, 404, &format!("GET {target}"))?;
            }
            Ok(())
        })
    })
}

#[test]
fn synthetic_rust_session_200() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some((inputs, transcript, returned)) =
        ok_case_or_not_run("synthetic_rust_session_200", &store)
    else {
        return Ok(());
    };
    let (status, body) = with_synthetic_dir("rust-200", |root| {
        with_server(store, replay(root), |addr| {
            post_session(addr, "rust", &body_for(&inputs))
        })
    })?;
    assert_eq!(status, 200, "rust backend: status");
    let (rust_transcript, rust_returned) = session_of(json_body(&body)?)?;
    assert_same(
        "rust transcript against inquire",
        &rust_transcript,
        &transcript,
    );
    assert_same_optional(
        "rust returned against inquire",
        rust_returned.as_deref(),
        returned.as_deref(),
    );
    Ok(())
}

#[test]
fn synthetic_mumps_replay_200() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some((inputs, transcript, _)) = ok_case_or_not_run("synthetic_mumps_replay_200", &store)
    else {
        return Ok(());
    };
    let (status, body) = with_synthetic_dir("mumps-200", |root| {
        // A zero-byte returned.txt, which replay must answer as null.
        write_case(root, &inputs, &transcript, None, "")?;
        with_server(store, replay(root), |addr| {
            post_session(addr, "mumps", &body_for(&inputs))
        })
    })?;
    assert_eq!(status, 200, "mumps backend: status");
    let json = json_body(&body)?;
    let (replayed, _) = session_of(json)?;
    assert_same(
        "replayed transcript against transcript.txt",
        &replayed,
        &transcript,
    );
    assert!(
        member(json, "returned")? == "null",
        "a zero-byte returned.txt must replay as null"
    );
    Ok(())
}

#[test]
fn synthetic_compare_match_true() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some((inputs, transcript, returned)) =
        ok_case_or_not_run("synthetic_compare_match_true", &store)
    else {
        return Ok(());
    };
    let (status, body) = with_synthetic_dir("compare-true", |root| {
        write_case(root, &inputs, &transcript, returned.as_deref(), "")?;
        with_server(store, replay(root), |addr| {
            post_session(addr, "compare", &body_for(&inputs))
        })
    })?;
    assert_eq!(status, 200, "compare backend: status");
    let json = json_body(&body)?;
    for side in ["mumps", "rust"] {
        let (side_transcript, side_returned) = session_of(member(json, side)?)?;
        assert_same(
            &format!("{side} side transcript"),
            &side_transcript,
            &transcript,
        );
        assert_same_optional(
            &format!("{side} side returned"),
            side_returned.as_deref(),
            returned.as_deref(),
        );
    }
    assert!(member(json, "match")? == "true", "match must be true");
    Ok(())
}

#[test]
fn synthetic_compare_match_false() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some((inputs, transcript, returned)) =
        ok_case_or_not_run("synthetic_compare_match_false", &store)
    else {
        return Ok(());
    };
    let differing = format!("{transcript}SYNTHETIC DIFFERENCE\n");
    let (status, body) = with_synthetic_dir("compare-false", |root| {
        write_case(root, &inputs, &differing, returned.as_deref(), "")?;
        with_server(store, replay(root), |addr| {
            post_session(addr, "compare", &body_for(&inputs))
        })
    })?;
    assert_eq!(status, 200, "compare backend: status");
    let json = json_body(&body)?;
    let (mumps_transcript, _) = session_of(member(json, "mumps")?)?;
    let (rust_transcript, _) = session_of(member(json, "rust")?)?;
    assert_same("mumps side is the capture", &mumps_transcript, &differing);
    assert_same("rust side is inquire", &rust_transcript, &transcript);
    assert!(member(json, "match")? == "false", "match must be false");
    Ok(())
}

// ---------------------------------------------------------------------------
// 400, on every backend
// ---------------------------------------------------------------------------

/// Asserts that each of `bodies` is answered 400 with `{"error": string}`
/// by the rust, mumps and compare backends alike.
fn assert_400_on_every_backend(test: &str, bodies: &[Vec<u8>]) -> Result<(), String> {
    with_synthetic_dir(test, |root| {
        with_server(synthetic_store()?, replay(root), |addr| {
            for backend in ["rust", "mumps", "compare"] {
                for (number, body) in (1usize..).zip(bodies) {
                    let resp = post_session(addr, backend, body)?;
                    let context = format!("{test}: POST /api/{backend}/session body {number}");
                    let error = expect_error(&resp, 400, &context)?;
                    assert!(!error.is_empty(), "{context}: empty error text");
                }
            }
            Ok(())
        })
    })
}

#[test]
fn synthetic_400_malformed_json() -> Result<(), String> {
    assert_400_on_every_backend("400-json", &[b"{\"inputs\": [".to_vec()])
}

#[test]
fn synthetic_400_inputs_missing_or_not_array() -> Result<(), String> {
    assert_400_on_every_backend(
        "400-inputs",
        &[b"{}".to_vec(), b"{\"inputs\":\"x\"}".to_vec()],
    )
}

#[test]
fn synthetic_400_non_string_element() -> Result<(), String> {
    assert_400_on_every_backend(
        "400-element",
        &[
            b"{\"inputs\":[1]}".to_vec(),
            b"{\"inputs\":[null]}".to_vec(),
        ],
    )
}

#[test]
fn synthetic_400_element_with_line_break() -> Result<(), String> {
    assert_400_on_every_backend(
        "400-line-break",
        &[body_for(&strings(&["a\rb"])), body_for(&strings(&["a\nb"]))],
    )
}

// ---------------------------------------------------------------------------
// 404
// ---------------------------------------------------------------------------

/// Asserts that each `(method, target)` is answered 404 with
/// `{"error": string}`; POST and PUT carry a well-formed session body.
fn assert_404(test: &str, requests: &[(&str, &str)]) -> Result<(), String> {
    let body = body_for(&strings(&SESSION_INPUTS));
    with_synthetic_dir(test, |root| {
        with_server(synthetic_store()?, replay(root), |addr| {
            for (method, target) in requests {
                let sent = match *method {
                    "POST" | "PUT" => Some(body.as_slice()),
                    _ => None,
                };
                let resp = http(addr, method, target, sent)?;
                expect_error(&resp, 404, &format!("{test}: {method} {target}"))?;
            }
            Ok(())
        })
    })
}

#[test]
fn synthetic_404_unknown_backend() -> Result<(), String> {
    assert_404("404-backend", &[("POST", "/api/java/session")])
}

#[test]
fn synthetic_404_unknown_path() -> Result<(), String> {
    assert_404(
        "404-path",
        &[
            ("GET", "/nope"),
            ("POST", "/nope"),
            ("POST", "/api/rust/other"),
        ],
    )
}

#[test]
fn synthetic_404_non_post_on_api() -> Result<(), String> {
    assert_404(
        "404-method",
        &[("GET", "/api/rust/session"), ("PUT", "/api/rust/session")],
    )
}

// ---------------------------------------------------------------------------
// 422
// ---------------------------------------------------------------------------

#[test]
fn synthetic_422_unsupported_branch_end_to_end() -> Result<(), String> {
    let store = synthetic_store()?;
    // RETURN at the station prompt, then `?` (help) at the order prompt.
    let inputs = strings(&["", "?"]);
    let id = match inquire(&store, &inputs) {
        Err(InquiryError::UnsupportedBranch(id)) => id,
        Err(InquiryError::CorruptStore(_)) => {
            return Err(
                "inquire on the help input must be UnsupportedBranch, got CorruptStore".to_string(),
            );
        }
        Ok(_) => {
            return Err("inquire on the help input must be UnsupportedBranch, got Ok".to_string());
        }
    };
    // PO-09.b4 is help at the order prompt; PO-05.b1 is station acceptance,
    // should SPEC.md block it.
    assert!(
        id == "PO-09.b4" || id == "PO-05.b1",
        "inquire on the help input gave UnsupportedBranch({id}), expected PO-09.b4 or PO-05.b1"
    );
    let body = body_for(&inputs);
    let (rust, mumps, compare) = with_synthetic_dir("422", |root| {
        write_case(
            root,
            &inputs,
            "SYNTHETIC TRANSCRIPT: any text; the MUMPS side only has to succeed\n",
            None,
            "",
        )?;
        with_server(store, replay(root), |addr| {
            Ok((
                post_session(addr, "rust", &body)?,
                post_session(addr, "mumps", &body)?,
                post_session(addr, "compare", &body)?,
            ))
        })
    })?;
    let error = expect_error(&rust, 422, "rust")?;
    assert!(error.contains(&id), "rust 422 error text must name {id}");
    assert_eq!(
        mumps.0, 200,
        "the synthetic capture must replay, so that compare reaches the Rust side: status"
    );
    let error = expect_error(&compare, 422, "compare")?;
    assert!(error.contains(&id), "compare 422 error text must name {id}");
    Ok(())
}

// ---------------------------------------------------------------------------
// 502, on the mumps backend
// ---------------------------------------------------------------------------

#[test]
fn synthetic_502_not_recorded() -> Result<(), String> {
    with_synthetic_dir("502-not-recorded", |root| {
        // A complete capture of another session, which must not match.
        write_case(
            root,
            &strings(&["SYNTHETIC OTHER INPUT"]),
            "SYNTHETIC TRANSCRIPT\n",
            None,
            "",
        )?;
        let error = mumps_502(replay(root), &strings(&SESSION_INPUTS))?;
        assert_same("502 error text", &error, "NOT RECORDED");
        Ok(())
    })
}

#[test]
fn synthetic_502_not_recorded_lists_unreadable_input() -> Result<(), String> {
    with_synthetic_dir("502-unreadable-input", |root| {
        let input = write_file(root, "GC-01/input.txt", &[0xFF, 0xFE, b'\n'])?;
        let error = mumps_502(replay(root), &strings(&SESSION_INPUTS))?;
        assert!(
            error.starts_with("NOT RECORDED"),
            "502 error text must start with NOT RECORDED"
        );
        let path = input.display().to_string();
        assert!(
            error.contains(&path),
            "502 error text must list the unreadable synthetic GC-01/input.txt"
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_capture_file_missing() -> Result<(), String> {
    let inputs = strings(&SESSION_INPUTS);
    with_synthetic_dir("502-missing", |root| {
        write_inputs(root, &inputs)?;
        write_file(root, "GC-01/returned.txt", b"")?;
        write_file(root, "GC-01/normalisation.txt", b"")?;
        let error = mumps_502(replay(root), &inputs)?;
        let path = case_file(root, "transcript.txt");
        assert_same(
            "502 error text",
            &error,
            format!("CAPTURE FILE {}: MISSING", path.display()),
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_capture_file_unreadable() -> Result<(), String> {
    let inputs = strings(&SESSION_INPUTS);
    with_synthetic_dir("502-unreadable", |root| {
        write_case(root, &inputs, "", None, "")?;
        // A directory in place of transcript.txt is present but unreadable.
        let path = case_file(root, "transcript.txt");
        fs::remove_file(&path)
            .map_err(|e| format!("removing synthetic GC-01/transcript.txt: {e}"))?;
        fs::create_dir(&path)
            .map_err(|e| format!("creating the directory GC-01/transcript.txt: {e}"))?;
        let error = mumps_502(replay(root), &inputs)?;
        assert_same(
            "502 error text",
            &error,
            format!("CAPTURE FILE {}: UNREADABLE", path.display()),
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_capture_file_non_utf8() -> Result<(), String> {
    let inputs = strings(&SESSION_INPUTS);
    with_synthetic_dir("502-non-utf8", |root| {
        write_case(root, &inputs, "", None, "")?;
        let path = write_file(root, "GC-01/transcript.txt", &[0xFF, 0xFE])?;
        let error = mumps_502(replay(root), &inputs)?;
        assert_same(
            "502 error text",
            &error,
            format!("CAPTURE FILE {}: NON-UTF8", path.display()),
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_malformed_normalisation() -> Result<(), String> {
    let inputs = strings(&SESSION_INPUTS);
    with_synthetic_dir("502-normalisation", |root| {
        write_case(
            root,
            &inputs,
            "SYNTHETIC TRANSCRIPT\n",
            None,
            "SYNTHETIC RULE WITHOUT A TAB\n",
        )?;
        let error = mumps_502(replay(root), &inputs)?;
        let path = case_file(root, "normalisation.txt");
        assert_same(
            "502 error text",
            &error,
            format!("MALFORMED NORMALISATION {} line 1", path.display()),
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_malformed_meta() -> Result<(), String> {
    with_synthetic_dir("502-meta", |root| {
        let meta = meta_text(SYNTHETIC_CONTEXT, "not-json");
        let path = write_file(root, "GC-01/meta.txt", meta.as_bytes())?;
        let error = mumps_502(live(root), &strings(&SESSION_INPUTS))?;
        assert_same(
            "502 error text",
            &error,
            format!("MALFORMED META {}", path.display()),
        );
        Ok(())
    })
}

#[test]
fn synthetic_502_context_mismatch() -> Result<(), String> {
    with_synthetic_dir("502-context", |root| {
        let meta = meta_text(SYNTHETIC_CONTEXT, r#"["synthetic-never-run"]"#);
        write_file(root, "GC-01/meta.txt", meta.as_bytes())?;
        let error = mumps_502(live(root), &strings(&SESSION_INPUTS))?;
        // A context error, not a spawn error or an `EXIT` cause, proves the
        // bridge stopped before it ran `docker`.
        assert!(
            error.starts_with("CONTEXT MISMATCH IOF: expected UNRESOLVED, actual #"),
            "502 error text must be the IOF context mismatch"
        );
        assert!(
            !error.contains("EXIT") && !error.contains("os error"),
            "502 error text shows that docker ran"
        );
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Other
// ---------------------------------------------------------------------------

/// Probe characters for the JSON round trip: a quote, a backslash, two
/// characters of the Basic Multilingual Plane, one beyond it, and a control
/// character.
const PROBE: &str = "\"\\\u{e9}\u{20ac}\u{1f600}\u{1}";

#[test]
fn synthetic_json_escaping_round_trip() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some((case_inputs, case_transcript, _)) =
        ok_case_or_not_run("synthetic_json_escaping_round_trip", &store)
    else {
        return Ok(());
    };
    // The probe rides as one extra input line, which the replay must match.
    let mut inputs = case_inputs;
    inputs.push(format!("SYNTHETIC PROBE {PROBE}"));
    // Synthetic: the inquire transcript, then the probe, a tab and BEL,
    // which the bridge must escape and this parser must decode.
    let transcript = format!("{case_transcript}{PROBE}\t\u{7}");
    let body = body_for_ascii(&inputs);
    assert!(
        body.is_ascii(),
        "the request must carry every non-ASCII character escaped"
    );
    let sent = String::from_utf8_lossy(&body).into_owned();
    for escape in [
        "\\\"",
        "\\\\",
        "\\u00e9",
        "\\u20ac",
        "\\ud83d\\ude00",
        "\\u0001",
    ] {
        assert!(
            sent.contains(escape),
            "the request body lacks the escape {escape}"
        );
    }
    let ((status, response), file) = with_synthetic_dir("json", |root| {
        write_case(root, &inputs, &transcript, None, "")?;
        let resp = with_server(store, replay(root), |addr| {
            post_session(addr, "mumps", &body)
        })?;
        let file = fs::read_to_string(case_file(root, "transcript.txt"))
            .map_err(|e| format!("reading synthetic GC-01/transcript.txt: {e}"))?;
        Ok((resp, file))
    })?;
    assert_eq!(status, 200, "mumps backend: status");
    let (decoded, _) = session_of(json_body(&response)?)?;
    assert_same("decoded transcript against transcript.txt", &decoded, &file);
    Ok(())
}

/// The interim response the bridge sends to `Expect: 100-continue`.
const CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

/// Reads `stream` until the peer closes it, and returns what arrived. A
/// reset counts as a close; a read that times out is an error.
fn read_until_closed(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut received = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return Ok(received),
            Ok(n) => received.extend_from_slice(buf.get(..n).unwrap_or_default()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => return Ok(received),
            Err(e) => return Err(format!("still open after {} bytes: {e}", received.len())),
        }
    }
}

#[test]
fn synthetic_shutdown_joins_within_bound() -> Result<(), String> {
    let inputs = strings(&SESSION_INPUTS);
    let body = body_for(&inputs);
    with_synthetic_dir("shutdown", |root| {
        // A capture the in-flight session matches, so that a connection
        // thread running on after shutdown would answer it.
        write_case(root, &inputs, "SYNTHETIC TRANSCRIPT\n", None, "")?;
        // The stop bound lies below the server's 5-second read timeout, so a
        // connection thread blocked in a read must be woken, not waited out:
        // `with_server_within` fails unless `serve_until` returns `Ok(())`
        // within the bound.
        let (mut idle, mut in_flight) =
            with_server_within(SHUTDOWN_BOUND, synthetic_store()?, replay(root), |addr| {
                // One answered request shows the accept loop was running.
                let (status, _) = http(addr, "GET", "/", None)?;
                assert_eq!(status, 200, "GET /: status");
                // The idle connection sends nothing. Connections are accepted
                // in arrival order, so once the in-flight one is answered
                // below, the idle one has been accepted too.
                let idle = connect(addr)?;
                let mut in_flight = connect(addr)?;
                let head = format!(
                    "POST /api/mumps/session HTTP/1.1\r\nHost: {addr}\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\
                     Expect: 100-continue\r\n\r\n",
                    body.len()
                );
                in_flight
                    .write_all(head.as_bytes())
                    .map_err(|e| format!("in-flight connection: sending the head: {e}"))?;
                // The interim response shows its connection thread is now
                // blocked reading the body.
                let mut interim = vec![0u8; CONTINUE.len()];
                in_flight
                    .read_exact(&mut interim)
                    .map_err(|e| format!("in-flight connection: reading 100 Continue: {e}"))?;
                assert_same("in-flight connection: interim response", &interim, CONTINUE);
                Ok((idle, in_flight))
            })?;
        // The session's resources go, as a caller of `serve_until` may remove
        // them once it has returned: a connection thread still running would
        // now answer 502, or 200 had it read the capture first.
        fs::remove_dir_all(root).map_err(|e| format!("removing the synthetic directory: {e}"))?;
        // The server may have reset the connection already, so the send may
        // fail; only an answer would be wrong.
        let _ = in_flight.write_all(&body);
        let in_flight_rest =
            read_until_closed(&mut in_flight).map_err(|e| format!("in-flight connection: {e}"))?;
        let started = Instant::now();
        let idle_rest = read_until_closed(&mut idle);
        let idle_wait = started.elapsed();
        assert!(
            in_flight_rest.is_empty(),
            "in-flight connection: {} response bytes after shutdown, none expected",
            in_flight_rest.len()
        );
        let idle_rest = idle_rest.map_err(|e| format!("idle connection: {e}"))?;
        assert!(
            idle_rest.is_empty(),
            "idle connection: {} bytes after shutdown, none expected",
            idle_rest.len()
        );
        assert!(
            idle_wait < SHUTDOWN_BOUND,
            "idle connection: closed {} ms after serve_until returned",
            idle_wait.as_millis()
        );
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// GC-02 end to end (not synthetic)
// ---------------------------------------------------------------------------

/// The seven capture files of one case.
const CAPTURE_FILES: [&str; 7] = [
    "input.txt",
    "transcript.txt",
    "returned.txt",
    "db-before.zwr",
    "db-after.zwr",
    "meta.txt",
    "normalisation.txt",
];

/// The eleven repository fixtures, the capture owner's runtime export and
/// the manifest, under `tests/fixtures/ifcap/`.
const FIXTURE_FILES: [&str; 13] = [
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
    "cases.zwr",
    "MANIFEST.txt",
];

#[test]
fn gc_02_end_to_end_compare() -> Result<(), String> {
    const LABEL: &str = "gc_02_end_to_end_compare (not synthetic)";
    const GOLDEN: &str = "tests/golden/ifcap";
    const CASE: &str = "tests/golden/ifcap/GC-02";
    const FIXTURES: &str = "tests/fixtures/ifcap";
    // Every path is named relative to the crate.
    let required = CAPTURE_FILES
        .iter()
        .map(|name| format!("{CASE}/{name}"))
        .chain(
            FIXTURE_FILES
                .iter()
                .map(|name| format!("{FIXTURES}/{name}")),
        );
    let missing: Vec<String> = required
        .filter(|relative| !crate_dir().join(relative).exists())
        .collect();
    if !missing.is_empty() {
        println!("{LABEL}: NOT COMPARED, missing: {}", missing.join(", "));
        return Ok(());
    }
    let fail = |step: &str, detail: String| format!("{LABEL}: FAIL at {step}: {detail}");
    let read = |name: &str| {
        fs::read_to_string(crate_dir().join(CASE).join(name))
            .map_err(|e| fail("reading the capture", format!("{CASE}/{name}: {e}")))
    };
    let store = load_fixtures(&crate_dir().join(FIXTURES))
        .map_err(|e| fail("load_fixtures", store_error_cause(&e)))?;
    let inputs = split_input_lines(&read("input.txt")?);
    let captured_transcript = read("transcript.txt")?;
    let captured_returned = read("returned.txt")?;
    let normalisation = read("normalisation.txt")?;

    let mode = MumpsMode::Replay {
        golden_dir: crate_dir().join(GOLDEN),
    };
    let (status, body) = with_server(store, mode, |addr| {
        post_session(addr, "compare", &body_for(&inputs))
    })?;

    assert_eq!(status, 200, "{LABEL}: FAIL at status 200");
    let json = json_body(&body).map_err(|e| fail("the response body", e))?;
    let matched = member(json, "match").map_err(|e| fail("match", e))?;
    assert!(matched == "true", "{LABEL}: FAIL at match == true");
    let rules = parse_normalisation(&normalisation).map_err(|line| {
        fail(
            "parse_normalisation",
            format!("normalisation.txt line {line}"),
        )
    })?;
    let rust = member(json, "rust").map_err(|e| fail("the rust side", e))?;
    let (rust_transcript, rust_returned) =
        session_of(rust).map_err(|e| fail("the rust side", e))?;
    assert_same(
        &format!("{LABEL}: FAIL at the normalised transcript"),
        normalise(&rust_transcript, &rules),
        normalise(&captured_transcript, &rules),
    );
    // returned.txt: zero bytes is null, otherwise one trailing newline is
    // not part of the value.
    let expected_returned = if captured_returned.is_empty() {
        None
    } else {
        Some(
            captured_returned
                .strip_suffix('\n')
                .unwrap_or(&captured_returned)
                .to_string(),
        )
    };
    assert_same_optional(
        &format!("{LABEL}: FAIL at returned against returned.txt"),
        rust_returned.as_deref(),
        expected_returned.as_deref(),
    );
    println!("{LABEL}: PASS");
    Ok(())
}
