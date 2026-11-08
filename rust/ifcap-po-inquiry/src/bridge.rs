//! The loopback bridge: one HTTP/1.1 server that lets a person run one input
//! sequence against the original MUMPS system and against the Rust port, from
//! the one-screen front-end. Rust-only infrastructure with no MUMPS
//! counterpart.
//!
//! - `POST /api/{mumps|rust|compare}/session` with `{"inputs": [string, ...]}`
//!   runs the whole sequence statelessly. The MUMPS side is a replay of the
//!   recorded captures or a live `docker exec`, chosen by [`MumpsMode`]; the
//!   Rust side is [`inquire`] on the fixture [`Store`].
//! - `GET /`, `/index.html`, `/style.css` and `/app.js` serve `frontend/`,
//!   matched exactly and read at request time.
//!
//! There is no authentication, TLS or session state: the trust boundary is
//! the loopback address the caller binds. A request supplies only `inputs`
//! and, through its path, the backend; the live container comes only from
//! [`MumpsMode::Live`]. Nothing here writes to either backend's data.
//!
//! This module also owns the capture-format helpers that the adapters and
//! both integration test files must apply identically: [`split_input_lines`],
//! [`parse_normalisation`], [`normalise`], [`parse_meta`] and
//! [`validate_context`]. Only the MUMPS adapters, compare routing and these
//! helpers touch capture content.

use std::collections::HashSet;
use std::fmt::{self, Write as _};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::inquiry::{InquiryError, InquiryOutput, STUB_CONTEXT, inquire};
use crate::store::{Store, load_fixtures};

/// How often the accept loop polls its shutdown flag and `run_command`
/// polls its child.
const POLL: Duration = Duration::from_millis(10);
/// Read and write timeout of one client connection.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest request line plus headers accepted.
const MAX_HEAD: usize = 16 * 1024;
/// The longest request body accepted.
const MAX_BODY: usize = 1024 * 1024;
/// How long, and how much, unread client data is discarded after a response,
/// so that closing a socket with unread input does not reset the connection
/// and destroy the response before the client reads it.
const DRAIN_TIME: Duration = Duration::from_secs(1);
const DRAIN_BYTES: usize = 2 * MAX_BODY;
/// Deepest JSON nesting the reader accepts. Recursive descent on a 1 MiB
/// body of `[` would otherwise overflow the connection thread's stack.
const MAX_JSON_DEPTH: usize = 64;
/// Live-mode limits on the `docker exec` child.
const LIVE_TIMEOUT: Duration = Duration::from_secs(10);
const LIVE_OUTPUT_LIMIT: usize = 1024 * 1024;
/// How long `run_command` waits for its pipe threads once the child is gone.
const PIPE_GRACE: Duration = Duration::from_secs(1);

const JSON_TYPE: &str = "application/json; charset=utf-8";

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// Where the MUMPS side of a session comes from.
///
/// `Live` carries `golden_dir` as well, because live mode reads the capture
/// command and context from the captures' `meta.txt`, and the directory can
/// reach the bridge only as an argument: no test sets an environment
/// variable.
#[derive(Clone, Debug)]
pub enum MumpsMode {
    /// Answer from the recorded captures `GC-01` to `GC-05` under
    /// `golden_dir`.
    Replay { golden_dir: PathBuf },
    /// Run the original with `docker exec -i <container> ...`, using the
    /// capture command recorded in `golden_dir`.
    Live {
        container: String,
        golden_dir: PathBuf,
    },
}

/// Serves until the process ends. Loads the fixture store once from
/// `tests/fixtures/ifcap/` of this crate; a fixture that cannot be loaded is
/// an `InvalidData` error naming it.
pub fn serve(listener: TcpListener, mumps: MumpsMode) -> io::Result<()> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ifcap");
    let store = load_fixtures(&fixtures)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    serve_until(listener, store, mumps, Arc::new(AtomicBool::new(false)))
}

/// Serves `listener` until `shutdown` is set, then returns `Ok(())` within
/// about one poll interval. Public only as the test hook behind [`serve`].
///
/// Each connection is handled on its own thread, so an idle browser
/// connection never blocks another. Connection threads still running when
/// this returns finish their one request and end on their own.
pub fn serve_until(
    listener: TcpListener,
    store: Store,
    mumps: MumpsMode,
    shutdown: Arc<AtomicBool>,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let store = Arc::new(store);
    let mumps = Arc::new(mumps);
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let store = Arc::clone(&store);
                let mumps = Arc::clone(&mumps);
                // A thread that cannot be created drops the connection; the
                // server keeps running.
                let _ = thread::Builder::new()
                    .name("bridge-connection".to_string())
                    .spawn(move || handle_connection(stream, &store, &mumps));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            // WouldBlock means no pending connection; any other accept error
            // (a reset peer, a full descriptor table) is retried the same way.
            Err(_) => thread::sleep(POLL),
        }
    }
}

/// Reads one request from `stream`, answers it and closes the connection.
fn handle_connection(mut stream: TcpStream, store: &Store, mumps: &MumpsMode) {
    let configured = stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_read_timeout(Some(IO_TIMEOUT)))
        .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)));
    if configured.is_err() {
        return;
    }
    let response = match read_request(&mut stream) {
        Ok(Some(request)) => route(&request, store, mumps),
        // The peer closed before sending anything: there is no one to answer.
        Ok(None) => return,
        Err(cause) => error_response(400, &cause),
    };
    if write_response(&mut stream, &response).is_ok() {
        drain(&mut stream);
    }
}

/// Ends the write side, then discards what the client still sends, bounded
/// in time and bytes, so the close is orderly.
fn drain(stream: &mut TcpStream) {
    if stream.shutdown(Shutdown::Write).is_err()
        || stream.set_read_timeout(Some(DRAIN_TIME)).is_err()
    {
        return;
    }
    let started = Instant::now();
    let mut discarded = 0usize;
    let mut buf = [0u8; 8192];
    while started.elapsed() < DRAIN_TIME && discarded < DRAIN_BYTES {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => discarded = discarded.saturating_add(n),
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP/1.1
// ---------------------------------------------------------------------------

/// The 400 cause for a request whose head cannot be read or parsed.
const BAD_REQUEST: &str = "BAD REQUEST";

/// One request, as far as routing needs it: the method, the target, and the
/// body read by `Content-Length`, or `None` when the request carried no
/// `Content-Length`.
type HttpRequest = (String, String, Option<Vec<u8>>);

/// Reads the request line and headers up to the blank line, then the body by
/// `Content-Length`. `Ok(None)` means the peer closed before sending a byte;
/// `Err` carries the 400 cause. Chunked bodies are not supported.
fn read_request(stream: &mut TcpStream) -> Result<Option<HttpRequest>, String> {
    let mut received: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = find_head_end(&received) {
            break end;
        }
        if received.len() > MAX_HEAD {
            return Err(BAD_REQUEST.to_string());
        }
        match stream.read(&mut buf) {
            Ok(0) if received.is_empty() => return Ok(None),
            Ok(0) => return Err(BAD_REQUEST.to_string()),
            Ok(n) => received.extend_from_slice(buf.get(..n).unwrap_or(&[])),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            // A read timeout before the head is complete is answered as a
            // bad request, best effort.
            Err(_) => return Err(BAD_REQUEST.to_string()),
        }
    };
    if head_end > MAX_HEAD {
        return Err(BAD_REQUEST.to_string());
    }
    let (head, rest) = received
        .split_at_checked(head_end)
        .ok_or_else(|| BAD_REQUEST.to_string())?;
    let (method, target, content_length, expect_continue) = parse_head(head)?;
    let body = match content_length {
        None => None,
        Some(length) if length > MAX_BODY => {
            return Err(format!("{BAD_REQUEST}: BODY TOO LARGE"));
        }
        Some(length) => {
            let mut body = rest.to_vec();
            if expect_continue && body.len() < length {
                stream
                    .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
                    .map_err(|_| BAD_REQUEST.to_string())?;
            }
            while body.len() < length {
                match stream.read(&mut buf) {
                    Ok(0) => return Err(format!("{BAD_REQUEST}: INCOMPLETE BODY")),
                    Ok(n) => body.extend_from_slice(buf.get(..n).unwrap_or(&[])),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => return Err(format!("{BAD_REQUEST}: INCOMPLETE BODY")),
                }
            }
            // Bytes past the declared length (a pipelined request) are not
            // part of this request and are not answered.
            body.truncate(length);
            Some(body)
        }
    };
    Ok(Some((method.to_string(), target.to_string(), body)))
}

/// The offset just past the first `\r\n\r\n`, if the head is complete.
fn find_head_end(received: &[u8]) -> Option<usize> {
    received
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| start.saturating_add(4))
}

/// Parses `head`, which ends with the blank line, into the request line and
/// the headers routing needs: the method, the target, the `Content-Length`
/// and whether `Expect: 100-continue` was sent. Errors are 400 causes.
fn parse_head(head: &[u8]) -> Result<(&str, &str, Option<usize>, bool), String> {
    let bad = || BAD_REQUEST.to_string();
    let text = std::str::from_utf8(head).map_err(|_| bad())?;
    let text = text.strip_suffix("\r\n\r\n").ok_or_else(bad)?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or_else(bad)?;
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    if !is_token(method) || !target.starts_with('/') || !matches!(version, "HTTP/1.1" | "HTTP/1.0")
    {
        return Err(bad());
    }
    let mut content_length: Option<usize> = None;
    let mut expect_continue = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(bad)?;
        // A name must be a token; this also rejects obsolete line folding.
        if !is_token(name) {
            return Err(bad());
        }
        let value = value.trim_matches(|c| c == ' ' || c == '\t');
        if name.eq_ignore_ascii_case("content-length") {
            let length = parse_content_length(value)
                .ok_or_else(|| format!("{BAD_REQUEST}: INVALID CONTENT-LENGTH"))?;
            if content_length.is_some_and(|earlier| earlier != length) {
                return Err(format!("{BAD_REQUEST}: INVALID CONTENT-LENGTH"));
            }
            content_length = Some(length);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(format!("{BAD_REQUEST}: TRANSFER-ENCODING NOT SUPPORTED"));
        } else if name.eq_ignore_ascii_case("expect") && value.eq_ignore_ascii_case("100-continue")
        {
            expect_continue = true;
        }
    }
    Ok((method, target, content_length, expect_continue))
}

/// An HTTP token: one or more `tchar`s (RFC 9110 5.6.2).
fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A `Content-Length` value: one or more ASCII digits that fit `usize`.
fn parse_content_length(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// `{"error": cause}` with `status`, as a response: `(status, content type,
/// body)`.
fn error_response(status: u16, cause: &str) -> (u16, &'static str, Vec<u8>) {
    (status, JSON_TYPE, error_body(cause).into_bytes())
}

/// Writes `response`, `(status, content type, body)`; every response closes
/// its connection.
fn write_response(
    stream: &mut impl Write,
    response: &(u16, &'static str, Vec<u8>),
) -> io::Result<()> {
    let (status, content_type, body) = response;
    let reason = reason_phrase(*status);
    let length = body.len();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        422 => "Unprocessable Content",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "Status",
    }
}

/// `{"error": cause}`.
fn error_body(cause: &str) -> String {
    let mut out = String::from("{\"error\":");
    write_json_string(&mut out, cause);
    out.push('}');
    out
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

/// The 404 causes of routing. Each is fixed text: no cause repeats the
/// request's method, target, query or backend name.
const NOT_FOUND: &str = "NOT FOUND";
const UNKNOWN_BACKEND: &str = "UNKNOWN BACKEND";
const POST_REQUIRED: &str = "NOT FOUND: POST REQUIRED";
const GET_REQUIRED: &str = "NOT FOUND: GET REQUIRED";

/// Answers `request` with `(status, content type, body)`: a static path, an
/// API path, or 404.
fn route(request: &HttpRequest, store: &Store, mumps: &MumpsMode) -> (u16, &'static str, Vec<u8>) {
    let (method, target, body) = request;
    if let Some((file, content_type)) = static_file(target) {
        if method == "GET" {
            return serve_static(file, content_type);
        }
        return error_response(404, GET_REQUIRED);
    }
    if let Some(rest) = target.strip_prefix("/api/") {
        let (status, json) = route_api(method, rest, body.as_deref(), store, mumps);
        return (status, JSON_TYPE, json.into_bytes());
    }
    error_response(404, NOT_FOUND)
}

/// The four static paths, matched exactly on the full request target, so a
/// query string, `/../Cargo.toml` or `/frontend/app.js` matches none.
fn static_file(target: &str) -> Option<(&'static str, &'static str)> {
    match target {
        "/" | "/index.html" => Some(("index.html", "text/html; charset=utf-8")),
        "/style.css" => Some(("style.css", "text/css; charset=utf-8")),
        "/app.js" => Some(("app.js", "text/javascript; charset=utf-8")),
        _ => None,
    }
}

/// Reads `frontend/<file>` at request time; a file that cannot be read is
/// a 404.
fn serve_static(file: &str, content_type: &'static str) -> (u16, &'static str, Vec<u8>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("frontend")
        .join(file);
    match fs::read(&path) {
        Ok(body) => (200, content_type, body),
        Err(_) => error_response(404, &format!("NOT FOUND /{file}")),
    }
}

/// `rest` is the target after `/api/`. Unknown backends and paths, and any
/// method but POST, are 404; the body is checked only for a known route.
/// The answer is `(status, JSON body)`.
fn route_api(
    method: &str,
    rest: &str,
    body: Option<&[u8]>,
    store: &Store,
    mumps: &MumpsMode,
) -> (u16, String) {
    let not_found = || (404, error_body(NOT_FOUND));
    let Some((name, tail)) = rest.split_once('/') else {
        return not_found();
    };
    if tail != "session" {
        return not_found();
    }
    let session: fn(&Store, &MumpsMode, &[String]) -> (u16, String) = match name {
        "mumps" => |_, mode, inputs| mumps_session(mode, inputs),
        "rust" => |fixtures, _, inputs| rust_session(fixtures, inputs),
        "compare" => compare_session,
        _ => return (404, error_body(UNKNOWN_BACKEND)),
    };
    if method != "POST" {
        return (404, error_body(POST_REQUIRED));
    }
    let Some(body) = body else {
        return (
            400,
            error_body(&format!("{BAD_REQUEST}: CONTENT-LENGTH REQUIRED")),
        );
    };
    match parse_session_request(body) {
        Ok(inputs) => session(store, mumps, &inputs),
        Err(cause) => (400, error_body(&cause)),
    }
}

/// Validates `{"inputs": [string, ...]}` and returns the inputs unmodified
/// and in order. An element holding a carriage return or line feed is
/// rejected, because each input becomes one line of a captured input file
/// and of live standard input. Other members of the object are ignored.
fn parse_session_request(body: &[u8]) -> Result<Vec<String>, String> {
    let text = std::str::from_utf8(body).map_err(|_| "MALFORMED JSON".to_string())?;
    let Some(json) = parse_json(text) else {
        return Err("MALFORMED JSON".to_string());
    };
    let Some(members) = json_members(json) else {
        return Err("BODY IS NOT A JSON OBJECT".to_string());
    };
    let mut found = members.into_iter().filter(|(key, _)| key == "inputs");
    let (Some((_, inputs)), None) = (found.next(), found.next()) else {
        return Err("INPUTS MISSING OR REPEATED".to_string());
    };
    let Some(elements) = json_elements(inputs) else {
        return Err("INPUTS IS NOT AN ARRAY".to_string());
    };
    elements
        .into_iter()
        .enumerate()
        .map(|(position, element)| match json_string(element) {
            Some(text) if text.contains(['\r', '\n']) => Err(format!(
                "INPUT {} CONTAINS A CARRIAGE RETURN OR LINE FEED",
                position.saturating_add(1)
            )),
            Some(text) => Ok(text),
            None => Err(format!(
                "INPUT {} IS NOT A STRING",
                position.saturating_add(1)
            )),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// JSON reader and writer (private, minimal)
// ---------------------------------------------------------------------------

/// Parses `text` as exactly one JSON value with optional surrounding
/// whitespace, anything after it being malformed, and returns the value's
/// own text without that whitespace.
///
/// A value is kept as its text, not as a tree: [`json_members`],
/// [`json_elements`] and [`json_string`] read from it the kind a caller
/// expects. Numbers keep their text, since nothing here computes with them.
/// Throughout this reader, `None` means malformed.
fn parse_json(text: &str) -> Option<&str> {
    let mut pos = 0;
    skip_whitespace(text, &mut pos);
    let value = value_text(text, &mut pos, 0)?;
    skip_whitespace(text, &mut pos);
    if pos == text.len() { Some(value) } else { None }
}

/// The members of `value`, the text of exactly one JSON object: keys and
/// value texts in document order, duplicates included, so each caller
/// decides what a repeated key means.
fn json_members(value: &str) -> Option<Vec<(String, &str)>> {
    let mut pos = 0;
    let members = read_object(value, &mut pos, 0)?;
    if pos == value.len() {
        Some(members)
    } else {
        None
    }
}

/// The element texts of `value`, the text of exactly one JSON array, in
/// order.
fn json_elements(value: &str) -> Option<Vec<&str>> {
    let mut pos = 0;
    let elements = read_array(value, &mut pos, 0)?;
    if pos == value.len() {
        Some(elements)
    } else {
        None
    }
}

/// The unescaped content of `value`, the text of exactly one JSON string.
fn json_string(value: &str) -> Option<String> {
    let mut pos = 0;
    let text = read_string(value, &mut pos)?;
    if pos == value.len() { Some(text) } else { None }
}

/// The byte of `text` at `pos`, if any.
fn peek(text: &str, pos: usize) -> Option<u8> {
    text.as_bytes().get(pos).copied()
}

/// The byte of `text` at `*pos`, moving past it.
fn bump(text: &str, pos: &mut usize) -> Option<u8> {
    let byte = peek(text, *pos)?;
    *pos = pos.saturating_add(1);
    Some(byte)
}

/// Moves past `byte`, which must come next.
fn eat(text: &str, pos: &mut usize, byte: u8) -> Option<()> {
    if bump(text, pos) == Some(byte) {
        Some(())
    } else {
        None
    }
}

/// Moves past `literal`, which must come next.
fn eat_literal(text: &str, pos: &mut usize, literal: &str) -> Option<()> {
    literal.bytes().try_for_each(|byte| eat(text, pos, byte))
}

/// Moves past any JSON whitespace.
fn skip_whitespace(text: &str, pos: &mut usize) {
    while matches!(peek(text, *pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *pos = pos.saturating_add(1);
    }
}

/// The text of the value at `*pos`, moving past it as [`skip_value`] does.
fn value_text<'t>(text: &'t str, pos: &mut usize, depth: usize) -> Option<&'t str> {
    let start = *pos;
    skip_value(text, pos, depth)?;
    text.get(start..*pos)
}

/// Checks the value at `*pos`, nested `depth` levels deep, and moves past
/// it; a value nested deeper than `MAX_JSON_DEPTH` is malformed. This is
/// recursive descent over the bytes of `text`, and a position is always a
/// char boundary between tokens, because every token delimiter is ASCII.
fn skip_value(text: &str, pos: &mut usize, depth: usize) -> Option<()> {
    if depth > MAX_JSON_DEPTH {
        return None;
    }
    match peek(text, *pos) {
        Some(b'{') => read_object(text, pos, depth).map(|_| ()),
        Some(b'[') => read_array(text, pos, depth).map(|_| ()),
        Some(b'"') => read_string(text, pos).map(|_| ()),
        Some(b't') => eat_literal(text, pos, "true"),
        Some(b'f') => eat_literal(text, pos, "false"),
        Some(b'n') => eat_literal(text, pos, "null"),
        Some(b'-' | b'0'..=b'9') => skip_number(text, pos),
        _ => None,
    }
}

/// The object at `*pos`, nested `depth` levels deep, moving past it: its
/// keys and value texts in document order, duplicates included.
fn read_object<'t>(text: &'t str, pos: &mut usize, depth: usize) -> Option<Vec<(String, &'t str)>> {
    eat(text, pos, b'{')?;
    let mut members = Vec::new();
    skip_whitespace(text, pos);
    if peek(text, *pos) == Some(b'}') {
        *pos = pos.saturating_add(1);
        return Some(members);
    }
    loop {
        skip_whitespace(text, pos);
        let key = read_string(text, pos)?;
        skip_whitespace(text, pos);
        eat(text, pos, b':')?;
        skip_whitespace(text, pos);
        let value = value_text(text, pos, depth.saturating_add(1))?;
        members.push((key, value));
        skip_whitespace(text, pos);
        match bump(text, pos) {
            Some(b',') => {}
            Some(b'}') => return Some(members),
            _ => return None,
        }
    }
}

/// The array at `*pos`, nested `depth` levels deep, moving past it: its
/// element texts in order.
fn read_array<'t>(text: &'t str, pos: &mut usize, depth: usize) -> Option<Vec<&'t str>> {
    eat(text, pos, b'[')?;
    let mut elements = Vec::new();
    skip_whitespace(text, pos);
    if peek(text, *pos) == Some(b']') {
        *pos = pos.saturating_add(1);
        return Some(elements);
    }
    loop {
        skip_whitespace(text, pos);
        elements.push(value_text(text, pos, depth.saturating_add(1))?);
        skip_whitespace(text, pos);
        match bump(text, pos) {
            Some(b',') => {}
            Some(b']') => return Some(elements),
            _ => return None,
        }
    }
}

/// The string at `*pos`, unescaped, moving past it. The escapes are `\"`,
/// `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\u` followed by four
/// hexadecimal digits. A high-surrogate `\u` escape must be followed by a
/// low-surrogate `\u` escape; an unpaired surrogate, an unknown escape or
/// an unescaped control character is malformed.
fn read_string(text: &str, pos: &mut usize) -> Option<String> {
    eat(text, pos, b'"')?;
    let mut out = String::new();
    let mut run_start = *pos;
    loop {
        match peek(text, *pos) {
            None => return None,
            Some(b'"') => {
                out.push_str(text.get(run_start..*pos)?);
                *pos = pos.saturating_add(1);
                return Some(out);
            }
            Some(b'\\') => {
                out.push_str(text.get(run_start..*pos)?);
                *pos = pos.saturating_add(1);
                let unescaped = match bump(text, pos) {
                    Some(b'"') => '"',
                    Some(b'\\') => '\\',
                    Some(b'/') => '/',
                    Some(b'b') => '\u{8}',
                    Some(b'f') => '\u{c}',
                    Some(b'n') => '\n',
                    Some(b'r') => '\r',
                    Some(b't') => '\t',
                    Some(b'u') => read_unicode_escape(text, pos)?,
                    _ => return None,
                };
                out.push(unescaped);
                run_start = *pos;
            }
            Some(byte) if byte < 0x20 => return None,
            Some(_) => *pos = pos.saturating_add(1),
        }
    }
}

/// The character of a `\u` escape whose `\u` is already consumed: four
/// hexadecimal digits, a surrogate pair being combined into one character.
fn read_unicode_escape(text: &str, pos: &mut usize) -> Option<char> {
    let first = read_hex4(text, pos)?;
    match first {
        0xD800..=0xDBFF => {
            eat(text, pos, b'\\')?;
            eat(text, pos, b'u')?;
            let second = read_hex4(text, pos)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return None;
            }
            let code = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            char::from_u32(code)
        }
        0xDC00..=0xDFFF => None,
        _ => char::from_u32(first),
    }
}

/// Four hexadecimal digits at `*pos` as one code unit, moving past them.
fn read_hex4(text: &str, pos: &mut usize) -> Option<u32> {
    let mut code = 0u32;
    for _ in 0..4 {
        let digit = bump(text, pos).and_then(|byte| char::from(byte).to_digit(16))?;
        code = code * 16 + digit;
    }
    Some(code)
}

/// Moves past the number at `*pos`:
/// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`
fn skip_number(text: &str, pos: &mut usize) -> Option<()> {
    if peek(text, *pos) == Some(b'-') {
        *pos = pos.saturating_add(1);
    }
    match peek(text, *pos) {
        Some(b'0') => *pos = pos.saturating_add(1),
        Some(b'1'..=b'9') => skip_digits(text, pos),
        _ => return None,
    }
    if peek(text, *pos) == Some(b'.') {
        *pos = pos.saturating_add(1);
        skip_required_digits(text, pos)?;
    }
    if matches!(peek(text, *pos), Some(b'e' | b'E')) {
        *pos = pos.saturating_add(1);
        if matches!(peek(text, *pos), Some(b'+' | b'-')) {
            *pos = pos.saturating_add(1);
        }
        skip_required_digits(text, pos)?;
    }
    Some(())
}

/// Moves past any ASCII digits.
fn skip_digits(text: &str, pos: &mut usize) {
    while matches!(peek(text, *pos), Some(b'0'..=b'9')) {
        *pos = pos.saturating_add(1);
    }
}

/// Moves past one or more ASCII digits, which must come next.
fn skip_required_digits(text: &str, pos: &mut usize) -> Option<()> {
    let start = *pos;
    skip_digits(text, pos);
    if *pos > start { Some(()) } else { None }
}

/// Appends `text` as a JSON string: `"` and `\` escaped, `\n \r \t \b \f`
/// by name, every other character below U+0020 as `\u00XX` (BEL is
/// `\u0007`), and everything else as itself.
fn write_json_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Appends `value` as a JSON string, or `null` for `None`.
fn write_json_optional(out: &mut String, value: Option<&str>) {
    match value {
        Some(text) => write_json_string(out, text),
        None => out.push_str("null"),
    }
}

// ---------------------------------------------------------------------------
// Session backends
// ---------------------------------------------------------------------------

/// The capture directories, scanned in this order.
const CASE_DIRS: [&str; 5] = ["GC-01", "GC-02", "GC-03", "GC-04", "GC-05"];
/// The capture files the bridge reads. Replay and compare read the first
/// four; live mode reads only `META_FILE`.
const INPUT_FILE: &str = "input.txt";
const TRANSCRIPT_FILE: &str = "transcript.txt";
const RETURNED_FILE: &str = "returned.txt";
const NORMALISATION_FILE: &str = "normalisation.txt";
const META_FILE: &str = "meta.txt";

/// One MUMPS-side session result: the transcript; what the original's entry
/// point returned, as far as the MUMPS side knows; and the normalisation
/// rules of the capture whose inputs equal the session's (none in live
/// mode). The return value is `None` when live mode cannot obtain it (open
/// item O-06), and otherwise `Some` of the recorded value, zero bytes being
/// `Some(None)`.
type MumpsSide = (String, Option<Option<String>>, Vec<(String, String)>);

/// `{"transcript": ..., "returned": ...}`.
fn write_session(out: &mut String, transcript: &str, returned: Option<&str>) {
    out.push_str("{\"transcript\":");
    write_json_string(out, transcript);
    out.push_str(",\"returned\":");
    write_json_optional(out, returned);
    out.push('}');
}

fn session_body(transcript: &str, returned: Option<&str>) -> String {
    let mut out = String::new();
    write_session(&mut out, transcript, returned);
    out
}

/// `(status, JSON body)` of a Rust-side session.
fn rust_session(store: &Store, inputs: &[String]) -> (u16, String) {
    match inquire(store, inputs) {
        Ok(output) => (
            200,
            session_body(&output.transcript, output.returned.as_deref()),
        ),
        Err(error) => inquiry_error_to_response(&error),
    }
}

/// `(status, JSON body)` of a MUMPS-side session; its failure is 502.
fn mumps_session(mumps: &MumpsMode, inputs: &[String]) -> (u16, String) {
    match run_mumps(mumps, inputs) {
        Ok((transcript, returned, _)) => (
            200,
            session_body(&transcript, returned.flatten().as_deref()),
        ),
        Err(cause) => (502, error_body(&cause)),
    }
}

/// `(status, JSON body)` of a compare session: the MUMPS side first, whose
/// failure is 502; then the Rust side, whose `CorruptStore` is 500 and
/// `UnsupportedBranch` 422; only then 200.
fn compare_session(store: &Store, mumps: &MumpsMode, inputs: &[String]) -> (u16, String) {
    let original = match run_mumps(mumps, inputs) {
        Ok(output) => output,
        Err(cause) => return (502, error_body(&cause)),
    };
    let port = match inquire(store, inputs) {
        Ok(output) => output,
        Err(error) => return inquiry_error_to_response(&error),
    };
    let (transcript, returned, _) = &original;
    let mut out = String::from("{\"mumps\":");
    write_session(
        &mut out,
        transcript,
        returned.as_ref().and_then(Option::as_deref),
    );
    out.push_str(",\"rust\":");
    write_session(&mut out, &port.transcript, port.returned.as_deref());
    out.push_str(",\"match\":");
    out.push_str(match decide_match(&original, &port) {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    });
    out.push('}');
    (200, out)
}

/// `match` of a compare response. `None` when the MUMPS return value is
/// unavailable. Otherwise `true` only when both the transcripts and the
/// return values are equal after normalising both sides with the capture's
/// rules: transcript-only agreement is never a match.
fn decide_match(original: &MumpsSide, port: &InquiryOutput) -> Option<bool> {
    let (transcript, returned, rules) = original;
    let expected = returned.as_ref()?;
    let rules = rules.as_slice();
    let transcripts_equal = normalise(transcript, rules) == normalise(&port.transcript, rules);
    let returns_equal = expected.as_deref().map(|value| normalise(value, rules))
        == port
            .returned
            .as_deref()
            .map(|value| normalise(value, rules));
    Some(transcripts_equal && returns_equal)
}

fn run_mumps(mumps: &MumpsMode, inputs: &[String]) -> Result<MumpsSide, String> {
    match mumps {
        MumpsMode::Replay { golden_dir } => replay(golden_dir, inputs),
        MumpsMode::Live {
            container,
            golden_dir,
        } => live(container, golden_dir, inputs),
    }
}

/// Why a capture file gave no text: it is absent; it is present but not
/// readable, a directory in its place counting here; or it is not UTF-8.
const CAPTURE_MISSING: &str = "MISSING";
const CAPTURE_UNREADABLE: &str = "UNREADABLE";
const CAPTURE_NON_UTF8: &str = "NON-UTF8";

/// The text of one capture file, or which of `CAPTURE_MISSING`,
/// `CAPTURE_UNREADABLE` and `CAPTURE_NON_UTF8` it is.
fn read_capture_file(path: &Path) -> Result<String, &'static str> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| CAPTURE_NON_UTF8),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(CAPTURE_MISSING),
        Err(_) => Err(CAPTURE_UNREADABLE),
    }
}

/// A capture file that must be present, readable and UTF-8.
fn required_capture_file(path: &Path) -> Result<String, String> {
    read_capture_file(path).map_err(|problem| format!("CAPTURE FILE {}: {problem}", path.display()))
}

/// The first capture under `golden_dir` whose input lines equal `inputs`,
/// used whole or not at all. A directory with no readable UTF-8 input file
/// does not match, and its path is listed when nothing matches.
fn replay(golden_dir: &Path, inputs: &[String]) -> Result<MumpsSide, String> {
    let mut unreadable: Vec<String> = Vec::new();
    for case in CASE_DIRS {
        let dir = golden_dir.join(case);
        let input_path = dir.join(INPUT_FILE);
        match read_capture_file(&input_path) {
            Ok(text) if split_input_lines(&text) == inputs => {
                return read_capture(&dir);
            }
            Ok(_) | Err(CAPTURE_MISSING) => {}
            // CAPTURE_UNREADABLE or CAPTURE_NON_UTF8.
            Err(_) => {
                unreadable.push(input_path.display().to_string());
            }
        }
    }
    if unreadable.is_empty() {
        Err("NOT RECORDED".to_string())
    } else {
        Err(format!(
            "NOT RECORDED (unreadable: {})",
            unreadable.join(", ")
        ))
    }
}

/// The transcript, return value and normalisation rules of the matching
/// capture in `dir`. An empty return-value or normalisation file is valid;
/// a non-empty return value loses one trailing newline.
fn read_capture(dir: &Path) -> Result<MumpsSide, String> {
    let transcript = required_capture_file(&dir.join(TRANSCRIPT_FILE))?;
    let returned = required_capture_file(&dir.join(RETURNED_FILE))?;
    let normalisation_path = dir.join(NORMALISATION_FILE);
    let normalisation = required_capture_file(&normalisation_path)?;
    let rules = parse_normalisation(&normalisation).map_err(|line| {
        format!(
            "MALFORMED NORMALISATION {} line {line}",
            normalisation_path.display()
        )
    })?;
    let returned = if returned.is_empty() {
        None
    } else {
        Some(returned.strip_suffix('\n').unwrap_or(&returned).to_string())
    };
    Ok((transcript, Some(returned), rules))
}

/// Runs the original live: the capture command and context come from the
/// first capture directory holding a meta file, the context must equal
/// `STUB_CONTEXT` before anything runs, and the container comes only from
/// the bridge's own configuration. Standard input is each input followed by
/// one `\n`, the form of a captured input file. Output is used only when
/// the run succeeded, and the return value is unavailable.
fn live(container: &str, golden_dir: &Path, inputs: &[String]) -> Result<MumpsSide, String> {
    let meta = live_meta(golden_dir)?;
    validate_context(&meta.context)?;
    let mut args = vec!["exec".to_string(), "-i".to_string(), container.to_string()];
    args.extend(meta.docker_exec_args);
    let mut stdin = String::new();
    for input in inputs {
        stdin.push_str(input);
        stdin.push('\n');
    }
    let result = run_command(
        "docker",
        &args,
        stdin.as_bytes(),
        LIVE_TIMEOUT,
        LIVE_OUTPUT_LIMIT,
    );
    let transcript = decide_live_result(&result).map_err(|failure| failure.to_string())?;
    Ok((transcript, None, Vec::new()))
}

/// The parsed meta file of the first capture directory that has one. With
/// none present, the error names where the first would be.
fn live_meta(golden_dir: &Path) -> Result<Meta, String> {
    for case in CASE_DIRS {
        let path = golden_dir.join(case).join(META_FILE);
        let malformed = || format!("MALFORMED META {}", path.display());
        match read_capture_file(&path) {
            Err(CAPTURE_MISSING) => {}
            Ok(text) => return parse_meta(&text).map_err(|_| malformed()),
            // CAPTURE_UNREADABLE or CAPTURE_NON_UTF8.
            Err(_) => return Err(malformed()),
        }
    }
    let first = golden_dir
        .join(CASE_DIRS.first().copied().unwrap_or_default())
        .join(META_FILE);
    Err(format!("MALFORMED META {}", first.display()))
}

// ---------------------------------------------------------------------------
// Shared capture-format helpers
// ---------------------------------------------------------------------------

/// The inputs of a captured input file: one per `\n`-terminated line. One
/// trailing `\n` ends the last line and adds no empty input; inner empty
/// lines are empty inputs. `""` gives no inputs and `"\n"` one empty input.
/// A `\r` is part of its line, never stripped.
pub fn split_input_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n').map(str::to_string).collect()
}

/// Parses a normalisation file into `(open, close)` rules, in file order.
///
/// Lines are split on `\n` and nothing is trimmed. A zero-length line is
/// blank and ignored. Every other line holds exactly one tab, separating two
/// literal, non-empty delimiters, open then close; a line with no tab, more
/// than one tab, or an empty delimiter gives `Err` with its 1-based line
/// number. A `\r` before the line end belongs to the close delimiter.
pub fn parse_normalisation(text: &str) -> Result<Vec<(String, String)>, usize> {
    let mut rules = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut delimiters = line.split('\t');
        match (delimiters.next(), delimiters.next(), delimiters.next()) {
            (Some(open), Some(close), None) if !open.is_empty() && !close.is_empty() => {
                rules.push((open.to_string(), close.to_string()));
            }
            _ => return Err(index.saturating_add(1)),
        }
    }
    Ok(rules)
}

/// Applies normalisation `rules` to `text`, in order, each to the result of
/// the one before.
///
/// For each rule the text is scanned left to right: from each occurrence of
/// `open` through the next `close` after it, both delimiters included, the
/// span becomes `<NORM>`, and scanning resumes after the replaced span. An
/// `open` with no later `close` leaves the rest of the text unchanged. A
/// rule with an empty delimiter, which [`parse_normalisation`] never
/// produces, changes nothing. No other transformation exists.
pub fn normalise(text: &str, rules: &[(String, String)]) -> String {
    let mut current = text.to_string();
    for (open, close) in rules {
        current = apply_rule(&current, open, close);
    }
    current
}

fn apply_rule(text: &str, open: &str, close: &str) -> String {
    if open.is_empty() || close.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let Some((before, from_open)) = rest.split_at_checked(start) else {
            break;
        };
        let after_open = from_open.get(open.len()..).unwrap_or("");
        let Some(end) = after_open.find(close) else {
            break;
        };
        out.push_str(before);
        out.push_str("<NORM>");
        rest = after_open
            .get(end.saturating_add(close.len())..)
            .unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// The parsed meta file of one capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub image_digest: String,
    pub source_revision: String,
    pub snapshot_id: String,
    pub record_ids: String,
    pub entry_point: String,
    /// The capture's context, in document order.
    pub context: Vec<(String, String)>,
    /// The arguments after `docker exec -i <container>`.
    pub docker_exec_args: Vec<String>,
}

/// Parses a meta file: `key=value` lines, split at the first `=`, with the
/// seven keys `image_digest`, `source_revision`, `snapshot_id`,
/// `record_ids`, `entry_point`, `context` and `docker_exec_args`, each
/// exactly once. Whitespace-only lines are blank and ignored, and one
/// trailing `\r` per line is dropped, as the ZWR loader does; values are
/// otherwise kept exactly. `context` must be a JSON object whose values are
/// all strings, with no repeated key; `docker_exec_args` a JSON array of
/// strings. A missing, repeated or unknown key, or a line without `=`, is
/// an error naming it.
pub fn parse_meta(text: &str) -> Result<Meta, String> {
    const KEYS: [&str; 7] = [
        "image_digest",
        "source_revision",
        "snapshot_id",
        "record_ids",
        "entry_point",
        "context",
        "docker_exec_args",
    ];
    let mut values: Vec<(&str, &str)> = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index.saturating_add(1);
        if raw.trim().is_empty() {
            continue;
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {number}: no '='"))?;
        if !KEYS.contains(&key) {
            return Err(format!("line {number}: unknown key {key}"));
        }
        if values.iter().any(|(seen, _)| *seen == key) {
            return Err(format!("line {number}: repeated key {key}"));
        }
        values.push((key, value));
    }
    for key in KEYS {
        meta_value(&values, key)?;
    }
    Ok(Meta {
        image_digest: meta_value(&values, "image_digest")?.to_string(),
        source_revision: meta_value(&values, "source_revision")?.to_string(),
        snapshot_id: meta_value(&values, "snapshot_id")?.to_string(),
        record_ids: meta_value(&values, "record_ids")?.to_string(),
        entry_point: meta_value(&values, "entry_point")?.to_string(),
        context: parse_context(meta_value(&values, "context")?)?,
        docker_exec_args: parse_exec_args(meta_value(&values, "docker_exec_args")?)?,
    })
}

fn meta_value<'t>(values: &[(&str, &'t str)], key: &str) -> Result<&'t str, String> {
    values
        .iter()
        .find(|(seen, _)| *seen == key)
        .map(|(_, value)| *value)
        .ok_or_else(|| format!("missing key {key}"))
}

fn parse_context(value: &str) -> Result<Vec<(String, String)>, String> {
    let not_object = || "context: not a JSON object of strings".to_string();
    let Some(members) = parse_json(value).and_then(json_members) else {
        return Err(not_object());
    };
    let mut seen: HashSet<&str> = HashSet::with_capacity(members.len());
    let mut context: Vec<(String, String)> = Vec::with_capacity(members.len());
    for (key, member) in &members {
        let Some(text) = json_string(member) else {
            return Err(not_object());
        };
        if !seen.insert(key.as_str()) {
            return Err(format!("context: repeated key {key}"));
        }
        context.push((key.clone(), text));
    }
    Ok(context)
}

fn parse_exec_args(value: &str) -> Result<Vec<String>, String> {
    let not_array = || "docker_exec_args: not a JSON array of strings".to_string();
    let Some(elements) = parse_json(value).and_then(json_elements) else {
        return Err(not_array());
    };
    elements
        .into_iter()
        .map(|element| json_string(element).ok_or_else(not_array))
        .collect()
}

/// Compares a capture's context with `STUB_CONTEXT`, key by key: first each
/// `STUB_CONTEXT` key in its order (`DUZ`, `IOF`, `IOSL`, `STATION`), then
/// each capture key `STUB_CONTEXT` lacks, in capture order. The first
/// mismatch is `Err("CONTEXT MISMATCH <key>: expected <value>, actual
/// <value>")`, where `UNRESOLVED` stands for an unresolved constant, which
/// matches nothing, `MISSING` for a key the capture lacks, and `ABSENT` for
/// the expected value of an extra capture key. A key the capture repeats
/// must match at every occurrence. No capture value overrides a constant.
pub fn validate_context(ctx: &[(String, String)]) -> Result<(), String> {
    validate_against(&STUB_CONTEXT, ctx)
}

/// [`validate_context`] against any constant table, so the unit tests can
/// reach the extra-key rule, which an unresolved constant in
/// `STUB_CONTEXT` always pre-empts.
fn validate_against(
    constants: &[(&str, Option<&str>)],
    ctx: &[(String, String)],
) -> Result<(), String> {
    for &(key, expected) in constants {
        let mut occurrences = ctx.iter().filter(|(name, _)| name == key).peekable();
        if occurrences.peek().is_none() {
            return Err(mismatch(key, expected.unwrap_or("UNRESOLVED"), "MISSING"));
        }
        for (_, actual) in occurrences {
            if expected != Some(actual.as_str()) {
                return Err(mismatch(key, expected.unwrap_or("UNRESOLVED"), actual));
            }
        }
    }
    for (key, actual) in ctx {
        if !constants.iter().any(|(name, _)| name == key) {
            return Err(mismatch(key, "ABSENT", actual));
        }
    }
    Ok(())
}

fn mismatch(key: &str, expected: &str, actual: &str) -> String {
    format!("CONTEXT MISMATCH {key}: expected {expected}, actual {actual}")
}

// ---------------------------------------------------------------------------
// Live execution
// ---------------------------------------------------------------------------

/// How one [`run_command`] child ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecResult {
    /// The OS error text when the program could not be started.
    pub spawn_error: Option<String>,
    /// The exit code; `None` when the child was killed or ended by a signal.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub output_limit_exceeded: bool,
    /// At most `output_limit` bytes of standard output.
    pub stdout: Vec<u8>,
    /// At most `output_limit` bytes of standard error.
    pub stderr: Vec<u8>,
}

/// Why a live run gives no transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveFailure {
    Spawn(String),
    NonZeroExit {
        code: Option<i32>,
        stderr_head: String,
    },
    Timeout,
    NonUtf8,
    OutputTooLarge,
}

impl fmt::Display for LiveFailure {
    /// The cause text of the 502 body.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiveFailure::Spawn(os_error) => f.write_str(os_error),
            LiveFailure::NonZeroExit {
                code: Some(code),
                stderr_head,
            } => write!(f, "EXIT {code}: {stderr_head}"),
            LiveFailure::NonZeroExit {
                code: None,
                stderr_head,
            } => write!(f, "EXIT NONE: {stderr_head}"),
            LiveFailure::Timeout => f.write_str("TIMEOUT"),
            LiveFailure::NonUtf8 => f.write_str("NON-UTF8"),
            LiveFailure::OutputTooLarge => f.write_str("OUTPUT TOO LARGE"),
        }
    }
}

impl ExecResult {
    fn spawn_failure(message: String) -> ExecResult {
        ExecResult {
            spawn_error: Some(message),
            exit_code: None,
            timed_out: false,
            output_limit_exceeded: false,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

/// Runs `program` with `args` directly, never through a shell, writes
/// `stdin` to it and collects its output.
///
/// Standard input is written, and standard output and error drained, each
/// on its own thread, so a full pipe cannot deadlock. The child is killed
/// when `timeout` passes before it has exited and its output has been read,
/// or as soon as standard output passes `output_limit` bytes; at most
/// `output_limit` bytes of each stream are kept. A pipe thread that is still
/// running shortly after the child is gone, because a descendant holds its
/// pipe, is left behind rather than waited for.
pub fn run_command(
    program: &str,
    args: &[String],
    stdin: &[u8],
    timeout: Duration,
    output_limit: usize,
) -> ExecResult {
    let started = Instant::now();
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return ExecResult::spawn_failure(e.to_string()),
    };
    let exceeded = Arc::new(AtomicBool::new(false));
    let threads = start_pipe_threads(&mut child, stdin, output_limit, &exceeded);
    let (writer, out_reader, err_reader) = match threads {
        Ok(threads) => threads,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return ExecResult::spawn_failure(e.to_string());
        }
    };

    let mut status: Option<ExitStatus> = None;
    let mut timed_out = false;
    let mut output_limit_exceeded = false;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(exited) => status = exited,
                // The child cannot be waited on; it is killed and reaped below.
                Err(_) => break,
            }
        }
        if exceeded.load(Ordering::SeqCst) {
            output_limit_exceeded = true;
            break;
        }
        if status.is_some() && out_reader.is_finished() && err_reader.is_finished() {
            break;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            break;
        }
        thread::sleep(POLL);
    }
    if status.is_none() || output_limit_exceeded {
        status = kill_and_reap(&mut child).or(status);
    }

    let grace = Instant::now()
        .checked_add(PIPE_GRACE)
        .unwrap_or_else(Instant::now);
    join_within(writer, grace);
    let stdout = join_within(out_reader, grace).unwrap_or_default();
    let stderr = join_within(err_reader, grace).unwrap_or_default();
    ExecResult {
        spawn_error: None,
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        output_limit_exceeded,
        stdout,
        stderr,
    }
}

type PipeThreads = (JoinHandle<()>, JoinHandle<Vec<u8>>, JoinHandle<Vec<u8>>);

/// Starts the standard-input writer and the two output readers. The
/// standard-output reader sets `exceeded` once more than `output_limit`
/// bytes have arrived.
fn start_pipe_threads(
    child: &mut Child,
    stdin: &[u8],
    output_limit: usize,
    exceeded: &Arc<AtomicBool>,
) -> io::Result<PipeThreads> {
    let missing = || io::Error::other("child pipe not captured");
    let mut input_pipe = child.stdin.take().ok_or_else(missing)?;
    let output_pipe = child.stdout.take().ok_or_else(missing)?;
    let error_pipe = child.stderr.take().ok_or_else(missing)?;
    let input = stdin.to_vec();
    // The pipe closes when the thread ends, so the child sees end of input.
    // A child that exits without reading makes the write fail; that is not
    // an error of the run.
    let writer = thread::Builder::new()
        .name("run-command-stdin".to_string())
        .spawn(move || {
            let _ = input_pipe.write_all(&input);
        })?;
    let flag = Arc::clone(exceeded);
    let out_reader = thread::Builder::new()
        .name("run-command-stdout".to_string())
        .spawn(move || read_bounded(output_pipe, output_limit, Some(flag.as_ref())))?;
    let err_reader = thread::Builder::new()
        .name("run-command-stderr".to_string())
        .spawn(move || read_bounded(error_pipe, output_limit, None))?;
    Ok((writer, out_reader, err_reader))
}

/// Reads `pipe` to its end, keeping at most `limit` bytes. Past the limit,
/// a reader with `exceeded` sets it and stops; one without keeps draining
/// and discarding, so the child never blocks on that pipe.
fn read_bounded(mut pipe: impl Read, limit: usize, exceeded: Option<&AtomicBool>) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return kept,
            Ok(n) => {
                let chunk = buf.get(..n).unwrap_or(&[]);
                let room = limit.saturating_sub(kept.len());
                if chunk.len() <= room {
                    kept.extend_from_slice(chunk);
                    continue;
                }
                kept.extend_from_slice(chunk.get(..room).unwrap_or(&[]));
                if let Some(flag) = exceeded {
                    flag.store(true, Ordering::SeqCst);
                    return kept;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return kept,
        }
    }
}

/// Kills the child and reaps it; `None` when it cannot be reaped.
fn kill_and_reap(child: &mut Child) -> Option<ExitStatus> {
    // Killing a child that has already exited fails harmlessly.
    let _ = child.kill();
    child.wait().ok()
}

/// The thread's result once it has finished, waiting until `deadline` at
/// most. `None` when it is still running, which detaches it, or when it
/// panicked.
fn join_within<T>(handle: JoinHandle<T>, deadline: Instant) -> Option<T> {
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    if !handle.is_finished() {
        return None;
    }
    handle.join().ok()
}

/// The transcript of a live run, or why there is none. The first match
/// wins: a spawn error, a timeout, too much output, an exit code other than
/// 0 (with the first 200 bytes of standard error, decoded lossily), then
/// standard output that is not UTF-8.
pub fn decide_live_result(r: &ExecResult) -> Result<String, LiveFailure> {
    if let Some(os_error) = &r.spawn_error {
        return Err(LiveFailure::Spawn(os_error.clone()));
    }
    if r.timed_out {
        return Err(LiveFailure::Timeout);
    }
    if r.output_limit_exceeded {
        return Err(LiveFailure::OutputTooLarge);
    }
    if r.exit_code != Some(0) {
        let head = r.stderr.get(..200).unwrap_or(r.stderr.as_slice());
        return Err(LiveFailure::NonZeroExit {
            code: r.exit_code,
            stderr_head: String::from_utf8_lossy(head).into_owned(),
        });
    }
    String::from_utf8(r.stdout.clone()).map_err(|_| LiveFailure::NonUtf8)
}

/// The status and complete JSON body for a Rust-side error:
/// `UnsupportedBranch(id)` is 422 and `CorruptStore(record)` is 500, each
/// naming the branch ID or record verbatim.
pub fn inquiry_error_to_response(e: &InquiryError) -> (u16, String) {
    match e {
        InquiryError::UnsupportedBranch(id) => {
            (422, error_body(&format!("UNSUPPORTED BRANCH {id}")))
        }
        InquiryError::CorruptStore(record) => (500, error_body(&format!("CORRUPT STORE {record}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect()
    }

    /// A child that exited 0 and wrote `T`.
    fn exited_ok() -> ExecResult {
        ExecResult {
            spawn_error: None,
            exit_code: Some(0),
            timed_out: false,
            output_limit_exceeded: false,
            stdout: b"T".to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn json_reads_escapes_and_surrogate_pairs() -> Result<(), String> {
        let text = r#" {"s":"q\" b\\ s\/ \b\f\n\r\t \u0041\u00e9\u20ac \ud83d\ude00 \u0007",
            "n":-1.5e3, "z":0, "a":[true,false,null], "o":{}} "#;
        let rejected = || "rejected well-formed JSON".to_string();
        let value = parse_json(text).ok_or_else(rejected)?;
        assert_eq!(value, text.trim());
        let members = json_members(value).ok_or_else(rejected)?;
        let keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["s", "n", "z", "a", "o"]);
        let member = |key: &str| {
            members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, member)| *member)
        };
        assert_eq!(
            member("s").and_then(json_string),
            Some("q\" b\\ s/ \u{8}\u{c}\n\r\t A\u{e9}\u{20ac} \u{1f600} \u{7}".to_string())
        );
        assert_eq!(member("n"), Some("-1.5e3"));
        assert_eq!(member("z"), Some("0"));
        assert_eq!(
            member("a").and_then(json_elements),
            Some(vec!["true", "false", "null"])
        );
        assert_eq!(member("o").and_then(json_members), Some(Vec::new()));
        // Each reader takes exactly one value of its own kind.
        assert_eq!(member("n").and_then(json_string), None);
        assert_eq!(member("a").and_then(json_members), None);
        assert_eq!(member("o").and_then(json_elements), None);
        assert_eq!(json_string(r#""a" "#), None);
        let malformed = [
            r#""\ud83d""#,
            r#""\ude00""#,
            r#""\ud83d\u0041""#,
            r#""\ud83dx""#,
            r#""\x""#,
            r#""\u12g4""#,
            "\"a\u{1}b\"",
            "\"unterminated",
            "",
            "tru",
            "[1,]",
            "{\"a\":1,}",
            "{\"a\"}",
            "{1:2}",
            "01",
            "1.",
            "1e",
            "-",
            "[1] x",
            "{} {}",
        ];
        for text in malformed {
            assert!(parse_json(text).is_none(), "accepted {text:?}");
        }
        let nested = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        assert!(parse_json(&nested(MAX_JSON_DEPTH)).is_some());
        assert!(parse_json(&nested(MAX_JSON_DEPTH.saturating_add(2))).is_none());
        Ok(())
    }

    #[test]
    fn json_writes_escapes() -> Result<(), String> {
        let original = "q\" b\\ \n\r\t\u{8}\u{c}\u{7}\u{1f}\u{0} \u{e9}\u{20ac}\u{1f600} /";
        let mut out = String::new();
        write_json_string(&mut out, original);
        assert_eq!(out, r#""q\" b\\ \n\r\t\b\f\u0007\u001f\u0000 é€😀 /""#);
        let read_back = parse_json(&out)
            .and_then(json_string)
            .ok_or_else(|| "writer output does not parse".to_string())?;
        assert_eq!(read_back, original);
        assert_eq!(error_body("NOT RECORDED"), r#"{"error":"NOT RECORDED"}"#);
        assert_eq!(
            session_body("a\u{7}\n", None),
            r#"{"transcript":"a\u0007\n","returned":null}"#
        );
        assert_eq!(
            session_body("", Some("1")),
            r#"{"transcript":"","returned":"1"}"#
        );
        Ok(())
    }

    #[test]
    fn decide_live_result_spawn() {
        let result = ExecResult {
            spawn_error: Some("No such file or directory (os error 2)".to_string()),
            timed_out: true,
            output_limit_exceeded: true,
            exit_code: None,
            ..exited_ok()
        };
        let failure = decide_live_result(&result);
        assert_eq!(
            failure,
            Err(LiveFailure::Spawn(
                "No such file or directory (os error 2)".to_string()
            ))
        );
        assert_eq!(
            failure.map_err(|f| f.to_string()),
            Err("No such file or directory (os error 2)".to_string())
        );
    }

    #[test]
    fn decide_live_result_timeout() {
        let result = ExecResult {
            timed_out: true,
            exit_code: None,
            ..exited_ok()
        };
        assert_eq!(decide_live_result(&result), Err(LiveFailure::Timeout));
        assert_eq!(LiveFailure::Timeout.to_string(), "TIMEOUT");
    }

    #[test]
    fn decide_live_result_output_too_large() {
        let result = ExecResult {
            output_limit_exceeded: true,
            exit_code: Some(2),
            ..exited_ok()
        };
        assert_eq!(
            decide_live_result(&result),
            Err(LiveFailure::OutputTooLarge)
        );
        assert_eq!(LiveFailure::OutputTooLarge.to_string(), "OUTPUT TOO LARGE");
    }

    #[test]
    fn decide_live_result_non_zero_exit() {
        // 250 bytes of standard error, the first not UTF-8: only the first
        // 200 are kept, decoded lossily.
        let mut stderr = vec![0xFF];
        stderr.extend(std::iter::repeat_n(b'e', 249));
        let head = format!("\u{fffd}{}", "e".repeat(199));
        for (code, label) in [(Some(3), "3"), (None, "NONE")] {
            let result = ExecResult {
                exit_code: code,
                stderr: stderr.clone(),
                ..exited_ok()
            };
            let failure = decide_live_result(&result);
            assert_eq!(
                failure,
                Err(LiveFailure::NonZeroExit {
                    code,
                    stderr_head: head.clone(),
                })
            );
            assert_eq!(
                failure.map_err(|f| f.to_string()),
                Err(format!("EXIT {label}: {head}"))
            );
        }
        let short = ExecResult {
            exit_code: Some(1),
            stderr: b"no such container".to_vec(),
            ..exited_ok()
        };
        assert_eq!(
            decide_live_result(&short).map_err(|f| f.to_string()),
            Err("EXIT 1: no such container".to_string())
        );
    }

    #[test]
    fn decide_live_result_non_utf8() {
        let result = ExecResult {
            stdout: vec![b'T', 0xFF],
            ..exited_ok()
        };
        assert_eq!(decide_live_result(&result), Err(LiveFailure::NonUtf8));
        assert_eq!(LiveFailure::NonUtf8.to_string(), "NON-UTF8");
    }

    #[test]
    fn decide_live_result_precedence_timeout_over_output_limit() {
        let result = ExecResult {
            timed_out: true,
            output_limit_exceeded: true,
            exit_code: None,
            stdout: vec![0xFF],
            ..exited_ok()
        };
        assert_eq!(decide_live_result(&result), Err(LiveFailure::Timeout));
    }

    #[test]
    fn decide_live_result_success() {
        let result = ExecResult {
            stderr: b"warning on stderr".to_vec(),
            ..exited_ok()
        };
        assert_eq!(decide_live_result(&result), Ok("T".to_string()));
    }

    #[test]
    fn run_command_kills_on_timeout() -> Result<(), String> {
        let started = Instant::now();
        let result = run_command(
            "sleep",
            &strings(&["5"]),
            b"",
            Duration::from_millis(200),
            4096,
        );
        if result.spawn_error.is_some() {
            println!("NOT RUN: sleep not found");
            return Ok(());
        }
        let elapsed = started.elapsed();
        assert!(result.timed_out, "not timed out: {result:?}");
        assert!(!result.output_limit_exceeded);
        assert_eq!(result.exit_code, None, "a killed child has no exit code");
        assert!(elapsed < Duration::from_secs(3), "took {elapsed:?}");
        assert_eq!(decide_live_result(&result), Err(LiveFailure::Timeout));
        Ok(())
    }

    #[test]
    fn run_command_kills_on_output_limit() -> Result<(), String> {
        let started = Instant::now();
        let result = run_command("yes", &[], b"", Duration::from_secs(10), 4096);
        if result.spawn_error.is_some() {
            println!("NOT RUN: yes not found");
            return Ok(());
        }
        assert!(
            result.output_limit_exceeded,
            "limit not enforced: {result:?}"
        );
        assert!(!result.timed_out);
        assert_eq!(result.stdout.len(), 4096, "kept more than the limit");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            decide_live_result(&result),
            Err(LiveFailure::OutputTooLarge)
        );
        Ok(())
    }

    #[test]
    fn run_command_writes_stdin_and_reports_spawn_and_exit() -> Result<(), String> {
        let missing = run_command(
            "/nonexistent/ifcap-po-inquiry-no-such-program",
            &[],
            b"",
            Duration::from_secs(1),
            4096,
        );
        assert!(missing.spawn_error.is_some(), "spawned a missing program");
        let echoed = run_command("cat", &[], b"\nA1\n^\n", Duration::from_secs(10), 4096);
        if echoed.spawn_error.is_some() {
            println!("NOT RUN: cat not found");
            return Ok(());
        }
        assert_eq!(decide_live_result(&echoed), Ok("\nA1\n^\n".to_string()));
        let failed = run_command("false", &[], b"", Duration::from_secs(10), 4096);
        if failed.spawn_error.is_some() {
            println!("NOT RUN: false not found");
            return Ok(());
        }
        assert_eq!(failed.exit_code, Some(1));
        Ok(())
    }

    #[test]
    fn inquiry_error_to_response_unsupported_branch_422() {
        let error = InquiryError::UnsupportedBranch("PO-09.b4".to_string());
        assert_eq!(
            inquiry_error_to_response(&error),
            (
                422,
                r#"{"error":"UNSUPPORTED BRANCH PO-09.b4"}"#.to_string()
            )
        );
    }

    #[test]
    fn inquiry_error_to_response_corrupt_store_500() {
        let error = InquiryError::CorruptStore("^PRC(442,\"C\",\"A1\",1)".to_string());
        assert_eq!(
            inquiry_error_to_response(&error),
            (
                500,
                r#"{"error":"CORRUPT STORE ^PRC(442,\"C\",\"A1\",1)"}"#.to_string()
            )
        );
    }

    #[test]
    fn split_input_lines_trailing_newline() {
        let cases: [(&str, &[&str]); 8] = [
            ("", &[]),
            ("\n", &[""]),
            ("a", &["a"]),
            ("a\n", &["a"]),
            ("a\n\n", &["a", ""]),
            ("\nA1\n\n\n^\n", &["", "A1", "", "", "^"]),
            ("\nINVALID!\n^", &["", "INVALID!", "^"]),
            ("a\r\n", &["a\r"]),
        ];
        for (text, expected) in cases {
            assert_eq!(split_input_lines(text), strings(expected), "for {text:?}");
        }
    }

    #[test]
    fn normalisation_rules_and_malformed_lines() {
        type Parsed = Result<Vec<(String, String)>, usize>;
        let parsed: [(&str, Parsed); 9] = [
            ("", Ok(Vec::new())),
            ("\n\n", Ok(Vec::new())),
            ("@\t#\n", Ok(pairs(&[("@", "#")]))),
            ("\nT=\t;\n\n@\t#\r", Ok(pairs(&[("T=", ";"), ("@", "#\r")]))),
            ("no tab", Err(1)),
            ("@\t#\na\tb\tc", Err(2)),
            ("\n\tb", Err(2)),
            ("a\t", Err(1)),
            (" ", Err(1)),
        ];
        for (text, expected) in parsed {
            assert_eq!(parse_normalisation(text), expected, "for {text:?}");
        }
        type Rules<'a> = &'a [(&'a str, &'a str)];
        let applied: [(&str, Rules, &str); 8] = [
            ("a@1#b@2#c", &[("@", "#")], "a<NORM>b<NORM>c"),
            ("a@1#b@2", &[("@", "#")], "a<NORM>b@2"),
            ("a@1", &[("@", "#")], "a@1"),
            ("a|1|b|2|c", &[("|", "|")], "a<NORM>b<NORM>c"),
            ("abab", &[("ab", "b")], "<NORM>"),
            ("x@1#y", &[("@", "#"), ("x<", "y")], "<NORM>"),
            ("x@1#y", &[("x<", "y"), ("@", "#")], "x<NORM>y"),
            ("a@1#b", &[("", "#"), ("@", "")], "a@1#b"),
        ];
        for (text, rules, expected) in applied {
            assert_eq!(normalise(text, &pairs(rules)), expected, "for {text:?}");
        }
    }

    fn recorded(transcript: &str, returned: Option<&str>, rules: &[(&str, &str)]) -> MumpsSide {
        (
            transcript.to_string(),
            Some(returned.map(str::to_string)),
            pairs(rules),
        )
    }

    fn port(transcript: &str, returned: Option<&str>) -> InquiryOutput {
        InquiryOutput {
            transcript: transcript.to_string(),
            returned: returned.map(str::to_string),
        }
    }

    #[test]
    fn compare_match_null_when_mumps_returned_unavailable() {
        let live: MumpsSide = ("T".to_string(), None, Vec::new());
        assert_eq!(decide_match(&live, &port("T", None)), None);
        assert_eq!(decide_match(&live, &port("other", Some("x"))), None);
    }

    #[test]
    fn compare_match_needs_transcript_and_returned() {
        let cases = [
            (recorded("T", None, &[]), port("T", None), Some(true)),
            (
                recorded("T", Some("1"), &[]),
                port("T", Some("1")),
                Some(true),
            ),
            (recorded("T", Some("1"), &[]), port("T", None), Some(false)),
            (recorded("T", None, &[]), port("T", Some("")), Some(false)),
            (recorded("T", None, &[]), port("U", None), Some(false)),
            (
                recorded("at @07# end", Some("@1#"), &[("@", "#")]),
                port("at @99# end", Some("@2#")),
                Some(true),
            ),
            (
                recorded("at @07# end", None, &[("@", "#")]),
                port("at 99 end", None),
                Some(false),
            ),
        ];
        for (original, rust, expected) in cases {
            assert_eq!(decide_match(&original, &rust), expected, "for {original:?}");
        }
    }

    /// A well-formed meta file whose context holds resolved device values.
    const META_TEXT: &str = r##"image_digest=sha256:0123
source_revision=dd3fd0f3124c8178657d9fff4173c324da30f7ae
snapshot_id=git:dd3fd0f3124c8178657d9fff4173c324da30f7ae

record_ids=1 500-A12345
entry_point=EN3^PRCHRPT
context={"DUZ":".5","IOF":"#","IOSL":"24","STATION":"500"}
docker_exec_args=["--user","vehu","synthetic","X=1"]
"##;

    #[test]
    fn parse_meta_reads_seven_keys_and_rejects_bad_lines() -> Result<(), String> {
        let meta = parse_meta(META_TEXT)?;
        assert_eq!(
            meta,
            Meta {
                image_digest: "sha256:0123".to_string(),
                source_revision: "dd3fd0f3124c8178657d9fff4173c324da30f7ae".to_string(),
                snapshot_id: "git:dd3fd0f3124c8178657d9fff4173c324da30f7ae".to_string(),
                record_ids: "1 500-A12345".to_string(),
                entry_point: "EN3^PRCHRPT".to_string(),
                context: pairs(&[
                    ("DUZ", ".5"),
                    ("IOF", "#"),
                    ("IOSL", "24"),
                    ("STATION", "500")
                ]),
                docker_exec_args: strings(&["--user", "vehu", "synthetic", "X=1"]),
            }
        );
        assert_eq!(parse_meta(&META_TEXT.replace('\n', "\r\n")), Ok(meta));
        let replaced = |from: &str, to: &str| parse_meta(&META_TEXT.replacen(from, to, 1));
        let rejected = [
            (
                replaced("record_ids=1 500-A12345\n", ""),
                "missing key record_ids",
            ),
            (
                replaced("entry_point=", "image_digest="),
                "line 6: repeated key image_digest",
            ),
            (
                replaced("entry_point=", "entry=point="),
                "line 6: unknown key entry",
            ),
            (replaced("entry_point=", "entry_point "), "line 6: no '='"),
            (
                replaced(r#"{"DUZ""#, r#"{"IOSL":"24","DUZ""#),
                "context: repeated key IOSL",
            ),
            (
                replaced(r#""500"}"#, r#"500}"#),
                "context: not a JSON object of strings",
            ),
            (
                replaced(r#"={"DUZ"#, r#"=["DUZ"#),
                "context: not a JSON object of strings",
            ),
            (
                replaced(r#"["--user""#, "not-json"),
                "docker_exec_args: not a JSON array of strings",
            ),
            (
                replaced(r#""X=1""#, "1"),
                "docker_exec_args: not a JSON array of strings",
            ),
        ];
        for (result, message) in rejected {
            assert_eq!(result, Err(message.to_string()));
        }
        Ok(())
    }

    #[test]
    fn context_duplicates_found_with_a_set() {
        let expected: Vec<(String, String)> = (0..10_000)
            .map(|n| (format!("K{n}"), format!("v{n}")))
            .collect();
        let members: Vec<String> = expected
            .iter()
            .map(|(key, value)| format!("\"{key}\":\"{value}\""))
            .collect();
        let object = format!("{{{}}}", members.join(","));
        assert_eq!(parse_context(&object), Ok(expected));
        let repeated = format!("{{{},\"K0\":\"again\"}}", members.join(","));
        assert_eq!(
            parse_context(&repeated),
            Err("context: repeated key K0".to_string())
        );
        // A value that is not a string is found before the key's repetition.
        assert_eq!(
            parse_context(r#"{"A":"1","A":2}"#),
            Err("context: not a JSON object of strings".to_string())
        );
    }

    #[test]
    fn validate_context_messages() {
        let resolved = pairs(&[
            ("DUZ", ".5"),
            ("IOF", "#"),
            ("IOSL", "24"),
            ("STATION", "500"),
        ]);
        let cases = [
            (
                resolved.clone(),
                "CONTEXT MISMATCH IOF: expected UNRESOLVED, actual #",
            ),
            (
                Vec::new(),
                "CONTEXT MISMATCH DUZ: expected .5, actual MISSING",
            ),
            (
                pairs(&[("DUZ", "1")]),
                "CONTEXT MISMATCH DUZ: expected .5, actual 1",
            ),
            (
                pairs(&[("DUZ", ".5"), ("DUZ", "1")]),
                "CONTEXT MISMATCH DUZ: expected .5, actual 1",
            ),
            (
                pairs(&[("DUZ", ".5"), ("STATION", "500")]),
                "CONTEXT MISMATCH IOF: expected UNRESOLVED, actual MISSING",
            ),
        ];
        for (context, message) in cases {
            assert_eq!(validate_context(&context), Err(message.to_string()));
        }
        // With every constant resolved, the extra-key rule is reachable.
        let constants = [("DUZ", Some(".5")), ("STATION", Some("500"))];
        let exact = pairs(&[("STATION", "500"), ("DUZ", ".5")]);
        assert_eq!(validate_against(&constants, &exact), Ok(()));
        let extra = pairs(&[("DUZ", ".5"), ("STATION", "500"), ("IOM", "80")]);
        assert_eq!(
            validate_against(&constants, &extra),
            Err("CONTEXT MISMATCH IOM: expected ABSENT, actual 80".to_string())
        );
    }

    #[test]
    fn session_request_validation() {
        let accepted =
            parse_session_request(br#"{"other":1,"inputs":[""," A1 ","^","\u0007\t\u00e9"]}"#);
        assert_eq!(accepted, Ok(strings(&["", " A1 ", "^", "\u{7}\t\u{e9}"])));
        let rejected: [(&[u8], &str); 11] = [
            (br#"{"inputs": ["#, "MALFORMED JSON"),
            (b"", "MALFORMED JSON"),
            (b"{\"inputs\":[\"\xff\"]}", "MALFORMED JSON"),
            (br#"["a"]"#, "BODY IS NOT A JSON OBJECT"),
            (br#"{}"#, "INPUTS MISSING OR REPEATED"),
            (
                br#"{"inputs":[],"inputs":[]}"#,
                "INPUTS MISSING OR REPEATED",
            ),
            (br#"{"inputs":"x"}"#, "INPUTS IS NOT AN ARRAY"),
            (br#"{"inputs":[1]}"#, "INPUT 1 IS NOT A STRING"),
            (br#"{"inputs":["",null]}"#, "INPUT 2 IS NOT A STRING"),
            (
                br#"{"inputs":["a\rb"]}"#,
                "INPUT 1 CONTAINS A CARRIAGE RETURN OR LINE FEED",
            ),
            (
                br#"{"inputs":["a\nb"]}"#,
                "INPUT 1 CONTAINS A CARRIAGE RETURN OR LINE FEED",
            ),
        ];
        for (body, cause) in rejected {
            assert_eq!(parse_session_request(body), Err(cause.to_string()));
        }
    }

    #[test]
    fn request_head_parsing() {
        assert_eq!(
            parse_head(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
            Ok(("GET", "/", None, false))
        );
        assert_eq!(
            parse_head(
                b"POST /api/rust/session HTTP/1.0\r\ncontent-length: 12\r\nContent-Length:12\r\nExpect: 100-continue\r\n\r\n"
            ),
            Ok(("POST", "/api/rust/session", Some(12), true))
        );
        let malformed: [&[u8]; 10] = [
            b"GET /\r\n\r\n",
            b"GET  / HTTP/1.1\r\n\r\n",
            b"GET x HTTP/1.1\r\n\r\n",
            b"GET / HTTP/2\r\n\r\n",
            b"GET / HTTP/1.1\r\nNoColon\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: a\r\n folded\r\n\r\n",
            b"POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
            b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
            b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: \xff\r\n\r\n",
        ];
        for head in malformed {
            assert!(parse_head(head).is_err(), "accepted {head:?}");
        }
    }

    #[test]
    fn route_errors_repeat_no_request_data() -> Result<(), String> {
        let store = Store::from_zwr("").map_err(|e| e.to_string())?;
        // Every case is answered before a backend runs, so no capture is read.
        let mumps = MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        };
        // `NAME` marks request data that no 404 body may repeat.
        let cases: [(&str, &str, &str); 12] = [
            ("GET", "/missing?patient=NAME", NOT_FOUND),
            ("POST", "/NAME", NOT_FOUND),
            ("POST", "/api/NAME/session", UNKNOWN_BACKEND),
            ("GET", "/api/rust/session", POST_REQUIRED),
            ("PUT", "/api/rust/session", POST_REQUIRED),
            ("NAME", "/api/compare/session", POST_REQUIRED),
            ("GET", "/api/rust/session?patient=NAME", NOT_FOUND),
            ("POST", "/api/rust/session/NAME", NOT_FOUND),
            ("POST", "/api/rust/NAME", NOT_FOUND),
            ("POST", "/api/NAME", NOT_FOUND),
            ("POST", "/style.css", GET_REQUIRED),
            ("NAME", "/", GET_REQUIRED),
        ];
        for (method, target, cause) in cases {
            let request = (
                method.to_string(),
                target.to_string(),
                Some(br#"{"inputs":["NAME"]}"#.to_vec()),
            );
            let (status, content_type, body) = route(&request, &store, &mumps);
            let body = String::from_utf8(body).map_err(|e| e.to_string())?;
            assert_eq!(
                (status, content_type, body.as_str()),
                (404, JSON_TYPE, error_body(cause).as_str()),
                "for {method} {target}"
            );
            assert!(!body.contains("NAME"), "{method} {target} gave {body}");
        }
        Ok(())
    }

    #[test]
    fn mumps_side_without_captures() {
        let golden_dir = std::env::temp_dir().join("ifcap-po-inquiry-unit-no-such-captures");
        let replay = MumpsMode::Replay {
            golden_dir: golden_dir.clone(),
        };
        assert_eq!(
            run_mumps(&replay, &strings(&["", "^"])),
            Err("NOT RECORDED".to_string())
        );
        // Live mode stops at the absent meta file, before any process runs.
        let live = MumpsMode::Live {
            container: "unit-test-never-run".to_string(),
            golden_dir: golden_dir.clone(),
        };
        let first_case = CASE_DIRS.first().copied().unwrap_or_default();
        let expected = golden_dir.join(first_case).join(META_FILE);
        assert_eq!(
            run_mumps(&live, &[]),
            Err(format!("MALFORMED META {}", expected.display()))
        );
    }
}
