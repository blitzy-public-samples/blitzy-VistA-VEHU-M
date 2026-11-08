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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ifcap_po_inquiry::bridge::{
    MumpsMode, normalise, parse_normalisation, serve_until, split_input_lines,
};
use ifcap_po_inquiry::store::load_fixtures;
use ifcap_po_inquiry::{InquiryError, Store, inquire};

/// Read and write timeout of every client connection.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `Server::stop` waits for the server thread to finish.
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

/// One test's own capture directory under `std::env::temp_dir()`, named
/// `ifcap-po-inquiry-synthetic-<test>-<pid>-<nanos>-<serial>` and marked by
/// `SYNTHETIC.txt`. It is the `golden_dir` of the test's bridge, and its one
/// case is `GC-01/`. Dropping it removes it, best effort.
struct SyntheticDir {
    root: PathBuf,
}

impl SyntheticDir {
    fn new(test: &str) -> Result<SyntheticDir, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let serial = SYNTHETIC_SERIAL.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "ifcap-po-inquiry-synthetic-{test}-{}-{nanos}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&root).map_err(|e| format!("create {}: {e}", root.display()))?;
        let dir = SyntheticDir { root };
        dir.write("SYNTHETIC.txt", SYNTHETIC_MARKER.as_bytes())?;
        Ok(dir)
    }

    /// The `GC-01/` case directory, the first one the bridge scans.
    fn case_dir(&self) -> PathBuf {
        self.root.join("GC-01")
    }

    /// The path of `name` inside `GC-01/`.
    fn case_file(&self, name: &str) -> PathBuf {
        self.case_dir().join(name)
    }

    /// Writes `bytes` to `rel` under the directory, creating its parents.
    fn write(&self, rel: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(path)
    }

    /// Replay mode over this directory.
    fn replay(&self) -> MumpsMode {
        MumpsMode::Replay {
            golden_dir: self.root.clone(),
        }
    }

    /// Live mode over this directory, in a container that does not exist.
    fn live(&self) -> MumpsMode {
        MumpsMode::Live {
            container: "synthetic-no-such-container".to_string(),
            golden_dir: self.root.clone(),
        }
    }
}

impl Drop for SyntheticDir {
    fn drop(&mut self) {
        // Best effort: a directory left behind is still named synthetic and
        // marked by SYNTHETIC.txt.
        let _ = fs::remove_dir_all(&self.root);
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
    Store::from_zwr(SYNTHETIC_ZWR).map_err(|e| format!("SYNTHETIC_ZWR does not load: {e}"))
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

/// A synthetic session that `inquire` answers with `Ok`, and its answer.
struct OkCase {
    inputs: Vec<String>,
    transcript: String,
    returned: Option<String>,
}

/// The first candidate sequence that `inquire` answers with `Ok` on `store`
/// and whose captured form reads back through `split_input_lines`
/// unchanged. The candidates shrink because station acceptance or the
/// order prompt may be BLOCKED by `SPEC.md`; `Err` is the NOT RUN reason.
fn ok_case(store: &Store) -> Result<OkCase, String> {
    let candidates: [&[&str]; 3] = [&["", "^"], &[""], &[]];
    let mut refusals: Vec<String> = Vec::new();
    for candidate in candidates {
        let inputs = strings(candidate);
        if split_input_lines(&capture_form(&inputs)) != inputs {
            refusals.push(format!("{inputs:?} not round-tripping split_input_lines"));
            continue;
        }
        match inquire(store, &inputs) {
            Ok(output) => {
                return Ok(OkCase {
                    inputs,
                    transcript: output.transcript,
                    returned: output.returned,
                });
            }
            Err(InquiryError::UnsupportedBranch(id)) => {
                refusals.push(format!("UnsupportedBranch({id})"));
            }
            Err(InquiryError::CorruptStore(record)) => {
                refusals.push(format!("CorruptStore({record})"));
            }
        }
    }
    Err(format!(
        "NOT RUN: inquire returns {} for every candidate input",
        refusals.join(", ")
    ))
}

/// [`ok_case`], or `None` after printing the NOT RUN reason for `test`.
fn ok_case_or_not_run(test: &str, store: &Store) -> Option<OkCase> {
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
fn write_inputs(dir: &SyntheticDir, inputs: &[String]) -> Result<(), String> {
    let text = capture_form(inputs);
    let read_back = split_input_lines(&text);
    if read_back != inputs {
        return Err(format!(
            "synthetic input.txt {text:?} reads back as {read_back:?}, not {inputs:?}"
        ));
    }
    dir.write("GC-01/input.txt", text.as_bytes())?;
    Ok(())
}

/// Writes a whole synthetic capture into `GC-01/`. `returned.txt` is zero
/// bytes for `None`, and otherwise the value with one trailing newline,
/// which the bridge removes.
fn write_case(
    dir: &SyntheticDir,
    inputs: &[String],
    transcript: &str,
    returned: Option<&str>,
    normalisation: &str,
) -> Result<(), String> {
    write_inputs(dir, inputs)?;
    dir.write("GC-01/transcript.txt", transcript.as_bytes())?;
    let returned = returned
        .map(|value| format!("{value}\n"))
        .unwrap_or_default();
    dir.write("GC-01/returned.txt", returned.as_bytes())?;
    dir.write("GC-01/normalisation.txt", normalisation.as_bytes())?;
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

/// A bridge serving on its own `127.0.0.1:0` port in its own thread.
struct Server {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl Server {
    fn start(store: Store, mode: MumpsMode) -> Result<Server, String> {
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
        Ok(Server {
            addr,
            shutdown,
            handle: Some(handle),
        })
    }

    /// Sets the shutdown flag and returns what `serve_until` returned, or an
    /// error when its thread is still running after [`STOP_BOUND`].
    fn stop(self) -> io::Result<()> {
        self.stop_within(STOP_BOUND)
    }

    fn stop_within(mut self, bound: Duration) -> io::Result<()> {
        self.shutdown.store(true, Ordering::SeqCst);
        let Some(handle) = self.handle.take() else {
            return Err(io::Error::other("server already stopped"));
        };
        let started = Instant::now();
        while !handle.is_finished() {
            if started.elapsed() >= bound {
                return Err(io::Error::other(format!(
                    "server thread still running {} ms after the shutdown flag was set",
                    bound.as_millis()
                )));
            }
            thread::sleep(Duration::from_millis(5));
        }
        handle
            .join()
            .map_err(|_| io::Error::other("server thread panicked"))?
    }
}

impl Drop for Server {
    /// A backstop for a test that fails before `stop`: the accept loop ends
    /// within one poll interval.
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

/// Stops `server` and asserts that `serve_until` returned `Ok(())`.
fn assert_stopped(server: Server) {
    let stopped = server.stop();
    assert!(stopped.is_ok(), "server stop: {stopped:?}");
}

/// One HTTP response.
struct Resp {
    status: u16,
    body: Vec<u8>,
}

impl Resp {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> Result<Json, String> {
        let text = std::str::from_utf8(&self.body)
            .map_err(|_| format!("response body is not UTF-8: {:?}", self.text()))?;
        parse_json(text).map_err(|e| format!("response body is not JSON ({e}): {text}"))
    }
}

/// Sends one request over a fresh connection and reads its response. The
/// target is sent literally, so `/../Cargo.toml` reaches the server as
/// typed; `None` sends an empty body.
fn http(addr: SocketAddr, method: &str, target: &str, body: Option<&[u8]>) -> Result<Resp, String> {
    let fail = |what: &str, e: io::Error| format!("{method} {target}: {what}: {e}");
    let mut stream = TcpStream::connect(addr).map_err(|e| fail("connect", e))?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| fail("set read timeout", e))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| fail("set write timeout", e))?;
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
fn post_session(addr: SocketAddr, backend: &str, body: &[u8]) -> Result<Resp, String> {
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
fn read_response(stream: &mut TcpStream) -> Result<Resp, String> {
    let mut received: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    let head_end = loop {
        if let Some(start) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            break start.saturating_add(4);
        }
        let n = read_some(stream, &mut buf)?;
        if n == 0 {
            return Err(format!(
                "connection closed inside the response head: {:?}",
                String::from_utf8_lossy(&received)
            ));
        }
        received.extend_from_slice(buf.get(..n).unwrap_or_default());
    };
    let (head, rest) = received
        .split_at_checked(head_end)
        .ok_or_else(|| "response head offset out of range".to_string())?;
    let head = std::str::from_utf8(head).map_err(|_| "response head is not UTF-8".to_string())?;
    let mut lines = head.trim_end_matches("\r\n").split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let (Some(version), Some(code)) = (parts.next(), parts.next()) else {
        return Err(format!("malformed status line {status_line:?}"));
    };
    if !version.starts_with("HTTP/1.") {
        return Err(format!("malformed status line {status_line:?}"));
    }
    let status: u16 = code
        .parse()
        .map_err(|_| format!("malformed status code in {status_line:?}"))?;
    let mut content_length: Option<usize> = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("malformed header line {line:?}"));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let length = value
                .trim()
                .parse()
                .map_err(|_| format!("malformed Content-Length {value:?}"))?;
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
    Ok(Resp { status, body })
}

// ---------------------------------------------------------------------------
// Test-local JSON: an oracle independent of the bridge's private reader and
// writer
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

/// Parses `text` as exactly one JSON value. Strings may hold `\uXXXX`
/// escapes, surrogate pairs combined; an unpaired surrogate or an unescaped
/// control character is an error, so a response that leaves BEL or a tab
/// raw fails here.
fn parse_json(text: &str) -> Result<Json, String> {
    let mut parser = JsonParser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.whitespace();
    let value = parser.value()?;
    parser.whitespace();
    if parser.pos == parser.bytes.len() {
        Ok(value)
    } else {
        Err(parser.error("trailing text"))
    }
}

struct JsonParser<'t> {
    bytes: &'t [u8],
    pos: usize,
}

impl JsonParser<'_> {
    fn error(&self, what: &str) -> String {
        format!("{what} at byte {}", self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Result<u8, String> {
        let byte = self.peek().ok_or_else(|| self.error("unexpected end"))?;
        self.pos += 1;
        Ok(byte)
    }

    fn eat(&mut self, byte: u8) -> Result<(), String> {
        if self.next_byte()? == byte {
            Ok(())
        } else {
            Err(self.error(&format!("expected {:?}", char::from(byte))))
        }
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a value")),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        for byte in word.bytes() {
            self.eat(byte)?;
        }
        Ok(value)
    }

    /// Consumes one or more ASCII digits; false when there is none.
    fn digits(&mut self) -> bool {
        let first = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos > first
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if !self.digits() {
            return Err(self.error("expected digits"));
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !self.digits() {
                return Err(self.error("expected fraction digits"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.digits() {
                return Err(self.error("expected exponent digits"));
            }
        }
        let text = self
            .bytes
            .get(start..self.pos)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .ok_or_else(|| self.error("number out of range"))?;
        Ok(Json::Num(text.to_string()))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.whitespace();
            items.push(self.value()?);
            self.whitespace();
            match self.next_byte()? {
                b',' => {}
                b']' => return Ok(Json::Arr(items)),
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.eat(b'{')?;
        let mut members = Vec::new();
        self.whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(members));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.eat(b':')?;
            self.whitespace();
            members.push((key, self.value()?));
            self.whitespace();
            match self.next_byte()? {
                b',' => {}
                b'}' => return Ok(Json::Obj(members)),
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    /// Raw bytes are copied as they stand, and every delimiter is ASCII, so
    /// the UTF-8 of the input survives intact.
    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            match self.next_byte()? {
                b'"' => {
                    return String::from_utf8(out).map_err(|_| self.error("string is not UTF-8"));
                }
                b'\\' => {
                    let c = match self.next_byte()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return Err(self.error("unknown escape")),
                    };
                    let mut utf8 = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                }
                byte if byte < 0x20 => return Err(self.error("unescaped control character")),
                byte => out.push(byte),
            }
        }
    }

    /// The character of a `\uXXXX` escape whose `\u` is consumed, reading
    /// the low half of a surrogate pair too.
    fn unicode(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        let code = match first {
            0xD800..=0xDBFF => {
                self.eat(b'\\')?;
                self.eat(b'u')?;
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return Err(self.error("high surrogate without a low surrogate"));
                }
                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.error("unpaired low surrogate")),
            _ => first,
        };
        char::from_u32(code).ok_or_else(|| self.error("invalid code point"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let digit = char::from(self.next_byte()?)
                .to_digit(16)
                .ok_or_else(|| self.error("expected a hex digit"))?;
            code = code * 16 + digit;
        }
        Ok(code)
    }
}

/// `text` as a JSON string: `"` and `\` escaped, `\n`, `\r` and `\t` by
/// name, every other control character as `\u00XX`, and the rest as itself.
fn json_str(text: &str) -> String {
    encode_json_string(text, false)
}

/// [`json_str`] with every non-ASCII character also escaped, as `\uXXXX`
/// or, beyond the Basic Multilingual Plane, as a surrogate pair.
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

/// `json` written back compactly, for failure messages.
fn render(json: &Json) -> String {
    match json {
        Json::Null => "null".to_string(),
        Json::Bool(value) => value.to_string(),
        Json::Num(text) => text.clone(),
        Json::Str(text) => json_str(text),
        Json::Arr(items) => {
            let items: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", items.join(","))
        }
        Json::Obj(members) => {
            let members: Vec<String> = members
                .iter()
                .map(|(key, value)| format!("{}:{}", json_str(key), render(value)))
                .collect();
            format!("{{{}}}", members.join(","))
        }
    }
}

/// The member `name` of the object `json`.
fn field<'j>(json: &'j Json, name: &str) -> Result<&'j Json, String> {
    let Json::Obj(members) = json else {
        return Err(format!("{} is not an object", render(json)));
    };
    members
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| format!("{} has no member {name:?}", render(json)))
}

/// The string member `name` of `json`.
fn str_field<'j>(json: &'j Json, name: &str) -> Result<&'j str, String> {
    match field(json, name)? {
        Json::Str(text) => Ok(text),
        other => Err(format!(
            "member {name:?} is {}, not a string",
            render(other)
        )),
    }
}

/// The member `name` of `json` that is a string or `null`.
fn optional_str_field<'j>(json: &'j Json, name: &str) -> Result<Option<&'j str>, String> {
    match field(json, name)? {
        Json::Str(text) => Ok(Some(text)),
        Json::Null => Ok(None),
        other => Err(format!(
            "member {name:?} is {}, not a string or null",
            render(other)
        )),
    }
}

/// The `transcript` and `returned` of one session object.
fn session_of(json: &Json) -> Result<(String, Option<String>), String> {
    let transcript = str_field(json, "transcript")?.to_string();
    let returned = optional_str_field(json, "returned")?.map(str::to_string);
    Ok((transcript, returned))
}

/// Asserts that `resp` has `status` and a JSON body `{"error": string}`, and
/// returns the error text.
fn expect_error(resp: &Resp, status: u16, context: &str) -> Result<String, String> {
    assert_eq!(resp.status, status, "{context}: body {}", resp.text());
    let json = resp.json().map_err(|e| format!("{context}: {e}"))?;
    let error = str_field(&json, "error").map_err(|e| format!("{context}: {e}"))?;
    Ok(error.to_string())
}

/// Starts a bridge over `dir` in `mode`, POSTs `inputs` to the mumps
/// backend, stops the bridge, and returns the error text of the 502 answer.
fn mumps_502(dir: &SyntheticDir, mode: MumpsMode, inputs: &[String]) -> Result<String, String> {
    let server = Server::start(synthetic_store()?, mode)?;
    let resp = post_session(server.addr, "mumps", &body_for(inputs))?;
    assert_stopped(server);
    expect_error(&resp, 502, &format!("mumps over {}", dir.root.display()))
}

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// 200
// ---------------------------------------------------------------------------

#[test]
fn synthetic_static_paths_exact_match() -> Result<(), String> {
    let dir = SyntheticDir::new("static")?;
    let server = Server::start(synthetic_store()?, dir.replay())?;
    let frontend = crate_dir().join("frontend");
    let static_paths = [
        ("/", "index.html"),
        ("/index.html", "index.html"),
        ("/style.css", "style.css"),
        ("/app.js", "app.js"),
    ];
    for (target, file) in static_paths {
        let path = frontend.join(file);
        let expected = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let resp = http(server.addr, "GET", target, None)?;
        assert_eq!(resp.status, 200, "GET {target}: body {}", resp.text());
        assert!(
            resp.body == expected,
            "GET {target}: {} bytes served, frontend/{file} holds {} bytes or other bytes",
            resp.body.len(),
            expected.len()
        );
    }
    for target in ["/../Cargo.toml", "/frontend/app.js"] {
        let resp = http(server.addr, "GET", target, None)?;
        expect_error(&resp, 404, &format!("GET {target}"))?;
    }
    assert_stopped(server);
    Ok(())
}

#[test]
fn synthetic_rust_session_200() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some(case) = ok_case_or_not_run("synthetic_rust_session_200", &store) else {
        return Ok(());
    };
    let dir = SyntheticDir::new("rust-200")?;
    let server = Server::start(store, dir.replay())?;
    let resp = post_session(server.addr, "rust", &body_for(&case.inputs))?;
    assert_stopped(server);
    assert_eq!(
        resp.status,
        200,
        "rust {:?}: body {}",
        case.inputs,
        resp.text()
    );
    let (transcript, returned) = session_of(&resp.json()?)?;
    assert_eq!(
        transcript, case.transcript,
        "rust transcript of {:?}",
        case.inputs
    );
    assert_eq!(
        returned, case.returned,
        "rust returned of {:?}",
        case.inputs
    );
    Ok(())
}

#[test]
fn synthetic_mumps_replay_200() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some(case) = ok_case_or_not_run("synthetic_mumps_replay_200", &store) else {
        return Ok(());
    };
    let dir = SyntheticDir::new("mumps-200")?;
    // A zero-byte returned.txt, which replay must answer as null.
    write_case(&dir, &case.inputs, &case.transcript, None, "")?;
    let server = Server::start(store, dir.replay())?;
    let resp = post_session(server.addr, "mumps", &body_for(&case.inputs))?;
    assert_stopped(server);
    assert_eq!(
        resp.status,
        200,
        "mumps {:?}: body {}",
        case.inputs,
        resp.text()
    );
    let json = resp.json()?;
    let (transcript, _) = session_of(&json)?;
    assert_eq!(transcript, case.transcript, "replayed transcript.txt");
    assert_eq!(
        field(&json, "returned")?,
        &Json::Null,
        "a zero-byte returned.txt replays as null"
    );
    Ok(())
}

#[test]
fn synthetic_compare_match_true() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some(case) = ok_case_or_not_run("synthetic_compare_match_true", &store) else {
        return Ok(());
    };
    let dir = SyntheticDir::new("compare-true")?;
    write_case(
        &dir,
        &case.inputs,
        &case.transcript,
        case.returned.as_deref(),
        "",
    )?;
    let server = Server::start(store, dir.replay())?;
    let resp = post_session(server.addr, "compare", &body_for(&case.inputs))?;
    assert_stopped(server);
    assert_eq!(
        resp.status,
        200,
        "compare {:?}: body {}",
        case.inputs,
        resp.text()
    );
    let json = resp.json()?;
    let expected = (case.transcript.clone(), case.returned.clone());
    assert_eq!(session_of(field(&json, "mumps")?)?, expected, "mumps side");
    assert_eq!(session_of(field(&json, "rust")?)?, expected, "rust side");
    assert_eq!(field(&json, "match")?, &Json::Bool(true), "match");
    Ok(())
}

#[test]
fn synthetic_compare_match_false() -> Result<(), String> {
    let store = synthetic_store()?;
    let Some(case) = ok_case_or_not_run("synthetic_compare_match_false", &store) else {
        return Ok(());
    };
    let dir = SyntheticDir::new("compare-false")?;
    let differing = format!("{}SYNTHETIC DIFFERENCE\n", case.transcript);
    write_case(&dir, &case.inputs, &differing, case.returned.as_deref(), "")?;
    let server = Server::start(store, dir.replay())?;
    let resp = post_session(server.addr, "compare", &body_for(&case.inputs))?;
    assert_stopped(server);
    assert_eq!(
        resp.status,
        200,
        "compare {:?}: body {}",
        case.inputs,
        resp.text()
    );
    let json = resp.json()?;
    let (mumps_transcript, _) = session_of(field(&json, "mumps")?)?;
    let (rust_transcript, _) = session_of(field(&json, "rust")?)?;
    assert_eq!(mumps_transcript, differing, "mumps side is the capture");
    assert_eq!(rust_transcript, case.transcript, "rust side is inquire");
    assert_eq!(field(&json, "match")?, &Json::Bool(false), "match");
    Ok(())
}

// ---------------------------------------------------------------------------
// 400, on every backend
// ---------------------------------------------------------------------------

/// Asserts that each of `bodies` is answered 400 with `{"error": string}`
/// by the rust, mumps and compare backends alike.
fn assert_400_on_every_backend(test: &str, bodies: &[Vec<u8>]) -> Result<(), String> {
    let dir = SyntheticDir::new(test)?;
    let server = Server::start(synthetic_store()?, dir.replay())?;
    for backend in ["rust", "mumps", "compare"] {
        for body in bodies {
            let resp = post_session(server.addr, backend, body)?;
            let context = format!(
                "{test}: POST /api/{backend}/session {}",
                String::from_utf8_lossy(body)
            );
            let error = expect_error(&resp, 400, &context)?;
            assert!(!error.is_empty(), "{context}: empty error text");
        }
    }
    assert_stopped(server);
    Ok(())
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
    let dir = SyntheticDir::new(test)?;
    let server = Server::start(synthetic_store()?, dir.replay())?;
    let body = body_for(&strings(&SESSION_INPUTS));
    for (method, target) in requests {
        let sent = match *method {
            "POST" | "PUT" => Some(body.as_slice()),
            _ => None,
        };
        let resp = http(server.addr, method, target, sent)?;
        expect_error(&resp, 404, &format!("{test}: {method} {target}"))?;
    }
    assert_stopped(server);
    Ok(())
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
        other => {
            return Err(format!(
                "inquire {inputs:?} must be UnsupportedBranch, got {other:?}"
            ));
        }
    };
    // PO-09.b4 is help at the order prompt; PO-05.b1 is station acceptance,
    // should SPEC.md block it.
    assert!(
        id == "PO-09.b4" || id == "PO-05.b1",
        "inquire {inputs:?} gave UnsupportedBranch({id}), expected PO-09.b4 or PO-05.b1"
    );
    let dir = SyntheticDir::new("422")?;
    write_case(
        &dir,
        &inputs,
        "SYNTHETIC TRANSCRIPT: any text; the MUMPS side only has to succeed\n",
        None,
        "",
    )?;
    let server = Server::start(store, dir.replay())?;
    let body = body_for(&inputs);
    let rust = post_session(server.addr, "rust", &body)?;
    let mumps = post_session(server.addr, "mumps", &body)?;
    let compare = post_session(server.addr, "compare", &body)?;
    assert_stopped(server);
    let error = expect_error(&rust, 422, "rust")?;
    assert!(
        error.contains(&id),
        "rust 422 error {error:?} must name {id}"
    );
    assert_eq!(
        mumps.status,
        200,
        "the synthetic capture must replay, so that compare reaches the Rust side: {}",
        mumps.text()
    );
    let error = expect_error(&compare, 422, "compare")?;
    assert!(
        error.contains(&id),
        "compare 422 error {error:?} must name {id}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 502, on the mumps backend
// ---------------------------------------------------------------------------

#[test]
fn synthetic_502_not_recorded() -> Result<(), String> {
    let dir = SyntheticDir::new("502-not-recorded")?;
    // A complete capture of another session, which must not match.
    write_case(
        &dir,
        &strings(&["SYNTHETIC OTHER INPUT"]),
        "SYNTHETIC TRANSCRIPT\n",
        None,
        "",
    )?;
    let error = mumps_502(&dir, dir.replay(), &strings(&SESSION_INPUTS))?;
    assert_eq!(error, "NOT RECORDED");
    Ok(())
}

#[test]
fn synthetic_502_not_recorded_lists_unreadable_input() -> Result<(), String> {
    let dir = SyntheticDir::new("502-unreadable-input")?;
    let input = dir.write("GC-01/input.txt", &[0xFF, 0xFE, b'\n'])?;
    let error = mumps_502(&dir, dir.replay(), &strings(&SESSION_INPUTS))?;
    assert!(
        error.starts_with("NOT RECORDED"),
        "error {error:?} must be NOT RECORDED"
    );
    let path = input.display().to_string();
    assert!(error.contains(&path), "error {error:?} must list {path}");
    Ok(())
}

#[test]
fn synthetic_502_capture_file_missing() -> Result<(), String> {
    let dir = SyntheticDir::new("502-missing")?;
    let inputs = strings(&SESSION_INPUTS);
    write_inputs(&dir, &inputs)?;
    dir.write("GC-01/returned.txt", b"")?;
    dir.write("GC-01/normalisation.txt", b"")?;
    let error = mumps_502(&dir, dir.replay(), &inputs)?;
    let path = dir.case_file("transcript.txt");
    assert_eq!(error, format!("CAPTURE FILE {}: MISSING", path.display()));
    Ok(())
}

#[test]
fn synthetic_502_capture_file_unreadable() -> Result<(), String> {
    let dir = SyntheticDir::new("502-unreadable")?;
    let inputs = strings(&SESSION_INPUTS);
    write_case(&dir, &inputs, "", None, "")?;
    // A directory in place of transcript.txt is present but unreadable.
    let path = dir.case_file("transcript.txt");
    fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    fs::create_dir(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let error = mumps_502(&dir, dir.replay(), &inputs)?;
    assert_eq!(
        error,
        format!("CAPTURE FILE {}: UNREADABLE", path.display())
    );
    Ok(())
}

#[test]
fn synthetic_502_capture_file_non_utf8() -> Result<(), String> {
    let dir = SyntheticDir::new("502-non-utf8")?;
    let inputs = strings(&SESSION_INPUTS);
    write_case(&dir, &inputs, "", None, "")?;
    let path = dir.write("GC-01/transcript.txt", &[0xFF, 0xFE])?;
    let error = mumps_502(&dir, dir.replay(), &inputs)?;
    assert_eq!(error, format!("CAPTURE FILE {}: NON-UTF8", path.display()));
    Ok(())
}

#[test]
fn synthetic_502_malformed_normalisation() -> Result<(), String> {
    let dir = SyntheticDir::new("502-normalisation")?;
    let inputs = strings(&SESSION_INPUTS);
    write_case(
        &dir,
        &inputs,
        "SYNTHETIC TRANSCRIPT\n",
        None,
        "SYNTHETIC RULE WITHOUT A TAB\n",
    )?;
    let error = mumps_502(&dir, dir.replay(), &inputs)?;
    let path = dir.case_file("normalisation.txt");
    assert_eq!(
        error,
        format!("MALFORMED NORMALISATION {} line 1", path.display())
    );
    Ok(())
}

#[test]
fn synthetic_502_malformed_meta() -> Result<(), String> {
    let dir = SyntheticDir::new("502-meta")?;
    let meta = meta_text(SYNTHETIC_CONTEXT, "not-json");
    let path = dir.write("GC-01/meta.txt", meta.as_bytes())?;
    let error = mumps_502(&dir, dir.live(), &strings(&SESSION_INPUTS))?;
    assert_eq!(error, format!("MALFORMED META {}", path.display()));
    Ok(())
}

#[test]
fn synthetic_502_context_mismatch() -> Result<(), String> {
    let dir = SyntheticDir::new("502-context")?;
    let meta = meta_text(SYNTHETIC_CONTEXT, r#"["synthetic-never-run"]"#);
    dir.write("GC-01/meta.txt", meta.as_bytes())?;
    let error = mumps_502(&dir, dir.live(), &strings(&SESSION_INPUTS))?;
    // A context error, not a spawn error or an `EXIT` cause, proves the
    // bridge stopped before it ran `docker`.
    assert!(
        error.starts_with("CONTEXT MISMATCH IOF: expected UNRESOLVED, actual #"),
        "error {error:?} must be the IOF context mismatch"
    );
    assert!(
        !error.contains("EXIT") && !error.contains("os error"),
        "error {error:?} shows that docker ran"
    );
    Ok(())
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
    let Some(case) = ok_case_or_not_run("synthetic_json_escaping_round_trip", &store) else {
        return Ok(());
    };
    let dir = SyntheticDir::new("json")?;
    // The probe rides as one extra input line, which the replay must match.
    let mut inputs = case.inputs.clone();
    inputs.push(format!("SYNTHETIC PROBE {PROBE}"));
    // Synthetic: the inquire transcript, then the probe, a tab and BEL,
    // which the bridge must escape and this parser must decode.
    let transcript = format!("{}{PROBE}\t\u{7}", case.transcript);
    write_case(&dir, &inputs, &transcript, None, "")?;
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
        assert!(sent.contains(escape), "request {sent} lacks {escape}");
    }
    let server = Server::start(store, dir.replay())?;
    let resp = post_session(server.addr, "mumps", &body)?;
    assert_stopped(server);
    assert_eq!(resp.status, 200, "mumps: body {}", resp.text());
    let (decoded, _) = session_of(&resp.json()?)?;
    let file = fs::read_to_string(dir.case_file("transcript.txt"))
        .map_err(|e| format!("read the synthetic transcript.txt: {e}"))?;
    assert_eq!(decoded, file, "decoded transcript against transcript.txt");
    Ok(())
}

#[test]
fn synthetic_shutdown_joins_within_bound() -> Result<(), String> {
    let dir = SyntheticDir::new("shutdown")?;
    let server = Server::start(synthetic_store()?, dir.replay())?;
    // One answered request shows the accept loop was running.
    let resp = http(server.addr, "GET", "/", None)?;
    assert_eq!(resp.status, 200, "GET /: body {}", resp.text());
    let started = Instant::now();
    let stopped = server.stop_within(SHUTDOWN_BOUND);
    let elapsed = started.elapsed();
    assert!(
        stopped.is_ok(),
        "serve_until after the shutdown flag: {stopped:?}"
    );
    assert!(
        elapsed < SHUTDOWN_BOUND,
        "serve_until took {} ms to return",
        elapsed.as_millis()
    );
    Ok(())
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
    let golden = crate_dir().join("tests").join("golden").join("ifcap");
    let case = golden.join("GC-02");
    let fixtures = crate_dir().join("tests").join("fixtures").join("ifcap");
    let required = CAPTURE_FILES
        .iter()
        .map(|name| case.join(name))
        .chain(FIXTURE_FILES.iter().map(|name| fixtures.join(name)));
    let missing: Vec<String> = required
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    if !missing.is_empty() {
        println!("{LABEL}: NOT COMPARED, missing: {}", missing.join(", "));
        return Ok(());
    }
    let fail = |step: &str, detail: String| format!("{LABEL}: FAIL at {step}: {detail}");
    let read = |name: &str| {
        let path = case.join(name);
        fs::read_to_string(&path)
            .map_err(|e| fail("reading the capture", format!("{}: {e}", path.display())))
    };
    let store = load_fixtures(&fixtures).map_err(|e| fail("load_fixtures", e.to_string()))?;
    let inputs = split_input_lines(&read("input.txt")?);
    let captured_transcript = read("transcript.txt")?;
    let captured_returned = read("returned.txt")?;
    let normalisation = read("normalisation.txt")?;

    let server = Server::start(
        store,
        MumpsMode::Replay {
            golden_dir: golden.clone(),
        },
    )?;
    let resp = post_session(server.addr, "compare", &body_for(&inputs))?;
    assert_stopped(server);

    assert_eq!(
        resp.status,
        200,
        "{LABEL}: FAIL at status 200: body {}",
        resp.text()
    );
    let json = resp.json().map_err(|e| fail("the response body", e))?;
    let matched = field(&json, "match").map_err(|e| fail("match", e))?;
    assert_eq!(matched, &Json::Bool(true), "{LABEL}: FAIL at match == true");
    let rules = parse_normalisation(&normalisation).map_err(|line| {
        fail(
            "parse_normalisation",
            format!("normalisation.txt line {line}"),
        )
    })?;
    let rust = field(&json, "rust").map_err(|e| fail("the rust side", e))?;
    let (rust_transcript, rust_returned) =
        session_of(rust).map_err(|e| fail("the rust side", e))?;
    assert_eq!(
        normalise(&rust_transcript, &rules),
        normalise(&captured_transcript, &rules),
        "{LABEL}: FAIL at the normalised transcript"
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
    assert_eq!(
        rust_returned, expected_returned,
        "{LABEL}: FAIL at returned against returned.txt"
    );
    println!("{LABEL}: PASS");
    Ok(())
}
