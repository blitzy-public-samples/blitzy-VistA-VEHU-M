//! The `bridge` binary: serves the one-screen front-end and
//! `POST /api/{mumps|rust|compare}/session` on `127.0.0.1:8080`. The loopback
//! address is the only exposure control; there is no authentication or TLS.
//!
//! Configuration comes from two environment variables, read once here and
//! never set:
//! - `VEHU_CONTAINER`: when non-empty, the MUMPS side runs live in that
//!   container through `docker exec`; otherwise it replays the captures.
//! - `GOLDEN_DIR`: the capture directory, by default `tests/golden/ifcap/`
//!   under this crate. Only the path is built here; the bridge reads it.

use std::env::{self, VarError};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ifcap_po_inquiry::bridge::{MumpsMode, serve};

/// The fixed listener address, on loopback only.
const ADDRESS: &str = "127.0.0.1:8080";

fn main() -> ExitCode {
    let golden_dir = match env::var_os("GOLDEN_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join("ifcap"),
    };
    let mode = match env::var("VEHU_CONTAINER") {
        Ok(container) if !container.is_empty() => MumpsMode::Live {
            container,
            golden_dir,
        },
        Ok(_) | Err(VarError::NotPresent) => MumpsMode::Replay { golden_dir },
        // The variable is set but cannot name a container; falling back to
        // replay would hide that the caller asked for live mode.
        Err(VarError::NotUnicode(_)) => {
            eprintln!("bridge: VEHU_CONTAINER is not valid UTF-8");
            return ExitCode::FAILURE;
        }
    };
    // The console line names the mode only and never the container name, so
    // the variable's text cannot reach standard error.
    let label = match &mode {
        MumpsMode::Replay { .. } => "replay",
        MumpsMode::Live { .. } => "live",
    };
    let listener = match TcpListener::bind(ADDRESS) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("bridge: cannot listen on {ADDRESS}: {error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("bridge listening on http://{ADDRESS} ({label})");
    // Errors are printed with their Display text and end the process with a
    // failure status; returning them from `main` would print their Debug form.
    // A fixture error's text names the file and the 1-based line.
    match serve(listener, mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bridge: {error}");
            ExitCode::FAILURE
        }
    }
}
