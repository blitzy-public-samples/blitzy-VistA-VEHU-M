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
//! Diagnostics go to standard error. The exit status is 1 when the address
//! cannot be bound, in which case nothing reaches standard output, and when
//! `serve` returns an error, such as fixtures that cannot be loaded. A
//! capture directory that does not exist, or is not a directory, is reported
//! as `bridge: golden directory missing: <path>`; one whose metadata cannot
//! be read for any other reason, such as a symbolic-link loop or a parent
//! folder that cannot be searched, is reported as
//! `bridge: golden directory unreadable: <path>: <error>`. In both cases
//! serving continues, with no case recorded.

use std::env;
use std::fs;
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
    let container = env::var_os("VEHU_CONTAINER")
        .filter(|container| !container.is_empty())
        .map(|container| {
            // A value that is not valid UTF-8 still selects live mode and is
            // converted lossily, because `MumpsMode` carries the container
            // name as text.
            container
                .into_string()
                .unwrap_or_else(|raw| raw.to_string_lossy().into_owned())
        });
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
    // The folder's metadata decides the warning the way the library decides a
    // case folder: nothing, or something other than a directory, at the path
    // is a missing folder; any other metadata error, such as a symbolic-link
    // loop or a parent folder that cannot be searched, leaves it unreadable.
    match fs::metadata(&golden_dir) {
        Ok(entry) if entry.is_dir() => {}
        Err(error)
            if !matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            let _ = writeln!(
                io::stderr(),
                "bridge: golden directory unreadable: {}: {error}",
                golden_dir.display()
            );
        }
        _ => {
            let _ = writeln!(
                io::stderr(),
                "bridge: golden directory missing: {}",
                golden_dir.display()
            );
        }
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
