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
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use ifcap_po_inquiry::bridge::{MumpsMode, serve};

/// Loopback only. Port 8080 is a default the team may change.
const ADDRESS: &str = "127.0.0.1:8080";

fn main() -> io::Result<()> {
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
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "VEHU_CONTAINER is not valid UTF-8",
            ));
        }
    };
    let label = match &mode {
        MumpsMode::Replay { .. } => "replay".to_string(),
        MumpsMode::Live { container, .. } => format!("live {container}"),
    };
    let listener = TcpListener::bind(ADDRESS)?;
    eprintln!("bridge listening on http://{ADDRESS} ({label})");
    serve(listener, mode)
}
