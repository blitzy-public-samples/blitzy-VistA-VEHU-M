//! Integration tests of the loopback bridge's HTTP contract, driven through
//! its public entry point `serve` over real TCP connections.
//!
//! They prove that the bridge contract holds: status codes and error texts,
//! the session, compare and status JSON, the boolean `match` rule, exact
//! static routing, and the exit input the served front-end sends. They are
//! not parity evidence. Only `end_to_end_real_captures` compares the two
//! sides on real captures, each READY case reached through
//! `golden::load_case` on `golden::default_dir()`; every other case prints
//! `GC-0N NOT COMPARED` and passes.
//!
//! Each bridge binds `127.0.0.1:0` and runs `serve` in replay mode over one
//! golden folder, on a detached thread: `serve` loads this crate's fixtures
//! once and then serves until the test process ends, so no test stops it.
//! Every golden folder but the checkout's own is synthetic: written by its
//! test under `std::env::temp_dir()`, named
//! `ifcap-po-inquiry-synthetic-<test>-…`, marked by `SYNTHETIC.txt`, read
//! only by the bridge, and removed when the test ends, on a failure too. A
//! synthetic capture is never parity evidence.
//!
//! No test sets or reads an environment variable: configuration reaches the
//! bridge only as `serve` arguments. Every test has its own folder and its
//! own port, so the tests also run in parallel.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::DirBuilderExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ifcap_po_inquiry::bridge::{MumpsMode, serve};
use ifcap_po_inquiry::golden::{CASE_IDS, CaseLoad, EXIT_INPUT, Readiness, default_dir, load_case};

/// Read and write timeout of every client connection.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a failed request waits for the bridge thread to end, so that
/// the failure can carry what `serve` returned.
const SERVER_END_WAIT: Duration = Duration::from_millis(500);

/// The detached thread running one bridge's `serve`.
type Server = JoinHandle<io::Result<()>>;

// ---------------------------------------------------------------------------
// Synthetic golden folders
// ---------------------------------------------------------------------------

/// Text of `SYNTHETIC.txt` in every synthetic golden folder.
const SYNTHETIC_MARKER: &str = "\
SYNTHETIC GOLDEN FOLDER
Created by tests/bridge.rs of the ifcap-po-inquiry crate for one bridge
test, and removed when that test ends. It holds no capture of the original
system and is never parity evidence.
";

/// Distinguishes folders created in the same nanosecond.
static SYNTHETIC_SERIAL: AtomicUsize = AtomicUsize::new(0);

/// Runs `body` with one test's own golden folder under
/// `std::env::temp_dir()`, named
/// `ifcap-po-inquiry-synthetic-<test>-<pid>-<nanos>-<serial>` and marked by
/// `SYNTHETIC.txt`.
///
/// The folder is acquired exclusively before `body` runs. Each attempt takes
/// a fresh `<nanos>` and `<serial>` and creates that one name,
/// non-recursively and with mode `0o700`, so only the owner can enter it.
/// Creation fails on any existing entry, a symbolic link included, without
/// following it, and such an entry is never reused or removed: the next name
/// is tried instead, up to `ATTEMPTS` names.
///
/// Removal of only the path this call created is attempted on every exit of
/// `body`: `Ok`, `Err` or a panic, which is resumed after that attempt. An
/// `Err` from `body` takes precedence over a removal failure, which is
/// returned only when `body` succeeded.
fn with_synthetic_dir<T>(
    test: &str,
    body: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    const ATTEMPTS: usize = 64;
    let base = std::env::temp_dir();
    let mut acquired = None;
    for _ in 0..ATTEMPTS {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let serial = SYNTHETIC_SERIAL.fetch_add(1, Ordering::SeqCst);
        let candidate = base.join(format!(
            "ifcap-po-inquiry-synthetic-{test}-{}-{nanos}-{serial}",
            std::process::id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => {
                acquired = Some(candidate);
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("{test}: creating the synthetic folder: {e}")),
        }
    }
    let Some(root) = acquired else {
        return Err(format!(
            "{test}: no unused synthetic folder name after {ATTEMPTS} attempts"
        ));
    };
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        write_file(&root, "SYNTHETIC.txt", SYNTHETIC_MARKER.as_bytes())?;
        body(&root)
    }));
    let removed = match fs::remove_dir_all(&root) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(format!("{test}: removing the synthetic folder: {e}"))
        }
        _ => Ok(()),
    };
    match outcome {
        Ok(result) => result.and_then(|value| removed.map(|()| value)),
        Err(payload) => panic::resume_unwind(payload),
    }
}

/// Writes `bytes` to `rel` under `root`, creating its parents. Errors name
/// `rel`, never the temporary path. Creating the parents with
/// `create_dir_all` is safe because they lie inside the owner-only root that
/// [`with_synthetic_dir`] acquired exclusively.
fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("creating the parent of synthetic {rel}: {e}"))?;
    }
    fs::write(&path, bytes).map_err(|e| format!("writing synthetic {rel}: {e}"))
}

/// The Rust side's whole transcript for the exit input alone, and for no
/// input: the station prompt, `SPEC.md` "Stubs". `^` at the station prompt
/// is the unimplemented branch PO-05.b2, and an empty sequence runs out of
/// input there, so either session ends after that prompt, whether or not
/// `cases.zwr` exists.
const STATION_PROMPT_TRANSCRIPT: &str = "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// ";

/// The station prompt with a station number the Rust side never writes: the
/// transcript of a READY capture that differs from the Rust side.
const DIFFERING_TRANSCRIPT: &str = "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 501// ";

/// A READY capture transcript that the Rust side never writes and that the
/// case's own rule would rewrite: [`DIFFERING_TRANSCRIPT`], a BEL, and one
/// span from the rule's open delimiter through its close delimiter. A replay
/// answer holding it byte for byte is the capture's raw transcript, neither
/// the Rust side's nor rewritten by a rule.
const RAW_TRANSCRIPT: &str = "\n\n\nSelect STATION NUMBER ('^' TO EXIT): 501// \u{7}<<SYNTHETIC-OPEN 12:00 SYNTHETIC-CLOSE>>";

/// A capture context that `golden::validate_context` accepts: the two
/// resolved stub constants, and values for `IOF` and `IOSL`, which stay
/// unresolved while open item O-01 stands and so accept any value.
const SYNTHETIC_CONTEXT: &str = r##"{"DUZ":".5","IOF":"#","IOSL":"24","STATION":"500"}"##;

/// [`SYNTHETIC_CONTEXT`] with `DUZ` `1`, which `golden::validate_context`
/// rejects.
const DUZ_1_CONTEXT: &str = r##"{"DUZ":"1","IOF":"#","IOSL":"24","STATION":"500"}"##;

/// One normalisation rule whose delimiters no Rust transcript holds.
const SYNTHETIC_RULES: &str = "<<SYNTHETIC-OPEN\tSYNTHETIC-CLOSE>>\n";

/// The text of both synthetic exports, `db-before.zwr` and `db-after.zwr`.
const SYNTHETIC_EXPORT: &str = "SYNTHETIC export written by tests/bridge.rs\nZWR\n\
     ^PRC(442,0)=\"PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^^0\"\n";

/// A synthetic `meta.txt` with all eight keys, one `key=value` line each.
fn synthetic_meta(context: &str) -> String {
    format!(
        "image_digest=synthetic\nsource_revision=synthetic\nsnapshot_id=synthetic\n\
         record_ids=442:none\nentry_point=EN3^PRCHRPT\ncontext={context}\n\
         setup=SYNTHETIC folder written by tests/bridge.rs; not a capture\n\
         docker_exec_args=synthetic-arg-1 synthetic-arg-2\n"
    )
}

/// The input sequence of every synthetic capture: the exit input alone.
fn exit_only() -> Vec<String> {
    vec![EXIT_INPUT.to_string()]
}

/// Writes the synthetic case `root/<case_id>/` by the capture convention:
/// `input.txt` holding the one line [`EXIT_INPUT`], `transcript` as
/// `transcript.txt` with no newline added, a zero-byte `returned.txt`,
/// [`synthetic_meta`] of `context`, [`SYNTHETIC_RULES`], and
/// [`SYNTHETIC_EXPORT`] as `db-before.zwr` and, when `with_db_after`, as
/// `db-after.zwr`. No file holds a carriage return, so with `with_db_after`
/// and an accepted context the case is READY; without `db-after.zwr` it is
/// INCOMPLETE on `seven_files_present` and `db_before_equals_after`.
fn write_synthetic_case(
    root: &Path,
    case_id: &str,
    transcript: &str,
    context: &str,
    with_db_after: bool,
) -> Result<(), String> {
    let file = |name: &str, bytes: &[u8]| write_file(root, &format!("{case_id}/{name}"), bytes);
    file("input.txt", format!("{EXIT_INPUT}\n").as_bytes())?;
    file("transcript.txt", transcript.as_bytes())?;
    file("returned.txt", b"")?;
    file("meta.txt", synthetic_meta(context).as_bytes())?;
    file("normalisation.txt", SYNTHETIC_RULES.as_bytes())?;
    file("db-before.zwr", SYNTHETIC_EXPORT.as_bytes())?;
    if with_db_after {
        file("db-after.zwr", SYNTHETIC_EXPORT.as_bytes())?;
    }
    Ok(())
}

/// The synthetic READY `GC-01`: [`STATION_PROMPT_TRANSCRIPT`] under
/// [`SYNTHETIC_CONTEXT`], with both exports.
fn ready_gc01(root: &Path) -> Result<(), String> {
    write_synthetic_case(
        root,
        "GC-01",
        STATION_PROMPT_TRANSCRIPT,
        SYNTHETIC_CONTEXT,
        true,
    )
}

/// Leaves the golden folder empty but for its marker: no case folder.
fn empty_folder(_root: &Path) -> Result<(), String> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------------------

/// Binds `127.0.0.1:0` and starts `serve` on it in replay mode over
/// `golden_dir`, on its own named thread, which is never joined unless a
/// request fails (see [`explain`]). The listener is bound before the thread
/// starts, so a request sent while `serve` still loads the fixtures waits in
/// the listener's backlog.
fn start(golden_dir: &Path) -> Result<(SocketAddr, Server), String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind 127.0.0.1:0: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;
    let mode = MumpsMode::Replay {
        golden_dir: golden_dir.to_path_buf(),
    };
    let server = thread::Builder::new()
        .name(format!("bridge-{}", addr.port()))
        .spawn(move || serve(listener, mode))
        .map_err(|e| format!("spawn the bridge thread: {e}"))?;
    Ok((addr, server))
}

/// `failure`, with what `serve` returned when its thread ends within
/// [`SERVER_END_WAIT`]: a bridge that stopped, because its fixtures did not
/// load for example, explains the failed request.
fn explain(failure: String, server: Server) -> String {
    let started = Instant::now();
    while !server.is_finished() {
        if started.elapsed() >= SERVER_END_WAIT {
            return format!("{failure} (serve is still running)");
        }
        thread::sleep(Duration::from_millis(10));
    }
    match server.join() {
        Ok(Ok(())) => format!("{failure} (serve returned Ok)"),
        Ok(Err(e)) => format!("{failure} (serve failed: {e})"),
        Err(_) => format!("{failure} (the bridge thread panicked)"),
    }
}

/// Runs `client` against a bridge of its own over a synthetic golden folder
/// of its own, which `setup` fills first (see [`with_synthetic_dir`]). A
/// failure names `test` and, through [`explain`], what `serve` returned.
fn with_bridge<T>(
    test: &str,
    setup: impl FnOnce(&Path) -> Result<(), String>,
    client: impl FnOnce(SocketAddr) -> Result<T, String>,
) -> Result<T, String> {
    with_synthetic_dir(test, |root| {
        setup(root).map_err(|e| format!("{test}: {e}"))?;
        let (addr, server) = start(root).map_err(|e| format!("{test}: {e}"))?;
        client(addr).map_err(|failure| explain(format!("{test}: {failure}"), server))
    })
}

// ---------------------------------------------------------------------------
// HTTP/1.1 client
// ---------------------------------------------------------------------------

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

/// Sends one HTTP/1.1 request with `Content-Length` and `Connection: close`
/// over a fresh connection, and reads its response as its status and body.
/// The target is sent literally, so `/../Cargo.toml` reaches the bridge as
/// typed; `None` sends an empty body.
fn http(
    addr: SocketAddr,
    method: &str,
    target: &str,
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    let mut stream = connect(addr).map_err(|e| format!("{method} {target}: {e}"))?;
    let body = body.unwrap_or_default();
    let mut request = format!(
        "{method} {target} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    stream
        .write_all(&request)
        .map_err(|e| format!("{method} {target}: send: {e}"))?;
    read_response(&mut stream).map_err(|e| format!("{method} {target}: {e}"))
}

/// `POST /api/<backend>/session` with `body`.
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
/// bytes, or everything up to the end of the stream when that header is
/// absent. Errors name the part of the response at fault, never its
/// content.
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
// writer (RFC 8259)
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
/// error, so a response that leaves a BEL raw fails here.
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
        Some(b'[') => json_array(bytes, pos, &mut |_, _| {}),
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

/// Scans a number; the integer part is `0` or a nonzero digit then digits.
fn json_number(bytes: &[u8], mut pos: usize) -> Result<usize, String> {
    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
    }
    pos = match bytes.get(pos) {
        Some(b'0') => pos + 1,
        Some(b'1'..=b'9') => digits(bytes, pos + 1).unwrap_or(pos + 1),
        _ => return Err(json_error("expected digits", pos)),
    };
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

/// Scans the array that starts at `pos`, calling `visit` with the start and
/// end of each element, in order.
fn json_array(
    bytes: &[u8],
    pos: usize,
    visit: &mut dyn FnMut(usize, usize),
) -> Result<usize, String> {
    let mut pos = skip_whitespace(bytes, eat(bytes, pos, b'[')?);
    if bytes.get(pos) == Some(&b']') {
        return Ok(pos + 1);
    }
    loop {
        let start = skip_whitespace(bytes, pos);
        let end = json_value(bytes, start)?;
        visit(start, end);
        pos = skip_whitespace(bytes, end);
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
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The session request `{"inputs":[...]}` for `inputs`, each element encoded
/// by [`json_str`].
fn body_for(inputs: &[String]) -> Vec<u8> {
    let elements: Vec<String> = inputs.iter().map(|input| json_str(input)).collect();
    format!("{{\"inputs\":[{}]}}", elements.join(",")).into_bytes()
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

/// The JSON value text `value`, which must be exactly one string, decoded.
fn string_of(value: &str) -> Result<String, String> {
    if !value.starts_with('"') {
        return Err("not a string".to_string());
    }
    let (text, end) = json_string(value.as_bytes(), 0)?;
    if end != value.len() {
        return Err(json_error("text after the string", end));
    }
    Ok(text)
}

/// The JSON value text `value`, which must be exactly one array of strings,
/// decoded in order.
fn strings_of(value: &str) -> Result<Vec<String>, String> {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'[') {
        return Err("not an array".to_string());
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    json_array(bytes, 0, &mut |start, end| spans.push((start, end)))?;
    spans
        .into_iter()
        .map(|(start, end)| {
            value
                .get(start..end)
                .ok_or_else(|| json_error("element out of range", start))
                .and_then(string_of)
                .map_err(|e| format!("element at byte {start}: {e}"))
        })
        .collect()
}

/// The member `name` of `json` that is a string or `null`, decoded.
fn optional_str_member(json: &str, name: &str) -> Result<Option<String>, String> {
    let value = member(json, name)?;
    if value == "null" {
        return Ok(None);
    }
    string_of(value)
        .map(Some)
        .map_err(|e| format!("member {name:?}: {e}"))
}

fn str_member(json: &str, name: &str) -> Result<String, String> {
    optional_str_member(json, name)?.ok_or_else(|| format!("member {name:?} is null, not a string"))
}

/// The member `name` of `json` that is `true` or `false`.
fn bool_member(json: &str, name: &str) -> Result<bool, String> {
    match member(json, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("member {name:?} is not a boolean")),
    }
}

/// The member `name` of `json` that is an array of strings, decoded.
fn string_array(json: &str, name: &str) -> Result<Vec<String>, String> {
    strings_of(member(json, name)?).map_err(|e| format!("member {name:?}: {e}"))
}

/// The member `name` of `json` that is an object whose every value is an
/// array of strings, as its keys and decoded arrays in document order.
fn object_of_string_arrays(json: &str, name: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let value = member(json, name)?;
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err(format!("member {name:?} is not an object"));
    }
    let mut spans: Vec<(String, usize, usize)> = Vec::new();
    json_object(bytes, 0, &mut |key, start, end| {
        spans.push((key.to_string(), start, end));
    })?;
    spans
        .into_iter()
        .map(|(key, start, end)| {
            let items = value
                .get(start..end)
                .ok_or_else(|| json_error("value out of range", start))
                .and_then(strings_of)
                .map_err(|e| format!("member {name:?}.{key:?}: {e}"))?;
            Ok((key, items))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Response checks
// ---------------------------------------------------------------------------

/// One session object of a response: `{"transcript": string, "returned":
/// string or null}`.
struct Session {
    transcript: String,
    returned: Option<String>,
}

fn session_of(json: &str) -> Result<Session, String> {
    Ok(Session {
        transcript: str_member(json, "transcript")?,
        returned: optional_str_member(json, "returned")?,
    })
}

/// A 200 compare answer: `{"mumps": session, "rust": session, "match":
/// boolean}`.
struct Compared {
    mumps: Session,
    rust: Session,
    matched: bool,
}

/// A 200 `GET /api/status` answer, every member compared by value.
struct StatusReport {
    mumps_mode: String,
    recorded: Vec<String>,
    ready: Vec<String>,
    incomplete: Vec<(String, Vec<String>)>,
}

/// The JSON body of `resp`, once its status is `status`. Any other status
/// fails, naming `context` and the answer's `error` text, which the bridge
/// builds from fixed causes and never from request data.
fn expect_json<'r>(
    resp: &'r (u16, Vec<u8>),
    status: u16,
    context: &str,
) -> Result<&'r str, String> {
    let (actual, body) = resp;
    let json = json_body(body).map_err(|e| format!("{context}: {e}"))?;
    if *actual != status {
        let cause = str_member(json, "error").unwrap_or_else(|e| format!("({e})"));
        return Err(format!(
            "{context}: status {actual} where {status} is expected: {cause}"
        ));
    }
    Ok(json)
}

/// The `error` text of `resp`, a `{"error": string}` answer with `status`.
fn expect_error(resp: &(u16, Vec<u8>), status: u16, context: &str) -> Result<String, String> {
    let json = expect_json(resp, status, context)?;
    str_member(json, "error").map_err(|e| format!("{context}: {e}"))
}

/// The session of `resp`, a 200 session answer.
fn expect_session(resp: &(u16, Vec<u8>), context: &str) -> Result<Session, String> {
    session_of(expect_json(resp, 200, context)?).map_err(|e| format!("{context}: {e}"))
}

/// The members of `resp`, a 200 compare answer.
fn expect_compare(resp: &(u16, Vec<u8>), context: &str) -> Result<Compared, String> {
    let json = expect_json(resp, 200, context)?;
    let side = |name: &str| {
        member(json, name)
            .and_then(session_of)
            .map_err(|e| format!("{context}: {name}: {e}"))
    };
    Ok(Compared {
        mumps: side("mumps")?,
        rust: side("rust")?,
        matched: bool_member(json, "match").map_err(|e| format!("{context}: {e}"))?,
    })
}

/// `GET /api/status` of the bridge at `addr`.
fn get_status(addr: SocketAddr) -> Result<StatusReport, String> {
    let resp = http(addr, "GET", "/api/status", None)?;
    let json = expect_json(&resp, 200, "GET /api/status")?;
    let named = |e: String| format!("GET /api/status: {e}");
    Ok(StatusReport {
        mumps_mode: str_member(json, "mumps_mode").map_err(named)?,
        recorded: string_array(json, "recorded").map_err(named)?,
        ready: string_array(json, "ready").map_err(named)?,
        incomplete: object_of_string_arrays(json, "incomplete").map_err(named)?,
    })
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

/// Whether `haystack` holds `needle` as a contiguous run of bytes.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Over a synthetic folder holding only the `GC-01` that
/// [`write_synthetic_case`] writes from these arguments: the ids
/// `GET /api/status` reports READY, then the compare answer to the exit
/// input alone, which equals that case's inputs.
fn compare_over_synthetic_gc01(
    test: &str,
    transcript: &str,
    context: &str,
    with_db_after: bool,
) -> Result<(Vec<String>, Compared), String> {
    with_bridge(
        test,
        |root| write_synthetic_case(root, "GC-01", transcript, context, with_db_after),
        |addr| {
            let ready = get_status(addr)?.ready;
            let resp = post_session(addr, "compare", &body_for(&exit_only()))?;
            Ok((ready, expect_compare(&resp, "compare [^]")?))
        },
    )
}

// ---------------------------------------------------------------------------
// Session backends
// ---------------------------------------------------------------------------

/// The Rust side answers 200 from the fixtures alone, ignoring the captures:
/// over a READY `GC-01` whose transcript differs from it, the exit input and
/// the empty sequence each end after the station prompt, returning nothing.
#[test]
fn rust_session_200() -> Result<(), String> {
    with_bridge(
        "rust_session_200",
        |root| write_synthetic_case(root, "GC-01", DIFFERING_TRANSCRIPT, SYNTHETIC_CONTEXT, true),
        |addr| {
            for inputs in [exit_only(), Vec::new()] {
                let context = format!("rust {inputs:?}");
                let resp = post_session(addr, "rust", &body_for(&inputs))?;
                let session = expect_session(&resp, &context)?;
                assert_same(
                    &format!("{context}: transcript"),
                    &session.transcript,
                    STATION_PROMPT_TRANSCRIPT,
                );
                assert_eq!(session.returned, None, "{context}: returned");
            }
            Ok(())
        },
    )
}

/// Replay answers 200 with the recorded capture whose inputs equal the
/// request's: its `transcript.txt` byte for byte, never rewritten by its
/// rule, and its zero-byte `returned.txt` as `null`.
#[test]
fn mumps_replay_200_when_recorded() -> Result<(), String> {
    with_bridge(
        "mumps_replay_200_when_recorded",
        |root| write_synthetic_case(root, "GC-01", RAW_TRANSCRIPT, SYNTHETIC_CONTEXT, true),
        |addr| {
            let resp = post_session(addr, "mumps", &body_for(&exit_only()))?;
            let session = expect_session(&resp, "mumps [^]")?;
            assert_same(
                "mumps [^]: transcript against the written transcript.txt",
                &session.transcript,
                RAW_TRANSCRIPT,
            );
            assert_eq!(session.returned, None, "mumps [^]: returned");
            Ok(())
        },
    )
}

/// Replay answers 502 `NOT RECORDED` when no capture's inputs equal the
/// request's.
#[test]
fn mumps_replay_502_not_recorded() -> Result<(), String> {
    with_bridge("mumps_replay_502_not_recorded", ready_gc01, |addr| {
        let inputs = vec![String::new(), EXIT_INPUT.to_string()];
        let resp = post_session(addr, "mumps", &body_for(&inputs))?;
        let error = expect_error(&resp, 502, "mumps [\"\", \"^\"]")?;
        assert_eq!(error, "NOT RECORDED", "mumps [\"\", \"^\"]: error");
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// COMPARE and the match rule
// ---------------------------------------------------------------------------

/// `match` is true for a READY case whose context is accepted and whose
/// transcript and return value equal the Rust side's; both raw outputs are
/// answered.
#[test]
fn compare_match_true_for_ready_equal_case() -> Result<(), String> {
    let (ready, compared) = compare_over_synthetic_gc01(
        "compare_match_true_for_ready_equal_case",
        STATION_PROMPT_TRANSCRIPT,
        SYNTHETIC_CONTEXT,
        true,
    )?;
    assert_eq!(ready, ["GC-01"], "status: ready");
    assert_same(
        "mumps transcript",
        &compared.mumps.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert_eq!(compared.mumps.returned, None, "mumps returned");
    assert_same(
        "rust transcript",
        &compared.rust.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert_eq!(compared.rust.returned, None, "rust returned");
    assert!(compared.matched, "match is false for a READY equal case");
    Ok(())
}

/// `match` is false for a READY case whose transcript differs from the Rust
/// side's, and the MUMPS side is still answered raw.
#[test]
fn compare_match_false_for_differing_ready_case() -> Result<(), String> {
    let (ready, compared) = compare_over_synthetic_gc01(
        "compare_match_false_for_differing_ready_case",
        DIFFERING_TRANSCRIPT,
        SYNTHETIC_CONTEXT,
        true,
    )?;
    assert_eq!(ready, ["GC-01"], "status: ready");
    assert_same(
        "mumps transcript",
        &compared.mumps.transcript,
        DIFFERING_TRANSCRIPT,
    );
    assert_same(
        "rust transcript",
        &compared.rust.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert!(!compared.matched, "match is true for a differing case");
    Ok(())
}

/// `match` is false for an INCOMPLETE case, here one without
/// `db-after.zwr`, even though both transcripts are equal.
#[test]
fn compare_match_false_for_incomplete_case() -> Result<(), String> {
    let (ready, compared) = compare_over_synthetic_gc01(
        "compare_match_false_for_incomplete_case",
        STATION_PROMPT_TRANSCRIPT,
        SYNTHETIC_CONTEXT,
        false,
    )?;
    assert!(ready.is_empty(), "status: ready {ready:?}");
    assert_same(
        "mumps transcript",
        &compared.mumps.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert_same(
        "rust transcript",
        &compared.rust.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert!(!compared.matched, "match is true for an INCOMPLETE case");
    Ok(())
}

/// `match` is false for a READY case whose `meta.txt` context has `DUZ` `1`,
/// which `golden::validate_context` rejects, even though both transcripts
/// are equal.
#[test]
fn compare_match_false_for_context_duz_1() -> Result<(), String> {
    let (ready, compared) = compare_over_synthetic_gc01(
        "compare_match_false_for_context_duz_1",
        STATION_PROMPT_TRANSCRIPT,
        DUZ_1_CONTEXT,
        true,
    )?;
    assert_eq!(ready, ["GC-01"], "status: ready");
    assert_same(
        "mumps transcript",
        &compared.mumps.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert_same(
        "rust transcript",
        &compared.rust.transcript,
        STATION_PROMPT_TRANSCRIPT,
    );
    assert!(!compared.matched, "match is true for a DUZ 1 context");
    Ok(())
}

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

/// Every backend answers 400 with an error text for malformed JSON, a
/// missing or non-array `inputs`, a non-string element, and an element
/// holding a carriage return or a line feed. The cause texts are the
/// bridge's own and are not pinned here.
#[test]
fn bad_request_400_on_every_backend() -> Result<(), String> {
    const BODIES: [(&str, &str); 7] = [
        ("malformed JSON", r#"{"inputs": ["#),
        ("missing inputs", "{}"),
        ("non-array inputs", r#"{"inputs":"x"}"#),
        ("number element", r#"{"inputs":[1]}"#),
        ("null element", r#"{"inputs":[null]}"#),
        ("carriage return element", r#"{"inputs":["a\rb"]}"#),
        ("line feed element", r#"{"inputs":["a\nb"]}"#),
    ];
    with_bridge("bad_request_400_on_every_backend", ready_gc01, |addr| {
        for backend in ["rust", "mumps", "compare"] {
            for (what, body) in BODIES {
                let context = format!("{backend}: {what}");
                let resp = post_session(addr, backend, body.as_bytes())?;
                let error = expect_error(&resp, 400, &context)?;
                assert!(!error.is_empty(), "{context}: empty error text");
            }
        }
        Ok(())
    })
}

/// 404 with the bridge's exact texts for an unknown backend, unknown paths
/// (paths outside the four static files included) and wrong methods.
#[test]
fn not_found_404_texts() -> Result<(), String> {
    const UNKNOWN_BACKEND: &str = "UNKNOWN BACKEND";
    const NOT_FOUND: &str = "NOT FOUND";
    const POST_REQUIRED: &str = "NOT FOUND: POST REQUIRED";
    const GET_REQUIRED: &str = "NOT FOUND: GET REQUIRED";
    // Method, target, whether the request carries a valid session body, and
    // the expected error text.
    const ROWS: [(&str, &str, bool, &str); 11] = [
        ("POST", "/api/java/session", true, UNKNOWN_BACKEND),
        ("GET", "/nope", false, NOT_FOUND),
        ("POST", "/api/rust/other", true, NOT_FOUND),
        ("GET", "/../Cargo.toml", false, NOT_FOUND),
        ("GET", "/frontend/app.js", false, NOT_FOUND),
        ("GET", "/api/rust/session", false, POST_REQUIRED),
        ("GET", "/api/mumps/session", false, POST_REQUIRED),
        ("GET", "/api/compare/session", false, POST_REQUIRED),
        ("POST", "/api/status", true, GET_REQUIRED),
        ("POST", "/index.html", true, GET_REQUIRED),
        ("POST", "/app.js", true, GET_REQUIRED),
    ];
    let session_body = body_for(&exit_only());
    with_bridge("not_found_404_texts", ready_gc01, |addr| {
        for (method, target, with_body, expected) in ROWS {
            let context = format!("{method} {target}");
            let body = with_body.then_some(session_body.as_slice());
            let resp = http(addr, method, target, body)?;
            let error = expect_error(&resp, 404, &context)?;
            assert_eq!(error, expected, "{context}: error");
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// `GET /api/status` over a folder holding the synthetic READY `GC-01`.
#[test]
fn status_ready_case() -> Result<(), String> {
    let report = with_bridge("status_ready_case", ready_gc01, get_status)?;
    assert_eq!(report.mumps_mode, "replay", "mumps_mode");
    assert_eq!(report.recorded, ["GC-01"], "recorded");
    assert_eq!(report.ready, ["GC-01"], "ready");
    assert!(
        report.incomplete.is_empty(),
        "incomplete: {:?}",
        report.incomplete
    );
    Ok(())
}

/// `GET /api/status` over a folder whose `GC-01` lacks `db-after.zwr`: it is
/// recorded, not READY, and INCOMPLETE on exactly the two checks the missing
/// file fails, in check order.
#[test]
fn status_incomplete_case() -> Result<(), String> {
    let report = with_bridge(
        "status_incomplete_case",
        |root| {
            write_synthetic_case(
                root,
                "GC-01",
                STATION_PROMPT_TRANSCRIPT,
                SYNTHETIC_CONTEXT,
                false,
            )
        },
        get_status,
    )?;
    assert_eq!(report.mumps_mode, "replay", "mumps_mode");
    assert_eq!(report.recorded, ["GC-01"], "recorded");
    assert!(report.ready.is_empty(), "ready: {:?}", report.ready);
    let expected = vec![(
        "GC-01".to_string(),
        vec![
            "seven_files_present".to_string(),
            "db_before_equals_after".to_string(),
        ],
    )];
    assert_eq!(report.incomplete, expected, "incomplete");
    Ok(())
}

/// `GET /api/status` over a folder with no case folder: three empty lists.
#[test]
fn status_empty_folder() -> Result<(), String> {
    let report = with_bridge("status_empty_folder", empty_folder, get_status)?;
    assert_eq!(report.mumps_mode, "replay", "mumps_mode");
    assert!(
        report.recorded.is_empty(),
        "recorded: {:?}",
        report.recorded
    );
    assert!(report.ready.is_empty(), "ready: {:?}", report.ready);
    assert!(
        report.incomplete.is_empty(),
        "incomplete: {:?}",
        report.incomplete
    );
    Ok(())
}

/// A bridge started over a folder with no capture serves: the Rust side
/// answers 200, while the MUMPS side answers 502 `NOT RECORDED` and COMPARE
/// 502 `MUMPS: NOT RECORDED`.
#[test]
fn startup_with_empty_folder() -> Result<(), String> {
    with_bridge("startup_with_empty_folder", empty_folder, |addr| {
        let body = body_for(&exit_only());
        let rust = expect_session(&post_session(addr, "rust", &body)?, "rust [^]")?;
        assert_same(
            "rust [^]: transcript",
            &rust.transcript,
            STATION_PROMPT_TRANSCRIPT,
        );
        assert_eq!(rust.returned, None, "rust [^]: returned");
        let mumps = expect_error(&post_session(addr, "mumps", &body)?, 502, "mumps [^]")?;
        assert_eq!(mumps, "NOT RECORDED", "mumps [^]: error");
        let compare = expect_error(&post_session(addr, "compare", &body)?, 502, "compare [^]")?;
        assert_eq!(compare, "MUMPS: NOT RECORDED", "compare [^]: error");
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Static paths
// ---------------------------------------------------------------------------

/// `GET` on each of the four static paths answers 200 with the file under
/// `frontend/` byte for byte, and the served `app.js` holds the front-end's
/// copy of the exit input, equal to `golden::EXIT_INPUT`.
#[test]
fn static_paths_exact_match() -> Result<(), String> {
    const ROWS: [(&str, &str); 4] = [
        ("/", "index.html"),
        ("/index.html", "index.html"),
        ("/style.css", "style.css"),
        ("/app.js", "app.js"),
    ];
    let frontend = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend");
    let exit_line = format!("const EXIT_INPUT = \"{EXIT_INPUT}\";");
    with_bridge("static_paths_exact_match", empty_folder, |addr| {
        for (target, file) in ROWS {
            let expected = fs::read(frontend.join(file))
                .map_err(|e| format!("reading frontend/{file}: {e}"))?;
            let (status, body) = http(addr, "GET", target, None)?;
            assert_eq!(status, 200, "GET {target}: status");
            assert_same(
                &format!("GET {target} against frontend/{file}"),
                &body,
                &expected,
            );
            if file == "app.js" {
                assert!(
                    contains(&body, exit_line.as_bytes()),
                    "GET {target}: the served app.js lacks `{exit_line}`"
                );
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// End to end on real captures
// ---------------------------------------------------------------------------

/// For each case `golden::load_case` reports READY in the checkout's golden
/// folder, COMPARE with that case's inputs answers 200 with `match` true,
/// through one bridge started on first need. Every other case prints
/// `GC-0N NOT COMPARED` and passes. Nothing here reads a capture file
/// itself or writes anything.
#[test]
fn end_to_end_real_captures() -> Result<(), String> {
    let golden_dir = default_dir();
    let mut bridge: Option<(SocketAddr, Server)> = None;
    for id in CASE_IDS {
        let case = match load_case(&golden_dir, id) {
            CaseLoad::Recorded(case) if matches!(case.readiness, Readiness::Ready) => case,
            CaseLoad::Absent
            | CaseLoad::NotRecorded(_)
            | CaseLoad::Recorded(_)
            | CaseLoad::Unreadable(_) => {
                println!("{id} NOT COMPARED");
                continue;
            }
        };
        let addr = match &bridge {
            Some((addr, _)) => *addr,
            None => {
                let (addr, server) = start(&golden_dir).map_err(|e| format!("{id}: {e}"))?;
                bridge = Some((addr, server));
                addr
            }
        };
        let answered = post_session(addr, "compare", &body_for(&case.inputs))
            .and_then(|resp| expect_compare(&resp, id));
        let compared = match answered {
            Ok(compared) => compared,
            Err(failure) => {
                let failure = format!("{id}: {failure}");
                return Err(match bridge.take() {
                    Some((_, server)) => explain(failure, server),
                    None => failure,
                });
            }
        };
        assert!(compared.matched, "{id}: COMPARE answered match false");
        println!("{id} MATCH");
    }
    Ok(())
}
