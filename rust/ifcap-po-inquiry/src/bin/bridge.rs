//! The `bridge` binary: serves the one-screen front-end,
//! `POST /api/{mumps|rust|compare}/session` and `GET /api/status` on
//! `127.0.0.1:8080`. The loopback address is the only exposure control; there
//! is no authentication or TLS.
//!
//! Configuration comes from two environment variables. This file is their
//! only reader: each is read once here and never set.
//! - `VEHU_CONTAINER`: when non-empty, the MUMPS side runs live in that
//!   container through `docker exec`; otherwise it replays the captures.
//! - `GOLDEN_DIR`: the capture directory, by default `golden::default_dir()`.
//!   Only the path is chosen here; the library reads it.
//!
//! Startup follows the convention of AAP 0.3.5.1, whose texts `RUNNING.md`
//! lists. Once listening, the bridge writes three lines to standard output:
//! `http://127.0.0.1:8080/`, then `MUMPS MODE: REPLAY` or
//! `MUMPS MODE: LIVE <container>`, then `RECORDED <n> OF 5 READY <m> OF 5`.
//! Diagnostics go to standard error. The exit status is 1 when
//! `VEHU_CONTAINER` is not valid UTF-8 or the address cannot be bound, in
//! which cases nothing reaches standard output, and when `serve` returns an
//! error, such as fixtures that cannot be loaded. A missing capture directory
//! is reported and serving continues, with no case recorded.

use std::env::{self, VarError};
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;

use ifcap_po_inquiry::bridge::{MumpsMode, serve};
use ifcap_po_inquiry::golden::{default_dir, status};

const ADDRESS: &str = "127.0.0.1:8080";

fn main() -> ExitCode {
    let golden_dir = match env::var_os("GOLDEN_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => default_dir(),
    };
    // Every line in `main`, diagnostic or startup, is written with its error
    // dropped: a line that its stream cannot take is lost, so an unwritable
    // standard output or standard error never changes the exit status or
    // stops a bridge that is otherwise valid.
    let container = match env::var("VEHU_CONTAINER") {
        Ok(container) if !container.is_empty() => Some(container),
        Ok(_) | Err(VarError::NotPresent) => None,
        // The variable is set but cannot name a container; falling back to
        // replay would hide that the caller asked for live mode.
        Err(VarError::NotUnicode(_)) => {
            let _ = writeln!(io::stderr(), "bridge: VEHU_CONTAINER is not valid UTF-8");
            return ExitCode::FAILURE;
        }
    };
    let listener = match TcpListener::bind(ADDRESS) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            let _ = writeln!(
                io::stderr(),
                "bridge: port 8080 on 127.0.0.1 is already in use"
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            let _ = writeln!(io::stderr(), "bridge: cannot listen on {ADDRESS}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if !golden_dir.is_dir() {
        let _ = writeln!(
            io::stderr(),
            "bridge: golden directory missing: {}",
            golden_dir.display()
        );
    }
    // The counts come from the same path that is checked above and then moved
    // into the mode `serve` receives.
    let counts = status(&golden_dir);
    {
        // One lock for the three lines, released before `serve` runs.
        let mut out = io::stdout().lock();
        let _ = writeln!(out, "http://{ADDRESS}/");
        let _ = match &container {
            Some(container) => writeln!(out, "MUMPS MODE: LIVE {container}"),
            None => writeln!(out, "MUMPS MODE: REPLAY"),
        };
        let _ = writeln!(
            out,
            "RECORDED {} OF 5 READY {} OF 5",
            counts.recorded.len(),
            counts.ready.len()
        );
        let _ = out.flush();
    }
    let mode = match container {
        Some(container) => MumpsMode::Live {
            container,
            golden_dir,
        },
        None => MumpsMode::Replay { golden_dir },
    };
    // Errors are printed with their Display text and end the process with a
    // failure status; returning them from `main` would print their Debug form.
    // `serve` loads the fixtures before serving anything, and the startup
    // convention gives every error it returns this one text. A fixture error's
    // text names the file and, for a malformed line, its number; a later
    // serving error's text names its stage.
    match serve(listener, mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "bridge: cannot load fixtures: {error}");
            ExitCode::FAILURE
        }
    }
}
