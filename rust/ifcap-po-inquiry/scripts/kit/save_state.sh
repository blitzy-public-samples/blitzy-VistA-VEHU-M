#!/usr/bin/env bash
# save_state.sh: SAVE the VEHU data the parity tests depend on, and freeze the container at that point.
# Run from the crate folder (rust/ifcap-po-inquiry/) AFTER load_orders.sh:
#   bash save_state.sh
#
# Saves   tests/golden/ifcap/state/state.zwr + MANIFEST.txt   (the data, in ZWR text, plus image digest,
#         date, node count and checksum; commit these)
# Freezes docker image vehu-snap:gc-base                      (capture_all.sh restores from it per case)
# Proves  a fresh container from that image exports byte-identical data (VERIFIED line in MANIFEST.txt)
# It only READS data. The export scope is the one in tests/golden/ifcap/README.md Part B step 1.
# Optional: ROOTS='...' to change the scope (default: the README scope). NAME, SNAP_IMAGE, BASE_IMAGE, OUT, WAIT_SECS.

set -u
HERE="$(cd "$(dirname "$0")" && pwd)"; . "$HERE/vehu_common.sh"
NAME="${NAME:-vehu}"; SNAP_IMAGE="${SNAP_IMAGE:-vehu-snap:gc-base}"; BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
ROOTS="${ROOTS:-$VC_SCOPE_ROOTS}"; OUT="${OUT:-tests/golden/ifcap/state}"; WAIT_SECS="${WAIT_SECS:-600}"
VERIFY_NAME="${NAME}-verify"
vc_find_docker || { echo "STOPPED: docker not found. Run setup_vehu.sh first."; exit 1; }
mkdir -p "$OUT" || { echo "STOPPED: cannot create $OUT"; exit 1; }
exec > >(tee "$OUT/save_state_report.txt") 2>&1
fail() { echo; echo "STOPPED: $*"; echo "Paste this whole screen (or $OUT/save_state_report.txt) back to Claude."; exit 1; }
step() { echo; echo "== $*"; }
PLATFORM_ARGS="$(vc_platform_args)"
MCMD="$(vc_mcmd "$ROOTS")"
export_zwr() { docker exec -i "$1" su - vehu -c "mumps -run %XCMD '$MCMD'" < /dev/null > "$2" 2> "$3"; }

step "1. Container check"
docker ps --format '{{.Names}}' | grep -qx "$NAME" || fail "container '$NAME' is not running. Run setup_vehu.sh and load_orders.sh first."
echo "M command: $MCMD"
orders=$(docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'write \$data(^PRC(442,990001)),!'" < /dev/null 2>/dev/null | tr -d '[:space:]')
case "$orders" in 0|"") echo "WARNING: the test orders are not in the container (run load_orders.sh first). Freezing anyway only makes sense for GC-03 and GC-04.";; *) echo "test orders present: yes";; esac

step "2. Export the data (read only)"
export_zwr "$NAME" "$OUT/body.tmp" "$OUT/export.stderr" || { head -5 "$OUT/body.tmp"; fail "export command failed. See above and $OUT/export.stderr"; }
grep -qE '%YDB-E|%GTM-E' "$OUT/body.tmp" && { grep -E '%YDB-E|%GTM-E' "$OUT/body.tmp" | head -3; fail "the export printed an M error"; }
nodes=$(grep -c '^\^' "$OUT/body.tmp" || true); echo "nodes exported: $nodes"
[ "$nodes" -gt 0 ] || fail "export is empty."
if LC_ALL=C grep -q $'\r' "$OUT/body.tmp"; then fail "carriage returns in the export (a terminal was used somewhere)."; fi
for root in '^PRC(442,' '^PRC(440,' '^DIC(5,' '^PRCD(442.3,'; do
  printf '  %-16s %s nodes\n' "$root" "$(grep -c -F "$root" "$OUT/body.tmp")"
done

step "3. Write $OUT/state.zwr"
DIGEST="$(docker image inspect --format '{{index .RepoDigests 0}}' "$BASE_IMAGE" 2>/dev/null || echo UNKNOWN)"
{ echo "VEHU 202609 state export from $DIGEST"; echo "ZWR"; cat "$OUT/body.tmp"; } > "$OUT/state.zwr"
SHA="$(shasum -a 256 "$OUT/state.zwr" | cut -d' ' -f1)"; BYTES="$(wc -c < "$OUT/state.zwr" | tr -d ' ')"
echo "saved: $OUT/state.zwr ($BYTES bytes, sha256 $SHA)"

step "4. Freeze the container as image $SNAP_IMAGE"
docker stop "$NAME" >/dev/null || fail "docker stop failed"
docker commit "$NAME" "$SNAP_IMAGE" >/dev/null || fail "docker commit failed"
SNAP_ID="$(docker image inspect --format '{{.Id}}' "$SNAP_IMAGE" 2>/dev/null || echo UNKNOWN)"
docker rm -f "$NAME" >/dev/null 2>&1
echo "snapshot id: $SNAP_ID"

step "5. Prove the snapshot reproduces the saved data"
docker rm -f "$VERIFY_NAME" >/dev/null 2>&1
# shellcheck disable=SC2086
docker run -d $PLATFORM_ARGS --name "$VERIFY_NAME" "$SNAP_IMAGE" >/dev/null || fail "could not start a container from $SNAP_IMAGE"
vc_wait_ready "$VERIFY_NAME" "$WAIT_SECS" || { docker logs --tail 20 "$VERIFY_NAME" 2>&1 | sed 's/^/    /'; fail "verify container not ready after ${WAIT_SECS}s"; }
export_zwr "$VERIFY_NAME" "$OUT/verify.tmp" "$OUT/verify.stderr" || fail "verify export failed"
docker rm -f "$VERIFY_NAME" >/dev/null 2>&1
if cmp -s "$OUT/body.tmp" "$OUT/verify.tmp"; then VERIFY="VERIFIED: a fresh container from $SNAP_IMAGE exports byte-identical data"
else VERIFY="NOT VERIFIED: fresh container export differs from saved data"; fi
echo "$VERIFY"

step "6. Manifest"
{ echo "saved_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; echo "image=$BASE_IMAGE"; echo "image_digest=$DIGEST"
  echo "snapshot_image=$SNAP_IMAGE"; echo "snapshot_id=$SNAP_ID"; echo "roots=$ROOTS"; echo "m_command=$MCMD"
  echo "nodes=$nodes"; echo "bytes=$BYTES"; echo "sha256=$SHA"; echo "verify=$VERIFY"; } > "$OUT/MANIFEST.txt"
rm -f "$OUT/body.tmp" "$OUT/verify.tmp"; cat "$OUT/MANIFEST.txt"
echo
case "$VERIFY" in
  VERIFIED*) echo "RESULT: SAVED. Data is in $OUT/state.zwr. Next: capture_all.sh"
             echo "NOTE: image $SNAP_IMAGE lives only in your local Docker. state.zwr is the portable copy."; exit 0;;
  *) fail "snapshot did not reproduce the data. Do not use these captures.";;
esac
