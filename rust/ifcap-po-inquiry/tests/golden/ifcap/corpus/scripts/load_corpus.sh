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
# One run at a time: a run holds the lock folder captures-work/corpus/load.lock until it ends,
# also on a FAIL or a signal; a signal that comes while the run takes the lock is acted on once
# the run holds it. A run that cannot remove the folder at its end reports so, and a run that
# loaded then ends with FAIL rather than RESULT: LOADED. A run killed outright (kill -9) leaves
# the folder behind; remove it by hand once no load_corpus.sh is running.
#
# Environment (each optional):
#   NAME=vehu                                                    container name (a letter or digit, then
#                                                                at least one letter, digit, _, . or -)
#   BASE_IMAGE=worldvista/vehu:202609                            the original image (printed as context)
#   SOURCE_REVISION=dd3fd0f3124c8178657d9fff4173c324da30f7ae     source revision (printed as context)
# Inherited: HOME, read only when docker is not on PATH, because vc_find_docker then also looks
# for Docker Desktop's docker in $HOME/.docker/bin.

set -u
fail() { echo; echo "FAIL: $*"; exit 1; }
step() { echo; echo "== $*"; }

# An inherited CDPATH could send the relative cd below into another checkout.
unset CDPATH
cd "$(dirname "$0")/../../../../.." || fail "cannot change to the crate folder, five levels above the folder of $0"
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || fail "cannot find the crate folder (rust/ifcap-po-inquiry) five levels above this script."

KIT=scripts/kit/vehu_common.sh
[ -f "$KIT" ] || fail "$KIT not found: this script needs the committed recording kit."
. "$KIT" || fail "cannot source $KIT (status $?)"

NAME="${NAME:-vehu}"
BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
SOURCE_REVISION="${SOURCE_REVISION:-dd3fd0f3124c8178657d9fff4173c324da30f7ae}"
# Docker's container-name rule, with the letters spelled out so that no locale changes them.
# The value is not printed: it may hold control characters.
case "$NAME" in
  ? | [!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789]* | *[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-]*)
    fail "invalid NAME: a container name is a letter or digit followed by at least one letter, digit, underscore, period or hyphen";;
esac

C=tests/golden/ifcap/corpus
ZWR=tests/fixtures/ifcap-corpus/corpus.zwr
W=captures-work/corpus
GC_SNAP=vehu-snap:gc-base
CORPUS_SNAP=vehu-snap:corpus-base

# vc_find_docker reads HOME when docker is not on PATH, and under set -u an unset HOME would end
# the script there with no FAIL line.
if [ -z "${HOME+set}" ] && ! command -v docker >/dev/null 2>&1; then
  fail "HOME is not set and docker is not on PATH: vc_find_docker also looks for Docker Desktop's docker in \$HOME/.docker/bin; set HOME or put docker on PATH."
fi
vc_find_docker || fail "docker not found: install Docker Desktop or the Docker CLI."
mkdir -p "$W" || fail "cannot create $W"

# One run at a time: two runs would share the scratch files below, the image tag and BASE.txt.
# mkdir is atomic, so the folder is the lock. While the lock is being taken a signal is only
# noted: mkdir runs with signals ignored, so it cannot be killed after it made the folder, and
# the noted signal ends the run through fail once LOCK_HELD records whether this run made it.
# At any other time a signal ends the run at once: on_signal removes the folder, then calls
# fail. The EXIT trap also removes it, with signals ignored, while this run still holds it. A
# run that loads removes it before it reports RESULT, and a folder it cannot remove is a FAIL.
LOCK="$W/load.lock"
LOCK_HELD=0
ACQUIRING=0
SIGNALLED=0
# release_lock: remove the lock folder if this run holds it; non-zero when rm -rf fails.
release_lock() {
  if [ "$LOCK_HELD" -eq 1 ]; then
    rm -rf "$LOCK" || return 1
    LOCK_HELD=0
  fi
  return 0
}
# on_exit: remove the lock with signals ignored, and say once if it cannot be removed. It leaves
# the exit status as it is. It is the EXIT trap, and on_signal runs it before fail: a signal that
# comes as the run exits is handled inside the EXIT trap, where the exit in fail would end the
# run before the EXIT trap removed the lock.
on_exit() {
  trap '' HUP INT TERM
  release_lock && return 0
  echo "cannot remove the lock $LOCK: remove that folder by hand once no load_corpus.sh is running"
  LOCK_HELD=0
}
# on_signal: the HUP, INT and TERM trap.
on_signal() {
  if [ "$ACQUIRING" -eq 1 ]; then
    SIGNALLED=1
    return 0
  fi
  on_exit
  fail "interrupted by a signal"
}
trap on_exit EXIT
trap on_signal HUP INT TERM
ACQUIRING=1
if (trap '' HUP INT TERM; exec mkdir "$LOCK"); then
  LOCK_HELD=1
fi
ACQUIRING=0
[ "$SIGNALLED" -eq 0 ] || fail "interrupted by a signal"
if [ "$LOCK_HELD" -ne 1 ]; then
  if [ -d "$LOCK" ]; then
    HOLDER="$(cat "$LOCK/pid" 2>/dev/null)"
    fail "another run holds the lock $LOCK (pid ${HOLDER:-unknown}); if no other load_corpus.sh is running, remove that folder and start again"
  fi
  fail "cannot create the lock $LOCK"
fi
echo "$$" > "$LOCK/pid" || fail "cannot write $LOCK/pid"

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

# count_data_lines <file>: set COUNT to the number of data lines (lines beginning ^) in <file>.
# grep -c exits 1 when it counts none, so only a status of 2 or more is a failure.
count_data_lines() {
  local rc
  COUNT="$(grep -c '^\^' "$1")"
  rc=$?
  [ "$rc" -le 1 ] || fail "cannot count the data lines of $1 (grep status $rc)"
  case "$COUNT" in
    '' | *[!0123456789]*) fail "cannot count the data lines of $1 (grep -c printed no count)";;
  esac
}

# no_m_errors <message> <file>...: stop with <message>, after the first M errors, when a <file>
# holds %YDB-E or %GTM-E. grep exits 1 for no match and 2 or more when it cannot read a file.
no_m_errors() {
  local message="$1" found rc
  shift
  found="$(grep -hE '%YDB-E|%GTM-E' "$@")"
  rc=$?
  case "$rc" in
    0) printf '%s\n' "$found" | sed -n 1,10p; fail "$message";;
    1) return 0;;
    *) fail "cannot scan $* for M errors (grep status $rc)";;
  esac
}

# is_hex64 <text>: true when <text> is exactly 64 lower-case hexadecimal digits.
is_hex64() {
  case "$1" in *[!0123456789abcdef]*) return 1;; esac
  [ "${#1}" -eq 64 ]
}

# sha256_file <file>: set HASH to the shasum -a 256 digest of <file>.
sha256_file() {
  local out rc
  out="$(shasum -a 256 "$1")"
  rc=$?
  [ "$rc" -eq 0 ] || fail "shasum -a 256 $1 exited with status $rc"
  HASH="${out%% *}"
  is_hex64 "$HASH" || fail "shasum -a 256 $1 printed no 64-digit hexadecimal digest"
}

step "1. Prerequisites"
[ -f "$ZWR" ] || fail "$ZWR not found: run cargo run --offline --locked --bin corpus_gen first"
[ -d "$C" ] || fail "$C not found: run cargo run --offline --locked --bin corpus_gen first"
# Only the data lines: this skips the two header lines of corpus.zwr. The copy must succeed and
# hold every data line, so that a stale or partial copy from an earlier run is never loaded.
count_data_lines "$ZWR"
ZWR_NODES="$COUNT"
[ "$ZWR_NODES" -gt 0 ] || fail "$ZWR holds no data line"
grep '^\^' "$ZWR" > "$W/corpus-data.zwr"
RC=$?
[ "$RC" -eq 0 ] || fail "cannot copy the data lines of $ZWR to $W/corpus-data.zwr (grep status $RC)"
count_data_lines "$W/corpus-data.zwr"
NODES="$COUNT"
[ "$NODES" = "$ZWR_NODES" ] || fail "$W/corpus-data.zwr holds $NODES data lines, but $ZWR holds $ZWR_NODES"
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
no_m_errors "the presence check printed an M error" "$W/present.out" "$W/present.err"
[ "$RC" -eq 0 ] || fail "the presence check exited with status $RC; see $W/present.out and $W/present.err"
grep -q '^CORPUS_PRESENT=' "$W/present.out" || fail "the presence check printed no CORPUS_PRESENT= line; see $W/present.out"
PRESENT="$(grep '^CORPUS_PRESENT=' "$W/present.out" | cut -d= -f2 | tr -d '[:space:]')"
echo "corpus orders present in 991001 to 991200: $PRESENT"
[ "$PRESENT" = "0" ] || fail "corpus orders already present: start again from $GC_SNAP"

step "4. Load $NODES corpus nodes"
# && makes a failed sed the status of the group, and then no halt is sent.
{ sed 's/^/set /' "$W/corpus-data.zwr" && echo "halt"; } | docker exec -i "$NAME" su - vehu -c 'mumps -dir' > "$W/load.out" 2>&1
LOAD_STATUS="${PIPESTATUS[*]}"
no_m_errors "load had errors" "$W/load.out"
[ "$LOAD_STATUS" = "0 0" ] || fail "load pipeline exited with status $LOAD_STATUS; see $W/load.out"
echo "load finished with no M errors"

step "5. Read back"
docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'zwrite ^PRC(442,*)'" < /dev/null > "$W/readback.zwr" 2> "$W/readback.err"
RC=$?
no_m_errors "the read back printed an M error" "$W/readback.zwr" "$W/readback.err"
[ "$RC" -eq 0 ] || fail "the read back exited with status $RC; see $W/readback.err"
# Every loaded line that did not come back exactly; an empty read back makes every line missing.
# With -v, grep exits 0 when it prints such a line, 1 when there is none, 2 or more on an error.
missing="$(grep -Fxv -f "$W/readback.zwr" "$W/corpus-data.zwr")"
RC=$?
case "$RC" in
  0)
    echo "$(printf '%s\n' "$missing" | grep -c '') of $NODES loaded lines did not read back as loaded. First of them:"
    printf '%s\n' "$missing" | sed -n 1,10p | sed 's/^/  MISSING OR DIFFERENT: /'
    echo "first lines read back:"
    sed -n 1,5p "$W/readback.zwr"
    fail "the corpus did not read back byte for byte";;
  1) ;;
  *) fail "cannot compare $W/readback.zwr with $W/corpus-data.zwr (grep status $RC)";;
esac
echo "all $NODES loaded nodes read back identical"

step "6. Freeze as $CORPUS_SNAP"
docker stop "$NAME" >/dev/null || fail "docker stop failed"
docker commit "$NAME" "$CORPUS_SNAP" >/dev/null || fail "docker commit failed"
SNAP_ID="$(docker image inspect --format '{{.Id}}' "$CORPUS_SNAP")"
RC=$?
[ "$RC" -eq 0 ] || fail "cannot read the id of $CORPUS_SNAP (docker image inspect exited with status $RC)"
case "$SNAP_ID" in
  sha256:*) is_hex64 "${SNAP_ID#sha256:}";;
  *) false;;
esac || fail "the id of $CORPUS_SNAP is not sha256: followed by 64 hexadecimal digits: $SNAP_ID"
echo "snapshot id: $SNAP_ID"

step "7. Base export from a fresh container"
restore_container "$CORPUS_SNAP" || fail "could not start a container from $CORPUS_SNAP"
vc_wait_ready "$NAME" 600 || fail "container from $CORPUS_SNAP not ready after 600s"
export_scope "$W/base-export.zwr" "$W/base-export.err"
RC=$?
no_m_errors "the export printed an M error" "$W/base-export.zwr" "$W/base-export.err"
[ "$RC" -eq 0 ] || fail "export failed with status $RC; see $W/base-export.err"
grep -q '^\^' "$W/base-export.zwr" || fail "export is empty; see $W/base-export.err"
sha256_file "$W/base-export.zwr"
EXPORT_SHA="$HASH"
EXPORT_BYTES="$(wc -c < "$W/base-export.zwr")"
RC=$?
[ "$RC" -eq 0 ] || fail "cannot count the bytes of $W/base-export.zwr (wc status $RC)"
# BSD wc pads the count with spaces.
EXPORT_BYTES="${EXPORT_BYTES// /}"
case "$EXPORT_BYTES" in
  '' | *[!0123456789]*) fail "wc -c printed no byte count for $W/base-export.zwr";;
esac
[ "$EXPORT_BYTES" -gt 0 ] || fail "$W/base-export.zwr is empty"
sha256_file "$ZWR"
CORPUS_SHA="$HASH"
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
docker rm -f "$NAME" >/dev/null || fail "cannot remove container $NAME (docker rm -f exited with status $?)"
cat "$C/BASE.txt"
echo
release_lock || fail "cannot remove the lock $LOCK; remove that folder by hand"
echo "RESULT: LOADED. Next: tests/golden/ifcap/corpus/scripts/record_corpus.sh"
exit 0
