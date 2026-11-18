#!/usr/bin/env bash
# setup_vehu.sh: one-command setup of the original MUMPS (VEHU) container on a Mac.
# Run from the crate folder (rust/ifcap-po-inquiry/):   bash setup_vehu.sh
# It finds Docker, starts it if needed, checks Rust, pulls the VEHU image,
# starts the container named "vehu", waits until M answers, and prints a report.
# It only pulls an image and starts a container. It changes no project files except
# setup_vehu_report.txt in the current folder. Safe to run again.
#
# Optional environment: NAME=vehu  IMAGE=worldvista/vehu:202609  WAIT_SECS=900

set -u
NAME="${NAME:-vehu}"
IMAGE="${IMAGE:-worldvista/vehu:202609}"
WAIT_SECS="${WAIT_SECS:-900}"
REPORT="setup_vehu_report.txt"

exec > >(tee "$REPORT") 2>&1

say()  { echo "$*"; }
step() { echo; echo "== $*"; }
die()  { echo; echo "STOPPED: $*"; echo "Paste this whole screen (or $REPORT) back to Claude."; exit 1; }

step "1. Docker command"
if ! command -v docker >/dev/null 2>&1; then
  for d in /Applications/Docker.app/Contents/Resources/bin /usr/local/bin "$HOME/.docker/bin"; do
    if [ -x "$d/docker" ]; then export PATH="$d:$PATH"; say "found docker in $d (added to PATH for this run)"; break; fi
  done
fi
command -v docker >/dev/null 2>&1 || die "docker not found. Install Docker Desktop, open it once, then run this again."
say "docker: $(command -v docker)"

step "2. Docker running?"
if ! docker info >/dev/null 2>&1; then
  say "Docker is not running. Trying to open Docker Desktop..."
  open -a Docker >/dev/null 2>&1 || true
  waited=0
  while ! docker info >/dev/null 2>&1; do
    [ "$waited" -ge 180 ] && die "Docker did not start within 180 seconds. Open Docker Desktop yourself, wait for 'running', then run this again."
    sleep 5; waited=$((waited + 5)); say "  waiting for Docker... ${waited}s"
  done
fi
say "docker server: $(docker version --format '{{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}' 2>/dev/null)"

step "3. Machine type"
arch="$(uname -m)"
say "uname -m: $arch"
PLATFORM_ARGS=""
case "$arch" in
  arm64|aarch64)
    PLATFORM_ARGS="--platform linux/amd64"
    say "Apple silicon: the image is Intel-only, so it will run under emulation (slower)."
    say "If boot fails below, in Docker Desktop > Settings > General tick 'Use Rosetta for x86_64/amd64 emulation', Apply & restart, run again." ;;
esac

step "4. Rust"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if command -v rustc >/dev/null 2>&1; then say "$(rustc --version)"; say "$(cargo --version)"
else say "rustc NOT found (needed later for cargo test; the container setup below does not need it)"; fi

step "5. Crate folder (only if run from the crate)"
if [ -f Cargo.toml ]; then
  say "Cargo.toml found in $(pwd)"
  grep -n -i 'rust-version\|edition' Cargo.toml || true
  say "scripts/: $(ls scripts 2>/dev/null | tr '\n' ' ')"
  say "tests/golden/ifcap/: $(ls tests/golden/ifcap 2>/dev/null | tr '\n' ' ')"
else
  say "No Cargo.toml here. Fine for container setup; run from rust/ifcap-po-inquiry/ for this section."
fi
say "port 8080: $(lsof -i :8080 2>/dev/null | sed -n '2p')"
[ -z "$(lsof -i :8080 2>/dev/null)" ] && say "port 8080 is free"

step "6. VEHU image ($IMAGE)"
if docker image inspect "$IMAGE" >/dev/null 2>&1; then say "already downloaded"
else
  say "downloading (about 1.6 GB, 5 to 15 minutes). Progress bars follow."
  # shellcheck disable=SC2086
  docker pull $PLATFORM_ARGS "$IMAGE" || die "docker pull failed (see messages above)."
fi
DIGEST="$(docker image inspect --format '{{index .RepoDigests 0}}' "$IMAGE" 2>/dev/null || true)"
say "image digest: ${DIGEST:-UNKNOWN}"

step "7. Container ($NAME)"
if docker ps --format '{{.Names}}' | grep -qx "$NAME"; then say "already running"
elif docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
  say "exists but stopped; starting it"
  docker start "$NAME" >/dev/null || die "docker start failed"
else
  # shellcheck disable=SC2086
  docker run -d $PLATFORM_ARGS --name "$NAME" "$IMAGE" >/dev/null || die "docker run failed"
  say "started"
fi

step "8. Waiting until MUMPS answers (up to ${WAIT_SECS}s; slow under emulation is normal)"
ready=0; waited=0; how=""
while [ "$waited" -lt "$WAIT_SECS" ]; do
  if ! docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
    say "container is not running (it exited). Last log lines:"
    docker logs --tail 40 "$NAME" 2>&1 | sed 's/^/    /'
    die "container exited. This usually means emulation trouble on Apple silicon."
  fi
  if out="$(docker exec "$NAME" su - vehu -c 'mumps -run %XCMD "write 1,!"' 2>/dev/null)" && [ "$(printf '%s' "$out" | tr -d '[:space:]')" = "1" ]; then
    ready=1; how="mumps -run %XCMD"; break
  fi
  if out="$(printf 'write 1,!\nhalt\n' | docker exec -i "$NAME" su - vehu -c 'mumps -dir' 2>/dev/null)" && printf '%s' "$out" | grep -q '^1$'; then
    ready=1; how="mumps -dir (programmer prompt)"; break
  fi
  sleep 10; waited=$((waited + 10))
  [ $((waited % 30)) -eq 0 ] && say "  still starting... ${waited}s"
done

step "REPORT (paste everything from here to Claude)"
say "date:           $(date)"
say "machine:        $arch"
say "image:          $IMAGE"
say "image_digest:   ${DIGEST:-UNKNOWN}"
say "container:      $(docker ps --filter "name=^${NAME}$" --format '{{.Names}} {{.Status}}')"
if [ "$ready" -eq 1 ]; then
  say "mumps answered: YES, via $how, after ${waited}s"
  say "RESULT: READY. The original MUMPS is running."
  exit 0
else
  say "mumps answered: NO after ${WAIT_SECS}s"
  say "log tail:"
  docker logs --tail 30 "$NAME" 2>&1 | sed 's/^/    /'
  die "MUMPS did not answer. The container is up but the readiness command may be wrong for this image, or it is still booting (run again to keep waiting)."
fi
