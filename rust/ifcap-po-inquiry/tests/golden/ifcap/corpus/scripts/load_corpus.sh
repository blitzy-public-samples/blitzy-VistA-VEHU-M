#!/usr/bin/env bash
# load_corpus.sh: load world B (the 200 CP parity-corpus orders) into a THROWAWAY VEHU container,
# prove it reads back byte for byte, freeze it as image vehu-snap:corpus-base, export the README
# Part B step 1 scope from a fresh container, and write tests/golden/ifcap/corpus/BASE.txt.
# This is not a recording run, so it may run M commands and wait for the database.
#
# Usage (from anywhere; it changes to the crate folder rust/ifcap-po-inquiry/ itself):
#   bash tests/golden/ifcap/corpus/scripts/load_corpus.sh
#
# Prerequisites:
#   - Docker;
#   - the snapshot image vehu-snap:gc-base, made by scripts/kit/load_orders.sh and scripts/kit/save_state.sh;
#   - tests/fixtures/ifcap-corpus/corpus.zwr, made by: cargo run --offline --locked --bin corpus_gen
#
# The team's order:
#   1. tests/golden/ifcap/corpus/scripts/load_corpus.sh     (this script)
#   2. tests/golden/ifcap/corpus/scripts/record_corpus.sh
#   3. tests/golden/ifcap/corpus/scripts/check_corpus.sh
#   4. cargo test --offline --locked
#
# Writes:  the throwaway container NAME, the image vehu-snap:corpus-base,
#          tests/golden/ifcap/corpus/BASE.txt, and scratch files under captures-work/corpus/.
# Never writes: the original image, vehu-snap:gc-base, or any committed file other than BASE.txt.
#
# Environment (each optional):
#   NAME=vehu                                                    container name
#   BASE_IMAGE=worldvista/vehu:202609                            the original image (printed as context)
#   SOURCE_REVISION=dd3fd0f3124c8178657d9fff4173c324da30f7ae     source revision (printed as context)

set -u
fail() { echo; echo "FAIL: $*"; exit 1; }
step() { echo; echo "== $*"; }

cd "$(dirname "$0")/../../../../.." || exit 1
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || fail "cannot find the crate folder (rust/ifcap-po-inquiry) five levels above this script."

KIT=scripts/kit/vehu_common.sh
[ -f "$KIT" ] || fail "$KIT not found: this script needs the committed recording kit."
. "$KIT"

NAME="${NAME:-vehu}"
BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
SOURCE_REVISION="${SOURCE_REVISION:-dd3fd0f3124c8178657d9fff4173c324da30f7ae}"

C=tests/golden/ifcap/corpus
ZWR=tests/fixtures/ifcap-corpus/corpus.zwr
W=captures-work/corpus
GC_SNAP=vehu-snap:gc-base
CORPUS_SNAP=vehu-snap:corpus-base

vc_find_docker || fail "docker not found: install Docker Desktop or the Docker CLI."
mkdir -p "$W" || fail "cannot create $W"

# restore_container <image>: replace container NAME with a fresh one started from <image>.
restore_container() {
  docker rm -f "$NAME" >/dev/null 2>&1
  # shellcheck disable=SC2046,SC2086
  docker run -d $(vc_platform_args) --name "$NAME" "$1" >/dev/null
}

# export_scope <out> <err>: export the README Part B step 1 scope from container NAME (read only).
export_scope() {
  docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD '$(vc_mcmd "$VC_SCOPE_ROOTS")'" < /dev/null > "$1" 2> "$2"
}

step "1. Prerequisites"
[ -f "$ZWR" ] || fail "$ZWR not found: run cargo run --offline --locked --bin corpus_gen first"
[ -d "$C" ] || fail "$C not found: run cargo run --offline --locked --bin corpus_gen first"
# Only the data lines: this skips the two header lines of corpus.zwr.
grep '^\^' "$ZWR" > "$W/corpus-data.zwr"
NODES="$(grep -c '^\^' "$W/corpus-data.zwr")"
[ "${NODES:-0}" -gt 0 ] || fail "$ZWR holds no data line"
docker image inspect "$GC_SNAP" >/dev/null 2>&1 || fail "image $GC_SNAP not found: make it with scripts/kit/load_orders.sh and scripts/kit/save_state.sh"
echo "container:       $NAME"
echo "base image:      $BASE_IMAGE"
echo "source revision: $SOURCE_REVISION"
echo "corpus nodes:    $NODES (from $ZWR)"

step "2. Throwaway container from $GC_SNAP"
restore_container "$GC_SNAP" || fail "docker run failed"
vc_wait_ready "$NAME" 600 || fail "container not ready after 600s"
echo "container $NAME ready"

step "3. Corpus orders must be absent"
# A FOR scopes to the end of the line, so the IF lets the WRITE run once, on the last IEN.
docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'S N=0 F I=991001:1:991200 S:\$D(^PRC(442,I)) N=N+1 I I=991200 W \"CORPUS_PRESENT=\",N,!'" < /dev/null > "$W/present.out" 2> "$W/present.err"
RC=$?
if grep -qE '%YDB-E|%GTM-E' "$W/present.out" "$W/present.err"; then
  grep -hE '%YDB-E|%GTM-E' "$W/present.out" "$W/present.err" | sed -n 1,10p
  fail "the presence check printed an M error"
fi
[ "$RC" -eq 0 ] || fail "the presence check exited with status $RC; see $W/present.out and $W/present.err"
grep -q '^CORPUS_PRESENT=' "$W/present.out" || fail "the presence check printed no CORPUS_PRESENT= line; see $W/present.out"
PRESENT="$(grep '^CORPUS_PRESENT=' "$W/present.out" | cut -d= -f2 | tr -d '[:space:]')"
echo "corpus orders present in 991001 to 991200: $PRESENT"
[ "$PRESENT" = "0" ] || fail "corpus orders already present: start again from $GC_SNAP"

step "4. Load $NODES corpus nodes"
{ sed 's/^/set /' "$W/corpus-data.zwr"; echo "halt"; } | docker exec -i "$NAME" su - vehu -c 'mumps -dir' > "$W/load.out" 2>&1
LOAD_STATUS="${PIPESTATUS[*]}"
if grep -qE '%YDB-E|%GTM-E' "$W/load.out"; then
  echo "M errors during load:"
  grep -E '%YDB-E|%GTM-E' "$W/load.out" | sed -n 1,10p
  fail "load had errors"
fi
[ "$LOAD_STATUS" = "0 0" ] || fail "load pipeline exited with status $LOAD_STATUS; see $W/load.out"
echo "load finished with no M errors"

step "5. Read back"
docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'zwrite ^PRC(442,*)'" < /dev/null > "$W/readback.zwr" 2> "$W/readback.err"
RC=$?
if grep -qE '%YDB-E|%GTM-E' "$W/readback.zwr" "$W/readback.err"; then
  grep -hE '%YDB-E|%GTM-E' "$W/readback.zwr" "$W/readback.err" | sed -n 1,10p
  fail "the read back printed an M error"
fi
[ "$RC" -eq 0 ] || fail "the read back exited with status $RC; see $W/readback.err"
# Every loaded line that did not come back exactly; an empty read back makes every line missing.
missing="$(grep -Fxv -f "$W/readback.zwr" "$W/corpus-data.zwr")"
if [ -n "$missing" ]; then
  echo "$(printf '%s\n' "$missing" | grep -c '') of $NODES loaded lines did not read back as loaded. First of them:"
  printf '%s\n' "$missing" | sed -n 1,10p | sed 's/^/  MISSING OR DIFFERENT: /'
  echo "first lines read back:"
  sed -n 1,5p "$W/readback.zwr"
  fail "the corpus did not read back byte for byte"
fi
echo "all $NODES loaded nodes read back identical"

step "6. Freeze as $CORPUS_SNAP"
docker stop "$NAME" >/dev/null || fail "docker stop failed"
docker commit "$NAME" "$CORPUS_SNAP" >/dev/null || fail "docker commit failed"
SNAP_ID="$(docker image inspect --format '{{.Id}}' "$CORPUS_SNAP" 2>/dev/null)"
[ -n "$SNAP_ID" ] || fail "cannot read the id of $CORPUS_SNAP"
echo "snapshot id: $SNAP_ID"

step "7. Base export from a fresh container"
restore_container "$CORPUS_SNAP" || fail "could not start a container from $CORPUS_SNAP"
vc_wait_ready "$NAME" 600 || fail "container from $CORPUS_SNAP not ready after 600s"
export_scope "$W/base-export.zwr" "$W/base-export.err"
RC=$?
if grep -qE '%YDB-E|%GTM-E' "$W/base-export.zwr" "$W/base-export.err"; then
  grep -hE '%YDB-E|%GTM-E' "$W/base-export.zwr" "$W/base-export.err" | sed -n 1,10p
  fail "the export printed an M error"
fi
[ "$RC" -eq 0 ] || fail "export failed with status $RC; see $W/base-export.err"
grep -q '^\^' "$W/base-export.zwr" || fail "export is empty; see $W/base-export.err"
EXPORT_SHA="$(shasum -a 256 "$W/base-export.zwr" | cut -d' ' -f1)"
EXPORT_BYTES="$(wc -c < "$W/base-export.zwr" | tr -d ' ')"
CORPUS_SHA="$(shasum -a 256 "$ZWR" | cut -d' ' -f1)"
[ -n "$EXPORT_SHA" ] && [ -n "$CORPUS_SHA" ] || fail "shasum -a 256 failed"
echo "export: $W/base-export.zwr ($EXPORT_BYTES bytes, sha256 $EXPORT_SHA)"

step "8. Write BASE.txt"
MCMD="$(vc_mcmd "$VC_SCOPE_ROOTS")"
{
  printf 'snapshot_image=%s\n' "$CORPUS_SNAP"
  printf 'snapshot_id=%s\n' "$SNAP_ID"
  printf 'base_image=%s\n' "$GC_SNAP"
  printf 'roots=%s\n' "$VC_SCOPE_ROOTS"
  printf 'export_command=%s\n' "docker exec -i $NAME su - vehu -c \"mumps -run %XCMD '$MCMD'\""
  printf 'export_sha256=%s\n' "$EXPORT_SHA"
  printf 'export_bytes=%s\n' "$EXPORT_BYTES"
  printf 'corpus_zwr_sha256=%s\n' "$CORPUS_SHA"
} > "$C/BASE.txt" || fail "cannot write $C/BASE.txt"
docker rm -f "$NAME" >/dev/null 2>&1
cat "$C/BASE.txt"
echo
echo "RESULT: LOADED. Next: tests/golden/ifcap/corpus/scripts/record_corpus.sh"
exit 0
