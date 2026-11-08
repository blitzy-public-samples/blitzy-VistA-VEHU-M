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
use std::net::{Ipv6Addr, Shutdown, TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::inquiry::{InquiryError, InquiryOutput, STUB_CONTEXT, inquire};
use crate::store::{Store, load_fixtures};

/// How often the accept loop polls its shutdown flag, a live request tries
/// the busy [`LIVE_ADMISSION`] slots again and `run_command` polls its
/// child, and how long a `run_command` stream worker blocks before it checks
/// for cancellation.
const POLL: Duration = Duration::from_millis(10);
/// How long a client connection may stay silent before the first byte of its
/// request, and the write timeout of a response.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a request may take from its first byte to the end of its body.
/// It is one deadline, which bytes trickled in one at a time do not renew.
const REQUEST_TIME: Duration = Duration::from_secs(5);
/// The longest request line plus headers accepted.
const MAX_HEAD: usize = 16 * 1024;
/// The longest request body accepted.
const MAX_BODY: usize = 1024 * 1024;
/// The most connections served at once. Each holds a thread, two descriptors
/// (the stream and the clone that wakes it at shutdown) and request buffers
/// of about [`MAX_HEAD`] plus [`MAX_BODY`] at most, so this bounds what
/// clients can make the bridge hold, far below a 1024-descriptor limit, and
/// well above the few connections a browser opens to one host. At the bound
/// no connection is taken: further ones wait in the listener's kernel
/// backlog.
const MAX_CONNECTIONS: usize = 64;
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
/// The most `docker exec` children live mode runs at once, across every
/// connection, so that a burst of requests cannot multiply the processes,
/// stream threads, descriptors and output buffers each run holds.
const LIVE_SLOTS: usize = 4;
/// The longest a live request waits, in all, for one of the `LIVE_SLOTS`;
/// a request that gets none in that time starts nothing.
const LIVE_ADMISSION_WAIT: Duration = LIVE_TIMEOUT;
/// The one bound on live runs, shared by every connection thread: a run
/// holds the lock of one slot for as long as it lasts.
static LIVE_ADMISSION: [Mutex<()>; LIVE_SLOTS] = [const { Mutex::new(()) }; LIVE_SLOTS];

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

/// Serves `listener` until serving fails. Loads the fixture store once from
/// `tests/fixtures/ifcap/` of this crate, before anything is served; a
/// fixture that cannot be loaded is an `InvalidData` error naming it. Then
/// hands `listener` to [`serve_until`] with a shutdown flag that is never
/// set, so this returns only with a serving error of `serve_until`,
/// unchanged.
pub fn serve(listener: TcpListener, mumps: MumpsMode) -> io::Result<()> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ifcap");
    let store = load_fixtures(&fixtures)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    serve_until(listener, store, mumps, Arc::new(AtomicBool::new(false)))
}

/// Serves `listener` until `shutdown` is set, then returns `Ok(())`. Public
/// only as the test hook behind [`serve`].
///
/// The listener is made non-blocking first; a failure to do so is returned
/// as it is, before anything is served. `shutdown` is polled about every
/// 10 ms: between attempts to accept while no connection is pending, and
/// while `MAX_CONNECTIONS` connections are open. At that bound no connection
/// is accepted, so further ones wait in the listener's kernel backlog, given
/// no status, until an open one ends. Each connection is handled on its own
/// thread, so an idle one never blocks another.
///
/// Serving fails when `accept` fails in a way no retry mends, when a
/// connection cannot be given its socket clone or its thread, which closes
/// that connection without a response, or with the failure a finished
/// connection thread returned because its socket could not be configured.
/// Once the flag is set or serving fails, every connection still open is
/// shut down, which wakes a thread waiting to read or write and cuts off a
/// response not yet fully written, though bytes already written stay sent,
/// and every connection thread is joined before this returns. The error
/// returned is the serving failure; otherwise the first failure to shut a
/// connection down, other than `NotConnected`; otherwise the first failure
/// a connection thread returned. Each keeps its kind and names its stage.
pub fn serve_until(
    listener: TcpListener,
    store: Store,
    mumps: MumpsMode,
    shutdown: Arc<AtomicBool>,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let store = Arc::new(store);
    let mumps = Arc::new(mumps);
    let mut workers: Vec<(JoinHandle<io::Result<()>>, TcpStream)> = Vec::new();
    let served = accept_until(
        &listener,
        &store,
        &mumps,
        &shutdown,
        &mut workers,
        MAX_CONNECTIONS,
    );
    let stopped = stop_workers(workers);
    served.and(stopped)
}

/// The accept loop of [`serve_until`]. It polls `shutdown` about every
/// [`POLL`], joins the workers that have finished, so that `workers` holds
/// only open connections, and starts one worker per connection while fewer
/// than `limit` are open. At `limit` it does not accept, so further
/// connections wait in the listener's backlog, untouched, until a worker
/// finishes. `Ok` once the flag is set. `Err` when `accept` fails in a way no
/// retry mends, when a connection cannot be given its worker, or with the
/// first failure of the finished workers, once all of them are joined; the
/// workers still open stay in `workers`.
fn accept_until(
    listener: &TcpListener,
    store: &Arc<Store>,
    mumps: &Arc<MumpsMode>,
    shutdown: &AtomicBool,
    workers: &mut Vec<(JoinHandle<io::Result<()>>, TcpStream)>,
    limit: usize,
) -> io::Result<()> {
    while !shutdown.load(Ordering::SeqCst) {
        let mut joined = Ok(());
        for (finished, _) in workers.extract_if(.., |(handle, _)| handle.is_finished()) {
            joined = joined.and(join_worker(finished));
        }
        joined?;
        if workers.len() >= limit {
            thread::sleep(POLL);
            continue;
        }
        match listener.accept() {
            Ok((stream, _)) => workers.push(start_worker(stream, store, mumps)?),
            Err(e) => {
                let delay = accept_retry_delay(e)?;
                if !delay.is_zero() {
                    thread::sleep(delay);
                }
            }
        }
    }
    Ok(())
}

/// Linux `errno` values with no `io::ErrorKind` of their own that `accept`
/// reports for one pending connection that failed before it was taken, the
/// listener staying sound (accept(2), "Error handling"): ENONET, EPROTO,
/// ENOPROTOOPT, EOPNOTSUPP and EHOSTDOWN.
const PENDING_CONNECTION_ERRNOS: [i32; 5] = [64, 71, 92, 95, 112];

/// How long the accept loop waits after `accept` failed with `e` before it
/// tries again: [`POLL`] when no connection is pending, and no time when one
/// pending connection failed but the listener is sound. Any other failure
/// ends serving, with an error that keeps `e`'s kind and names the stage.
fn accept_retry_delay(e: io::Error) -> io::Result<Duration> {
    match e.kind() {
        io::ErrorKind::WouldBlock => return Ok(POLL),
        io::ErrorKind::Interrupted
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::NetworkDown
        | io::ErrorKind::NetworkUnreachable
        | io::ErrorKind::HostUnreachable => return Ok(Duration::ZERO),
        _ => {}
    }
    if e.raw_os_error()
        .is_some_and(|code| PENDING_CONNECTION_ERRNOS.contains(&code))
    {
        return Ok(Duration::ZERO);
    }
    Err(io::Error::new(
        e.kind(),
        format!("accepting a connection failed: {e}"),
    ))
}

/// Starts the thread that answers `stream`, and returns it with a clone of
/// the socket, which [`stop_workers`] uses to wake it. When the clone or the
/// thread cannot be had, there is no worker: the connection is closed
/// without a response, and the failure is returned, naming that stage.
fn start_worker(
    stream: TcpStream,
    store: &Arc<Store>,
    mumps: &Arc<MumpsMode>,
) -> io::Result<(JoinHandle<io::Result<()>>, TcpStream)> {
    let control = stream
        .try_clone()
        .map_err(|e| abandon(&stream, "handling a connection", e))?;
    let store = Arc::clone(store);
    let mumps = Arc::clone(mumps);
    let spawned = thread::Builder::new()
        .name("bridge-connection".to_string())
        .spawn(move || handle_connection(stream, &store, &mumps));
    match spawned {
        Ok(handle) => Ok((handle, control)),
        // The failed spawn has dropped `stream`; the clone still holds the
        // socket.
        Err(e) => Err(abandon(&control, "handling a connection", e)),
    }
}

/// Closes a connection the bridge cannot serve, writing nothing to it: each
/// response status names an outcome of a request, and none names a failure
/// of the bridge's own resources, so the client sees a closed connection.
/// Returns the error that ends serving, which keeps `cause`'s kind and
/// names `stage`.
fn abandon(stream: &TcpStream, stage: &str, cause: io::Error) -> io::Error {
    // `NotConnected`, a peer that has already reset the connection, is the
    // only way this fails, and leaves nothing to close.
    let _ = stream.shutdown(Shutdown::Both);
    io::Error::new(cause.kind(), format!("{stage} failed: {cause}"))
}

/// Joins a connection thread and gives the failure it returned. A thread
/// that panicked has already been reported by the panic hook, and its
/// connection closed with it, so it adds no failure.
fn join_worker(handle: JoinHandle<io::Result<()>>) -> io::Result<()> {
    handle.join().unwrap_or(Ok(()))
}

/// Shuts down every connection of `workers`, which wakes a thread waiting
/// to read from or write to it, then joins every thread. Once every thread
/// is joined, the first failure is returned: a shutdown failure other than
/// `NotConnected`, otherwise a worker's own, in the order of `workers`.
fn stop_workers(workers: Vec<(JoinHandle<io::Result<()>>, TcpStream)>) -> io::Result<()> {
    let mut failure: Option<io::Error> = None;
    for (_, socket) in &workers {
        match socket.shutdown(Shutdown::Both) {
            Ok(()) => {}
            // The peer has already reset the connection: nothing is left to
            // wake.
            Err(e) if e.kind() == io::ErrorKind::NotConnected => {}
            Err(e) => {
                if failure.is_none() {
                    failure = Some(io::Error::new(
                        e.kind(),
                        format!("closing a connection at shutdown failed: {e}"),
                    ));
                }
            }
        }
    }
    let mut stopped = failure.map_or(Ok(()), Err);
    for (handle, _) in workers {
        stopped = stopped.and(join_worker(handle));
    }
    stopped
}

/// Reads one request from `stream`, answers it and closes the connection.
/// Every exit shuts the socket down: [`serve_until`] holds a clone of it
/// until it joins this thread, so dropping `stream` alone would not close
/// the connection. The result is that of [`exchange`].
fn handle_connection(mut stream: TcpStream, store: &Store, mumps: &MumpsMode) -> io::Result<()> {
    let exchanged = exchange(&mut stream, store, mumps);
    // `NotConnected`, a peer that has already reset the connection, is the
    // only way this fails, and leaves nothing to close.
    let _ = stream.shutdown(Shutdown::Both);
    exchanged
}

/// The one request and response of [`handle_connection`]. A connection that
/// cannot be made blocking with [`IO_TIMEOUT`] for writes, or whose read
/// timeout cannot be set, is closed without a response, and that failure is
/// returned, naming the stage. Every other outcome is `Ok`. The answer to a
/// `HEAD` request, accepted or rejected, carries no content; its method is
/// read after any empty-line prelude (see [`skip_prelude`]).
fn exchange(stream: &mut TcpStream, store: &Store, mumps: &MumpsMode) -> io::Result<()> {
    let configured = stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)));
    if let Err(e) = configured {
        return Err(abandon(stream, "configuring a connection", e));
    }
    let mut received: Vec<u8> = Vec::new();
    let response = match read_request(stream, &mut received, IO_TIMEOUT, REQUEST_TIME) {
        Err(e) => return Err(abandon(stream, "configuring a connection", e)),
        Ok(Ok(Some(request))) => route(&request, store, mumps),
        // Nothing to answer: the peer sent nothing, or cannot be written to.
        Ok(Ok(None)) => return Ok(()),
        Ok(Err(cause)) => error_response(400, &cause),
    };
    // Method tokens are case-sensitive, so only `HEAD` itself is a HEAD
    // request, rejected or not.
    let head_request = skip_prelude(&received).starts_with(b"HEAD ");
    // A response that cannot be written means the client has gone, or has
    // not read it within `IO_TIMEOUT`: an outcome of that client, which ends
    // only this connection, shut down by `handle_connection`.
    if write_response(stream, &response, head_request).is_ok() {
        drain(stream, DRAIN_TIME);
    }
    Ok(())
}

/// Ends the write side, then discards what the client still sends, so the
/// close is orderly, and returns how many bytes it discarded. Each read waits
/// only for the time left and takes only the bytes left, so the drain lasts
/// at most `limit`, up to the granularity of the socket's read timer, and
/// discards at most [`DRAIN_BYTES`]. A read interrupted by a signal is
/// retried within those budgets. The end of input, any other read error, or
/// a read timeout that cannot be set ends it sooner; a write side that cannot
/// be shut down ends it before any read.
fn drain(stream: &mut TcpStream, limit: Duration) -> usize {
    if stream.shutdown(Shutdown::Write).is_err() {
        return 0;
    }
    drain_reads(limit, |window, time_left| {
        stream.set_read_timeout(Some(time_left))?;
        Ok(stream.read(window))
    })
}

/// The reads of [`drain`], timed from this call: each one is `read` into a
/// window of at most the bytes left of [`DRAIN_BYTES`], waiting at most the
/// time left of `limit`. The outer `Err` of `read` is a read timeout that
/// cannot be set, and the inner result is the read's own. Returns how many
/// bytes were discarded once a budget is spent, the input ends, the timeout
/// cannot be set, or a read fails other than by `Interrupted`, which is
/// retried.
fn drain_reads(
    limit: Duration,
    mut read: impl FnMut(&mut [u8], Duration) -> io::Result<io::Result<usize>>,
) -> usize {
    let started = Instant::now();
    let mut discarded = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let time_left = limit.saturating_sub(started.elapsed());
        let bytes_left = DRAIN_BYTES.saturating_sub(discarded);
        // A spent budget ends the drain before `read`, whose
        // `set_read_timeout` rejects a zero timeout.
        if time_left.is_zero() || bytes_left == 0 {
            return discarded;
        }
        let take = bytes_left.min(buf.len());
        let Some(window) = buf.get_mut(..take) else {
            return discarded;
        };
        match read(window, time_left) {
            Ok(Ok(0)) | Err(_) => return discarded,
            Ok(Ok(n)) => discarded = discarded.saturating_add(n),
            Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
            Ok(Err(_)) => return discarded,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP/1.1
// ---------------------------------------------------------------------------

/// The prefix of every 400 cause of the request framing, followed by `: `
/// and the fixed name of the rule the request broke. No cause repeats
/// request bytes.
const BAD_REQUEST: &str = "BAD REQUEST";

/// One request, as far as routing needs it: the method, the target, and the
/// body read by `Content-Length`, or `None` when the request carried no
/// `Content-Length`.
type HttpRequest = (String, String, Option<Vec<u8>>);

/// Reads the request line and headers up to the blank line into `received`,
/// then the body by `Content-Length`. Empty lines before the request line
/// are read with the head and ignored (see [`skip_prelude`]); they count
/// towards [`MAX_HEAD`] like every other byte of the head. `received` keeps
/// every byte read while the head arrived, so the caller can tell a rejected
/// request's method. The connection may stay silent for `idle` before its
/// first byte. From that byte, a prelude byte included, the whole request,
/// body included, must arrive within `request_time`, one deadline that later
/// bytes do not renew.
///
/// `Ok(Ok(None))` means there is nothing to answer: no byte arrived before
/// the end of input, a read error or `idle`, or the interim `100 Continue`,
/// sent only to an HTTP/1.1 request that expects it (see [`parse_head`]),
/// could not be written. `Ok(Err)` carries the 400 cause. `Err` is a read
/// timeout that cannot be set. Chunked bodies are not supported.
fn read_request(
    stream: &mut TcpStream,
    received: &mut Vec<u8>,
    idle: Duration,
    request_time: Duration,
) -> io::Result<Result<Option<HttpRequest>, String>> {
    let bad = |cause: &str| format!("{BAD_REQUEST}: {cause}");
    let mut buf = [0u8; 4096];
    // The idle clock until the first byte arrives, then the request's.
    let mut started = Instant::now();
    let head_end = loop {
        if let Some(end) = find_head_end(received) {
            break end;
        }
        if received.len() > MAX_HEAD {
            return Ok(Err(bad("HEAD TOO LARGE")));
        }
        let budget = if received.is_empty() {
            idle
        } else {
            request_time
        };
        match read_within(stream, &mut buf, started, budget)? {
            // No byte arrived, so there is no request: a browser's idle
            // spare connection ends here unanswered.
            Ok(0) if received.is_empty() => return Ok(Ok(None)),
            Ok(0) => return Ok(Err(bad("INCOMPLETE HEAD"))),
            Ok(n) => {
                if received.is_empty() {
                    started = Instant::now();
                }
                received.extend_from_slice(buf.get(..n).unwrap_or(&[]));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) if received.is_empty() => return Ok(Ok(None)),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(Err(bad("READ TIMEOUT")));
            }
            Err(_) => return Ok(Err(bad("INCOMPLETE HEAD"))),
        }
    };
    if head_end > MAX_HEAD {
        return Ok(Err(bad("HEAD TOO LARGE")));
    }
    let Some((head, rest)) = received.split_at_checked(head_end) else {
        return Ok(Err(bad("INCOMPLETE HEAD")));
    };
    let (method, target, content_length, expect_continue) = match parse_head(head) {
        Ok(parsed) => parsed,
        Err(cause) => return Ok(Err(cause)),
    };
    let body = match content_length {
        None => None,
        Some(length) if length > MAX_BODY => return Ok(Err(bad("BODY TOO LARGE"))),
        Some(length) => {
            let mut body = rest.to_vec();
            // A client that cannot be written to cannot be answered either.
            if expect_continue
                && body.len() < length
                && stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err()
            {
                return Ok(Ok(None));
            }
            while body.len() < length {
                match read_within(stream, &mut buf, started, request_time)? {
                    Ok(0) => return Ok(Err(bad("INCOMPLETE BODY"))),
                    Ok(n) => body.extend_from_slice(buf.get(..n).unwrap_or(&[])),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => return Ok(Err(bad("INCOMPLETE BODY"))),
                }
            }
            // Bytes past the declared length (a pipelined request) are not
            // part of this request and are not answered.
            body.truncate(length);
            Some(body)
        }
    };
    Ok(Ok(Some((method.to_string(), target, body))))
}

/// One read of `stream` into `buf` that waits at most what is left of
/// `budget` since `started`. The inner result is the read's; a budget
/// already spent gives `TimedOut` without reading, and a read that waits it
/// out gives `WouldBlock` or `TimedOut`. The outer `Err` is a read timeout
/// that cannot be set.
fn read_within(
    stream: &mut TcpStream,
    buf: &mut [u8],
    started: Instant,
    budget: Duration,
) -> io::Result<io::Result<usize>> {
    let left = budget.saturating_sub(started.elapsed());
    // `set_read_timeout` rejects a zero timeout, so a spent budget ends here.
    if left.is_zero() {
        return Ok(Err(io::ErrorKind::TimedOut.into()));
    }
    stream.set_read_timeout(Some(left))?;
    Ok(stream.read(buf))
}

/// The offset in `received` just past the first `\r\n\r\n` after the
/// empty-line prelude (see [`skip_prelude`]), if the head is complete. The
/// offset counts the prelude.
fn find_head_end(received: &[u8]) -> Option<usize> {
    let request = skip_prelude(received);
    let prelude = received.len().saturating_sub(request.len());
    request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| prelude.saturating_add(start).saturating_add(4))
}

/// `bytes` without the empty lines, each a bare `\r\n`, before the request
/// line, which a server ignores (RFC 9112 2.2).
fn skip_prelude(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    while let Some(after) = rest.strip_prefix(b"\r\n") {
        rest = after;
    }
    rest
}

/// Parses `head`, which ends with the blank line, into the request line and
/// the headers routing needs: the method, the target in origin-form (see
/// [`origin_form`]), the `Content-Length` and whether `Expect: 100-continue`
/// was sent; an HTTP/1.0 request's expectation counts as not sent
/// (RFC 9110 10.1.1). Empty lines before the request line are ignored (see
/// [`skip_prelude`]). Every header value, those of ignored headers included,
/// must be free of control bytes other than HTAB (see [`is_field_value`]).
/// A request carries at most one `Host`, whose value must be a host (see
/// [`is_host`]), and an HTTP/1.1 request must carry one (RFC 9112 3.2).
/// Errors are 400 causes, each naming the rule the head broke: the request
/// line's first, then each header line's in order, then a missing `Host`.
fn parse_head(head: &[u8]) -> Result<(&str, String, Option<usize>, bool), String> {
    let bad = |cause: &str| format!("{BAD_REQUEST}: {cause}");
    let malformed_line = || bad("MALFORMED REQUEST LINE");
    let text = std::str::from_utf8(skip_prelude(head)).map_err(|_| bad("HEAD NOT UTF-8"))?;
    let text = text
        .strip_suffix("\r\n\r\n")
        .ok_or_else(|| bad("INCOMPLETE HEAD"))?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or_else(malformed_line)?;
    // Exactly three parts, each separated by one space and none empty.
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(malformed_line());
    };
    if !is_token(method) || target.is_empty() || version.is_empty() {
        return Err(malformed_line());
    }
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(bad("UNSUPPORTED HTTP VERSION"));
    }
    let target = origin_form(target).ok_or_else(|| bad("INVALID REQUEST TARGET"))?;
    let mut content_length: Option<usize> = None;
    let mut expect_continue = false;
    let mut host_seen = false;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| bad("MALFORMED HEADER"))?;
        // A name must be a token; this also rejects obsolete line folding.
        if !is_token(name) {
            return Err(bad("MALFORMED HEADER"));
        }
        let value = value.trim_matches(|c| c == ' ' || c == '\t');
        if !is_field_value(value) {
            return Err(bad("INVALID HEADER VALUE"));
        }
        if name.eq_ignore_ascii_case("host") {
            if host_seen {
                return Err(bad("REPEATED HOST"));
            }
            if !is_host(value) {
                return Err(bad("INVALID HOST"));
            }
            host_seen = true;
        } else if name.eq_ignore_ascii_case("content-length") {
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
            // An HTTP/1.0 client may take an interim response for the final
            // one, so its expectation is ignored (RFC 9110 10.1.1).
            expect_continue = version == "HTTP/1.1";
        }
    }
    if version == "HTTP/1.1" && !host_seen {
        return Err(bad("MISSING HOST"));
    }
    Ok((method, target, content_length, expect_continue))
}

/// The origin-form of a request target (RFC 9112 3.2): an origin-form
/// target as sent, or an `http` absolute-form target reduced to its path
/// and query, an empty path becoming `/` (RFC 9112 3.2.2). The scheme is
/// matched without regard to case. The authority ends at the first `/`, `?`
/// or `#` (RFC 3986 3.2) and takes no part in routing, but it must not be
/// empty (RFC 9110 4.2.1) or carry userinfo (RFC 9110 4.2.4). `None` for
/// any other target, such as `*` or another scheme.
fn origin_form(target: &str) -> Option<String> {
    if target.starts_with('/') {
        return Some(target.to_string());
    }
    let (scheme, rest) = target.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path_and_query) = rest.split_at_checked(authority_end)?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if path_and_query.starts_with('/') {
        Some(path_and_query.to_string())
    } else {
        Some(format!("/{path_and_query}"))
    }
}

/// An HTTP token: one or more `tchar`s (RFC 9110 5.6.2).
fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A header value, stripped of its surrounding whitespace, that holds no
/// control byte other than HTAB: only SP, HTAB, visible ASCII and bytes from
/// 0x80 up (RFC 9110 5.5).
fn is_field_value(value: &str) -> bool {
    value.bytes().all(|b| b == b'\t' || !b.is_ascii_control())
}

/// A `Host` value, `uri-host [ ":" port ]` (RFC 9110 7.2), whose host is not
/// empty (RFC 9110 4.2.1). The host is an IPv6 address or an IPvFuture
/// between brackets, or a reg-name, which also covers an IPv4 address
/// (RFC 3986 3.2.2); the port is digits, possibly none (RFC 3986 3.2.3).
/// Any host of this form is accepted, since the value takes no part in
/// routing.
fn is_host(value: &str) -> bool {
    let (host_valid, port) = if let Some(literal) = value.strip_prefix('[') {
        let Some((address, after)) = literal.split_once(']') else {
            return false;
        };
        let port = match after.strip_prefix(':') {
            Some(port) => port,
            None if after.is_empty() => after,
            None => return false,
        };
        (
            address.parse::<Ipv6Addr>().is_ok() || is_ip_future(address),
            port,
        )
    } else {
        let (name, port) = value.split_once(':').unwrap_or((value, ""));
        (!name.is_empty() && is_reg_name(name), port)
    };
    host_valid && port.bytes().all(|b| b.is_ascii_digit())
}

/// An IPvFuture address: `v` in either case, one or more hex digits, `.`,
/// then one or more unreserved, sub-delims or `:` characters (RFC 3986
/// 3.2.2).
fn is_ip_future(address: &str) -> bool {
    let Some((version, rest)) = address
        .strip_prefix(['v', 'V'])
        .and_then(|after| after.split_once('.'))
    else {
        return false;
    };
    !version.is_empty()
        && version.bytes().all(|b| b.is_ascii_hexdigit())
        && !rest.is_empty()
        && rest
            .bytes()
            .all(|b| b == b':' || is_unreserved_or_sub_delim(b))
}

/// A reg-name: unreserved characters, sub-delims and percent-encoded octets,
/// each `%` followed by two hex digits (RFC 3986 3.2.2).
fn is_reg_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    while let Some(b) = bytes.next() {
        let valid = if b == b'%' {
            matches!(
                (bytes.next(), bytes.next()),
                (Some(high), Some(low)) if high.is_ascii_hexdigit() && low.is_ascii_hexdigit()
            )
        } else {
            is_unreserved_or_sub_delim(b)
        };
        if !valid {
            return false;
        }
    }
    true
}

/// An unreserved character or a sub-delim (RFC 3986 2.2, 2.3).
fn is_unreserved_or_sub_delim(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=".contains(&b)
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

/// The headers every response carries besides its type, length and close:
/// no content-type sniffing, no stored copy of a response, and no rendering
/// inside a frame of another page.
const HARDENING_HEADERS: &str = "X-Content-Type-Options: nosniff\r\n\
    Cache-Control: no-store\r\n\
    X-Frame-Options: DENY\r\n\
    Content-Security-Policy: frame-ancestors 'none'\r\n";

/// Writes `response`, `(status, content type, body)`, with
/// [`HARDENING_HEADERS`]; every response closes its connection. The answer
/// to a `HEAD` request, `head_request`, carries no content (RFC 9110 9.3.2),
/// but its `Content-Length` is still that of the body a `GET` would get
/// (RFC 9110 8.6).
fn write_response(
    stream: &mut impl Write,
    response: &(u16, &'static str, Vec<u8>),
    head_request: bool,
) -> io::Result<()> {
    let (status, content_type, body) = response;
    let reason = reason_phrase(*status);
    let length = body.len();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {length}\r\n{HARDENING_HEADERS}Connection: close\r\n\r\n"
    );
    stream.write_all(head.as_bytes())?;
    if !head_request {
        stream.write_all(body)?;
    }
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
/// API path, or 404. A `HEAD` request is answered as a `GET` of the same
/// target would be, so its headers, `Content-Length` included, are those of
/// the `GET` (RFC 9110 9.3.2): a static path serves its file, and an API
/// path, which serves only `POST`, gives the 404 of any other method.
/// [`write_response`] leaves the content out.
fn route(request: &HttpRequest, store: &Store, mumps: &MumpsMode) -> (u16, &'static str, Vec<u8>) {
    let (method, target, body) = request;
    if let Some((file, content_type)) = static_file(target) {
        if method == "GET" || method == "HEAD" {
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

fn peek(text: &str, pos: usize) -> Option<u8> {
    text.as_bytes().get(pos).copied()
}

fn bump(text: &str, pos: &mut usize) -> Option<u8> {
    let byte = peek(text, *pos)?;
    *pos = pos.saturating_add(1);
    Some(byte)
}

fn eat(text: &str, pos: &mut usize, byte: u8) -> Option<()> {
    if bump(text, pos) == Some(byte) {
        Some(())
    } else {
        None
    }
}

fn eat_literal(text: &str, pos: &mut usize, literal: &str) -> Option<()> {
    literal.bytes().try_for_each(|byte| eat(text, pos, byte))
}

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

fn skip_digits(text: &str, pos: &mut usize) {
    while matches!(peek(text, *pos), Some(b'0'..=b'9')) {
        *pos = pos.saturating_add(1);
    }
}

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
/// used whole or not at all. A directory whose input file is missing is
/// skipped silently; one whose input file is present but unreadable (a
/// directory in its place included) or not UTF-8 does not match either, and
/// its path is listed in the `NOT RECORDED` error when nothing matches.
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
/// one `\n`, the form of a captured input file. Once the context has
/// passed, the run takes one of the [`LIVE_SLOTS`] that every connection
/// shares; a request that gets none within [`LIVE_ADMISSION_WAIT`] starts
/// nothing and fails as `TIMEOUT`. Output is used only when the run
/// succeeded, and the return value is unavailable.
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
    let transcript = run_admitted(
        &LIVE_ADMISSION,
        LIVE_ADMISSION_WAIT,
        "docker",
        &args,
        stdin.as_bytes(),
    )
    .map_err(|failure| failure.to_string())?;
    Ok((transcript, None, Vec::new()))
}

/// [`run_command`] of `program` with the live limits, decided by
/// [`decide_live_result`], once one of `slots` is free. The slot is held
/// until the run is over, and freed as well when the run unwinds. When no
/// slot frees within `wait`, nothing starts.
fn run_admitted(
    slots: &[Mutex<()>],
    wait: Duration,
    program: &str,
    args: &[String],
    stdin: &[u8],
) -> Result<String, LiveFailure> {
    let Some(slot) = admit(slots, wait) else {
        // The request could not complete within its time budget. A live
        // failure reports only the fixed 502 causes, and this is the one
        // that names it, so there is no status or cause of its own.
        return Err(LiveFailure::Timeout);
    };
    let result = run_command(program, args, stdin, LIVE_TIMEOUT, LIVE_OUTPUT_LIMIT);
    drop(slot);
    decide_live_result(&result)
}

/// The lock of the first of `slots` found free, or `None` when none frees
/// within `wait`, counted in all rather than per attempt. A busy slot is
/// tried again every [`POLL`]. A slot poisoned by a run that panicked is
/// free once that run has unwound, so its lock is taken all the same.
fn admit(slots: &[Mutex<()>], wait: Duration) -> Option<MutexGuard<'_, ()>> {
    let started = Instant::now();
    loop {
        for slot in slots {
            match slot.try_lock() {
                Ok(guard) => return Some(guard),
                Err(TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        let left = wait.saturating_sub(started.elapsed());
        if left.is_zero() {
            return None;
        }
        thread::sleep(left.min(POLL));
    }
}

/// The parsed meta file of the first capture directory that has one. A case
/// with no meta file is skipped, and so is a case entry that is not a
/// directory, which can hold none. A meta file that cannot be read for any
/// other reason, is not UTF-8 or does not parse gives `MALFORMED META`
/// naming it, never a skip. With none present, the error names where the
/// first would be.
fn live_meta(golden_dir: &Path) -> Result<Meta, String> {
    for case in CASE_DIRS {
        let dir = golden_dir.join(case);
        let path = dir.join(META_FILE);
        let malformed = || format!("MALFORMED META {}", path.display());
        match read_capture_file(&path) {
            Err(CAPTURE_MISSING) => {}
            // Opening a file below a case entry that is not a directory
            // fails without the file being missing. Only an entry known
            // not to be a directory is skipped; a failed check fails closed.
            Err(CAPTURE_UNREADABLE) if fs::metadata(&dir).is_ok_and(|entry| !entry.is_dir()) => {}
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
    /// The OS error text when the program could not be started; otherwise,
    /// when the run itself failed (a stream worker that could not start,
    /// failed or panicked, output not read to its end, or a child that could
    /// not be waited on or killed), each failure's stage and cause, joined
    /// by `; `. A run with this set is never a transcript, whatever the
    /// other fields hold.
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
/// on its own thread, so a full stream cannot deadlock. The child is killed
/// when `timeout` passes before it has exited and its output has been read,
/// or as soon as standard output passes `output_limit` bytes; at most
/// `output_limit` bytes of each stream are kept. Once the run is over, the
/// stream threads are cancelled, which stops each within about `POLL` even
/// while a descendant still holds the child's streams, and every thread
/// that started is joined before this returns. A failure of the run itself
/// rather than of the program, such as a thread that could not start or
/// output that could not be read, is reported in
/// [`ExecResult::spawn_error`].
pub fn run_command(
    program: &str,
    args: &[String],
    stdin: &[u8],
    timeout: Duration,
    output_limit: usize,
) -> ExecResult {
    match execute(
        program,
        args,
        stdin,
        timeout,
        output_limit,
        &mut start_thread,
    ) {
        Ok(result) => result,
        Err(e) => ExecResult::spawn_failure(e.to_string()),
    }
}

/// [`run_command`] with its stream threads started through `start`. `Err`,
/// with the OS error, only when the child's streams could not be created or
/// the program could not be started.
fn execute(
    program: &str,
    args: &[String],
    stdin: &[u8],
    timeout: Duration,
    output_limit: usize,
    start: &mut StartWorker,
) -> io::Result<ExecResult> {
    let started = Instant::now();
    let (streams, (child_stdin, child_stdout, child_stderr)) = stream_pairs()?;
    // One statement, so the parent's copies of the child's ends are closed
    // as soon as the child holds them; end of output depends on it.
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::from(OwnedFd::from(child_stdin)))
        .stdout(Stdio::from(OwnedFd::from(child_stdout)))
        .stderr(Stdio::from(OwnedFd::from(child_stderr)))
        .spawn()?;
    Ok(supervise(
        &mut child,
        streams,
        stdin,
        timeout,
        output_limit,
        started,
        start,
    ))
}

/// One end of each of the three connections that carry a child's standard
/// input, output and error, in that order.
type StdStreams = (UnixStream, UnixStream, UnixStream);

/// Creates the child's standard streams: the parent's ends, then the
/// child's. They are socket pairs rather than pipes because a socket's read
/// and write timeouts wake a blocked worker every `POLL` to check for
/// cancellation, which a pipe read cannot do.
fn stream_pairs() -> io::Result<(StdStreams, StdStreams)> {
    let (stdin, child_stdin) = UnixStream::pair()?;
    let (stdout, child_stdout) = UnixStream::pair()?;
    let (stderr, child_stderr) = UnixStream::pair()?;
    stdin.set_write_timeout(Some(POLL))?;
    stdout.set_read_timeout(Some(POLL))?;
    stderr.set_read_timeout(Some(POLL))?;
    Ok((
        (stdin, stdout, stderr),
        (child_stdin, child_stdout, child_stderr),
    ))
}

/// What a stream worker returns: the bytes it kept, none for standard
/// input, and how it ended. `Ok(true)` when it carried the stream to its
/// end: end of output read, the output limit reached, or all of standard
/// input written. `Ok(false)` when it stopped first, because the run was
/// over or the child closed its standard input. `Err` holds the text of any
/// other I/O error.
type PipeOutcome = (Vec<u8>, Result<bool, String>);

type Worker = Box<dyn FnOnce() -> PipeOutcome + Send>;

/// Starts a named stream worker on a thread of its own; [`start_thread`]
/// in every real run.
type StartWorker = dyn FnMut(&'static str, Worker) -> io::Result<JoinHandle<PipeOutcome>>;

fn start_thread(name: &'static str, work: Worker) -> io::Result<JoinHandle<PipeOutcome>> {
    thread::Builder::new().name(name.to_string()).spawn(work)
}

/// The stream workers in the order they start: the stage name their
/// diagnostics carry, and whether the worker reads an output stream rather
/// than writing standard input.
const STREAMS: [(&str, bool); 3] = [
    ("STANDARD INPUT", false),
    ("STANDARD OUTPUT", true),
    ("STANDARD ERROR", true),
];

/// Supervises a started child through its parent-side `streams`: starts the
/// stream workers through `start`, polls the child until it is done, kills
/// and reaps it when it is not, then cancels and joins every worker that
/// started. `timeout` counts from `started`.
fn supervise(
    child: &mut Child,
    streams: StdStreams,
    stdin: &[u8],
    timeout: Duration,
    output_limit: usize,
    started: Instant,
    start: &mut StartWorker,
) -> ExecResult {
    let exceeded = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::new();
    let mut failures = Vec::new();
    let begun = start_workers(
        streams,
        stdin,
        output_limit,
        &exceeded,
        &cancel,
        start,
        &mut workers,
    );
    let (mut status, timed_out, mut output_limit_exceeded) = match begun {
        Ok(()) => watch(
            &mut || child.try_wait(),
            &workers,
            &exceeded,
            timeout,
            started,
            &mut failures,
        ),
        Err(e) => {
            failures.push(format!("PIPE WORKER NOT STARTED: {e}"));
            (None, false, false)
        }
    };
    if status.is_none() || output_limit_exceeded {
        status = kill_and_reap(child, &mut failures).or(status);
    }
    cancel.store(true, Ordering::SeqCst);
    let joined = workers.into_iter().map(JoinHandle::join).collect();
    // The reader sets the flag before it ends, so once it is joined this
    // also catches a limit passed just as the child exited.
    output_limit_exceeded |= exceeded.load(Ordering::SeqCst);
    exec_result(status, timed_out, output_limit_exceeded, failures, joined)
}

/// Polls the child through `try_wait` every `POLL` until it has exited and
/// both output readers in `workers` have finished, standard output passes
/// its limit, `timeout` passes since `started`, or the child cannot be
/// waited on, which is recorded in `failures`. Gives the exit status seen,
/// whether the run timed out and whether standard output passed its limit.
/// A poll that finds the limit passed once `timeout` has passed too reports
/// both, so [`decide_live_result`] gives the timeout precedence.
fn watch(
    try_wait: &mut dyn FnMut() -> io::Result<Option<ExitStatus>>,
    workers: &[JoinHandle<PipeOutcome>],
    exceeded: &AtomicBool,
    timeout: Duration,
    started: Instant,
    failures: &mut Vec<String>,
) -> (Option<ExitStatus>, bool, bool) {
    let mut status = None;
    loop {
        if status.is_none() {
            match try_wait() {
                Ok(exited) => status = exited,
                Err(e) => {
                    failures.push(format!("WAIT FAILED: {e}"));
                    return (status, false, false);
                }
            }
        }
        if exceeded.load(Ordering::SeqCst) {
            return (status, started.elapsed() >= timeout, true);
        }
        if status.is_some() && readers_finished(workers) {
            return (status, false, false);
        }
        if started.elapsed() >= timeout {
            return (status, true, false);
        }
        thread::sleep(POLL);
    }
}

/// Whether every worker of [`STREAMS`] was started into `workers`, in that
/// order, and each output reader among them has finished.
fn readers_finished(workers: &[JoinHandle<PipeOutcome>]) -> bool {
    workers.len() == STREAMS.len()
        && STREAMS
            .iter()
            .zip(workers)
            .all(|((_, is_reader), handle)| !is_reader || handle.is_finished())
}

/// Starts the standard-input writer and the two output readers one at a
/// time, pushing each handle onto `workers` as soon as it exists, so a
/// worker that cannot start leaves the earlier ones to be joined. The
/// standard-output reader sets `exceeded` once more than `output_limit`
/// bytes have arrived. Every worker stops once `cancel` is set.
fn start_workers(
    streams: StdStreams,
    stdin: &[u8],
    output_limit: usize,
    exceeded: &Arc<AtomicBool>,
    cancel: &Arc<AtomicBool>,
    start: &mut StartWorker,
    workers: &mut Vec<JoinHandle<PipeOutcome>>,
) -> io::Result<()> {
    let (input_stream, output_stream, error_stream) = streams;
    let input = stdin.to_vec();
    let stop = Arc::clone(cancel);
    workers.push(start(
        "run-command-stdin",
        Box::new(move || write_input(input_stream, &input, &stop)),
    )?);
    let stop = Arc::clone(cancel);
    let flag = Arc::clone(exceeded);
    workers.push(start(
        "run-command-stdout",
        Box::new(move || read_bounded(output_stream, output_limit, Some(flag.as_ref()), &stop)),
    )?);
    let stop = Arc::clone(cancel);
    workers.push(start(
        "run-command-stderr",
        Box::new(move || read_bounded(error_stream, output_limit, None, &stop)),
    )?);
    Ok(())
}

/// Writes `input` to the child's standard input, then closes it by dropping
/// `stream`, so the child sees end of input. A child that closes its input
/// first refuses the rest, `Ok(false)`, which is not a failure of the run.
/// The writer stops, also `Ok(false)`, once `cancel` is set, checked before
/// each write and after each write that timed out.
fn write_input(mut stream: impl Write, input: &[u8], cancel: &AtomicBool) -> PipeOutcome {
    let mut rest = input;
    let end = loop {
        if rest.is_empty() {
            break Ok(true);
        }
        if cancel.load(Ordering::SeqCst) {
            break Ok(false);
        }
        match stream.write(rest) {
            Ok(0) => break Err(io::Error::from(io::ErrorKind::WriteZero).to_string()),
            Ok(n) => rest = rest.get(n..).unwrap_or(&[]),
            Err(e) => match e.kind() {
                io::ErrorKind::Interrupted
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::TimedOut => {}
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset => {
                    break Ok(false);
                }
                _ => break Err(e.to_string()),
            },
        }
    };
    (Vec::new(), end)
}

/// Reads `stream` to its end, keeping at most `limit` bytes. Past the limit,
/// a reader with `exceeded` sets it and stops; one without keeps draining
/// and discarding, so the child never blocks on that stream. The reader
/// stops, `Ok(false)`, once `cancel` is set, checked before each read and
/// after each read that timed out.
fn read_bounded(
    mut stream: impl Read,
    limit: usize,
    exceeded: Option<&AtomicBool>,
    cancel: &AtomicBool,
) -> PipeOutcome {
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    let end = loop {
        if cancel.load(Ordering::SeqCst) {
            break Ok(false);
        }
        match stream.read(&mut buf) {
            Ok(0) => break Ok(true),
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
                    break Ok(true);
                }
            }
            Err(e) => match e.kind() {
                io::ErrorKind::Interrupted
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::TimedOut => {}
                _ => break Err(e.to_string()),
            },
        }
    };
    (kept, end)
}

/// Kills the child and reaps it, recording what fails in `failures`;
/// `None` when it was not reaped.
fn kill_and_reap(child: &mut Child, failures: &mut Vec<String>) -> Option<ExitStatus> {
    let killed = child.kill();
    reap(
        killed,
        |block| {
            if block {
                child.wait().map(Some)
            } else {
                child.try_wait()
            }
        },
        failures,
    )
}

/// Reaps a child that `killed`, the result of killing it, says was or was
/// not killed, through `wait`, which blocks until the child has exited when
/// given `true` and only polls it when given `false`. What fails is
/// recorded in `failures`; `None` when the child was not reaped. Killing a
/// child that has already exited succeeds, so after a failed kill the child
/// may still be running: it is then polled once, never waited on, so the
/// run cannot hang on it.
fn reap(
    killed: io::Result<()>,
    wait: impl FnOnce(bool) -> io::Result<Option<ExitStatus>>,
    failures: &mut Vec<String>,
) -> Option<ExitStatus> {
    let block = match killed {
        Ok(()) => true,
        Err(e) => {
            failures.push(format!("KILL FAILED: {e}"));
            false
        }
    };
    match wait(block) {
        Ok(status) => status,
        Err(e) => {
            failures.push(format!("WAIT FAILED: {e}"));
            None
        }
    }
}

/// The run's result from what supervision observed and what each worker
/// that started returned, in [`STREAMS`] order. Any failure of the run,
/// recorded during supervision in `failures` or found here, becomes
/// `spawn_error`, so the run never gives a transcript.
fn exec_result(
    status: Option<ExitStatus>,
    timed_out: bool,
    output_limit_exceeded: bool,
    mut failures: Vec<String>,
    joined: Vec<thread::Result<PipeOutcome>>,
) -> ExecResult {
    let explained = timed_out || output_limit_exceeded;
    let mut outputs = Vec::new();
    for ((name, is_reader), result) in STREAMS.into_iter().zip(joined) {
        let kept = worker_output(name, is_reader, result, explained, &mut failures);
        if is_reader {
            outputs.push(kept);
        }
    }
    // A reader that never started kept nothing; its start failure is
    // already in `failures`, so the run is not a transcript.
    let mut outputs = outputs.into_iter();
    let stdout = outputs.next().unwrap_or_default();
    let stderr = outputs.next().unwrap_or_default();
    ExecResult {
        spawn_error: (!failures.is_empty()).then(|| failures.join("; ")),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        output_limit_exceeded,
        stdout,
        stderr,
    }
}

/// The bytes a joined worker kept. A panic or an I/O failure is recorded in
/// `failures` under the worker's stage `name`, and so is a reader stopped
/// before the end of its stream, unless `explained` (a timeout or the
/// output limit) or an earlier failure already accounts for it. A writer
/// stopped first is no failure: input it did not write is input the child
/// never read, as when the child leaves input over.
fn worker_output(
    name: &str,
    is_reader: bool,
    joined: thread::Result<PipeOutcome>,
    explained: bool,
    failures: &mut Vec<String>,
) -> Vec<u8> {
    let (worker, action) = if is_reader {
        ("READER", "READ")
    } else {
        ("WRITER", "WRITE")
    };
    let (kept, end) = match joined {
        Ok(outcome) => outcome,
        Err(_) => {
            failures.push(format!("{name} {worker} PANICKED"));
            return Vec::new();
        }
    };
    match end {
        Err(e) => failures.push(format!("{name} {action} FAILED: {e}")),
        Ok(false) if is_reader && !explained && failures.is_empty() => {
            failures.push(format!("{name} NOT READ TO ITS END"));
        }
        Ok(_) => {}
    }
    kept
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
    use std::any::Any;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::AtomicUsize;

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
        // 251 bytes of standard error: 199 ASCII bytes, then `é` (0xC3 0xA9)
        // as bytes 200 and 201. The 200-byte cut keeps only 0xC3, which
        // decodes lossily to U+FFFD; a 200-character cut would keep the
        // whole `é`.
        let mut crossing = vec![b'a'; 199];
        crossing.extend("é".bytes());
        crossing.extend(std::iter::repeat_n(b'z', 50));
        assert_eq!(crossing.get(199..201), Some([0xC3, 0xA9].as_slice()));
        // 250 bytes of standard error, the first not UTF-8.
        let mut invalid_first = vec![0xFF];
        invalid_first.extend(std::iter::repeat_n(b'e', 249));
        let stderrs = [
            (crossing, format!("{}\u{fffd}", "a".repeat(199))),
            (invalid_first, format!("\u{fffd}{}", "e".repeat(199))),
            (
                b"no such container".to_vec(),
                "no such container".to_string(),
            ),
        ];
        // Standard output is not UTF-8 either: the exit code is decided
        // first, so every row is `NonZeroExit`, never `NonUtf8`.
        for (code, label) in [(Some(3), "3"), (None, "NONE"), (Some(1), "1")] {
            for (number, (stderr, head)) in stderrs.iter().enumerate() {
                let result = ExecResult {
                    spawn_error: None,
                    exit_code: code,
                    timed_out: false,
                    output_limit_exceeded: false,
                    stdout: vec![b'T', 0xFF],
                    stderr: stderr.clone(),
                };
                let failure = decide_live_result(&result);
                assert_eq!(
                    failure,
                    Err(LiveFailure::NonZeroExit {
                        code,
                        stderr_head: head.clone(),
                    }),
                    "exit {label}, stderr {number}"
                );
                assert_eq!(
                    failure.map_err(|f| f.to_string()),
                    Err(format!("EXIT {label}: {head}")),
                    "exit {label}, stderr {number}"
                );
            }
        }
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

    /// Runs `program` as [`run_command`] does. `Ok(None)` when the program
    /// is absent, which is reported as NOT RUN; any other failure to start
    /// it, or a failure of the run itself, is an error naming its cause.
    fn run_available(
        program: &str,
        args: &[String],
        stdin: &[u8],
        timeout: Duration,
        output_limit: usize,
    ) -> Result<Option<ExecResult>, String> {
        match execute(
            program,
            args,
            stdin,
            timeout,
            output_limit,
            &mut start_thread,
        ) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                println!("NOT RUN: {program} not found");
                Ok(None)
            }
            Err(e) => Err(format!("{program} could not be started: {e}")),
            Ok(result) => match &result.spawn_error {
                Some(diagnostic) => Err(format!("{program} run failed: {diagnostic}")),
                None => Ok(Some(result)),
            },
        }
    }

    #[test]
    fn run_command_kills_on_timeout() -> Result<(), String> {
        let started = Instant::now();
        let run = run_available(
            "sleep",
            &strings(&["5"]),
            b"",
            Duration::from_millis(200),
            4096,
        )?;
        let Some(result) = run else {
            return Ok(());
        };
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
        let run = run_available("yes", &[], b"", Duration::from_secs(10), 4096)?;
        let Some(result) = run else {
            return Ok(());
        };
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
        let absent = "/nonexistent/ifcap-po-inquiry-no-such-program";
        let timeout = Duration::from_secs(1);
        let missing = run_command(absent, &[], b"", timeout, 4096);
        assert!(missing.spawn_error.is_some(), "spawned a missing program");
        let error = match execute(absent, &[], b"", timeout, 4096, &mut start_thread) {
            Ok(result) => return Err(format!("spawned a missing program: {result:?}")),
            Err(e) => e,
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(missing.spawn_error, Some(error.to_string()));
        // A program that exists but cannot be executed fails; only an
        // absent program is NOT RUN.
        let not_executable = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let refused = run_available(not_executable, &[], b"", timeout, 4096);
        assert!(
            matches!(&refused, Err(cause) if cause.contains("could not be started")),
            "{refused:?}"
        );
        let echoed = run_available("cat", &[], b"\nA1\n^\n", Duration::from_secs(10), 4096)?;
        let Some(echoed) = echoed else {
            return Ok(());
        };
        assert_eq!(decide_live_result(&echoed), Ok("\nA1\n^\n".to_string()));
        let failed = run_available("false", &[], b"", Duration::from_secs(10), 4096)?;
        let Some(failed) = failed else {
            return Ok(());
        };
        assert_eq!(failed.exit_code, Some(1));
        Ok(())
    }

    fn io_text(e: io::Error) -> String {
        e.to_string()
    }

    /// Runs `work` with a cancel flag that another thread sets after
    /// `delay`, then joins that thread.
    fn cancelled_after<T>(
        delay: Duration,
        work: impl FnOnce(&AtomicBool) -> T,
    ) -> Result<T, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let setter = thread::Builder::new()
            .spawn(move || {
                thread::sleep(delay);
                flag.store(true, Ordering::SeqCst);
            })
            .map_err(io_text)?;
        let outcome = work(&cancel);
        setter
            .join()
            .map_err(|_| "the cancelling thread panicked".to_string())?;
        Ok(outcome)
    }

    /// A `read_bounded` case over two chunks read one after the other: the
    /// chunks, the limit and whether the reader has the exceeded flag, then
    /// the bytes kept, whether the flag was set and how many bytes were left
    /// unread.
    type ReadCase = (
        &'static [u8],
        &'static [u8],
        usize,
        bool,
        &'static [u8],
        bool,
        usize,
    );

    #[test]
    fn read_bounded_reports_failure_retry_limit_and_cancel() -> Result<(), String> {
        let cases: Vec<ReadCase> = vec![
            (b"ab", b"cd", 16, true, b"abcd", false, 0),
            (b"abcdef", b"gh", 4, true, b"abcd", true, 2),
            (b"abcdef", b"gh", 4, false, b"abcd", false, 0),
        ];
        let cancel = AtomicBool::new(false);
        for (number, (first, second, limit, flagged, kept, flag_set, left)) in
            cases.into_iter().enumerate()
        {
            let exceeded = AtomicBool::new(false);
            let mut stream = first.chain(second);
            let outcome = read_bounded(&mut stream, limit, flagged.then_some(&exceeded), &cancel);
            assert_eq!(outcome, (kept.to_vec(), Ok(true)), "case {number}");
            assert_eq!(exceeded.load(Ordering::SeqCst), flag_set, "case {number}");
            let (first_rest, second_rest) = stream.get_ref();
            assert_eq!(first_rest.len() + second_rest.len(), left, "case {number}");
        }
        // Reading a file opened only for writing fails, after the bytes
        // before it were kept.
        let write_only = || {
            fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .map_err(io_text)
        };
        let refused = match write_only()?.read(&mut [0u8; 1]) {
            Ok(n) => return Err(format!("read {n} bytes from a write-only file")),
            Err(e) => e.to_string(),
        };
        let failing = b"ab".as_slice().chain(write_only()?);
        assert_eq!(
            read_bounded(failing, 16, None, &cancel),
            (b"ab".to_vec(), Err(refused))
        );
        // A stream held open with nothing more to send times out each read,
        // which is retried until the run is cancelled.
        let ((_, stdout, _), (_, mut peer, _)) = stream_pairs().map_err(io_text)?;
        peer.write_all(b"ab").map_err(io_text)?;
        let outcome = cancelled_after(Duration::from_millis(50), |cancel| {
            read_bounded(stdout, 16, None, cancel)
        })?;
        drop(peer);
        assert_eq!(outcome, (b"ab".to_vec(), Ok(false)));
        Ok(())
    }

    #[test]
    fn write_input_reports_refusal_failure_retry_and_cancel() -> Result<(), String> {
        let cancel = AtomicBool::new(false);
        let mut sink = Vec::new();
        assert_eq!(
            write_input(&mut sink, b"abc", &cancel),
            (Vec::new(), Ok(true))
        );
        assert_eq!(sink, b"abc");
        // Empty input writes nothing, so a file that refuses every write
        // still takes all of it.
        let read_only = || fs::File::open("/dev/null").map_err(io_text);
        assert_eq!(
            write_input(read_only()?, b"", &cancel),
            (Vec::new(), Ok(true))
        );
        let refused = match read_only()?.write(b"x") {
            Ok(n) => return Err(format!("wrote {n} bytes to a read-only file")),
            Err(e) => e.to_string(),
        };
        assert_eq!(
            write_input(read_only()?, b"abc", &cancel),
            (Vec::new(), Err(refused))
        );
        // A stream that takes no more bytes is a failure, not a retry.
        let mut room = [0u8; 2];
        let mut short: &mut [u8] = &mut room;
        let write_zero = io::Error::from(io::ErrorKind::WriteZero).to_string();
        assert_eq!(
            write_input(&mut short, b"abc", &cancel),
            (Vec::new(), Err(write_zero))
        );
        assert_eq!(room, *b"ab");
        // A child that closes its standard input refuses the rest. The input
        // is more than the stream holds, so it is refused even while a child
        // that another test is starting still holds a copy of the closed end.
        let input = vec![b'x'; 4 * 1024 * 1024];
        let ((stdin, _, _), (peer, _, _)) = stream_pairs().map_err(io_text)?;
        drop(peer);
        assert_eq!(write_input(stdin, &input, &cancel), (Vec::new(), Ok(false)));
        // Input the child leaves unread fills the stream, which times out
        // each write; it is retried until the run is cancelled.
        let ((stdin, _, _), (peer, _, _)) = stream_pairs().map_err(io_text)?;
        let outcome = cancelled_after(Duration::from_millis(50), |cancel| {
            write_input(stdin, &input, cancel)
        })?;
        drop(peer);
        assert_eq!(outcome, (Vec::new(), Ok(false)));
        Ok(())
    }

    /// A worker that carried its stream to its end, keeping `kept`.
    fn done(kept: &[u8]) -> thread::Result<PipeOutcome> {
        Ok((kept.to_vec(), Ok(true)))
    }

    /// A worker that stopped before the end of its stream, keeping `kept`.
    fn stopped(kept: &[u8]) -> thread::Result<PipeOutcome> {
        Ok((kept.to_vec(), Ok(false)))
    }

    /// A worker whose stream failed with an injected error, keeping `kept`.
    fn failed(kept: &[u8]) -> thread::Result<PipeOutcome> {
        Ok((kept.to_vec(), Err("injected".to_string())))
    }

    /// A worker that panicked with an injected payload.
    fn panicked() -> thread::Result<PipeOutcome> {
        Err(Box::new("injected") as Box<dyn Any + Send>)
    }

    /// An `exec_result` case: the writer, the two readers, whether the run
    /// timed out and whether standard output passed its limit, then the
    /// failure diagnostic expected.
    type ResultCase = (
        thread::Result<PipeOutcome>,
        thread::Result<PipeOutcome>,
        thread::Result<PipeOutcome>,
        bool,
        bool,
        Option<&'static str>,
    );

    #[test]
    fn exec_result_rejects_failed_panicked_and_unfinished_workers() {
        let cases: Vec<ResultCase> = vec![
            (done(b""), done(b"T"), done(b""), false, false, None),
            (stopped(b""), done(b"T"), done(b""), false, false, None),
            (done(b""), done(b"T"), stopped(b""), false, true, None),
            (
                done(b""),
                failed(b"T"),
                done(b""),
                false,
                false,
                Some("STANDARD OUTPUT READ FAILED: injected"),
            ),
            (
                done(b""),
                panicked(),
                done(b""),
                false,
                false,
                Some("STANDARD OUTPUT READER PANICKED"),
            ),
            (
                panicked(),
                done(b"T"),
                done(b""),
                false,
                false,
                Some("STANDARD INPUT WRITER PANICKED"),
            ),
            (
                done(b""),
                done(b"T"),
                panicked(),
                false,
                false,
                Some("STANDARD ERROR READER PANICKED"),
            ),
            (
                failed(b""),
                done(b"T"),
                done(b""),
                false,
                false,
                Some("STANDARD INPUT WRITE FAILED: injected"),
            ),
            (
                done(b""),
                done(b"T"),
                failed(b""),
                false,
                false,
                Some("STANDARD ERROR READ FAILED: injected"),
            ),
            (
                done(b""),
                stopped(b"T"),
                done(b""),
                false,
                false,
                Some("STANDARD OUTPUT NOT READ TO ITS END"),
            ),
            (
                done(b""),
                done(b"T"),
                stopped(b""),
                false,
                false,
                Some("STANDARD ERROR NOT READ TO ITS END"),
            ),
            (
                failed(b""),
                stopped(b"T"),
                done(b""),
                false,
                false,
                Some("STANDARD INPUT WRITE FAILED: injected"),
            ),
            (
                done(b""),
                panicked(),
                failed(b""),
                false,
                false,
                Some("STANDARD OUTPUT READER PANICKED; STANDARD ERROR READ FAILED: injected"),
            ),
            (done(b""), stopped(b"T"), stopped(b""), true, false, None),
        ];
        for (number, (writer, out_reader, err_reader, timed_out, limited, diagnostic)) in
            cases.into_iter().enumerate()
        {
            let result = exec_result(
                Some(ExitStatus::from_raw(0)),
                timed_out,
                limited,
                Vec::new(),
                vec![writer, out_reader, err_reader],
            );
            assert_eq!(result.spawn_error.as_deref(), diagnostic, "case {number}");
            assert_eq!(result.exit_code, Some(0), "case {number}");
            let expected = match (diagnostic, timed_out, limited) {
                (Some(text), _, _) => Err(LiveFailure::Spawn(text.to_string())),
                (None, true, _) => Err(LiveFailure::Timeout),
                (None, false, true) => Err(LiveFailure::OutputTooLarge),
                (None, false, false) => Ok("T".to_string()),
            };
            assert_eq!(decide_live_result(&result), expected, "case {number}");
        }
    }

    /// The status of a child ended by SIGKILL.
    fn killed() -> ExitStatus {
        ExitStatus::from_raw(9)
    }

    /// A `reap` case: the kill error, if any, and what the wait or poll
    /// answers, then the status, the failures and the calls expected.
    type ReapCase = (
        Option<&'static str>,
        Result<Option<ExitStatus>, &'static str>,
        Option<ExitStatus>,
        &'static [&'static str],
        &'static [&'static str],
    );

    #[test]
    fn reap_records_kill_and_wait_failures() {
        let exited = ExitStatus::from_raw(0);
        let cases: Vec<ReapCase> = vec![
            (None, Ok(Some(killed())), Some(killed()), &[], &["wait"]),
            (None, Ok(Some(exited)), Some(exited), &[], &["wait"]),
            (
                Some("injected kill"),
                Ok(None),
                None,
                &["KILL FAILED: injected kill"],
                &["try_wait"],
            ),
            (
                Some("injected kill"),
                Err("injected try_wait"),
                None,
                &[
                    "KILL FAILED: injected kill",
                    "WAIT FAILED: injected try_wait",
                ],
                &["try_wait"],
            ),
            (
                None,
                Err("injected wait"),
                None,
                &["WAIT FAILED: injected wait"],
                &["wait"],
            ),
        ];
        for (number, (kill_error, answer, status, failures, calls)) in cases.into_iter().enumerate()
        {
            let kill = kill_error.map_or(Ok(()), |message| Err(io::Error::other(message)));
            let mut made = Vec::new();
            let mut recorded = Vec::new();
            let reaped = reap(
                kill,
                |block| {
                    made.push(if block { "wait" } else { "try_wait" });
                    answer.map_err(io::Error::other)
                },
                &mut recorded,
            );
            assert_eq!(reaped, status, "case {number}");
            assert_eq!(recorded, failures, "case {number}");
            assert_eq!(made, calls, "case {number}");
        }
    }

    #[test]
    fn watch_records_a_child_it_cannot_wait_on() {
        let mut failures = Vec::new();
        let watched = watch(
            &mut || Err(io::Error::other("injected try_wait")),
            &[],
            &AtomicBool::new(false),
            Duration::from_secs(5),
            Instant::now(),
            &mut failures,
        );
        assert_eq!(watched, (None, false, false));
        assert_eq!(failures, ["WAIT FAILED: injected try_wait"]);
    }

    #[test]
    fn watch_reports_deadline_and_limit_seen_together() {
        type Watched = (Option<ExitStatus>, bool, bool);
        let cases: [(Duration, Watched, LiveFailure); 2] = [
            (Duration::ZERO, (None, true, true), LiveFailure::Timeout),
            (
                Duration::from_secs(3600),
                (None, false, true),
                LiveFailure::OutputTooLarge,
            ),
        ];
        for (timeout, expected, failure) in cases {
            let mut failures = Vec::new();
            let watched = watch(
                &mut || Ok(None),
                &[],
                &AtomicBool::new(true),
                timeout,
                Instant::now(),
                &mut failures,
            );
            assert_eq!(watched, expected, "timeout {timeout:?}");
            assert!(failures.is_empty(), "timeout {timeout:?}: {failures:?}");
            let (status, timed_out, output_limit_exceeded) = watched;
            let result = exec_result(
                status,
                timed_out,
                output_limit_exceeded,
                failures,
                Vec::new(),
            );
            assert_eq!(
                decide_live_result(&result),
                Err(failure),
                "timeout {timeout:?}"
            );
        }
    }

    /// Starts `sleep 5` on the child ends of fresh [`stream_pairs`], giving
    /// the child, the parent's ends and a copy of each child end, which
    /// stands for a descendant holding the child's streams open, reading
    /// and writing nothing. `Ok(None)` when `sleep` is absent, which is
    /// reported as NOT RUN; any other failure to start it is an error.
    fn spawn_sleep() -> Result<Option<(Child, StdStreams, StdStreams)>, String> {
        let (streams, (stdin, stdout, stderr)) = stream_pairs().map_err(io_text)?;
        let held = (
            stdin.try_clone().map_err(io_text)?,
            stdout.try_clone().map_err(io_text)?,
            stderr.try_clone().map_err(io_text)?,
        );
        let spawned = Command::new("sleep")
            .arg("5")
            .stdin(Stdio::from(OwnedFd::from(stdin)))
            .stdout(Stdio::from(OwnedFd::from(stdout)))
            .stderr(Stdio::from(OwnedFd::from(stderr)))
            .spawn();
        match spawned {
            Ok(child) => Ok(Some((child, streams, held))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                println!("NOT RUN: sleep not found");
                Ok(None)
            }
            Err(e) => Err(format!("sleep could not be started: {e}")),
        }
    }

    /// Starts workers on real threads, counting in `finished` each one whose
    /// work has returned; the call numbered `fail_on`, if any, fails instead.
    fn counting_start(
        finished: Arc<AtomicUsize>,
        fail_on: Option<usize>,
    ) -> impl FnMut(&'static str, Worker) -> io::Result<JoinHandle<PipeOutcome>> {
        let mut calls = 0;
        move |name, work| {
            calls += 1;
            if fail_on == Some(calls) {
                return Err(io::Error::other("injected"));
            }
            let done = Arc::clone(&finished);
            start_thread(
                name,
                Box::new(move || {
                    let outcome = work();
                    done.fetch_add(1, Ordering::SeqCst);
                    outcome
                }),
            )
        }
    }

    #[test]
    fn run_command_joins_workers_held_open_by_a_descendant() -> Result<(), String> {
        let Some((mut child, streams, held)) = spawn_sleep()? else {
            return Ok(());
        };
        let finished = Arc::new(AtomicUsize::new(0));
        let mut start = counting_start(Arc::clone(&finished), None);
        let input = vec![b'x'; 4 * 1024 * 1024];
        let timeout = Duration::from_millis(200);
        let started = Instant::now();
        let result = supervise(
            &mut child, streams, &input, timeout, 4096, started, &mut start,
        );
        let elapsed = started.elapsed();
        drop(held);
        assert_eq!(
            finished.load(Ordering::SeqCst),
            3,
            "a worker was not joined"
        );
        assert!(
            elapsed < timeout + Duration::from_millis(500),
            "took {elapsed:?}"
        );
        assert!(result.timed_out, "not timed out: {result:?}");
        assert_eq!(result.spawn_error, None);
        assert_eq!(result.exit_code, None, "a killed child has no exit code");
        assert_eq!(decide_live_result(&result), Err(LiveFailure::Timeout));
        Ok(())
    }

    #[test]
    fn run_command_partial_worker_start_kills_reaps_and_joins() -> Result<(), String> {
        let diagnostic = "PIPE WORKER NOT STARTED: injected";
        for fail_on in 1..=3 {
            let Some((mut child, streams, held)) = spawn_sleep()? else {
                return Ok(());
            };
            let finished = Arc::new(AtomicUsize::new(0));
            let mut start = counting_start(Arc::clone(&finished), Some(fail_on));
            let input = vec![b'x'; 4 * 1024 * 1024];
            let started = Instant::now();
            let result = supervise(
                &mut child,
                streams,
                &input,
                Duration::from_secs(5),
                4096,
                started,
                &mut start,
            );
            let elapsed = started.elapsed();
            drop(held);
            assert_eq!(
                result.spawn_error.as_deref(),
                Some(diagnostic),
                "worker {fail_on}"
            );
            assert!(
                matches!(child.try_wait(), Ok(Some(_))),
                "worker {fail_on}: child not reaped"
            );
            assert_eq!(result.exit_code, None, "worker {fail_on}");
            assert_eq!(
                finished.load(Ordering::SeqCst),
                fail_on - 1,
                "worker {fail_on}: a started worker was not joined"
            );
            assert!(
                elapsed < Duration::from_secs(1),
                "worker {fail_on}: took {elapsed:?}"
            );
            assert_eq!(
                decide_live_result(&result),
                Err(LiveFailure::Spawn(diagnostic.to_string()))
            );
        }
        Ok(())
    }

    fn held_slots(slots: &[Mutex<()>]) -> usize {
        slots
            .iter()
            .filter(|slot| matches!(slot.try_lock(), Err(TryLockError::WouldBlock)))
            .count()
    }

    #[test]
    fn live_admission_bounds_concurrent_runs() -> Result<(), String> {
        let slots = [const { Mutex::new(()) }; 2];
        let inside = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let admitted = AtomicUsize::new(0);
        thread::scope(|scope| -> Result<(), String> {
            let mut workers = Vec::new();
            for number in 0..8 {
                let worker = thread::Builder::new()
                    .spawn_scoped(scope, || {
                        let Some(slot) = admit(&slots, Duration::from_secs(30)) else {
                            return;
                        };
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(50));
                        inside.fetch_sub(1, Ordering::SeqCst);
                        admitted.fetch_add(1, Ordering::SeqCst);
                        drop(slot);
                    })
                    .map_err(|e| format!("starting thread {number}: {e}"))?;
                workers.push(worker);
            }
            for worker in workers {
                worker
                    .join()
                    .map_err(|_| "an admitted thread panicked".to_string())?;
            }
            Ok(())
        })?;
        let peak = peak.load(Ordering::SeqCst);
        assert!((1..=2).contains(&peak), "{peak} ran at once in two slots");
        assert_eq!(admitted.load(Ordering::SeqCst), 8, "a thread was refused");
        assert_eq!(held_slots(&slots), 0, "a slot was not freed");
        Ok(())
    }

    #[test]
    fn live_admission_wait_is_bounded() -> Result<(), String> {
        let slots = [const { Mutex::new(()) }; 1];
        let held =
            admit(&slots, Duration::ZERO).ok_or_else(|| "a free slot was refused".to_string())?;
        assert!(
            admit(&slots, Duration::ZERO).is_none(),
            "admitted past the bound"
        );
        let wait = Duration::from_millis(200);
        let started = Instant::now();
        let refused = admit(&slots, wait).is_none();
        let elapsed = started.elapsed();
        assert!(refused, "admitted past the bound");
        assert!(elapsed >= wait, "gave up after {elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "waited {elapsed:?}");
        // A slot freed lets a waiting thread in, and only then.
        let released = AtomicBool::new(false);
        let after_release = thread::scope(|scope| -> Result<Option<bool>, String> {
            let waiter = thread::Builder::new()
                .spawn_scoped(scope, || {
                    let slot = admit(&slots, Duration::from_secs(30))?;
                    let after = released.load(Ordering::SeqCst);
                    drop(slot);
                    Some(after)
                })
                .map_err(io_text)?;
            thread::sleep(Duration::from_millis(100));
            released.store(true, Ordering::SeqCst);
            drop(held);
            waiter
                .join()
                .map_err(|_| "the waiting thread panicked".to_string())
        })?;
        assert_eq!(after_release, Some(true), "admitted before the release");
        assert_eq!(held_slots(&slots), 0, "a slot was not freed");
        Ok(())
    }

    #[test]
    fn live_admission_bounds_real_runs() -> Result<(), String> {
        let probe = run_available(
            "sleep",
            &strings(&["0"]),
            b"",
            Duration::from_secs(10),
            4096,
        )?;
        if probe.is_none() {
            return Ok(());
        }
        // Six runs of 0.3 s through two slots take three turns at least.
        let slots = [const { Mutex::new(()) }; 2];
        let args = strings(&["0.3"]);
        let started = Instant::now();
        let results = thread::scope(|scope| -> Result<Vec<_>, String> {
            let mut runs = Vec::new();
            for number in 0..6 {
                let run = thread::Builder::new()
                    .spawn_scoped(scope, || {
                        run_admitted(&slots, Duration::from_secs(60), "sleep", &args, b"")
                    })
                    .map_err(|e| format!("starting run {number}: {e}"))?;
                runs.push(run);
            }
            runs.into_iter()
                .map(|run| run.join().map_err(|_| "a run panicked".to_string()))
                .collect()
        })?;
        let elapsed = started.elapsed();
        assert_eq!(results, vec![Ok(String::new()); 6]);
        assert!(
            elapsed >= Duration::from_millis(900),
            "more than two ran at once: {elapsed:?}"
        );
        assert_eq!(held_slots(&slots), 0, "a slot was not freed");
        Ok(())
    }

    #[test]
    fn live_admission_expiry_starts_nothing() -> Result<(), String> {
        let absent = "/nonexistent/ifcap-po-inquiry-no-such-program";
        let slots = [const { Mutex::new(()) }; 1];
        let held =
            admit(&slots, Duration::ZERO).ok_or_else(|| "a free slot was refused".to_string())?;
        // Starting the absent program would be `Spawn`; with no slot free it
        // is never tried.
        let wait = Duration::from_millis(200);
        let started = Instant::now();
        let expired = run_admitted(&slots, wait, absent, &[], b"");
        let elapsed = started.elapsed();
        assert_eq!(expired, Err(LiveFailure::Timeout));
        assert_eq!(
            expired.map_err(|f| f.to_string()),
            Err("TIMEOUT".to_string())
        );
        assert!(elapsed >= wait, "gave up after {elapsed:?}");
        assert!(elapsed < Duration::from_secs(3), "waited {elapsed:?}");
        drop(held);
        let attempted = run_admitted(&slots, wait, absent, &[], b"");
        assert!(
            matches!(&attempted, Err(LiveFailure::Spawn(_))),
            "{attempted:?}"
        );
        assert_eq!(held_slots(&slots), 0, "a slot was not freed");
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

    /// `parse_context` over 10,000 keys keeps them in document order and
    /// rejects a key repeated at the end, and a non-string value is reported
    /// before a repetition. Only results are asserted: that repetitions are
    /// found with a set lookup, not by rescanning earlier keys, is verified by
    /// reading `parse_context`, because a timing assertion would be flaky.
    #[test]
    fn context_keeps_document_order_and_rejects_repeats_at_scale() {
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
            (
                pairs(&[
                    ("DUZ", ".5"),
                    ("IOF", "UNRESOLVED"),
                    ("IOSL", "UNRESOLVED"),
                    ("STATION", "500"),
                ]),
                "CONTEXT MISMATCH IOF: expected UNRESOLVED, actual UNRESOLVED",
            ),
        ];
        for (context, message) in cases {
            assert_eq!(validate_context(&context), Err(message.to_string()));
        }
        // An unresolved constant matches nothing, its sentinel spelling included.
        let partial = [("DUZ", Some(".5")), ("IOSL", None)];
        let sentinel = pairs(&[("DUZ", ".5"), ("IOSL", "UNRESOLVED")]);
        assert_eq!(
            validate_against(&partial, &sentinel),
            Err("CONTEXT MISMATCH IOSL: expected UNRESOLVED, actual UNRESOLVED".to_string())
        );
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
            Ok(("GET", "/".to_string(), None, false))
        );
        // An HTTP/1.0 request needs no `Host`, and its `Expect: 100-continue`
        // is ignored (RFC 9110 10.1.1).
        assert_eq!(
            parse_head(
                b"POST /api/rust/session HTTP/1.0\r\ncontent-length: 12\r\nContent-Length:12\r\nExpect: 100-continue\r\n\r\n"
            ),
            Ok(("POST", "/api/rust/session".to_string(), Some(12), false))
        );
        // An HTTP/1.1 request's `Expect: 100-continue` is reported.
        assert_eq!(
            parse_head(
                b"POST /api/rust/session HTTP/1.1\r\nHost: h\r\nContent-Length: 12\r\nExpect: 100-continue\r\n\r\n"
            ),
            Ok(("POST", "/api/rust/session".to_string(), Some(12), true))
        );
        // Each head accepted as `GET /`: an empty-line prelude, `Host` values
        // of every form, any host name included, and a value with an HTAB
        // and a byte from 0x80.
        let accepted: [&[u8]; 9] = [
            b"\r\nGET / HTTP/1.1\r\nHost: h\r\n\r\n",
            b"\r\n\r\nGET / HTTP/1.1\r\nHost: h\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: h:8080\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: [::1]:8080\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: [v1.a:b]\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: %41-b~!$&'()*+,;=\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: h:\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: h\r\nX-Test: a\tb \xc3\xa9\r\n\r\n",
        ];
        for head in accepted {
            assert_eq!(
                parse_head(head),
                Ok(("GET", "/".to_string(), None, false)),
                "for {}",
                String::from_utf8_lossy(head)
            );
        }
        // Each absolute-form target, and the origin-form it is routed by.
        let absolute: [(&str, &str); 7] = [
            ("http://127.0.0.1:18221/", "/"),
            ("http://h", "/"),
            ("http://h?x", "/?x"),
            ("HTTP://h/app.js", "/app.js"),
            ("hTtP://h:1/api/rust/session?q=1", "/api/rust/session?q=1"),
            ("http://h/a@b", "/a@b"),
            ("http://h#f", "/#f"),
        ];
        for (target, origin) in absolute {
            let head = format!("GET {target} HTTP/1.1\r\nHost: h\r\n\r\n");
            assert_eq!(
                parse_head(head.as_bytes()),
                Ok(("GET", origin.to_string(), None, false)),
                "for {target}"
            );
        }
        // Each rejected head, and the rule its 400 cause names.
        let rejected: [(&[u8], &str); 43] = [
            (b"GET /\r\n\r\n", "MALFORMED REQUEST LINE"),
            (b"GET / HTTP/1.1 x\r\n\r\n", "MALFORMED REQUEST LINE"),
            (b"GET  / HTTP/1.1\r\n\r\n", "MALFORMED REQUEST LINE"),
            (b"GET / \r\n\r\n", "MALFORMED REQUEST LINE"),
            (b"\r\n\r\n", "INCOMPLETE HEAD"),
            (
                b"\r\n \r\nGET / HTTP/1.1\r\nHost: h\r\n\r\n",
                "MALFORMED REQUEST LINE",
            ),
            (b"G/T / HTTP/1.1\r\n\r\n", "MALFORMED REQUEST LINE"),
            (b"GET / HTTP/2\r\n\r\n", "UNSUPPORTED HTTP VERSION"),
            (b"GET / HTTP/2.0\r\n\r\n", "UNSUPPORTED HTTP VERSION"),
            (b"GET / http/1.1\r\n\r\n", "UNSUPPORTED HTTP VERSION"),
            (b"GET x HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"OPTIONS * HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"GET ftp://x/ HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"GET https://x/ HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"GET http:///x HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"GET http://?x HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (
                b"GET http://u@x/ HTTP/1.1\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (b"GET http:/x HTTP/1.1\r\n\r\n", "INVALID REQUEST TARGET"),
            (b"GET / HTTP/1.1\r\nNoColon\r\n\r\n", "MALFORMED HEADER"),
            (b"GET / HTTP/1.1\r\nBad Name: x\r\n\r\n", "MALFORMED HEADER"),
            (
                b"GET / HTTP/1.1\r\nHost: a\r\n folded\r\n\r\n",
                "MALFORMED HEADER",
            ),
            (b"GET /\xff HTTP/1.1\r\n\r\n", "HEAD NOT UTF-8"),
            (b"GET / HTTP/1.1\r\nHost: \xff\r\n\r\n", "HEAD NOT UTF-8"),
            (b"GET / HTTP/1.1\r\n", "INCOMPLETE HEAD"),
            (
                b"POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
                "INVALID CONTENT-LENGTH",
            ),
            (
                b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
                "INVALID CONTENT-LENGTH",
            ),
            (
                b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
                "TRANSFER-ENCODING NOT SUPPORTED",
            ),
            (b"GET / HTTP/1.1\r\n\r\n", "MISSING HOST"),
            (b"GET http://h/ HTTP/1.1\r\n\r\n", "MISSING HOST"),
            (
                b"GET / HTTP/1.1\r\nHost: h\r\nhost: h\r\n\r\n",
                "REPEATED HOST",
            ),
            (
                b"GET / HTTP/1.0\r\nHost: h\r\nHost: h\r\n\r\n",
                "REPEATED HOST",
            ),
            (b"GET / HTTP/1.1\r\nHost: a b\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: a/b\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: h:x\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: [zz]\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost:\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: u@h\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: [::1\r\n\r\n", "INVALID HOST"),
            (b"GET / HTTP/1.1\r\nHost: %4g\r\n\r\n", "INVALID HOST"),
            (
                b"GET / HTTP/1.1\r\nHost: h\r\nX-Test: a\x00b\r\n\r\n",
                "INVALID HEADER VALUE",
            ),
            (
                b"GET / HTTP/1.1\r\nHost: h\r\nX-Test: a\rb\r\n\r\n",
                "INVALID HEADER VALUE",
            ),
            (
                b"GET / HTTP/1.1\r\nHost: h\r\nX-Test: a\x7fb\r\n\r\n",
                "INVALID HEADER VALUE",
            ),
            (
                b"GET / HTTP/1.1\r\nX-Test: \x00\r\nHost: a b\r\n\r\n",
                "INVALID HEADER VALUE",
            ),
        ];
        for (head, cause) in rejected {
            assert_eq!(
                parse_head(head),
                Err(format!("{BAD_REQUEST}: {cause}")),
                "for {}",
                String::from_utf8_lossy(head)
            );
        }
    }

    /// Sends each chunk of `chunks` after its delay, then ends the output
    /// when `close` is set, and keeps `client` open until the server end
    /// closes or the client's read timeout passes.
    fn send_chunks(
        mut client: TcpStream,
        chunks: Vec<(Duration, Vec<u8>)>,
        close: bool,
    ) -> io::Result<()> {
        for (delay, chunk) in chunks {
            thread::sleep(delay);
            client.write_all(&chunk)?;
        }
        if close {
            client.shutdown(Shutdown::Write)?;
        }
        let mut byte = [0u8; 1];
        client.read(&mut byte).map(|_| ())
    }

    #[test]
    fn read_request_bounds_idle_and_request_time() -> Result<(), String> {
        let idle = Duration::from_millis(300);
        let request_time = Duration::from_millis(400);
        // Every case ends long before a trickle of 100 bytes, one per tick,
        // would; the margin allows for a loaded host.
        let tick = Duration::from_millis(50);
        let bound = idle
            .saturating_add(request_time)
            .saturating_add(Duration::from_millis(1500));
        let now = |bytes: &[u8]| vec![(Duration::ZERO, bytes.to_vec())];
        let trickle = |bytes: &[u8]| -> Vec<(Duration, Vec<u8>)> {
            bytes.iter().map(|byte| (tick, vec![*byte])).collect()
        };
        let head = b"POST /api/rust/session HTTP/1.1\r\nHost: h\r\nContent-Length: 100\r\n\r\n";
        let mut trickled_body = now(head);
        trickled_body.extend(trickle(&[b'x'; 100]));
        let oversized = |end: &[u8], filler: usize| {
            let mut bytes = b"GET / HTTP/1.1\r\nX-Big: ".to_vec();
            bytes.resize(bytes.len().saturating_add(filler), b'a');
            bytes.extend_from_slice(end);
            vec![(Duration::ZERO, bytes)]
        };
        let bad = |cause: &str| Err(format!("{BAD_REQUEST}: {cause}"));
        let answered = Ok(Some((
            "POST".to_string(),
            "/api/rust/session".to_string(),
            Some(b"{}".to_vec()),
        )));
        let request_then_body = vec![
            (
                Duration::ZERO,
                b"POST /api/rust/session HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\n\r\n".to_vec(),
            ),
            (Duration::from_millis(100), b"{}".to_vec()),
        ];
        // (case, what the client sends, whether it then ends its output,
        // the outcome, the least time the outcome takes)
        let cases = [
            ("nothing sent", Vec::new(), false, Ok(None), idle),
            (
                "closed before a byte",
                Vec::new(),
                true,
                Ok(None),
                Duration::ZERO,
            ),
            (
                "head trickled",
                trickle(&[b'G'; 100]),
                false,
                bad("READ TIMEOUT"),
                request_time,
            ),
            (
                "partial head, then silence",
                now(b"POST /api/rust/session HTTP/1.1\r\nHost: x\r\n"),
                false,
                bad("READ TIMEOUT"),
                request_time,
            ),
            (
                "closed mid-head",
                now(b"GET / HTTP/1.1\r\nHost: x\r\n"),
                true,
                bad("INCOMPLETE HEAD"),
                Duration::ZERO,
            ),
            (
                "body trickled",
                trickled_body,
                false,
                bad("INCOMPLETE BODY"),
                request_time,
            ),
            (
                "unterminated head over the limit",
                oversized(b"", MAX_HEAD + 4096),
                false,
                bad("HEAD TOO LARGE"),
                Duration::ZERO,
            ),
            (
                "terminated head over the limit",
                oversized(b"\r\n\r\n", MAX_HEAD),
                false,
                bad("HEAD TOO LARGE"),
                Duration::ZERO,
            ),
            (
                "empty-line prelude trickled",
                trickle(&b"\r\n".repeat(50)),
                false,
                bad("READ TIMEOUT"),
                request_time,
            ),
            (
                "empty-line prelude over the limit",
                now(&b"\r\n".repeat(MAX_HEAD / 2 + 2048)),
                false,
                bad("HEAD TOO LARGE"),
                Duration::ZERO,
            ),
            (
                "empty-line prelude, then a request",
                now(b"\r\n\r\nGET / HTTP/1.1\r\nHost: h\r\n\r\n"),
                false,
                Ok(Some(("GET".to_string(), "/".to_string(), None))),
                Duration::ZERO,
            ),
            (
                "body within the deadline",
                request_then_body,
                false,
                answered,
                Duration::ZERO,
            ),
        ];
        for (case, chunks, close, expected, least) in cases {
            let (client, mut server) = loopback_pair()?;
            let sent: Vec<u8> = chunks
                .iter()
                .flat_map(|(_, chunk)| chunk.iter().copied())
                .collect();
            let client = thread::Builder::new()
                .spawn(move || send_chunks(client, chunks, close))
                .map_err(io_text)?;
            let mut received = Vec::new();
            let started = Instant::now();
            let outcome = read_request(&mut server, &mut received, idle, request_time);
            let took = started.elapsed();
            drop(server);
            // A client still sending fails once the server end closes; only
            // the server's outcome counts.
            let _ = client
                .join()
                .map_err(|_| format!("{case}: the client panicked"))?;
            let outcome = outcome.map_err(|e| format!("{case}: read timeout not set: {e}"))?;
            println!("read_request, {case}: {outcome:?} after {took:?}");
            assert_eq!(outcome, expected, "{case}");
            assert!(sent.starts_with(&received), "{case}: received out of order");
            assert!(
                took >= least && took < bound,
                "{case}: returned after {took:?}, expected from {least:?} to {bound:?}"
            );
        }
        Ok(())
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
    fn every_status_written_with_hardening_headers() -> Result<(), String> {
        let statuses = [
            (200, "OK"),
            (400, "Bad Request"),
            (404, "Not Found"),
            (422, "Unprocessable Content"),
            (500, "Internal Server Error"),
            (502, "Bad Gateway"),
        ];
        for (status, reason) in statuses {
            let response = (status, JSON_TYPE, br#"{"error":"X"}"#.to_vec());
            for head_request in [false, true] {
                let mut written: Vec<u8> = Vec::new();
                write_response(&mut written, &response, head_request).map_err(io_text)?;
                let expected_content: &[u8] = if head_request {
                    b""
                } else {
                    br#"{"error":"X"}"#
                };
                let mut expected = format!(
                    "HTTP/1.1 {status} {reason}\r\n\
                     Content-Type: application/json; charset=utf-8\r\n\
                     Content-Length: 13\r\n\
                     X-Content-Type-Options: nosniff\r\n\
                     Cache-Control: no-store\r\n\
                     X-Frame-Options: DENY\r\n\
                     Content-Security-Policy: frame-ancestors 'none'\r\n\
                     Connection: close\r\n\r\n"
                )
                .into_bytes();
                expected.extend_from_slice(expected_content);
                assert_eq!(
                    String::from_utf8_lossy(&written),
                    String::from_utf8_lossy(&expected),
                    "status {status}, HEAD {head_request}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn head_requests_answered_as_get_without_content() -> Result<(), String> {
        let store = Store::from_zwr("").map_err(|e| e.to_string())?;
        // No case reaches a backend, so no capture is read.
        let mumps = MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        };
        let app_js = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("frontend")
                .join("app.js"),
        )
        .map_err(io_text)?;
        let js_type = "text/javascript; charset=utf-8";
        let get_required = error_body(GET_REQUIRED).into_bytes();
        let post_required = error_body(POST_REQUIRED).into_bytes();
        let malformed = error_body(&format!("{BAD_REQUEST}: MALFORMED REQUEST LINE")).into_bytes();
        let missing_host = error_body(&format!("{BAD_REQUEST}: MISSING HOST")).into_bytes();
        let request = |line: &str| format!("{line} HTTP/1.1\r\nHost: h\r\n\r\n");
        // (request, status line, content type, the body a GET gets, whether
        // the answer leaves that body out)
        let cases = [
            (request("GET /app.js"), "200 OK", js_type, &app_js, false),
            (request("HEAD /app.js"), "200 OK", js_type, &app_js, true),
            (
                request("HEAD http://h/app.js"),
                "200 OK",
                js_type,
                &app_js,
                true,
            ),
            (
                request("HEAD /api/rust/session"),
                "404 Not Found",
                JSON_TYPE,
                &post_required,
                true,
            ),
            // Method tokens are case-sensitive: `head` is not `HEAD`.
            (
                request("head /app.js"),
                "404 Not Found",
                JSON_TYPE,
                &get_required,
                false,
            ),
            (
                "HEAD /\r\n\r\n".to_string(),
                "400 Bad Request",
                JSON_TYPE,
                &malformed,
                true,
            ),
            // A `HEAD` after an empty-line prelude, accepted or rejected.
            (
                format!("\r\n{}", request("HEAD /app.js")),
                "200 OK",
                js_type,
                &app_js,
                true,
            ),
            (
                "\r\n\r\nHEAD / HTTP/1.1\r\n\r\n".to_string(),
                "400 Bad Request",
                JSON_TYPE,
                &missing_host,
                true,
            ),
        ];
        for (request, status_line, content_type, body, head_request) in cases {
            let (mut client, mut server) = loopback_pair()?;
            let request_line = request
                .trim_start()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            client
                .write_all(request.as_bytes())
                .and_then(|()| client.shutdown(Shutdown::Write))
                .map_err(io_text)?;
            let (store, mumps) = (store.clone(), mumps.clone());
            // The server end answers on its own thread, so a response larger
            // than the socket buffers cannot block the test.
            let server_side = thread::Builder::new()
                .spawn(move || exchange(&mut server, &store, &mumps))
                .map_err(io_text)?;
            let mut response = Vec::new();
            let read = client.read_to_end(&mut response);
            let exchanged = server_side
                .join()
                .map_err(|_| format!("{request_line}: the server end panicked"))?;
            exchanged.map_err(io_text)?;
            read.map_err(io_text)?;
            let length = body.len();
            let mut expected = format!(
                "HTTP/1.1 {status_line}\r\n\
                 Content-Type: {content_type}\r\n\
                 Content-Length: {length}\r\n\
                 {HARDENING_HEADERS}Connection: close\r\n\r\n"
            )
            .into_bytes();
            if !head_request {
                expected.extend_from_slice(body);
            }
            assert_eq!(
                String::from_utf8_lossy(&response),
                String::from_utf8_lossy(&expected),
                "for {request_line}"
            );
        }
        Ok(())
    }

    #[test]
    fn accept_errors_retried_or_returned() -> Result<(), String> {
        let retried = [
            (io::Error::from(io::ErrorKind::WouldBlock), POLL),
            (io::Error::from(io::ErrorKind::Interrupted), Duration::ZERO),
            (
                io::Error::from(io::ErrorKind::ConnectionAborted),
                Duration::ZERO,
            ),
            // EPROTO: one pending connection failed a protocol check.
            (io::Error::from_raw_os_error(71), Duration::ZERO),
        ];
        for (error, expected) in retried {
            let kind = error.kind();
            let delay =
                accept_retry_delay(error).map_err(|e| format!("{kind:?} not retried: {e}"))?;
            assert_eq!(delay, expected, "retry delay after {kind:?}");
        }
        // EMFILE, a full descriptor table, and a refused permission.
        let unexpected = [
            io::Error::from_raw_os_error(24),
            io::Error::from(io::ErrorKind::PermissionDenied),
        ];
        for error in unexpected {
            let kind = error.kind();
            let text = error.to_string();
            let Err(returned) = accept_retry_delay(error) else {
                return Err(format!("{kind:?} was retried"));
            };
            assert_eq!(returned.kind(), kind, "kind kept");
            assert_eq!(
                returned.to_string(),
                format!("accepting a connection failed: {text}")
            );
        }
        Ok(())
    }

    #[test]
    fn abandoned_connection_closed_without_response() -> Result<(), String> {
        let fail = |what: &str, e: io::Error| format!("{what}: {e}");
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| fail("bind", e))?;
        let addr = listener.local_addr().map_err(|e| fail("local_addr", e))?;
        let mut client = TcpStream::connect(addr).map_err(|e| fail("connect", e))?;
        client
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| fail("set read timeout", e))?;
        let (accepted, _) = listener.accept().map_err(|e| fail("accept", e))?;
        // EAGAIN, as a thread that cannot be created reports it.
        let cause = io::Error::from_raw_os_error(11);
        let (kind, text) = (cause.kind(), cause.to_string());
        let returned = abandon(&accepted, "handling a connection", cause);
        // `accepted` is still open, so only the shutdown can end the stream.
        let mut received = Vec::new();
        let read = client.read_to_end(&mut received);
        drop(accepted);
        read.map_err(|e| fail("read to the close", e))?;
        assert_eq!(String::from_utf8_lossy(&received), "", "no response");
        assert_eq!(returned.kind(), kind, "kind kept");
        assert_eq!(
            returned.to_string(),
            format!("handling a connection failed: {text}")
        );
        Ok(())
    }

    /// A connected loopback pair, the client end first, each end with
    /// `IO_TIMEOUT` for reads and writes so that no step can hang a test.
    fn loopback_pair() -> Result<(TcpStream, TcpStream), String> {
        let fail = |what: &str, e: io::Error| format!("{what}: {e}");
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| fail("bind", e))?;
        let addr = listener.local_addr().map_err(|e| fail("local_addr", e))?;
        let client = TcpStream::connect(addr).map_err(|e| fail("connect", e))?;
        let (server, _) = listener.accept().map_err(|e| fail("accept", e))?;
        for end in [&client, &server] {
            end.set_read_timeout(Some(IO_TIMEOUT))
                .and_then(|()| end.set_write_timeout(Some(IO_TIMEOUT)))
                .map_err(|e| fail("set timeouts", e))?;
        }
        Ok((client, server))
    }

    #[test]
    fn drain_ends_within_its_time_budget() -> Result<(), String> {
        let (mut client, mut server) = loopback_pair()?;
        let mut late = client.try_clone().map_err(io_text)?;
        client.write_all(b"a").map_err(io_text)?;
        // A second byte late in the budget, then silence with the client
        // still open: only the time budget can end the drain.
        let writer = thread::Builder::new()
            .spawn(move || {
                thread::sleep(Duration::from_millis(600));
                late.write_all(b"b")
            })
            .map_err(io_text)?;
        let limit = Duration::from_secs(1);
        let started = Instant::now();
        let discarded = drain(&mut server, limit);
        let took = started.elapsed();
        let written = writer
            .join()
            .map_err(|_| "the late writer panicked".to_string())?;
        drop(client);
        written.map_err(io_text)?;
        println!("drain with a {limit:?} limit returned after {took:?}");
        assert_eq!(discarded, 2, "both bytes discarded");
        // A read that kept the whole limit as its timeout would return
        // near 1.6 s; the margin allows for a loaded host.
        assert!(
            took >= Duration::from_millis(900) && took < limit + Duration::from_millis(400),
            "drain with a {limit:?} limit returned after {took:?}"
        );
        Ok(())
    }

    #[test]
    fn drain_discards_exactly_its_byte_budget() -> Result<(), String> {
        let (mut client, mut server) = loopback_pair()?;
        // One byte read alone first, so that later reads of the full buffer
        // do not end on the budget by alignment.
        let writer = thread::Builder::new()
            .spawn(move || {
                client.write_all(b"x")?;
                thread::sleep(Duration::from_millis(100));
                let payload = vec![0u8; DRAIN_BYTES + 3 * 8192];
                client.write_all(&payload)
            })
            .map_err(io_text)?;
        let limit = IO_TIMEOUT * 2;
        let started = Instant::now();
        let discarded = drain(&mut server, limit);
        let took = started.elapsed();
        // Closing the server end with input unread ends a write still
        // blocked on full buffers, with an error the test tolerates.
        drop(server);
        let written = writer
            .join()
            .map_err(|_| "the writer panicked".to_string())?;
        if let Err(e) = written {
            println!("the writer ended once the server closed: {e}");
        }
        println!("drain discarded {discarded} bytes in {took:?}");
        assert_eq!(discarded, DRAIN_BYTES, "the byte budget, exactly");
        assert!(took < limit, "the byte budget, not the time, ended it");
        Ok(())
    }

    /// One scripted read of a drain: `Ok(Ok(n))` delivers `n` bytes, or as
    /// many as the window holds; `Ok(Err(kind))` is a read that fails with
    /// `kind`; `Err(kind)` is a read timeout that cannot be set.
    type DrainStep = Result<Result<usize, io::ErrorKind>, io::ErrorKind>;

    /// A `drain_reads` case: its name, the time limit and the scripted
    /// reads, then the bytes discarded, how many reads were made and whether
    /// the time budget ended it. A read past the script waits out the time
    /// left and is then interrupted.
    type DrainCase = (&'static str, Duration, Vec<DrainStep>, usize, usize, bool);

    #[test]
    fn drain_retries_interrupted_reads_within_its_budgets() {
        use io::ErrorKind::{ConnectionReset, Interrupted, InvalidInput, WouldBlock};
        let cases: Vec<DrainCase> = vec![
            (
                "interrupted between data reads, then the end of input",
                IO_TIMEOUT,
                vec![
                    Ok(Ok(3)),
                    Ok(Err(Interrupted)),
                    Ok(Ok(5)),
                    Ok(Err(Interrupted)),
                    Ok(Err(Interrupted)),
                    Ok(Ok(7)),
                    Ok(Ok(0)),
                    Ok(Ok(11)),
                ],
                15,
                7,
                false,
            ),
            (
                "interrupted, then a reset",
                IO_TIMEOUT,
                vec![
                    Ok(Ok(4)),
                    Ok(Err(Interrupted)),
                    Ok(Err(ConnectionReset)),
                    Ok(Ok(9)),
                ],
                4,
                3,
                false,
            ),
            (
                "interrupted, then a read that times out",
                IO_TIMEOUT,
                vec![
                    Ok(Err(Interrupted)),
                    Ok(Ok(2)),
                    Ok(Err(WouldBlock)),
                    Ok(Ok(9)),
                ],
                2,
                3,
                false,
            ),
            (
                "interrupted, then a read timeout that cannot be set",
                IO_TIMEOUT,
                vec![
                    Ok(Ok(6)),
                    Ok(Err(Interrupted)),
                    Err(InvalidInput),
                    Ok(Ok(9)),
                ],
                6,
                3,
                false,
            ),
            // One byte first, so that the last window holds only the bytes
            // left: 255 full windows, then one a byte short.
            (
                "the byte budget, with every other read interrupted",
                IO_TIMEOUT,
                std::iter::once(Ok(Ok(1)))
                    .chain((0..300).flat_map(|_| [Ok(Err(Interrupted)), Ok(Ok(usize::MAX))]))
                    .collect(),
                DRAIN_BYTES,
                1 + 2 * (DRAIN_BYTES / 8192),
                false,
            ),
            (
                "the time budget, with the last read interrupted",
                Duration::from_millis(50),
                vec![Ok(Ok(1)), Ok(Err(Interrupted)), Ok(Ok(1))],
                2,
                4,
                true,
            ),
        ];
        for (name, limit, script, expected, reads, timed_out) in cases {
            let mut steps = script.into_iter();
            let mut made = 0usize;
            let mut within = true;
            let started = Instant::now();
            let discarded = drain_reads(limit, |window, time_left| {
                made += 1;
                within &= !window.is_empty()
                    && window.len() <= 8192
                    && !time_left.is_zero()
                    && time_left <= limit;
                match steps.next() {
                    Some(step) => step
                        .map(|read| read.map(|n| n.min(window.len())).map_err(io::Error::from))
                        .map_err(io::Error::from),
                    None => {
                        thread::sleep(time_left);
                        Ok(Err(io::Error::from(Interrupted)))
                    }
                }
            });
            let took = started.elapsed();
            assert_eq!(discarded, expected, "{name}: bytes discarded");
            assert_eq!(made, reads, "{name}: reads made");
            assert!(within, "{name}: every read within the budgets left");
            assert_eq!(
                took >= limit,
                timed_out,
                "{name}: the time budget ended it, after {took:?}"
            );
        }
    }

    #[test]
    fn worker_failures_returned_once_every_worker_is_joined() -> Result<(), String> {
        /// Waits up to `IO_TIMEOUT` for `release`. When it never comes, sets
        /// `shutdown`, so that an accept loop still serving ends.
        fn await_release(release: &AtomicBool, shutdown: &AtomicBool) {
            let started = Instant::now();
            while !release.load(Ordering::SeqCst) {
                if started.elapsed() >= IO_TIMEOUT {
                    shutdown.store(true, Ordering::SeqCst);
                    return;
                }
                thread::sleep(POLL);
            }
        }
        let fail = |what: &str, e: io::Error| format!("{what}: {e}");
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| fail("bind", e))?;
        let addr = listener.local_addr().map_err(|e| fail("local_addr", e))?;
        // Each worker's control socket is the accepted end of a connection
        // whose client stays open until the end.
        let mut clients = Vec::new();
        let mut accept_one = || -> Result<TcpStream, String> {
            clients.push(TcpStream::connect(addr).map_err(|e| fail("connect", e))?);
            let (accepted, _) = listener.accept().map_err(|e| fail("accept", e))?;
            Ok(accepted)
        };
        let sockets = (accept_one()?, accept_one()?, accept_one()?);
        listener
            .set_nonblocking(true)
            .map_err(|e| fail("set nonblocking", e))?;
        let finished = thread::spawn(|| -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "first"))
        });
        let started = Instant::now();
        while !finished.is_finished() {
            if started.elapsed() >= IO_TIMEOUT {
                return Err("the failing worker never finished".to_string());
            }
            thread::sleep(POLL);
        }
        let shutdown = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let last_joined = Arc::new(AtomicBool::new(false));
        let second = {
            let (release, shutdown) = (Arc::clone(&release), Arc::clone(&shutdown));
            thread::spawn(move || -> io::Result<()> {
                await_release(&release, &shutdown);
                Err(io::Error::new(io::ErrorKind::InvalidData, "second"))
            })
        };
        let third = {
            let (release, shutdown) = (Arc::clone(&release), Arc::clone(&shutdown));
            let last_joined = Arc::clone(&last_joined);
            thread::spawn(move || -> io::Result<()> {
                await_release(&release, &shutdown);
                thread::sleep(POLL * 5);
                last_joined.store(true, Ordering::SeqCst);
                Err(io::Error::new(io::ErrorKind::TimedOut, "third"))
            })
        };
        let mut workers = vec![
            (finished, sockets.0),
            (second, sockets.1),
            (third, sockets.2),
        ];
        let store = Arc::new(Store::from_zwr("").map_err(|e| e.to_string())?);
        let mumps = Arc::new(MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        });
        let served = accept_until(
            &listener,
            &store,
            &mumps,
            &shutdown,
            &mut workers,
            MAX_CONNECTIONS,
        );
        let still_open = workers.len();
        release.store(true, Ordering::SeqCst);
        let stopped = stop_workers(workers);
        let described = |result: io::Result<()>| result.map_err(|e| (e.kind(), e.to_string()));
        assert_eq!(
            described(served),
            Err((io::ErrorKind::PermissionDenied, "first".to_string())),
            "the finished worker's failure ends the accept loop"
        );
        assert!(!shutdown.load(Ordering::SeqCst), "ended before the guard");
        assert_eq!(still_open, 2, "the open workers are left to stop_workers");
        assert_eq!(
            described(stopped),
            Err((io::ErrorKind::InvalidData, "second".to_string())),
            "the first open worker's failure wins"
        );
        assert!(last_joined.load(Ordering::SeqCst), "every worker joined");
        drop(clients);
        Ok(())
    }

    #[test]
    fn connections_past_the_bound_wait_unanswered_until_a_worker_finishes() -> Result<(), String> {
        /// Sends a request on a connection past the bound, which `idle`
        /// fills, and checks that it is not answered while the bound holds.
        /// Then closes one idle connection and gives the status line of the
        /// response, with how long after the close it began.
        fn answered_once_a_slot_frees(
            addr: std::net::SocketAddr,
            idle: &mut Vec<TcpStream>,
        ) -> Result<(String, Duration), String> {
            let fail = |what: &str, e: io::Error| format!("{what}: {e}");
            let mut waiting = TcpStream::connect(addr).map_err(|e| fail("connect", e))?;
            waiting
                .write_all(b"GET /no-such-path HTTP/1.1\r\nHost: x\r\n\r\n")
                .and_then(|()| waiting.set_read_timeout(Some(Duration::from_millis(500))))
                .map_err(|e| fail("send the request", e))?;
            let mut buf = [0u8; 4096];
            match waiting.read(&mut buf) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Ok(0) => return Err("closed past the bound".to_string()),
                Ok(n) => return Err(format!("answered past the bound: {n} bytes")),
                Err(e) => return Err(fail("read past the bound", e)),
            }
            waiting
                .set_read_timeout(Some(IO_TIMEOUT))
                .map_err(|e| fail("set read timeout", e))?;
            drop(idle.pop());
            let freed = Instant::now();
            let mut received = Vec::new();
            while !received.windows(2).any(|pair| pair == b"\r\n") {
                let n = waiting
                    .read(&mut buf)
                    .map_err(|e| fail("read the response", e))?;
                if n == 0 {
                    break;
                }
                received.extend_from_slice(buf.get(..n).unwrap_or(&[]));
            }
            let took = freed.elapsed();
            let text = String::from_utf8_lossy(&received);
            Ok((text.lines().next().unwrap_or_default().to_string(), took))
        }
        const LIMIT: usize = 2;
        let fail = |what: &str, e: io::Error| format!("{what}: {e}");
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| fail("bind", e))?;
        let addr = listener.local_addr().map_err(|e| fail("local_addr", e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| fail("set nonblocking", e))?;
        // Queued before the connection past the bound, so they are accepted
        // first; they send nothing, so their workers stay open.
        let mut idle = Vec::new();
        for _ in 0..LIMIT {
            idle.push(TcpStream::connect(addr).map_err(|e| fail("connect", e))?);
        }
        let store = Arc::new(Store::from_zwr("").map_err(|e| e.to_string())?);
        let mumps = Arc::new(MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        });
        let shutdown = AtomicBool::new(false);
        let mut workers = Vec::new();
        let (answered, served) = thread::scope(|scope| -> Result<_, String> {
            let serving = thread::Builder::new()
                .spawn_scoped(scope, || {
                    accept_until(&listener, &store, &mumps, &shutdown, &mut workers, LIMIT)
                })
                .map_err(io_text)?;
            let answered = answered_once_a_slot_frees(addr, &mut idle);
            shutdown.store(true, Ordering::SeqCst);
            let served = serving
                .join()
                .map_err(|_| "the accept loop panicked".to_string())?;
            Ok((answered, served))
        })?;
        let still_open = workers.len();
        let stopped = stop_workers(workers);
        drop(idle);
        let (status_line, took) = answered?;
        println!("answered {took:?} after a slot freed: {status_line}");
        served.map_err(io_text)?;
        stopped.map_err(io_text)?;
        assert!(
            status_line.starts_with("HTTP/1.1 404 "),
            "status line {status_line:?}"
        );
        // The idle connection left open would end its worker only after
        // `IO_TIMEOUT`; the margin allows for a loaded host.
        assert!(
            took < IO_TIMEOUT / 2,
            "answered {took:?} after a slot freed"
        );
        assert!(
            still_open <= LIMIT,
            "{still_open} workers open past the bound"
        );
        Ok(())
    }
}
