#!/bin/bash
# demo_up.sh: start both sides of the demo.
#   http://127.0.0.1:8081  ORIGINAL MUMPS, a live terminal (ttyd -> docker exec -> EN3^PRCHRPT)
#   http://127.0.0.1:8080  RUST port, the bridge page (REPLAY mode; its MUMPS pane replays captures)
# Usage: demo_up.sh [--fresh] [--no-original] [--no-rust]
#   --fresh        remove and recreate the VEHU container from the snapshot image
#   --no-original  start only the Rust bridge
#   --no-rust      start only the original terminal
set -u
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/demo_env.sh"
FRESH=0; DO_ORIG=1; DO_RUST=1
for a in "$@"; do
  case "$a" in
    --fresh) FRESH=1;; --no-original) DO_ORIG=0;; --no-rust) DO_RUST=0;;
    *) echo "unknown option: $a"; exit 2;;
  esac
done
ok()   { printf '  [ok]   %s\n' "$*"; }
warn() { printf '  [WARN] %s\n' "$*"; }
die()  { printf '  [FAIL] %s\n' "$*" >&2; exit 1; }

echo "Preflight"
command -v python3 >/dev/null 2>&1 || die "python3 not found (used for port checks and compare_live.py)"
ok "python3"

# ---------------------------------------------------------------- Rust side (8080)
if [ "$DO_RUST" = 1 ]; then
  command -v cargo >/dev/null 2>&1 || die "cargo not found. Install rustup, then: rustup toolchain install 1.99.1"
  ok "cargo: $(cargo --version 2>/dev/null)"
  if [ -f "$RUN_DIR/bridge.pid" ] && kill -0 "$(cat "$RUN_DIR/bridge.pid")" 2>/dev/null; then
    ok "bridge already running (pid $(cat "$RUN_DIR/bridge.pid")); run demo_down.sh to restart it"
  else
    demo_port_busy "$RUST_PORT" && die "port $RUST_PORT is already in use. Find it with: lsof -nP -iTCP:$RUST_PORT -sTCP:LISTEN"
    echo "Building the bridge (offline, locked)"
    # shellcheck disable=SC2086
    ( cd "$CRATE_DIR" && cargo build $CARGO_FLAGS --bin bridge ) || die "cargo build failed (see the message above)"
    BIN="$CRATE_DIR/target/debug/bridge"
    [ -x "$BIN" ] || die "built binary not found at $BIN"
    # The bridge loads fixtures once, at start. Restart it after any change to cases.zwr or src/.
    ( cd "$CRATE_DIR"; GOLDEN_DIR="$GOLDEN_DIR" nohup "$BIN" > "$RUN_DIR/bridge.log" 2>&1 < /dev/null & echo $! > "$RUN_DIR/bridge.pid" )
    for i in $(seq 1 30); do
      curl -fsS "$RUST_URL/api/status" >/dev/null 2>&1 && break
      sleep 1
    done
    curl -fsS "$RUST_URL/api/status" >/dev/null 2>&1 || { cat "$RUN_DIR/bridge.log"; die "bridge did not answer on $RUST_URL"; }
    ok "bridge up, pid $(cat "$RUN_DIR/bridge.pid")"
  fi
  echo "  bridge says:"; sed 's/^/    /' "$RUN_DIR/bridge.log"
  echo "  /api/status: $(curl -fsS "$RUST_URL/api/status")"
fi

# ---------------------------------------------------------------- original side (8081)
if [ "$DO_ORIG" = 1 ]; then
  demo_find_docker || die "docker not found. Start Docker Desktop first."
  docker info >/dev/null 2>&1 || die "docker daemon is not running. Start Docker Desktop and wait for it to settle."
  ok "docker is running"
  if [ "$FRESH" = 1 ]; then docker rm -f "$VEHU_CONTAINER" >/dev/null 2>&1; fi
  if docker exec "$VEHU_CONTAINER" true >/dev/null 2>&1; then
    ok "container $VEHU_CONTAINER already running"
  else
    docker image inspect "$SNAP_IMAGE" >/dev/null 2>&1 || die "snapshot image $SNAP_IMAGE not found on this Docker. It exists only where the captures were made. Rebuild it with scripts/kit/setup_vehu.sh and load_orders.sh."
    docker rm -f "$VEHU_CONTAINER" >/dev/null 2>&1
    echo "Starting $VEHU_CONTAINER from $SNAP_IMAGE (emulated on Apple silicon: allow 30 to 120 seconds)"
    # shellcheck disable=SC2046
    docker run -d $(demo_platform_args) --name "$VEHU_CONTAINER" "$SNAP_IMAGE" >/dev/null || die "docker run failed"
  fi
  waited=0
  until [ "$(docker exec "$VEHU_CONTAINER" su - vehu -c 'mumps -run %XCMD "write 1,!"' 2>/dev/null | tr -d '[:space:]')" = "1" ]; do
    [ "$waited" -ge 180 ] && die "VEHU not ready after 180 s (docker logs $VEHU_CONTAINER)"
    sleep 5; waited=$((waited + 5))
  done
  ok "VEHU answers (waited ${waited}s)"
  if [ -f "$RUN_DIR/ttyd.pid" ] && kill -0 "$(cat "$RUN_DIR/ttyd.pid")" 2>/dev/null; then
    ok "ttyd already running (pid $(cat "$RUN_DIR/ttyd.pid"))"
  elif command -v ttyd >/dev/null 2>&1; then
    demo_port_busy "$ORIG_PORT" && die "port $ORIG_PORT is already in use"
    IFACE="${TTYD_IFACE:-$([ "$(uname)" = Darwin ] && echo lo0 || echo lo)}"
    nohup ttyd -p "$ORIG_PORT" -i "$IFACE" -W -t fontSize=15 -t titleFixed="ORIGINAL MUMPS ($ORIG_PORT)" \
      bash "$DEMO_SCRIPTS_DIR/original_session.sh" > "$RUN_DIR/ttyd.log" 2>&1 < /dev/null &
    echo $! > "$RUN_DIR/ttyd.pid"; sleep 1
    kill -0 "$(cat "$RUN_DIR/ttyd.pid")" 2>/dev/null || { cat "$RUN_DIR/ttyd.log"; die "ttyd exited. If it rejected the interface, try TTYD_IFACE=127.0.0.1 or TTYD_IFACE=lo0"; }
    ok "ttyd up on http://127.0.0.1:$ORIG_PORT (pid $(cat "$RUN_DIR/ttyd.pid"))"
  else
    warn "ttyd is not installed (brew install ttyd). Fallback: run scripts/demo/original_session.sh in a Terminal window."
  fi
fi

echo
echo "Ready"
[ "$DO_ORIG" = 1 ] && echo "  Original MUMPS : http://127.0.0.1:$ORIG_PORT/   (a new session on each page load)"
[ "$DO_RUST" = 1 ]  && echo "  Rust port      : $RUST_URL/   (use the RUST or COMPARE mode)"
echo "  Side by side   : scripts/demo/open_side_by_side.sh"
echo "  Byte compare   : scripts/demo/compare_live.sh"
echo "  Stop           : scripts/demo/demo_down.sh"
