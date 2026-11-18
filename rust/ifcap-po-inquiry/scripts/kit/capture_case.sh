#!/usr/bin/env bash
# capture_case.sh: record one golden capture case from the original MUMPS (VEHU) container.
#
# Usage (from rust/ifcap-po-inquiry/):
#   INQUIRY_CMD='...' EXPORT_CMD='...' ./capture_case.sh tests/golden/ifcap/GC-01
#
# The case directory must already hold input.txt (one input per line, ending with the exit input).
# For each case it does: restore container from the snapshot image, export db-before.zwr,
# run the inquiry, export db-after.zwr, restore again, run the inquiry a second time,
# compare the two transcripts, and write the capture files and meta.txt.
# It only reads data. It never runs anything but INQUIRY_CMD and EXPORT_CMD in the container.
#
# Required environment:
#   INQUIRY_CMD   shell string run as: docker exec -i NAME su - vehu -c "$INQUIRY_CMD"
#                 (the inquiry launch command from the crate's README / SPEC.md)
#   EXPORT_CMD    shell string run the same way; must print ZWR text of the globals this case reads
# Optional environment:
#   NAME=vehu  SNAP_IMAGE=vehu-snap:gc-base  BASE_IMAGE=worldvista/vehu:202609
#   READY_CMD='mumps -run %XCMD "write 1"'   WAIT_SECS=300
#   ENTRY_POINT  CONTEXT  RECORD_IDS  SOURCE_REVISION  SETUP   (written to meta.txt)
#   RETURNS_NOTHING=1   write a zero-byte returned.txt (the inquiry returns no value)

set -u

CASE_DIR="${1:-}"
if [ -z "$CASE_DIR" ] || [ ! -f "$CASE_DIR/input.txt" ]; then
  echo "usage: INQUIRY_CMD=... EXPORT_CMD=... $0 <case dir containing input.txt>" >&2
  exit 2
fi
: "${INQUIRY_CMD:?INQUIRY_CMD is required}"
: "${EXPORT_CMD:?EXPORT_CMD is required}"
NAME="${NAME:-vehu}"
SNAP_IMAGE="${SNAP_IMAGE:-vehu-snap:gc-base}"
BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
READY_CMD="${READY_CMD:-mumps -run %XCMD \"write 1\"}"
WAIT_SECS="${WAIT_SECS:-300}"
case "$(uname -m)" in arm64|aarch64) PLATFORM_ARGS="--platform linux/amd64";; *) PLATFORM_ARGS="";; esac

case_id=$(basename "$CASE_DIR")
work="captures-work/$case_id"
mkdir -p "$work"

fail() { echo "FAILED: $*" >&2; exit 1; }

wait_ready() {
  local waited=0
  while [ "$waited" -lt "$WAIT_SECS" ]; do
    if docker exec "$NAME" su - vehu -c "$READY_CMD" >/dev/null 2>&1; then
      echo "  container ready after ${waited}s"
      return 0
    fi
    sleep 5
    waited=$((waited + 5))
  done
  fail "container '$NAME' not ready after ${WAIT_SECS}s (check: docker logs $NAME)"
}

restore() {
  echo "- restoring $NAME from $SNAP_IMAGE"
  docker rm -f "$NAME" >/dev/null 2>&1
  docker run -d $PLATFORM_ARGS --name "$NAME" "$SNAP_IMAGE" >/dev/null || fail "docker run failed"
  wait_ready
}

run_inquiry() { # $1 = stdout file, $2 = stderr file
  docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" < "$CASE_DIR/input.txt" > "$1" 2> "$2"
}

export_zwr() { # $1 = stdout file, $2 = stderr file
  docker exec -i "$NAME" su - vehu -c "$EXPORT_CMD" < /dev/null > "$1" 2> "$2"
}

echo "== $case_id"
restore
echo "- export before"
export_zwr "$CASE_DIR/db-before.zwr" "$work/export-before.stderr" || fail "export before"
echo "- run inquiry (run 1)"
run_inquiry "$CASE_DIR/transcript.txt" "$work/inquiry-run1.stderr" || echo "  note: inquiry exited non-zero (see $work/inquiry-run1.stderr)"
echo "- export after"
export_zwr "$CASE_DIR/db-after.zwr" "$work/export-after.stderr" || fail "export after"

restore
echo "- run inquiry (run 2, fresh container)"
run_inquiry "$work/transcript.run2.txt" "$work/inquiry-run2.stderr" || echo "  note: inquiry exited non-zero (see $work/inquiry-run2.stderr)"

if [ "${RETURNS_NOTHING:-0}" = "1" ]; then
  : > "$CASE_DIR/returned.txt"
elif [ ! -f "$CASE_DIR/returned.txt" ]; then
  echo "  WARNING: returned.txt not written. Set RETURNS_NOTHING=1 if the inquiry returns no value,"
  echo "           otherwise write the return value as the README directs."
fi
[ -f "$CASE_DIR/normalisation.txt" ] || : > "$CASE_DIR/normalisation.txt"

digest=$(docker image inspect --format '{{index .RepoDigests 0}}' "$BASE_IMAGE" 2>/dev/null || true)
snap=$(docker image inspect --format '{{.Id}}' "$SNAP_IMAGE" 2>/dev/null || true)
{
  echo "image_digest=${digest:-UNKNOWN}"
  echo "source_revision=${SOURCE_REVISION:-UNKNOWN}"
  echo "snapshot_id=${snap:-UNKNOWN}"
  echo "record_ids=${RECORD_IDS:-UNKNOWN}"
  echo "entry_point=${ENTRY_POINT:-UNKNOWN}"
  echo "context=${CONTEXT:-UNKNOWN}"
  echo "setup=${SETUP:-none, existing data}"
  echo "docker_exec_args=su - vehu -c \"$INQUIRY_CMD\""
} > "$CASE_DIR/meta.txt"

echo "== checks for $case_id"
status=0
if LC_ALL=C grep -q $'\r' "$CASE_DIR"/*.txt "$CASE_DIR"/*.zwr 2>/dev/null; then
  echo "  FAIL: carriage return found (a pseudo-terminal was used somewhere)"; status=1
else echo "  ok: no carriage returns"; fi
if [ -s "$CASE_DIR/transcript.txt" ]; then echo "  ok: transcript.txt is not empty"
else echo "  FAIL: transcript.txt is empty (see $work/inquiry-run1.stderr)"; status=1; fi
if cmp -s "$CASE_DIR/db-before.zwr" "$CASE_DIR/db-after.zwr"; then echo "  ok: db-before and db-after identical (read-only confirmed)"
else echo "  STOP: db-before and db-after differ. The inquiry changed data. Do not use this capture."; status=1; fi
if cmp -s "$CASE_DIR/transcript.txt" "$work/transcript.run2.txt"; then
  echo "  ok: two runs produced identical transcripts (normalisation.txt can stay empty)"
else
  echo "  DIFFERENT: the two runs differ (exit code 3, not a failure by itself). Differences:"
  diff "$CASE_DIR/transcript.txt" "$work/transcript.run2.txt" | head -40
  echo "  Each differing span must be covered by a rule in $CASE_DIR/normalisation.txt"
  echo "  (open text, TAB, close text, one rule per line). If a difference is not a clock,"
  echo "  date or record-number style field, the case is not reproducible: leave it INCOMPLETE."
  [ "$status" -eq 0 ] && status=3
fi
[ -s "$work/export-before.stderr" ] && echo "  note: export printed messages, see $work/export-before.stderr"
exit $status
