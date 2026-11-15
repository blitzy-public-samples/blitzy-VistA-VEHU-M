//! The loopback bridge: one HTTP/1.1 server that lets a person run one input
//! sequence against the original MUMPS system and against the Rust port, from
//! the one-screen front-end. Rust-only infrastructure with no MUMPS
//! counterpart; it implements the request's HTTP contract as AAP 0.3.5.1
//! states it.
//!
//! - `POST /api/{mumps|rust|compare}/session` with `{"inputs": [string, ...]}`
//!   runs the whole sequence statelessly. The MUMPS side replays the recorded
//!   capture whose inputs equal the request's, or runs the original live
//!   through `docker exec`, as [`MumpsMode`] chooses; the Rust side is
//!   [`inquire`] on the fixture [`Store`]. COMPARE runs both and reports
//!   `match`, the PASS rule of one case applied to one request.
//! - `GET /api/status` reports the MUMPS mode and which of the five cases are
//!   recorded, READY or INCOMPLETE, read afresh on every request.
//! - `GET /`, `/index.html`, `/style.css` and `/app.js` serve `frontend/`,
//!   matched exactly and read at request time.
//!
//! There is no authentication, TLS or session state: the trust boundary is
//! the loopback address the caller binds. A request supplies only `inputs`
//! and, through its path, the backend. The live container comes only from
//! [`MumpsMode::Live`], and the live start arguments and context only from
//! the captures, so no request field reaches a command line. No endpoint
//! changes data, and nothing here writes a file.
//!
//! Capture content (inputs, transcripts, return values, normalisation rules,
//! readiness, the recorded start arguments and context) is read only through
//! [`crate::golden`]; this module re-implements none of its parsing or checks.

use std::fmt::{self, Write as _};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Ipv6Addr, Shutdown, TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::golden::{self, Case, Readiness};
use crate::inquiry::{InquiryError, InquiryOutput, inquire};
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
/// of about [`MAX_HEAD`] plus [`MAX_BODY`] at most. The inputs decoded from a
/// body hold no more text than the body, and what each input costs beyond
/// its text is bounded by [`FREE_INPUTS`] per connection and, across the
/// sessions with more inputs, by [`INPUT_BUDGET`]. So this bounds what
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
/// A session with at most this many inputs never waits for
/// [`INPUT_BUDGET`], nor is charged against it: more than any typed session
/// sends.
const FREE_INPUTS: usize = 1024;
/// The most decoded inputs that the sessions with more than [`FREE_INPUTS`]
/// inputs hold at once, across every connection: as many as two maximal
/// bodies of one-character inputs, `"a",` each, hold, so two such sessions
/// run at once and a third waits. A decoded input costs several times its
/// bytes in the body, so it is this, not [`MAX_CONNECTIONS`] alone, that
/// bounds the inputs clients can make the bridge hold.
const MAX_HELD_INPUTS: usize = MAX_BODY / 2;
/// The one bound on decoded inputs, shared by every connection thread: a
/// session holds its charge from before its inputs are decoded until they
/// are freed.
static INPUT_BUDGET: InputBudget = InputBudget::new(MAX_HELD_INPUTS, FREE_INPUTS);
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

/// Where the MUMPS side of a session comes from (AAP 0.3.5.1).
///
/// `Live` carries `golden_dir` as well, because live mode takes its start
/// arguments and context from the captures, and the directory can reach the
/// bridge only as an argument: no test sets an environment variable. In both
/// modes `GET /api/status` describes `golden_dir`, and COMPARE judges `match`
/// against the capture whose inputs equal the request's.
#[derive(Clone, Debug)]
pub enum MumpsMode {
    /// Replay adapter of AAP 0.3.5.1: answer from the recorded captures
    /// `GC-01` to `GC-05` under `golden_dir`, read afresh on every request.
    Replay { golden_dir: PathBuf },
    /// Live adapter of AAP 0.3.5.1: run the original with
    /// `docker exec -i <container> ...`, using the start arguments and
    /// context recorded in the captures under `golden_dir`.
    Live {
        container: String,
        golden_dir: PathBuf,
    },
}

/// Serves `listener` until serving fails (AAP 0.3.5.1). The text of each
/// error names the stage that failed, and its kind is that of the failure.
///
/// - Loads the fixture store once with [`load_fixtures`] from
///   `tests/fixtures/ifcap/` of this crate, before anything is served. Its
///   error, a fixture that cannot be read or a malformed line
///   (`InvalidData`), each naming the file, is returned as
///   `cannot load fixtures: <error>`.
/// - Then hands `listener` to `serve_until` with a shutdown flag that is
///   never set, so this returns only when serving fails after the store
///   loaded, as `serving failed: <error>`, where `<error>` names the step,
///   such as `accepting a connection failed: <cause>`.
pub fn serve(listener: TcpListener, mumps: MumpsMode) -> io::Result<()> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ifcap");
    let store = load_store(&fixtures)?;
    serve_until(listener, store, mumps, Arc::new(AtomicBool::new(false))).map_err(serving_failed)
}

/// The fixture store of [`serve`], loaded from `fixtures` by
/// [`load_fixtures`]. An error keeps its kind and its text, which names the
/// file, behind `cannot load fixtures: `.
fn load_store(fixtures: &Path) -> io::Result<Store> {
    load_fixtures(fixtures)
        .map_err(|e| io::Error::new(e.kind(), format!("cannot load fixtures: {e}")))
}

/// A failure of `serve_until` in [`serve`], after the store loaded, with its
/// kind and its text, which names the step, kept behind `serving failed: `.
fn serving_failed(e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("serving failed: {e}"))
}

/// Serves `listener` until `shutdown` is set, then returns `Ok(())`: the
/// serving loop behind [`serve`]; unit tests in this module drive it with a
/// shutdown flag.
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
fn serve_until(
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
/// read after any empty-line prelude (see [`skip_prelude`]). The bytes of the
/// head are freed once that method is known, before the request is routed,
/// and [`route`] takes the request, body included, so no copy of the request
/// outlives what the session needs of it.
fn exchange(stream: &mut TcpStream, store: &Store, mumps: &MumpsMode) -> io::Result<()> {
    let configured = stream
        .set_nonblocking(false)
        .and_then(|()| stream.set_write_timeout(Some(IO_TIMEOUT)));
    if let Err(e) = configured {
        return Err(abandon(stream, "configuring a connection", e));
    }
    let mut received: Vec<u8> = Vec::new();
    let read = read_request(stream, &mut received, IO_TIMEOUT, REQUEST_TIME);
    // Method tokens are case-sensitive, so only `HEAD` itself is a HEAD
    // request, rejected or not.
    let head_request = skip_prelude(&received).starts_with(b"HEAD ");
    drop(received);
    let response = match read {
        Err(e) => return Err(abandon(stream, "configuring a connection", e)),
        Ok(Ok(Some(request))) => route(request, store, mumps),
        // Nothing to answer: the peer sent nothing, or cannot be written to.
        Ok(Ok(None)) => return Ok(()),
        Ok(Err(cause)) => error_response(400, &cause),
    };
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
            // The body is allocated once at its declared length and never
            // grows past it: it takes what arrived with the head up to that
            // length, then reads into a window of only the bytes still owed.
            // Bytes past the declared length (a pipelined request) are not
            // part of this request: they are not copied, nor read from the
            // socket, and are not answered.
            let mut body = Vec::with_capacity(length);
            body.extend_from_slice(rest.get(..length).unwrap_or(rest));
            // A client that cannot be written to cannot be answered either.
            if expect_continue
                && body.len() < length
                && stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err()
            {
                return Ok(Ok(None));
            }
            while body.len() < length {
                let take = length.saturating_sub(body.len()).min(buf.len());
                let Some(window) = buf.get_mut(..take) else {
                    return Ok(Err(bad("INCOMPLETE BODY")));
                };
                match read_within(stream, window, started, request_time)? {
                    Ok(0) => return Ok(Err(bad("INCOMPLETE BODY"))),
                    Ok(n) => body.extend_from_slice(window.get(..n).unwrap_or(&[])),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => return Ok(Err(bad("INCOMPLETE BODY"))),
                }
            }
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
/// (RFC 9110 10.1.1). A target holding a byte other than visible ASCII or a
/// `%` not followed by two hex digits, or an absolute-form target whose
/// authority is not a host with an optional port, is an invalid request
/// target, rejected before any header is read (RFC 9112 3.2, RFC 3986 2.1
/// and 3.2.2). Empty lines before the request line are ignored (see
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
/// and query, an empty path becoming `/` (RFC 9112 3.2.2). The whole target,
/// in either form, must be visible ASCII in which each `%` begins a
/// percent-encoded octet (RFC 9112 3.2, RFC 3986 2.1); visible delimiters
/// that browsers send raw, such as `|` or `[`, stay accepted (see
/// [`is_target_text`]). The scheme is matched without regard to case. The
/// authority ends at the first `/`, `?` or `#` (RFC 3986 3.2) and takes no
/// part in routing, but it must be a host with an optional port, as a
/// `Host` value is (see [`is_host`], RFC 3986 3.2.2 and 3.2.3), so it is
/// never empty (RFC 9110 4.2.1) and never carries userinfo (RFC 9110
/// 4.2.4). `None` for any other target, such as `*` or another scheme.
fn origin_form(target: &str) -> Option<String> {
    if !is_target_text(target) {
        return None;
    }
    if target.starts_with('/') {
        return Some(target.to_string());
    }
    let (scheme, rest) = target.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path_and_query) = rest.split_at_checked(authority_end)?;
    if !is_host(authority) {
        return None;
    }
    if path_and_query.starts_with('/') {
        Some(path_and_query.to_string())
    } else {
        Some(format!("/{path_and_query}"))
    }
}

/// Request-target text: visible ASCII only, 0x21 to 0x7E, which covers
/// every octet the request-target grammar admits (RFC 9112 3.2), so no
/// control byte (HTAB, NUL and a CR or LF that does not end the line
/// included), no DEL and no raw byte from 0x80 up; and each `%` followed by
/// two hex digits (RFC 3986 2.1). A percent-encoded octet is never decoded,
/// so `%00` or `%2e%2e` is routed as sent. Visible characters RFC 3986
/// admits in a path or query only percent-encoded, such as `|`, `^`, `[`,
/// `]`, `{` or `}`, are accepted and routed as sent, matching no route:
/// browsers send several of them raw, since the WHATWG URL Standard's path
/// and query percent-encode sets leave them unencoded.
fn is_target_text(target: &str) -> bool {
    let mut bytes = target.bytes();
    while let Some(b) = bytes.next() {
        let valid = if b == b'%' {
            matches!(
                (bytes.next(), bytes.next()),
                (Some(high), Some(low)) if high.is_ascii_hexdigit() && low.is_ascii_hexdigit()
            )
        } else {
            b.is_ascii_graphic()
        };
        if !valid {
            return false;
        }
    }
    true
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
/// API path, or 404. A static path answers `GET` and `HEAD`, `/api/status`
/// only `GET`, and a session path only `POST`; any other method, `HEAD` on
/// an API path included, gets the path's 404, `GET REQUIRED` or
/// `POST REQUIRED`. The `HEAD` of a static path has the headers,
/// `Content-Length` included, of its `GET` (RFC 9110 9.3.2), and
/// [`write_response`] leaves the content of every `HEAD` answer out. The
/// request is taken, so its body goes to [`route_api`], which frees it once
/// the inputs are decoded.
fn route(request: HttpRequest, store: &Store, mumps: &MumpsMode) -> (u16, &'static str, Vec<u8>) {
    let (method, target, body) = request;
    if let Some((file, content_type)) = static_file(&target) {
        if method == "GET" || method == "HEAD" {
            return serve_static(file, content_type);
        }
        return error_response(404, GET_REQUIRED);
    }
    if let Some(rest) = target.strip_prefix("/api/") {
        let (status, json) = route_api(&method, rest, body, store, mumps);
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

/// `rest` is the target after `/api/`. `status` answers only `GET`, with
/// [`status_body`]; any other method, `HEAD` included, is 404
/// `GET REQUIRED`. `<backend>/session` answers only `POST`. Unknown backends
/// and paths, a query string included, and any other method are 404; the
/// body is checked only for a known session route, and a body
/// [`scan_session_body`] rejects is 400 `BAD REQUEST: <cause>`. An accepted
/// body's inputs are decoded once [`INPUT_BUDGET`] admits their session,
/// which may wait for heavier sessions to end; the body is freed then,
/// before the session runs, and the inputs before its charge is released.
/// The answer is `(status, JSON body)`.
fn route_api(
    method: &str,
    rest: &str,
    body: Option<Vec<u8>>,
    store: &Store,
    mumps: &MumpsMode,
) -> (u16, String) {
    let not_found = || (404, error_body(NOT_FOUND));
    if rest == "status" {
        return match method {
            "GET" => (200, status_body(mumps)),
            _ => (404, error_body(GET_REQUIRED)),
        };
    }
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
    let bad = |cause: &str| (400, error_body(&format!("{BAD_REQUEST}: {cause}")));
    let (text, array) = match scan_session_body(&body) {
        Ok(found) => found,
        Err(cause) => return bad(&cause),
    };
    // The charge is taken before the inputs exist and declared before them,
    // so they are freed first on every exit, unwinding included.
    let charge = INPUT_BUDGET.admit(array.count);
    let Some(inputs) = decode_inputs(text, array) else {
        return bad("MALFORMED JSON");
    };
    drop(body);
    let answer = session(store, mumps, &inputs);
    drop(inputs);
    drop(charge);
    answer
}

/// Where the first `inputs` array of a session body opens, and how many
/// elements it holds, as [`scan_session_body`] found them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct InputsArray {
    /// The byte offset of the array's `[` in the body.
    start: usize,
    /// The number of elements.
    count: usize,
}

/// The first element of the `inputs` array that a session cannot take, by
/// its position counted from 1.
#[derive(Clone, Copy)]
enum ElementFault {
    /// An element that is not a string.
    NotString(usize),
    /// A string whose content, unescaped, holds a carriage return or a line
    /// feed.
    LineBreak(usize),
}

/// The value of the first `inputs` member of a session body, as
/// [`scan_session_body`] found it.
enum InputsValue {
    /// A value that is not an array.
    NotArray,
    /// An array, with its first element fault, if any.
    Array(InputsArray, Option<ElementFault>),
}

/// Checks that `body` is `{"inputs": [string, ...]}` and finds its `inputs`
/// array, building nothing from the body: the inputs are decoded only once
/// their session is admitted (see [`decode_inputs`]). An element holding a
/// carriage return or line feed is rejected, because each input becomes one
/// line of a captured input file and of live standard input. Other members
/// of the object are ignored. Returns the body as text, with the array.
///
/// The whole body is checked, past a fault of the inputs too, and the cause
/// is the first of these that holds: the body is not UTF-8 or not exactly
/// one JSON value (`MALFORMED JSON`); the value is not an object
/// (`BODY IS NOT A JSON OBJECT`); no member, or more than one, has the key
/// `inputs`, compared unescaped (`INPUTS MISSING OR REPEATED`); the first
/// `inputs` value is not an array (`INPUTS IS NOT AN ARRAY`); then the first
/// element, by position from 1, that is not a string
/// (`INPUT <n> IS NOT A STRING`) or holds a carriage return or line feed
/// (`INPUT <n> CONTAINS A CARRIAGE RETURN OR LINE FEED`).
fn scan_session_body(body: &[u8]) -> Result<(&str, InputsArray), String> {
    let malformed = || "MALFORMED JSON".to_string();
    let text = std::str::from_utf8(body).map_err(|_| malformed())?;
    let mut pos = 0;
    skip_whitespace(text, &mut pos);
    let is_object = peek(text, pos) == Some(b'{');
    let mut inputs_members = 0usize;
    let mut first_inputs = None;
    let scanned = if is_object {
        scan_object(
            text,
            &mut pos,
            0,
            Some("inputs"),
            |pos, depth, is_inputs| {
                if is_inputs {
                    inputs_members = inputs_members.saturating_add(1);
                }
                if !is_inputs || inputs_members > 1 {
                    return scan_value(text, pos, depth);
                }
                let value = if depth <= MAX_JSON_DEPTH && peek(text, *pos) == Some(b'[') {
                    scan_inputs(text, pos, depth)?
                } else {
                    scan_value(text, pos, depth)?;
                    InputsValue::NotArray
                };
                first_inputs = Some(value);
                Some(())
            },
        )
    } else {
        scan_value(text, &mut pos, 0)
    };
    skip_whitespace(text, &mut pos);
    if scanned.is_none() || pos != text.len() {
        return Err(malformed());
    }
    if !is_object {
        return Err("BODY IS NOT A JSON OBJECT".to_string());
    }
    let (Some(value), 1) = (first_inputs, inputs_members) else {
        return Err("INPUTS MISSING OR REPEATED".to_string());
    };
    match value {
        InputsValue::NotArray => Err("INPUTS IS NOT AN ARRAY".to_string()),
        InputsValue::Array(_, Some(ElementFault::NotString(position))) => {
            Err(format!("INPUT {position} IS NOT A STRING"))
        }
        InputsValue::Array(_, Some(ElementFault::LineBreak(position))) => Err(format!(
            "INPUT {position} CONTAINS A CARRIAGE RETURN OR LINE FEED"
        )),
        InputsValue::Array(array, None) => Ok((text, array)),
    }
}

/// Checks the first `inputs` array of a session body, at `*pos` and nested
/// `depth` levels deep, and moves past it, building nothing: where it
/// opens, how many elements it holds, and the first element a session
/// cannot take. Every element is checked to its end, a faulty one included.
fn scan_inputs(text: &str, pos: &mut usize, depth: usize) -> Option<InputsValue> {
    let start = *pos;
    let mut count = 0usize;
    let mut fault = None;
    scan_array(text, pos, depth, |pos, depth| {
        count = count.saturating_add(1);
        if depth <= MAX_JSON_DEPTH && peek(text, *pos) == Some(b'"') {
            let mut line_break = false;
            walk_string(text, pos, |piece| {
                line_break |= piece.contains(['\r', '\n']);
            })?;
            if line_break && fault.is_none() {
                fault = Some(ElementFault::LineBreak(count));
            }
        } else {
            scan_value(text, pos, depth)?;
            if fault.is_none() {
                fault = Some(ElementFault::NotString(count));
            }
        }
        Some(())
    })?;
    Some(InputsValue::Array(InputsArray { start, count }, fault))
}

/// The elements of the `inputs` array that [`scan_session_body`] found in
/// `text`, unescaped and in order, in one pass over the array alone, into a
/// vector allocated once at exactly their count. `None`, which an accepted
/// body never gives, means `array` does not describe an accepted array of
/// `text`.
fn decode_inputs(text: &str, array: InputsArray) -> Option<Vec<String>> {
    let mut inputs = Vec::with_capacity(array.count);
    let mut pos = array.start;
    // The array is a member's value, one level deep, as it was scanned.
    scan_array(text, &mut pos, 1, |pos, _| {
        inputs.push(decode_string(text, pos)?);
        Some(())
    })?;
    (inputs.len() == array.count).then_some(inputs)
}

/// The string at `*pos`, unescaped, moving past it. The first piece
/// [`walk_string`] hands over is copied as it stands, so a string without
/// escapes, one piece, is held at exactly its length.
fn decode_string(text: &str, pos: &mut usize) -> Option<String> {
    let mut out = String::new();
    walk_string(text, pos, |piece| {
        if out.is_empty() {
            out = piece.to_owned();
        } else {
            out.push_str(piece);
        }
    })?;
    Some(out)
}

/// A bound on the decoded inputs that sessions hold at once, weighted by
/// their count. A session with at most `free` inputs is not charged and
/// never waits. A heavier one is charged its count, at most `capacity`, and
/// waits until the charges held leave room for it, so one over `capacity`
/// runs only while no other charge is held, and none ever blocks a light
/// one.
struct InputBudget {
    /// The sum of the charges held.
    held: Mutex<usize>,
    /// Signalled whenever a charge is released.
    released: Condvar,
    /// The most the charges held may sum to.
    capacity: usize,
    /// The most inputs a session may have without being charged.
    free: usize,
}

impl InputBudget {
    /// A budget of `capacity` inputs, none of them held, that charges only
    /// the sessions with more than `free` inputs.
    const fn new(capacity: usize, free: usize) -> InputBudget {
        InputBudget {
            held: Mutex::new(0),
            released: Condvar::new(),
            capacity,
            free,
        }
    }

    /// The charge of a session with `inputs` inputs, taken once the charges
    /// held leave room for it, and released when it is dropped. A lock
    /// poisoned by a thread that panicked while holding it is taken all the
    /// same: the sum it guards is only ever replaced whole.
    fn admit(&self, inputs: usize) -> InputCharge<'_> {
        let charge = if inputs <= self.free {
            0
        } else {
            inputs.min(self.capacity)
        };
        if charge > 0 {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            while held.saturating_add(charge) > self.capacity {
                held = self
                    .released
                    .wait(held)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            *held = held.saturating_add(charge);
        }
        InputCharge {
            budget: self,
            charge,
        }
    }
}

/// One session's charge against an [`InputBudget`]. Dropping it, on
/// unwinding too, releases the charge and wakes every session waiting for
/// room.
struct InputCharge<'b> {
    /// The budget charged.
    budget: &'b InputBudget,
    /// The inputs charged; 0 for a session that was not charged.
    charge: usize,
}

impl Drop for InputCharge<'_> {
    fn drop(&mut self) {
        if self.charge == 0 {
            return;
        }
        let mut held = self
            .budget
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *held = held.saturating_sub(self.charge);
        drop(held);
        self.budget.released.notify_all();
    }
}

// ---------------------------------------------------------------------------
// JSON reader and writer (private, minimal)
// ---------------------------------------------------------------------------

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

/// Checks the value at `*pos`, nested `depth` levels deep, and moves past
/// it, building nothing; a value nested deeper than `MAX_JSON_DEPTH` is
/// malformed. This is recursive descent over the bytes of `text`, and a
/// position is always a char boundary between tokens, because every token
/// delimiter is ASCII. Numbers are only checked, since nothing here
/// computes with them. Throughout this reader, `None` means malformed.
fn scan_value(text: &str, pos: &mut usize, depth: usize) -> Option<()> {
    if depth > MAX_JSON_DEPTH {
        return None;
    }
    match peek(text, *pos) {
        Some(b'{') => scan_object(text, pos, depth, None, |pos, depth, _| {
            scan_value(text, pos, depth)
        }),
        Some(b'[') => scan_array(text, pos, depth, |pos, depth| scan_value(text, pos, depth)),
        Some(b'"') => walk_string(text, pos, |_| {}),
        Some(b't') => eat_literal(text, pos, "true"),
        Some(b'f') => eat_literal(text, pos, "false"),
        Some(b'n') => eat_literal(text, pos, "null"),
        Some(b'-' | b'0'..=b'9') => skip_number(text, pos),
        _ => None,
    }
}

/// Checks the object at `*pos`, nested `depth` levels deep, and moves past
/// it. `member` checks each member's value, at its position and nested one
/// level deeper, and moves past it; it is told whether the member's key,
/// unescaped, equals `wanted`. Keys are compared, never kept, and what a
/// repeated key means is the caller's to decide.
fn scan_object(
    text: &str,
    pos: &mut usize,
    depth: usize,
    wanted: Option<&str>,
    mut member: impl FnMut(&mut usize, usize, bool) -> Option<()>,
) -> Option<()> {
    eat(text, pos, b'{')?;
    skip_whitespace(text, pos);
    if peek(text, *pos) == Some(b'}') {
        *pos = pos.saturating_add(1);
        return Some(());
    }
    loop {
        skip_whitespace(text, pos);
        // The part of `wanted` the key has still to match, while it matches.
        let mut unmatched = wanted;
        walk_string(text, pos, |piece| {
            unmatched = unmatched.and_then(|rest| rest.strip_prefix(piece));
        })?;
        skip_whitespace(text, pos);
        eat(text, pos, b':')?;
        skip_whitespace(text, pos);
        member(pos, depth.saturating_add(1), unmatched == Some(""))?;
        skip_whitespace(text, pos);
        match bump(text, pos) {
            Some(b',') => {}
            Some(b'}') => return Some(()),
            _ => return None,
        }
    }
}

/// Checks the array at `*pos`, nested `depth` levels deep, and moves past
/// it. `element` checks each element, at its position and nested one level
/// deeper, and moves past it.
fn scan_array(
    text: &str,
    pos: &mut usize,
    depth: usize,
    mut element: impl FnMut(&mut usize, usize) -> Option<()>,
) -> Option<()> {
    eat(text, pos, b'[')?;
    skip_whitespace(text, pos);
    if peek(text, *pos) == Some(b']') {
        *pos = pos.saturating_add(1);
        return Some(());
    }
    loop {
        skip_whitespace(text, pos);
        element(pos, depth.saturating_add(1))?;
        skip_whitespace(text, pos);
        match bump(text, pos) {
            Some(b',') => {}
            Some(b']') => return Some(()),
            _ => return None,
        }
    }
}

/// Checks the string at `*pos` and moves past it, handing `sink` its
/// content, unescaped, in order and in pieces, none of them empty: each run
/// without escapes as it stands in `text`, and the character of each
/// escape. Skipping a value, comparing a key, checking an input for line
/// breaks and decoding an input all read a string through this one walk.
/// The escapes are `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\u`
/// followed by four hexadecimal digits. A high-surrogate `\u` escape must be
/// followed by a low-surrogate `\u` escape; an unpaired surrogate, an
/// unknown escape or an unescaped control character is malformed.
fn walk_string(text: &str, pos: &mut usize, mut sink: impl FnMut(&str)) -> Option<()> {
    eat(text, pos, b'"')?;
    let mut run_start = *pos;
    loop {
        match peek(text, *pos) {
            None => return None,
            Some(byte @ (b'"' | b'\\')) => {
                let run = text.get(run_start..*pos)?;
                if !run.is_empty() {
                    sink(run);
                }
                *pos = pos.saturating_add(1);
                if byte == b'"' {
                    return Some(());
                }
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
                sink(unescaped.encode_utf8(&mut [0u8; 4]));
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

/// Appends `items` as a JSON array of strings, in order.
fn write_json_strings(out: &mut String, items: &[&str]) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_json_string(out, item);
    }
    out.push(']');
}

// ---------------------------------------------------------------------------
// Session backends
// ---------------------------------------------------------------------------

/// The MUMPS-side cause when no capture's inputs equal the request's.
const NOT_RECORDED: &str = "NOT RECORDED";

/// One MUMPS-side session result: the raw transcript and return value
/// answered with, and the capture whose inputs equal the session's, which
/// alone decides COMPARE's `match`. Each variant holds its output once.
enum MumpsRun {
    /// Replay: the capture whose inputs equal the session's. The transcript
    /// and return value are that capture's own, borrowed from it.
    Replay(Case),
    /// Live: the run's standard output as the transcript, and the capture
    /// whose inputs equal the session's, which may be absent. The return
    /// value is `None`, because `EN3^PRCHRPT` is entered by `DO` and returns
    /// nothing (open item O-06, closed).
    Live {
        transcript: String,
        capture: Option<Case>,
    },
}

impl MumpsRun {
    /// The raw transcript answered with, never normalised.
    fn transcript(&self) -> &str {
        match self {
            MumpsRun::Replay(case) => &case.transcript,
            MumpsRun::Live { transcript, .. } => transcript,
        }
    }

    /// The return value answered with: the capture's own in replay, and
    /// `None` in live mode (open item O-06, closed).
    fn returned(&self) -> Option<&str> {
        match self {
            MumpsRun::Replay(case) => case.returned.as_deref(),
            MumpsRun::Live { .. } => None,
        }
    }

    /// The capture that decides COMPARE's `match`; always present in replay.
    fn capture(&self) -> Option<&Case> {
        match self {
            MumpsRun::Replay(case) => Some(case),
            MumpsRun::Live { capture, .. } => capture.as_ref(),
        }
    }
}

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

/// The JSON body of `GET /api/status`:
/// `{"mumps_mode":"replay"|"live","recorded":[...],"ready":[...],
/// "incomplete":{"GC-0N":[check names]}}`, from [`golden::status`] of the
/// mode's golden folder, read afresh on every request. The lists describe
/// that folder in both modes.
fn status_body(mumps: &MumpsMode) -> String {
    let (mode, golden_dir) = match mumps {
        MumpsMode::Replay { golden_dir } => ("replay", golden_dir),
        MumpsMode::Live { golden_dir, .. } => ("live", golden_dir),
    };
    let status = golden::status(golden_dir);
    let mut out = String::from("{\"mumps_mode\":");
    write_json_string(&mut out, mode);
    out.push_str(",\"recorded\":");
    write_json_strings(&mut out, &status.recorded);
    out.push_str(",\"ready\":");
    write_json_strings(&mut out, &status.ready);
    out.push_str(",\"incomplete\":{");
    for (index, (id, failed)) in status.incomplete.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_json_string(&mut out, id);
        out.push(':');
        write_json_strings(&mut out, failed);
    }
    out.push_str("}}");
    out
}

/// `(status, JSON body)` of a Rust-side session: 200 with the transcript and
/// a `null` return value, or the 500 of [`inquiry_error_to_response`].
fn rust_session(store: &Store, inputs: &[String]) -> (u16, String) {
    match inquire(store, inputs) {
        Ok(output) => (
            200,
            session_body(&output.transcript, output.returned.as_deref()),
        ),
        Err(error) => inquiry_error_to_response(&error),
    }
}

/// `(status, JSON body)` of a MUMPS-side session: 200 with the raw
/// transcript and return value, or 502 with the cause, such as
/// `NOT RECORDED`.
fn mumps_session(mumps: &MumpsMode, inputs: &[String]) -> (u16, String) {
    match run_mumps(mumps, inputs) {
        Ok(run) => (200, session_body(run.transcript(), run.returned())),
        Err(cause) => (502, error_body(&cause)),
    }
}

/// `(status, JSON body)` of a compare session: the MUMPS side first, whose
/// failure is 502 `MUMPS: <cause>`; then the Rust side, whose
/// `CorruptStore` is 502 `RUST: CORRUPT STORE <node>` here, as the request
/// specifies for COMPARE; only then 200 with both raw outputs, never
/// normalised, and the boolean `match` of [`decide_match`].
fn compare_session(store: &Store, mumps: &MumpsMode, inputs: &[String]) -> (u16, String) {
    let original = match run_mumps(mumps, inputs) {
        Ok(run) => run,
        Err(cause) => return (502, error_body(&format!("MUMPS: {cause}"))),
    };
    let port = match inquire(store, inputs) {
        Ok(output) => output,
        Err(error) => return (502, error_body(&format!("RUST: {error}"))),
    };
    let matched = decide_match(
        original.capture(),
        original.transcript(),
        original.returned(),
        &port,
    );
    let mut out = String::from("{\"mumps\":");
    write_session(&mut out, original.transcript(), original.returned());
    out.push_str(",\"rust\":");
    write_session(&mut out, &port.transcript, port.returned.as_deref());
    out.push_str(",\"match\":");
    out.push_str(if matched { "true" } else { "false" });
    out.push('}');
    (200, out)
}

/// `match` of a compare response: the PASS rule of AAP 0.7.2 applied to one
/// request, the same in replay and live mode (AAP 0.3.5.1; conflict K11).
/// True only when a capture's inputs equal the request's, that capture is
/// READY, [`golden::validate_context`] accepts its recorded context, both
/// transcripts are equal after the capture's rules, and both return values
/// are equal. Otherwise false, inputs with no capture included, which only
/// live mode answers with 200. The rules apply only inside this comparison;
/// the response carries the raw outputs.
fn decide_match(
    capture: Option<&Case>,
    mumps_transcript: &str,
    mumps_returned: Option<&str>,
    port: &InquiryOutput,
) -> bool {
    let Some(case) = capture else {
        return false;
    };
    if case.readiness != Readiness::Ready {
        return false;
    }
    let context_accepted = case
        .meta
        .as_ref()
        .and_then(|meta| meta.context().ok())
        .is_some_and(|context| golden::validate_context(&context).is_ok());
    context_accepted
        && golden::normalise(mumps_transcript, &case.rules)
            == golden::normalise(&port.transcript, &case.rules)
        && mumps_returned == port.returned.as_deref()
}

/// The MUMPS side of a session. Replay answers with the capture that
/// [`golden::find_capture`] finds for `inputs`, raw: no capture is
/// `NOT RECORDED`, and a capture file that cannot be read is
/// `NOT RECORDED (unreadable: <path>: <cause>)`. Live mode answers with
/// [`live`], and looks the capture up only for `match`, a lookup that fails
/// leaving none.
fn run_mumps(mumps: &MumpsMode, inputs: &[String]) -> Result<MumpsRun, String> {
    match mumps {
        MumpsMode::Replay { golden_dir } => match golden::find_capture(golden_dir, inputs) {
            Ok(Some(case)) => Ok(MumpsRun::Replay(case)),
            Ok(None) => Err(NOT_RECORDED.to_string()),
            Err(cause) => Err(format!("{NOT_RECORDED} (unreadable: {cause})")),
        },
        MumpsMode::Live {
            container,
            golden_dir,
        } => {
            let transcript = live(container, golden_dir, inputs)?;
            Ok(MumpsRun::Live {
                transcript,
                capture: golden::find_capture(golden_dir, inputs).ok().flatten(),
            })
        }
    }
}

/// Runs the original live: the start arguments and context come from
/// [`golden::live_start`], the context must pass [`golden::validate_context`]
/// before anything runs, and the container comes only from the bridge's own
/// configuration. The one command is `docker exec -i <container>` followed
/// by the recorded start arguments, run directly, never through a shell.
/// Standard input is each input followed by one `\n`, the form of a captured
/// input file. Once the context has passed, the run takes one of the
/// [`LIVE_SLOTS`] that every connection shares; a request that gets none
/// within [`LIVE_ADMISSION_WAIT`] starts nothing and fails as `TIMEOUT`.
/// Standard output is the transcript only when the run succeeded; any other
/// outcome is the [`LiveFailure`] text, such as
/// `EXIT <code>: <first line of docker's standard error>`.
fn live(container: &str, golden_dir: &Path, inputs: &[String]) -> Result<String, String> {
    let (start_args, context) = golden::live_start(golden_dir)?;
    golden::validate_context(&context)?;
    let mut args = vec!["exec".to_string(), "-i".to_string(), container.to_string()];
    args.extend(start_args);
    let mut stdin = String::new();
    for input in inputs {
        stdin.push_str(input);
        stdin.push('\n');
    }
    run_admitted(
        &LIVE_ADMISSION,
        LIVE_ADMISSION_WAIT,
        "docker",
        &args,
        stdin.as_bytes(),
    )
    .map_err(|failure| failure.to_string())
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

// ---------------------------------------------------------------------------
// Live execution
// ---------------------------------------------------------------------------

/// How one [`run_command`] child ended.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecResult {
    /// When nothing could be started, the stage and the OS error, as
    /// [`execute`] gives them, such as `CANNOT RUN <program>: <error>`;
    /// otherwise, when the run itself failed (a stream worker that could not
    /// start, failed or panicked, output not read to its end, or a child
    /// that could not be waited on or killed), each failure's stage and
    /// cause, joined by `; `. A run with this set is never a transcript,
    /// whatever the other fields hold.
    spawn_error: Option<String>,
    /// The exit code; `None` when the child was killed or ended by a signal.
    exit_code: Option<i32>,
    timed_out: bool,
    output_limit_exceeded: bool,
    /// At most `output_limit` bytes of standard output.
    stdout: Vec<u8>,
    /// At most `output_limit` bytes of standard error.
    stderr: Vec<u8>,
}

/// Why a live run gives no transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LiveFailure {
    /// [`ExecResult::spawn_error`], the 502 cause as it stands, such as
    /// `CANNOT RUN docker: No such file or directory (os error 2)`.
    Spawn(String),
    NonZeroExit {
        code: Option<i32>,
        /// The first line of standard error, as [`decide_live_result`]
        /// takes it.
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
            LiveFailure::Spawn(cause) => f.write_str(cause),
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
/// or as soon as standard output passes `output_limit` bytes, and only the
/// child itself is killed, never a process it started ([`kill_and_reap`]);
/// at most `output_limit` bytes of each stream are kept. Once the run is
/// over, the stream threads are cancelled, which stops each within about
/// `POLL` even while a descendant still holds the child's streams, and
/// every thread that started is joined before this returns. A run that
/// could not start, such as `CANNOT RUN <program>: <error>` from
/// [`execute`], and a failure of the run itself rather than of the
/// program, such as a thread that could not start or output that could
/// not be read, are reported in [`ExecResult::spawn_error`].
fn run_command(
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

/// [`run_command`] with its stream threads started through `start`. `Err`
/// only when nothing could be started: the child's streams could not be
/// created, `STANDARD STREAMS NOT CREATED: <error>`, or the program could
/// not be started, `CANNOT RUN <program>: <error>`. `<error>` is the OS
/// error's text, and the error keeps its kind.
fn execute(
    program: &str,
    args: &[String],
    stdin: &[u8],
    timeout: Duration,
    output_limit: usize,
    start: &mut StartWorker,
) -> io::Result<ExecResult> {
    let started = Instant::now();
    let (streams, (child_stdin, child_stdout, child_stderr)) = stream_pairs()
        .map_err(|e| io::Error::new(e.kind(), format!("STANDARD STREAMS NOT CREATED: {e}")))?;
    // One statement, so the parent's copies of the child's ends are closed
    // as soon as the child holds them; end of output depends on it.
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::from(OwnedFd::from(child_stdin)))
        .stdout(Stdio::from(OwnedFd::from(child_stdout)))
        .stderr(Stdio::from(OwnedFd::from(child_stderr)))
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("CANNOT RUN {program}: {e}")))?;
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
///
/// Only the child itself is killed. A process it started is not signalled,
/// because stable std has no call that signals a process group, and this
/// crate admits only safe code, takes no dependency and starts no second
/// command. Such a process can keep running after the run ends, outside
/// [`LIVE_SLOTS`]. While it holds the child's standard output or standard
/// error, that output has not ended, so the run lasts until its `timeout`
/// and ends as a timeout even when the child exited 0.
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
/// 0 (with the first line of standard error: the bytes before its first
/// `\n`, or all of them when it has none, less one trailing `\r`, decoded
/// lossily), then standard output that is not UTF-8.
fn decide_live_result(r: &ExecResult) -> Result<String, LiveFailure> {
    if let Some(cause) = &r.spawn_error {
        return Err(LiveFailure::Spawn(cause.clone()));
    }
    if r.timed_out {
        return Err(LiveFailure::Timeout);
    }
    if r.output_limit_exceeded {
        return Err(LiveFailure::OutputTooLarge);
    }
    if r.exit_code != Some(0) {
        let line = r
            .stderr
            .split(|&byte| byte == b'\n')
            .next()
            .unwrap_or_default();
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        return Err(LiveFailure::NonZeroExit {
            code: r.exit_code,
            stderr_head: String::from_utf8_lossy(line).into_owned(),
        });
    }
    String::from_utf8(r.stdout.clone()).map_err(|_| LiveFailure::NonUtf8)
}

/// The status and complete JSON body for a Rust-side error outside COMPARE:
/// `CorruptStore(record)` is 500 with its `CORRUPT STORE <record>` text,
/// naming the record verbatim. The request defines no status for a Rust-side
/// failure, so 500 is kept (conflict K8).
fn inquiry_error_to_response(e: &InquiryError) -> (u16, String) {
    (500, error_body(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
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

    /// The inputs of a session body, scanned and then decoded as
    /// `route_api` does, without the admission between the two.
    fn session_inputs(body: &[u8]) -> Result<Vec<String>, String> {
        let (text, array) = scan_session_body(body)?;
        decode_inputs(text, array).ok_or_else(|| "decoding failed after the scan".to_string())
    }

    #[test]
    fn json_reads_escapes_and_surrogate_pairs() -> Result<(), String> {
        let escaped = r#""q\" b\\ s\/ \b\f\n\r\t \u0041\u00e9\u20ac \ud83d\ude00 \u0007""#;
        let unescaped = "q\" b\\ s/ \u{8}\u{c}\n\r\t A\u{e9}\u{20ac} \u{1f600} \u{7}";
        let mut pos = 0;
        let text = format!("{escaped} ");
        assert_eq!(decode_string(&text, &mut pos).as_deref(), Some(unescaped));
        assert_eq!(
            pos,
            escaped.len(),
            "the reader stops past the closing quote"
        );
        // A string without escapes is held at exactly its length.
        let mut pos = 0;
        let plain = decode_string(r#""plain text é""#, &mut pos).ok_or("plain string rejected")?;
        assert_eq!(
            (plain.as_str(), plain.capacity()),
            ("plain text é", plain.len())
        );
        // Every kind of value, in members a session ignores, and the same
        // escapes in an input, which may hold no CR or LF.
        let body = format!(
            r#" {{"s":{escaped},
            "n":-1.5e3, "z":0, "a":[true,false,null], "o":{{}},
            "inputs":["q\" b\\ s\/ \b\f\t \u0041\u00e9\u20ac \ud83d\ude00 \u0007",""]}} "#
        );
        assert_eq!(
            session_inputs(body.as_bytes()),
            Ok(strings(&[
                "q\" b\\ s/ \u{8}\u{c}\t A\u{e9}\u{20ac} \u{1f600} \u{7}",
                ""
            ]))
        );
        // Each reader takes exactly one value of its own kind.
        assert_eq!(decode_string("-1.5e3", &mut 0), None);
        assert_eq!(scan_array("{}", &mut 0, 0, |_, _| None), None);
        assert_eq!(scan_object("[]", &mut 0, 0, None, |_, _, _| None), None);
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
        let malformed_json = Err("MALFORMED JSON".to_string());
        for text in malformed {
            // As the whole body, and as the value of a member a session
            // ignores.
            assert_eq!(session_inputs(text.as_bytes()), malformed_json, "{text:?}");
            let ignored = format!(r#"{{"x":{text},"inputs":[]}}"#);
            assert_eq!(
                session_inputs(ignored.as_bytes()),
                malformed_json,
                "{ignored:?}"
            );
        }
        // The top-level value is at depth 0, so one more array than the
        // limit still nests within it.
        let nested = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let not_object = Err("BODY IS NOT A JSON OBJECT".to_string());
        for depth in [MAX_JSON_DEPTH, MAX_JSON_DEPTH.saturating_add(1)] {
            assert_eq!(
                session_inputs(nested(depth).as_bytes()),
                not_object,
                "{depth}"
            );
        }
        let too_deep = nested(MAX_JSON_DEPTH.saturating_add(2));
        assert_eq!(session_inputs(too_deep.as_bytes()), malformed_json);
        Ok(())
    }

    #[test]
    fn json_writes_escapes() -> Result<(), String> {
        let original = "q\" b\\ \n\r\t\u{8}\u{c}\u{7}\u{1f}\u{0} \u{e9}\u{20ac}\u{1f600} /";
        let mut out = String::new();
        write_json_string(&mut out, original);
        assert_eq!(out, r#""q\" b\\ \n\r\t\b\f\u0007\u001f\u0000 é€😀 /""#);
        let mut pos = 0;
        let read_back = decode_string(&out, &mut pos)
            .ok_or_else(|| "writer output does not parse".to_string())?;
        assert_eq!((read_back.as_str(), pos), (original, out.len()));
        // Written without its CR and LF, it is read back as an input.
        let input = original.replace(['\r', '\n'], "");
        let mut element = String::new();
        write_json_string(&mut element, &input);
        let body = format!(r#"{{"inputs":[{element}]}}"#);
        assert_eq!(session_inputs(body.as_bytes()), Ok(vec![input]));
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
        let cause = "CANNOT RUN docker: No such file or directory (os error 2)";
        let result = ExecResult {
            spawn_error: Some(cause.to_string()),
            timed_out: true,
            output_limit_exceeded: true,
            exit_code: None,
            ..exited_ok()
        };
        let failure = decide_live_result(&result);
        assert_eq!(failure, Err(LiveFailure::Spawn(cause.to_string())));
        assert_eq!(failure.map_err(|f| f.to_string()), Err(cause.to_string()));
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
        // A first line of 250 bytes, kept whole, then a second line.
        let mut long = vec![b'L'; 250];
        long.extend(b"\nrest");
        // A first line of 251 bytes: 199 ASCII bytes, `é` (0xC3 0xA9) as
        // bytes 200 and 201, then 50 more, kept whole with the `é` intact.
        let mut crossing = vec![b'a'; 199];
        crossing.extend("é".bytes());
        crossing.extend(std::iter::repeat_n(b'z', 50));
        assert_eq!(crossing.get(199..201), Some([0xC3, 0xA9].as_slice()));
        crossing.extend(b"\nnext line\n");
        // A first line whose first byte is not UTF-8.
        let mut invalid_first = vec![0xFF];
        invalid_first.extend(b"bad byte\nsecond line\n");
        let stderrs = [
            (
                b"first failure\nextra diagnostic\n".to_vec(),
                "first failure".to_string(),
            ),
            (
                b"first failure\r\nmore\r\n".to_vec(),
                "first failure".to_string(),
            ),
            // Only the one `\r` of the line end goes.
            (
                b"first failure\r\r\nmore\n".to_vec(),
                "first failure\r".to_string(),
            ),
            (long, "L".repeat(250)),
            (crossing, format!("{}é{}", "a".repeat(199), "z".repeat(50))),
            (invalid_first, "\u{fffd}bad byte".to_string()),
            (
                b"no such container".to_vec(),
                "no such container".to_string(),
            ),
            (Vec::new(), String::new()),
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
        // The cause names the stage and the program, and the error keeps
        // the OS error's kind.
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(
            error
                .to_string()
                .starts_with(&format!("CANNOT RUN {absent}: ")),
            "{error}"
        );
        assert_eq!(missing.spawn_error, Some(error.to_string()));
        // A program that exists but cannot be executed fails; only an
        // absent program is NOT RUN.
        let not_executable = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let refused = run_available(not_executable, &[], b"", timeout, 4096);
        let named = format!("could not be started: CANNOT RUN {not_executable}: ");
        assert!(
            matches!(&refused, Err(cause) if cause.contains(&named)),
            "{refused:?}"
        );
        // The `cat` and `false` probes are independent: an absent program
        // is NOT RUN on its own and leaves the other probe to run.
        if let Some(echoed) =
            run_available("cat", &[], b"\nA1\n^\n", Duration::from_secs(10), 4096)?
        {
            assert_eq!(decide_live_result(&echoed), Ok("\nA1\n^\n".to_string()));
        }
        if let Some(failed) = run_available("false", &[], b"", Duration::from_secs(10), 4096)? {
            assert_eq!(failed.exit_code, Some(1));
            let decision = decide_live_result(&failed);
            assert!(
                matches!(
                    decision,
                    Err(LiveFailure::NonZeroExit { code: Some(1), .. })
                ),
                "{decision:?}"
            );
        }
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
    fn input_budget_holds_back_only_heavy_sessions() -> Result<(), String> {
        let budget = &InputBudget::new(10, 2);
        let held = || *budget.held.lock().unwrap_or_else(PoisonError::into_inner);
        // A light session is admitted uncharged with the budget full, and
        // heavy sessions that fit together are admitted together.
        let full = budget.admit(10);
        let light = budget.admit(2);
        assert_eq!((full.charge, light.charge, held()), (10, 0, 10));
        drop((light, full));
        let halves = (budget.admit(5), budget.admit(5));
        assert_eq!(held(), 10);
        drop(halves);
        assert_eq!(held(), 0);
        // (the inputs of the session holding a charge, the inputs of a
        // session that must wait for it, the charge that one then holds):
        // a heavy session that does not fit, and one over the capacity,
        // which waits for the budget to empty.
        let waits = [(10, 3, 3), (3, 25, 10)];
        for (holding, inputs, charged) in waits {
            let holder = budget.admit(holding);
            thread::scope(|scope| -> Result<(), String> {
                let (admitted, admissions) = mpsc::channel();
                let (release, released) = mpsc::channel::<()>();
                let waiter = thread::Builder::new()
                    .spawn_scoped(scope, move || {
                        let charge = budget.admit(inputs);
                        let _ = admitted.send(charge.charge);
                        let _ = released.recv_timeout(Duration::from_secs(30));
                        drop(charge);
                    })
                    .map_err(|e| format!("starting the waiter: {e}"))?;
                assert_eq!(
                    admissions.recv_timeout(Duration::from_millis(100)),
                    Err(mpsc::RecvTimeoutError::Timeout),
                    "{inputs} inputs admitted while {holding} were held"
                );
                drop(holder);
                assert_eq!(
                    admissions.recv_timeout(Duration::from_secs(10)),
                    Ok(charged)
                );
                assert_eq!(held(), charged);
                let _ = release.send(());
                waiter.join().map_err(|_| "the waiter panicked".to_string())
            })?;
            assert_eq!(held(), 0, "a charge of {charged} was not released");
        }
        // A lock poisoned by a thread that panicked holding it still admits
        // and releases.
        thread::scope(|scope| -> Result<(), String> {
            let poisoner = thread::Builder::new()
                .spawn_scoped(scope, || -> Result<(), String> {
                    let _held = budget.held.lock();
                    std::panic::resume_unwind(Box::new("poisoning the budget"))
                })
                .map_err(|e| format!("starting the poisoner: {e}"))?;
            assert!(poisoner.join().is_err(), "the poisoner did not unwind");
            Ok(())
        })?;
        assert!(budget.held.is_poisoned());
        let charge = budget.admit(5);
        assert_eq!(held(), 5);
        drop(charge);
        assert_eq!(held(), 0);
        Ok(())
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
        let named = format!("CANNOT RUN {absent}: ");
        assert!(
            matches!(&attempted, Err(LiveFailure::Spawn(cause)) if cause.starts_with(&named)),
            "{attempted:?}"
        );
        assert_eq!(held_slots(&slots), 0, "a slot was not freed");
        Ok(())
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

    /// A new directory under the system temporary directory. A test that
    /// reaches its end removes it with everything in it by
    /// [`Scratch::close`], which reports a failed removal; `Drop` removes it
    /// best effort when a test fails early or unwinds. Each attempt
    /// takes the next name `ifcap-bridge-unit-<pid>-<n>` and creates it with
    /// `fs::create_dir`, which fails on any existing entry, so the directory
    /// belongs to this test alone.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new() -> Result<Scratch, String> {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            for _ in 0..100 {
                let n = NEXT.fetch_add(1, Ordering::SeqCst);
                let name = format!("ifcap-bridge-unit-{}-{n}", std::process::id());
                let path = std::env::temp_dir().join(name);
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Scratch { path }),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(format!("creating {}: {e}", path.display())),
                }
            }
            Err("no free scratch directory name".to_string())
        }

        fn path(&self) -> &Path {
            &self.path
        }

        /// Writes `text` to the capture file `name` of case `id`, creating
        /// the case folder when it is missing.
        fn write(&self, id: &str, name: &str, text: &str) -> Result<(), String> {
            let dir = self.path.join(id);
            fs::create_dir_all(&dir)
                .and_then(|()| fs::write(dir.join(name), text))
                .map_err(|e| format!("writing {id}/{name}: {e}"))
        }

        /// Removes the directory with everything in it, on a test's normal
        /// path. A failure, a directory already gone included, is an error
        /// naming the directory, so a passing test cannot leave it behind
        /// unnoticed.
        fn close(mut self) -> Result<(), String> {
            let path = std::mem::take(&mut self.path);
            fs::remove_dir_all(&path).map_err(|e| format!("removing {}: {e}", path.display()))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // Best-effort removal for a test that ends before `close`, by an
            // early error return or an unwind: that test has already failed,
            // and a removal failure cannot be reported from here. A closed
            // `Scratch` holds an empty path and is skipped.
            if !self.path.as_os_str().is_empty() {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    /// The capture file of `golden::CAPTURE_FILES` whose name before its
    /// extension is `stem`, so that this module never spells a capture file
    /// name itself.
    fn capture_file(stem: &str) -> Result<&'static str, String> {
        golden::CAPTURE_FILES
            .iter()
            .copied()
            .find(|name| name.split('.').next() == Some(stem))
            .ok_or_else(|| format!("no capture file {stem}"))
    }

    /// A capture's recorded metadata text holding each of the eight
    /// `golden::META_KEYS` once, `context` set to `context` and every other
    /// key to `x`.
    fn meta_text(context: &str) -> String {
        golden::META_KEYS
            .iter()
            .map(|key| {
                let value = if *key == "context" { context } else { "x" };
                format!("{key}={value}\n")
            })
            .collect()
    }

    /// The `STUB_CONTEXT` keys with sample device values, which
    /// `golden::validate_context` accepts.
    const CONTEXT: &str = r##"{"DUZ":".5","IOF":"#","IOSL":"24","STATION":"500"}"##;

    /// Site (a) of `SPEC.md` "Contracts": the only order filed under `C` has
    /// a zero node holding only a descendant, so the lookup's read of
    /// `^PRC(442,1,0)` [DIC3.m:110] gives `CorruptStore` for the inputs
    /// `""`, `A12345`, `^`.
    const CORRUPT_ORDER: &str = r#"^PRC(442,0)="PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^1^1"
^PRC(442,1,0,1)="x"
^PRC(442,"C","A12345",1)=""
"#;

    fn fixture_store() -> Result<Store, String> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("ifcap");
        load_fixtures(&fixtures).map_err(io_text)
    }

    /// A capture held in memory, with `readiness`, `meta` and the
    /// normalisation `rules` given.
    fn case(readiness: Readiness, meta: Option<golden::Meta>, rules: &[(&str, &str)]) -> Case {
        Case {
            id: "GC-01".to_string(),
            inputs: strings(&["^"]),
            transcript: String::new(),
            returned: None,
            rules: rules
                .iter()
                .map(|(open, close)| golden::Rule {
                    open: (*open).to_string(),
                    close: (*close).to_string(),
                })
                .collect(),
            meta,
            readiness,
        }
    }

    fn port(transcript: &str, returned: Option<&str>) -> InquiryOutput {
        InquiryOutput {
            transcript: transcript.to_string(),
            returned: returned.map(str::to_string),
        }
    }

    #[test]
    fn decide_match_is_the_pass_rule() -> Result<(), String> {
        let valid = Some(golden::parse_meta(&meta_text(CONTEXT))?);
        let duz_one = Some(golden::parse_meta(&meta_text(
            r##"{"DUZ":"1","IOF":"#","IOSL":"24","STATION":"500"}"##,
        ))?);
        let unparsable = Some(golden::parse_meta(&meta_text("not json"))?);
        let ready = Readiness::Ready;
        let incomplete = Readiness::Incomplete(vec!["db_before_equals_after"]);
        let marked = [("@", "#")];
        // (label, capture, MUMPS transcript, MUMPS return value, Rust output,
        // expected match)
        let cases = [
            (
                "no capture (live, unmatched)",
                None,
                "T",
                None,
                port("T", None),
                false,
            ),
            (
                "READY, context DUZ 1",
                Some(case(ready.clone(), duz_one, &[])),
                "T",
                None,
                port("T", None),
                false,
            ),
            (
                "INCOMPLETE, equal texts",
                Some(case(incomplete, valid.clone(), &[])),
                "T",
                None,
                port("T", None),
                false,
            ),
            (
                "READY, no recorded metadata",
                Some(case(ready.clone(), None, &[])),
                "T",
                None,
                port("T", None),
                false,
            ),
            (
                "READY, context not an object",
                Some(case(ready.clone(), unparsable, &[])),
                "T",
                None,
                port("T", None),
                false,
            ),
            (
                "READY, valid context, equal texts",
                Some(case(ready.clone(), valid.clone(), &[])),
                "T",
                None,
                port("T", None),
                true,
            ),
            (
                "READY, differing texts",
                Some(case(ready.clone(), valid.clone(), &[])),
                "T",
                None,
                port("U", None),
                false,
            ),
            (
                "READY, differing return values",
                Some(case(ready.clone(), valid.clone(), &[])),
                "T",
                Some("1"),
                port("T", None),
                false,
            ),
            (
                "READY, rule makes the texts equal",
                Some(case(ready.clone(), valid.clone(), &marked)),
                "at @07# end",
                None,
                port("at @99# end", None),
                true,
            ),
            (
                "READY, rule finds no span on one side",
                Some(case(ready, valid, &marked)),
                "at @07# end",
                None,
                port("at 99 end", None),
                false,
            ),
        ];
        for (label, capture, transcript, returned, rust, expected) in cases {
            assert_eq!(
                decide_match(capture.as_ref(), transcript, returned, &rust),
                expected,
                "{label}"
            );
        }
        Ok(())
    }

    #[test]
    fn status_json_shape() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let replay = MumpsMode::Replay {
            golden_dir: scratch.path().to_path_buf(),
        };
        let live = MumpsMode::Live {
            container: "vehu".to_string(),
            golden_dir: scratch.path().to_path_buf(),
        };
        assert_eq!(
            status_body(&replay),
            r#"{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}"#
        );
        assert_eq!(
            status_body(&live),
            r#"{"mumps_mode":"live","recorded":[],"ready":[],"incomplete":{}}"#
        );
        // GC-01 holds only the first two capture files, an input sequence
        // ending with the exit input and a transcript: recorded, INCOMPLETE.
        for name in golden::CAPTURE_FILES.iter().take(2) {
            scratch.write("GC-01", name, "^\n")?;
        }
        let incomplete = r#"{"GC-01":["seven_files_present","meta_keys_present","normalisation_parses","db_before_equals_after"]}"#;
        for (mode, label) in [(&replay, "replay"), (&live, "live")] {
            assert_eq!(
                status_body(mode),
                format!(
                    r#"{{"mumps_mode":"{label}","recorded":["GC-01"],"ready":[],"incomplete":{incomplete}}}"#
                )
            );
        }
        scratch.close()
    }

    #[test]
    fn sessions_answer_from_the_capture_whose_inputs_match() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let fixtures = fixture_store()?;
        let mumps = MumpsMode::Replay {
            golden_dir: scratch.path().to_path_buf(),
        };
        let inputs = strings(&["^"]);
        let other = strings(&["", "^"]);
        let not_recorded = (502, error_body(NOT_RECORDED));
        assert_eq!(mumps_session(&mumps, &inputs), not_recorded);
        assert_eq!(
            compare_session(&fixtures, &mumps, &inputs),
            (502, error_body("MUMPS: NOT RECORDED"))
        );
        // GC-01 is recorded but INCOMPLETE: five capture files are missing.
        let (input, transcript) = (capture_file("input")?, capture_file("transcript")?);
        scratch.write("GC-01", input, "^\n")?;
        scratch.write("GC-01", transcript, "RECORDED\n")?;
        assert_eq!(
            mumps_session(&mumps, &inputs),
            (200, session_body("RECORDED\n", None))
        );
        assert_eq!(mumps_session(&mumps, &other), not_recorded);
        // COMPARE answers both raw outputs, and `match` is false because the
        // capture is not READY.
        let rust = inquire(&fixtures, &inputs).map_err(|e| e.to_string())?;
        let mut compared = String::from("{\"mumps\":");
        write_session(&mut compared, "RECORDED\n", None);
        compared.push_str(",\"rust\":");
        write_session(&mut compared, &rust.transcript, rust.returned.as_deref());
        compared.push_str(",\"match\":false}");
        assert_eq!(compare_session(&fixtures, &mumps, &inputs), (200, compared));
        assert_eq!(
            rust_session(&fixtures, &inputs),
            (200, session_body(&rust.transcript, None))
        );
        // A Rust-side `CorruptStore` is 500 alone and 502 inside COMPARE.
        let corrupt = Store::from_zwr(CORRUPT_ORDER).map_err(|e| e.to_string())?;
        let corrupt_inputs = strings(&["", "A12345", "^"]);
        scratch.write("GC-02", input, "\nA12345\n^\n")?;
        scratch.write("GC-02", transcript, "RECORDED\n")?;
        assert_eq!(
            rust_session(&corrupt, &corrupt_inputs),
            (500, error_body("CORRUPT STORE ^PRC(442,1,0)"))
        );
        assert_eq!(
            compare_session(&corrupt, &mumps, &corrupt_inputs),
            (502, error_body("RUST: CORRUPT STORE ^PRC(442,1,0)"))
        );
        // GC-03 is recorded, but a directory in place of its input file
        // cannot be read: inputs no earlier case matches name it.
        fs::create_dir_all(scratch.path().join("GC-03").join(input)).map_err(io_text)?;
        scratch.write("GC-03", transcript, "RECORDED\n")?;
        let cause = golden::find_capture(scratch.path(), &other)
            .err()
            .ok_or_else(|| "the unreadable capture was not reported".to_string())?;
        assert!(cause.contains("GC-03"), "{cause}");
        let unreadable = format!("NOT RECORDED (unreadable: {cause})");
        assert_eq!(
            mumps_session(&mumps, &other),
            (502, error_body(&unreadable))
        );
        assert_eq!(
            compare_session(&fixtures, &mumps, &other),
            (502, error_body(&format!("MUMPS: {unreadable}")))
        );
        // A capture found before the unreadable case still answers.
        assert_eq!(
            mumps_session(&mumps, &inputs),
            (200, session_body("RECORDED\n", None))
        );
        scratch.close()
    }

    #[test]
    fn live_checks_the_context_before_any_command() -> Result<(), String> {
        let scratch = Scratch::new()?;
        // No container has this name, so a command that ran would fail with
        // an `EXIT` or spawn cause, never with either cause asserted here.
        let container = "ifcap-bridge-unit-no-such-container";
        let mumps = MumpsMode::Live {
            container: container.to_string(),
            golden_dir: scratch.path().to_path_buf(),
        };
        let store = Store::from_zwr("").map_err(|e| e.to_string())?;
        let inputs = strings(&["^"]);
        // No recorded start arguments anywhere: golden's own cause.
        let none = golden::live_start(scratch.path())
            .err()
            .ok_or_else(|| "an empty golden folder gave start arguments".to_string())?;
        assert_eq!(live(container, scratch.path(), &inputs), Err(none.clone()));
        assert_eq!(mumps_session(&mumps, &inputs), (502, error_body(&none)));
        // Recorded start arguments whose context names another user.
        let duz_one = r##"{"DUZ":"1","IOF":"#","IOSL":"24","STATION":"500"}"##;
        scratch.write("GC-01", capture_file("meta")?, &meta_text(duz_one))?;
        let mismatch = "CONTEXT MISMATCH DUZ: expected .5, actual 1";
        assert_eq!(
            live(container, scratch.path(), &inputs),
            Err(mismatch.to_string())
        );
        assert_eq!(
            compare_session(&store, &mumps, &inputs),
            (502, error_body(&format!("MUMPS: {mismatch}")))
        );
        scratch.close()
    }

    #[test]
    fn serve_until_answers_until_shutdown_is_set() -> Result<(), String> {
        let scratch = Scratch::new()?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(io_text)?;
        let addr = listener.local_addr().map_err(io_text)?;
        let store = Store::from_zwr("").map_err(|e| e.to_string())?;
        let mumps = MumpsMode::Replay {
            golden_dir: scratch.path().to_path_buf(),
        };
        let status = status_body(&mumps);
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&shutdown);
        let serving = thread::Builder::new()
            .spawn(move || serve_until(listener, store, mumps, flag))
            .map_err(io_text)?;
        let exchanged = (|| -> Result<Vec<u8>, String> {
            let mut client = TcpStream::connect(addr).map_err(io_text)?;
            client
                .set_read_timeout(Some(IO_TIMEOUT))
                .and_then(|()| client.set_write_timeout(Some(IO_TIMEOUT)))
                .map_err(io_text)?;
            client
                .write_all(b"GET /api/status HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .map_err(io_text)?;
            let mut response = Vec::new();
            client.read_to_end(&mut response).map_err(io_text)?;
            Ok(response)
        })();
        // Set on every path, so the serving thread always ends and is joined.
        shutdown.store(true, Ordering::SeqCst);
        let served = serving
            .join()
            .map_err(|_| "the serving loop panicked".to_string())?;
        served.map_err(io_text)?;
        let response = String::from_utf8(exchanged?).map_err(|e| e.to_string())?;
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(
            response.ends_with(&format!("\r\n\r\n{status}")),
            "{response}"
        );
        scratch.close()
    }

    #[test]
    fn session_request_validation() -> Result<(), String> {
        // The inputs array is a member's value, at depth 1, so its elements
        // are at depth 2, and a value that deep holds one array fewer than
        // a member's value can.
        let brackets = |depth: usize| format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let deepest_element = format!(
            r#"{{"inputs":[{}]}}"#,
            brackets(MAX_JSON_DEPTH.saturating_sub(1))
        );
        let too_deep_element = format!(r#"{{"inputs":[{}]}}"#, brackets(MAX_JSON_DEPTH));
        let deepest_member = format!(r#"{{"x":{},"inputs":["^"]}}"#, brackets(MAX_JSON_DEPTH));
        let too_deep_member = format!(
            r#"{{"x":{},"inputs":["^"]}}"#,
            brackets(MAX_JSON_DEPTH.saturating_add(1))
        );
        let many = format!(r#"{{"inputs":[{}"^"]}}"#, r#""a","#.repeat(5000));
        let mut many_inputs = vec!["a".to_string(); 5000];
        many_inputs.push("^".to_string());
        let accepted: &[(&[u8], Vec<String>)] = &[
            (
                br#"{"other":1,"inputs":[""," A1 ","^","\u0007\t\u00e9"]}"#,
                strings(&["", " A1 ", "^", "\u{7}\t\u{e9}"]),
            ),
            // The key is compared unescaped.
            (br#"{"\u0069nputs":["^"]}"#, strings(&["^"])),
            // An `inputs` key inside another member is not a member.
            (br#"{"x":{"inputs":1},"inputs":["^"]}"#, strings(&["^"])),
            (deepest_member.as_bytes(), strings(&["^"])),
            (
                b" \t\r\n{ \"inputs\" : [ \"a\" , \"b\" ] } \r\n",
                strings(&["a", "b"]),
            ),
            (br#"{"inputs":[]}"#, Vec::new()),
            (many.as_bytes(), many_inputs),
        ];
        for (body, expected) in accepted {
            let inputs = session_inputs(body)?;
            assert_eq!(&inputs, expected, "{}", String::from_utf8_lossy(body));
            // The inputs are held in one allocation of exactly their count.
            assert_eq!(inputs.capacity(), inputs.len());
        }
        let line_break = "INPUT 1 CONTAINS A CARRIAGE RETURN OR LINE FEED";
        // Causes in precedence order: a syntax error anywhere, an element
        // fault's array included, is malformed; then the kind of body; then
        // `inputs`; then the first faulty element by position.
        let rejected: &[(&[u8], &str)] = &[
            (br#"{"inputs": ["#, "MALFORMED JSON"),
            (b"", "MALFORMED JSON"),
            (b"{\"inputs\":[\"\xff\"]}", "MALFORMED JSON"),
            (b"{\"x\":\"\xff\",\"inputs\":[]}", "MALFORMED JSON"),
            (br#"{"inputs":[1],}"#, "MALFORMED JSON"),
            (br#"{"inputs":["a\nb"],}"#, "MALFORMED JSON"),
            (br#"{"inputs":["a"]} x"#, "MALFORMED JSON"),
            (too_deep_element.as_bytes(), "MALFORMED JSON"),
            (too_deep_member.as_bytes(), "MALFORMED JSON"),
            (br#"["a"]"#, "BODY IS NOT A JSON OBJECT"),
            (br#""str""#, "BODY IS NOT A JSON OBJECT"),
            (b"1", "BODY IS NOT A JSON OBJECT"),
            (br#"{}"#, "INPUTS MISSING OR REPEATED"),
            (
                br#"{"inputs":[],"inputs":[]}"#,
                "INPUTS MISSING OR REPEATED",
            ),
            (
                br#"{"inputs":"x","inputs":[]}"#,
                "INPUTS MISSING OR REPEATED",
            ),
            (
                br#"{"inputs":[1],"\u0069nputs":[]}"#,
                "INPUTS MISSING OR REPEATED",
            ),
            (br#"{"inputs":"x"}"#, "INPUTS IS NOT AN ARRAY"),
            (br#"{"inputs":{}}"#, "INPUTS IS NOT AN ARRAY"),
            (br#"{"inputs":[1]}"#, "INPUT 1 IS NOT A STRING"),
            (br#"{"inputs":["",null]}"#, "INPUT 2 IS NOT A STRING"),
            (deepest_element.as_bytes(), "INPUT 1 IS NOT A STRING"),
            (br#"{"inputs":[1,"a\nb"]}"#, "INPUT 1 IS NOT A STRING"),
            (br#"{"inputs":["a",{},"b\rc"]}"#, "INPUT 2 IS NOT A STRING"),
            (br#"{"inputs":["a\rb"]}"#, line_break),
            (br#"{"inputs":["a\nb"]}"#, line_break),
            (br#"{"inputs":["a\nb",1]}"#, line_break),
            (br#"{"inputs":["a\u000ab"]}"#, line_break),
            (
                br#"{"inputs":["a","\u000Db"]}"#,
                "INPUT 2 CONTAINS A CARRIAGE RETURN OR LINE FEED",
            ),
        ];
        let store = Store::from_zwr("").map_err(|e| e.to_string())?;
        // Every rejected body is answered before a backend runs, so no
        // capture is read.
        let mumps = MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        };
        // The wire text of one rejection, spelled out.
        let malformed = (
            "POST".to_string(),
            "/api/rust/session".to_string(),
            Some(br#"{"inputs": ["#.to_vec()),
        );
        let (status, content_type, answer) = route(malformed, &store, &mumps);
        let answer = String::from_utf8(answer).map_err(|e| e.to_string())?;
        assert_eq!(
            (status, content_type, answer.as_str()),
            (400, JSON_TYPE, r#"{"error":"BAD REQUEST: MALFORMED JSON"}"#)
        );
        for &(body, cause) in rejected {
            assert_eq!(
                scan_session_body(body).map(|(_, array)| array),
                Err(cause.to_string()),
                "{}",
                String::from_utf8_lossy(body)
            );
            // Each session route answers the cause inside the 400 envelope.
            let expected = error_body(&format!("{BAD_REQUEST}: {cause}"));
            for backend in ["mumps", "rust", "compare"] {
                let request = (
                    "POST".to_string(),
                    format!("/api/{backend}/session"),
                    Some(body.to_vec()),
                );
                let (status, content_type, answer) = route(request, &store, &mumps);
                let answer = String::from_utf8(answer).map_err(|e| e.to_string())?;
                assert_eq!(
                    (status, content_type, answer.as_str()),
                    (400, JSON_TYPE, expected.as_str()),
                    "POST /api/{backend}/session with {}",
                    String::from_utf8_lossy(body)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn largest_session_body_answers_as_its_first_inputs() -> Result<(), String> {
        let store = fixture_store()?;
        // Rust sessions read no capture.
        let mumps = MumpsMode::Replay {
            golden_dir: PathBuf::new(),
        };
        let session = |body: Vec<u8>| {
            let request = (
                "POST".to_string(),
                "/api/rust/session".to_string(),
                Some(body),
            );
            route(request, &store, &mumps)
        };
        // As many one-character inputs as fit within `MAX_BODY`, then the
        // exit: a session the budget charges, answered as the session of
        // its first input and the exit is.
        let mut body = br#"{"inputs":["#.to_vec();
        let filler = br#""a","#;
        let rest = br#""^"]}"#;
        let count = MAX_BODY.saturating_sub(body.len().saturating_add(rest.len())) / filler.len();
        for _ in 0..count {
            body.extend_from_slice(filler);
        }
        body.extend_from_slice(rest);
        assert!(body.len() <= MAX_BODY && count > FREE_INPUTS);
        let largest = session(body);
        let short = session(br#"{"inputs":["a","^"]}"#.to_vec());
        assert_eq!(largest, short);
        assert_eq!(largest.0, 200);
        // No other test sends a session heavy enough to be charged, so the
        // shared budget is empty again once the session has answered.
        let held = *INPUT_BUDGET
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(held, 0);
        Ok(())
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
        // Each origin-form target routed as sent: percent-encoded octets are
        // never decoded, and visible characters RFC 3986 admits only encoded
        // are left as they are.
        let origin: [&str; 4] = ["/%00", "/%2e%2e/Cargo.toml", "/a|b", "/^[]{}?q=|\"<>`"];
        for target in origin {
            let head = format!("GET {target} HTTP/1.1\r\nHost: h\r\n\r\n");
            assert_eq!(
                parse_head(head.as_bytes()),
                Ok(("GET", target.to_string(), None, false)),
                "for {target}"
            );
        }
        // Each absolute-form target, and the origin-form it is routed by.
        let absolute: [(&str, &str); 9] = [
            ("http://127.0.0.1:18221/", "/"),
            ("http://h", "/"),
            ("http://h?x", "/?x"),
            ("HTTP://h/app.js", "/app.js"),
            ("hTtP://h:1/api/rust/session?q=1", "/api/rust/session?q=1"),
            ("http://h/a@b", "/a@b"),
            ("http://h#f", "/#f"),
            ("http://[::1]:8080/app.js", "/app.js"),
            ("http://h:/", "/"),
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
        let rejected: [(&[u8], &str); 62] = [
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
            // A target byte outside visible ASCII, or a malformed
            // percent-encoding, in either form; an absolute-form authority
            // that is not a host with an optional port. The heads are
            // otherwise valid.
            (
                b"GET /a\x00b HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\tb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\rb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\nb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\x01b HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\x1fb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a\x7fb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /\xc3\xa9 HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /%zz HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /%4 HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /a% HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET /?q=\x00 HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://[]/ HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://[zz]/ HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://localhost:abc/ HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://h:1:2/ HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://h\x01/ HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://h/a\x00b HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
            (
                b"GET http://h/a\x7fb HTTP/1.1\r\nHost: h\r\n\r\n",
                "INVALID REQUEST TARGET",
            ),
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

    /// One `read`, retried on `Interrupted`.
    fn read_some(stream: &mut TcpStream, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match stream.read(buf) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                read => return read,
            }
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
        read_some(&mut client, &mut byte).map(|_| ())
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
        // A body followed by a pipelined request, with the head and later.
        let pipelined = b"GET / HTTP/1.1\r\nHost: h\r\n\r\n";
        let mut body_and_more = request_then_body.clone();
        if let Some((_, body)) = body_and_more.last_mut() {
            body.extend_from_slice(pipelined);
        }
        let mut with_the_head =
            b"POST /api/rust/session HTTP/1.1\r\nHost: h\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        with_the_head.extend_from_slice(pipelined);
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
                answered.clone(),
                Duration::ZERO,
            ),
            (
                "body, then a pipelined request",
                body_and_more,
                false,
                answered.clone(),
                Duration::ZERO,
            ),
            (
                "body and a pipelined request with the head",
                now(&with_the_head),
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
            if let Ok(Some((_, _, Some(body)))) = &outcome {
                // Allocated once, at the declared length.
                assert_eq!(body.capacity(), body.len(), "{case}");
            }
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
        let cases: [(&str, &str, &str); 17] = [
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
            ("POST", "/api/status", GET_REQUIRED),
            ("PUT", "/api/status", GET_REQUIRED),
            ("HEAD", "/api/status", GET_REQUIRED),
            ("NAME", "/api/status", GET_REQUIRED),
            ("GET", "/api/status?x=NAME", NOT_FOUND),
        ];
        for (method, target, cause) in cases {
            let request = (
                method.to_string(),
                target.to_string(),
                Some(br#"{"inputs":["NAME"]}"#.to_vec()),
            );
            let (status, content_type, body) = route(request, &store, &mumps);
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
        // No case reaches a session backend; only `/api/status` reads the
        // golden folder, an empty scratch folder here.
        let scratch = Scratch::new()?;
        let mumps = MumpsMode::Replay {
            golden_dir: scratch.path().to_path_buf(),
        };
        let status = status_body(&mumps).into_bytes();
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
        // (request, status line, content type, the answer's body, whether
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
            (
                request("GET /api/status"),
                "200 OK",
                JSON_TYPE,
                &status,
                false,
            ),
            // `/api/status` answers only `GET`: its `HEAD` is the 404
            // `GET REQUIRED`, written without content.
            (
                request("HEAD /api/status"),
                "404 Not Found",
                JSON_TYPE,
                &get_required,
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
        scratch.close()
    }

    /// A fixture failure of `serve` keeps the kind and the text of
    /// `load_fixtures`, which name the file, behind `cannot load fixtures: `:
    /// a malformed line, and a fixtures folder that does not exist.
    #[test]
    fn fixture_failure_labelled_cannot_load_fixtures() -> Result<(), String> {
        let scratch = Scratch::new()?;
        scratch.write("fixtures", "bad.zwr", "ZWR\n^X(1)=\"a\"\n^X(01)=\"b\"\n")?;
        let malformed = scratch.path().join("fixtures");
        let absent = scratch.path().join("absent");
        let cases = [
            (
                &malformed,
                io::ErrorKind::InvalidData,
                format!(
                    "cannot load fixtures: {}: MALFORMED ZWR LINE 3: ",
                    malformed.join("bad.zwr").display()
                ),
            ),
            (
                &absent,
                io::ErrorKind::NotFound,
                format!("cannot load fixtures: {}: cannot read: ", absent.display()),
            ),
        ];
        for (fixtures, kind, start) in cases {
            match load_store(fixtures) {
                Err(e) => {
                    assert_eq!(e.kind(), kind, "{e:?}");
                    let text = e.to_string();
                    assert!(text.starts_with(&start), "{text:?} should start {start:?}");
                }
                Ok(store) => {
                    return Err(format!("{} loaded as {store:?}", fixtures.display()));
                }
            }
        }
        scratch.close()
    }

    /// A failure of `serve_until` after the store loaded keeps its kind and
    /// its text, which names the step, behind `serving failed: `.
    #[test]
    fn serving_failure_labelled_serving_failed() -> Result<(), String> {
        // EMFILE, a full descriptor table, as the accept loop returns it.
        let Err(accept) = accept_retry_delay(io::Error::from_raw_os_error(24)) else {
            return Err("EMFILE was retried".to_string());
        };
        // A listener that cannot be made non-blocking, returned as it is.
        let nonblocking = io::Error::from(io::ErrorKind::PermissionDenied);
        for failure in [accept, nonblocking] {
            let (kind, text) = (failure.kind(), failure.to_string());
            let labelled = serving_failed(failure);
            assert_eq!(labelled.kind(), kind, "kind kept");
            assert_eq!(labelled.to_string(), format!("serving failed: {text}"));
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
            match read_some(&mut waiting, &mut buf) {
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
                let n =
                    read_some(&mut waiting, &mut buf).map_err(|e| fail("read the response", e))?;
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
