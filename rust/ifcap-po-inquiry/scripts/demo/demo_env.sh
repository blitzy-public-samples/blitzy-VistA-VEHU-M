# demo_env.sh: shared settings for the two-port demo. Sourced, never run on its own.
# Every value can be overridden from the environment before you call a script.

DEMO_SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# This folder is rust/ifcap-po-inquiry/scripts/demo, so the crate is two levels up.
CRATE_DIR="${CRATE_DIR:-$(cd "$DEMO_SCRIPTS_DIR/../.." && pwd)}"
RUN_DIR="${RUN_DIR:-$CRATE_DIR/.demo}"          # pid files, logs, compare output (git-ignore it)
mkdir -p "$RUN_DIR"

# The original (MUMPS) side.
VEHU_CONTAINER="${VEHU_CONTAINER:-vehu-demo}"   # container name used for the demo
SNAP_IMAGE="${SNAP_IMAGE:-vehu-snap:gc-base}"   # snapshot that holds the loaded orders
ORIG_PORT="${ORIG_PORT:-8081}"                  # ttyd port for the original terminal

# The Rust side. The bridge binds 127.0.0.1:8080 in source (src/bin/bridge.rs, ADDRESS).
RUST_PORT=8080
RUST_URL="http://127.0.0.1:${RUST_PORT}"

# Cases to compare. Each sub-folder holds input.txt, transcript.txt, normalisation.txt.
CASES_DIR="${CASES_DIR:-$CRATE_DIR/tests/golden/ifcap}"
GOLDEN_DIR="${GOLDEN_DIR:-$CASES_DIR}"

# Extra cargo flags. Add --ignore-rust-version only if you knowingly build on an older toolchain.
CARGO_FLAGS="${CARGO_FLAGS:---offline --locked}"

# The original start command, exactly as tests/golden/ifcap/README.md Part B gives it.
# Single quotes keep the backslashes: the shell inside the container turns \$ into $.
INQUIRY_CMD='source /home/vehu/etc/env && yottadb -run %XCMD "S U=\$C(94),DTIME=300,DT=\$\$DT^XLFDT D DUZ^XUP(.5) D EN3^PRCHRPT"'

demo_find_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    for d in /Applications/Docker.app/Contents/Resources/bin /usr/local/bin "$HOME/.docker/bin"; do
      [ -x "$d/docker" ] && { export PATH="$d:$PATH"; break; }
    done
  fi
  command -v docker >/dev/null 2>&1
}

demo_platform_args() {
  case "$(uname -m)" in arm64|aarch64) echo "--platform linux/amd64";; *) echo "";; esac
}

# demo_port_busy <port>: true when something already listens on 127.0.0.1:<port>.
demo_port_busy() {
  python3 - "$1" <<'PY' >/dev/null 2>&1
import socket, sys
s = socket.socket(); s.settimeout(0.5)
try:
    s.connect(("127.0.0.1", int(sys.argv[1]))); sys.exit(0)
except Exception:
    sys.exit(1)
PY
}
