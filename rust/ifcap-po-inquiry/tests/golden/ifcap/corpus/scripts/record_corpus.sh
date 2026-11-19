#!/usr/bin/env bash
# record_corpus.sh: record every parity-corpus case on the ORIGINAL MUMPS (VEHU) in one batch, twice,
# each batch on a fresh container from the image vehu-snap:corpus-base. One export of the README
# Part B step 1 scope before run 1 and one after it prove the inquiries changed no data, and the two
# runs must give byte-identical transcripts. It then writes the run files that src/golden.rs
# (load_corpus_run, parse_corpus_meta) and check_corpus.sh read: run-summary.txt, meta.txt and,
# last, SHA256SUMS. Every transcript is kept exactly as the original printed it: a case whose
# docker exec exits non-zero, or whose transcript shows an M runtime error, is still recorded,
# never skipped, retried or edited, and nothing is normalised.
#
# Usage (from anywhere; it changes to the crate folder rust/ifcap-po-inquiry/ itself):
#   bash tests/golden/ifcap/corpus/scripts/record_corpus.sh
# A run of 231 cases takes a while: every case starts its own session on the original, twice.
#
# Prerequisites:
#   - Docker;
#   - the image vehu-snap:corpus-base and tests/golden/ifcap/corpus/BASE.txt, both made by
#     tests/golden/ifcap/corpus/scripts/load_corpus.sh (which itself needs vehu-snap:gc-base);
#   - tests/golden/ifcap/corpus/manifest.tsv and every <id>/input.txt, made by:
#     cargo run --offline --locked --bin corpus_gen
#   - the original image worldvista/vehu:202609 in the local Docker, for its digest.
#
# The team's order:
#   1. tests/golden/ifcap/corpus/scripts/load_corpus.sh
#   2. tests/golden/ifcap/corpus/scripts/record_corpus.sh     (this script)
#   3. tests/golden/ifcap/corpus/scripts/check_corpus.sh
#   4. cargo test --offline --locked
#
# Writes:  tests/golden/ifcap/corpus/<id>/transcript.txt for every case, and in
#          tests/golden/ifcap/corpus/ the run files run-summary.txt, meta.txt and SHA256SUMS;
#          scratch files under captures-work/corpus/; the container NAME.
# Never writes: any database (it runs only the inquiry and the scope export), input.txt,
#          manifest.tsv, BASE.txt, tests/golden/ifcap/GC-0N/, tests/golden/ifcap/state/,
#          tests/fixtures/ or scripts/kit/.
#
# The only M commands it runs are the README start command (twice per case) and the scope export
# (twice in all). Readiness is a shell-only readiness probe (SPEC.md O-16), and the start command
# is detected with shell tests only. No context probe runs, so meta.txt records the context as the
# constant SPEC.md O-01 answer.
#
# Exit status: 0 when the two exports and the two runs are identical, whatever was quarantined by
# the paging detector and whatever exit statuses the cases recorded; 1 on a failed prerequisite or
# operation (a FAIL: line, at once) or when a difference was collected (after every file is written).
#
# Deadlines: every Docker operation except the inquiry sessions runs under a wall-clock deadline,
# checked once a second, and one still running at its deadline is stopped with every process it
# started (TERM, then KILL after a 3-s grace) and ends the run with a FAIL: line naming the
# operation and its deadline. Nothing is retried. The deadlines are 120 s for each docker image
# inspect and for the start-command detection; 300 s for each container restore (docker rm -f,
# then docker run -d) and for the final docker rm -f; 600 s for each readiness probe, which keeps
# its own limit; and 1800 s for each of the two scope exports. The inquiry sessions, the README
# Part B step 1 start command run twice per case, are not supervised: each runs as given, for as
# long as the original takes. A FAIL or a signal stops an operation or probe still running the
# same way, within about 5 s, before the lock is removed.
#
# Environment (each optional):
#   NAME=vehu                                                    container name
#   BASE_IMAGE=worldvista/vehu:202609                            the original image (its digest goes to meta.txt)
#   SOURCE_REVISION=dd3fd0f3124c8178657d9fff4173c324da30f7ae     source revision written to meta.txt
# Inherited: HOME, read only when docker is not on PATH, because vc_find_docker then also looks
# for Docker Desktop's docker in $HOME/.docker/bin.

set -u
fail() { echo; echo "FAIL: $*"; exit 1; }
step() { echo; echo "== $*"; }

unset CDPATH || fail "setup: cannot unset CDPATH (read-only)"  # an inherited CDPATH must not pick the folder the relative cd below lands in
cd "$(dirname "$0")/../../../../.." || fail "setup: cannot change to the crate folder five levels above this script ($0)"
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || fail "cannot find the crate folder (rust/ifcap-po-inquiry) five levels above this script."

KIT=scripts/kit/vehu_common.sh
[ -f "$KIT" ] || fail "$KIT not found: this script needs the committed recording kit."
. "$KIT" || fail "cannot source $KIT (status $?)"
# vc_find_docker reads HOME when docker is not on PATH, and under set -u an unset HOME would end
# the script there with no FAIL line.
if [ -z "${HOME+set}" ] && ! command -v docker >/dev/null 2>&1; then
  fail "HOME is not set and docker is not on PATH: vc_find_docker also looks for Docker Desktop's docker in \$HOME/.docker/bin; set HOME or put docker on PATH."
fi
vc_find_docker || fail "docker not found: install Docker Desktop or the Docker CLI."

NAME="${NAME:-vehu}"
BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
SOURCE_REVISION="${SOURCE_REVISION:-dd3fd0f3124c8178657d9fff4173c324da30f7ae}"

C=tests/golden/ifcap/corpus
MANIFEST="$C/manifest.tsv"
BASE="$C/BASE.txt"
W=captures-work/corpus
CORPUS_SNAP=vehu-snap:corpus-base
TAB="$(printf '\t')"
W_ABS="$(pwd)/$W"

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

# Deadline supervision, the same text in load_corpus.sh and record_corpus.sh. A Docker operation
# that hangs, in the daemon, the transport or the container, must end the run with a FAIL: line
# rather than hold it up, so each one runs under one of these wall-clock deadlines, in seconds.
DEADLINE_INSPECT=120    # each docker image inspect; the start-command detection
DEADLINE_CONTAINER=300  # each container restore (docker rm -f, docker run -d); docker stop; the final docker rm -f
DEADLINE_READY=600      # each readiness wait of load_corpus.sh (record_corpus.sh's probe_ready keeps its own 600)
DEADLINE_BULK=1800      # the presence check, the load, the read back, each scope export; docker commit
# JOB_PID: the process group of the operation run_bounded is running, empty when none is, so that
# the exit cleanup can stop it. BOUNDED_EXPIRED: what the last run_bounded call stopped, or empty.
JOB_PID=""
BOUNDED_EXPIRED=""

# stop_owned <target>: stop a process this run started, <target> being its pid, or -<pgid> for
# its whole process group: TERM, then KILL after a 3-s grace, checked once a second. 0 when it is
# gone; 1 when KILL has not ended it 1 s later, which means it is stuck in the kernel, and it is
# not waited for. It takes at most 5 s. Silenced as a group, so that the shell's report of the
# stopped job is not printed either.
stop_owned() {
  local target="$1" spent=0
  {
    kill -TERM -- "$target"
    while [ "$spent" -lt 3 ] && kill -0 -- "$target"; do
      sleep 1
      spent=$((spent + 1))
    done
    kill -0 -- "$target" && kill -KILL -- "$target" && sleep 1
    ! kill -0 -- "$target"
  } 2>/dev/null
}

# run_bounded <limit> <label> <command> [<arg>...]: run <command>, a function or a program, with
# standard input from /dev/null, in the background as a process group of its own (JOB_PID), and
# return its status. It takes at most <limit> s of wall-clock time in all, stopping included:
# checked once a second, the command gets <limit> s by bash SECONDS less 6 s, which are the 3-s
# grace, 1 s for KILL to take effect, 1 s by which SECONDS, counting whole seconds, can lag real
# time, and 1 s for the check. A command still running then is stopped with its whole group
# (stop_owned) and reaped, or, when KILL has not ended the group, left in JOB_PID for the exit
# cleanup; BOUNDED_EXPIRED then names <label> and <limit>, and the status is 124. It never calls
# fail, because callers send its output to files: each caller runs check_deadline straight after.
run_bounded() {
  local limit="$1" label="$2" start="$SECONDS" window rc
  shift 2
  BOUNDED_EXPIRED=""
  window=$((limit - 6))
  # Job control only while the command starts, so that it alone gets a process group of its own.
  set -m
  ( "$@" ) < /dev/null &
  JOB_PID=$!
  set +m
  while kill -0 "$JOB_PID" 2>/dev/null; do
    if [ $((SECONDS - start)) -ge "$window" ]; then
      if stop_owned "-$JOB_PID"; then
        { wait "$JOB_PID"; } 2>/dev/null
        JOB_PID=""
        BOUNDED_EXPIRED="$label did not finish within $limit s and was stopped"
      else
        BOUNDED_EXPIRED="$label did not finish within $limit s, and KILL has not ended its process group $JOB_PID"
      fi
      return 124
    fi
    sleep 1
  done
  wait "$JOB_PID"
  rc=$?
  JOB_PID=""
  return "$rc"
}

# check_deadline: straight after a run_bounded call, stop with a FAIL: line if it ran out of time.
check_deadline() {
  [ -z "$BOUNDED_EXPIRED" ] || fail "$BOUNDED_EXPIRED"
}

# read_one_line <file>: set LINE to the one line <file> holds, ended by a line feed and at most
# 4095 characters long. 1, with nothing cut short, when <file> cannot be read, holds no line, a
# longer one, or anything after it.
read_one_line() {
  local extra=""
  LINE=""
  { IFS= read -r -n 4096 LINE && [ "${#LINE}" -lt 4096 ] && ! IFS= read -r -n 1 extra && [ -z "$extra" ]; } 2>/dev/null < "$1"
}
# End of the deadline supervision.

# The overrides are checked before any scratch or container work; an empty value keeps the default
# above. NAME must be a Docker container name and BASE_IMAGE an image reference, so neither holds
# white space or a control character or begins like an option. SOURCE_REVISION goes to meta.txt
# unchanged, so it must be one line with no control character. The letters are spelled out because
# a bracket range follows the locale.
ALNUM=abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789
case "$NAME" in
  ?|[!"$ALNUM"]*|*[!"$ALNUM"_.-]*)
    fail "NAME=$(printf '%q' "$NAME") is not a Docker container name: NAME may hold only letters, digits, _ . and -, at least two characters, the first a letter or digit" ;;
esac
# valid_image_ref <ref>: 0 when <ref> follows Docker's image reference grammar, 1 otherwise.
# A reference is name[:tag][@digest]. The digest follows the first @ and is algorithm:hex: the
# algorithm is letter-led runs of letters and digits joined by one + . _ or -, the hex is 32 or
# more hex digits. The tag follows a : in the last /-separated part: 1 to 128 letters, digits,
# _ . and -, not beginning with . or -. The name, at most 255 characters, is [domain/]path. Its
# first part is a domain only when a / follows it and it holds . or :, is localhost or holds an
# upper-case letter. A domain is host[:port]: the host is dot-joined parts of letters, digits and
# inner dashes, or [hex digits and :]; the port is digits. The path is /-joined components of
# lower-case letters and digits, joined inside a component by one . or _, by __, or by dashes.
valid_image_ref() {
  local lower=abcdefghijklmnopqrstuvwxyz upper=ABCDEFGHIJKLMNOPQRSTUVWXYZ digit=0123456789
  local hex=0123456789abcdefABCDEF ref="$1" algo digest tag domain host port path
  case "$ref" in
    *@*)
      digest="${ref#*@}"
      ref="${ref%%@*}"
      case "$digest" in *:*) ;; *) return 1 ;; esac
      algo="${digest%%:*}"
      digest="${digest#*:}"
      case "$algo" in
        ''|[!"$lower$upper"]*|*[+._-]|*[+._-][!"$lower$upper"]*|*[!"$lower$upper$digit"+._-]*) return 1 ;;
      esac
      case "$digest" in ''|*[!"$hex"]*) return 1 ;; esac
      [ "${#digest}" -ge 32 ] || return 1 ;;
  esac
  case "${ref##*/}" in
    *:*)
      tag="${ref##*/}"
      tag="${tag#*:}"
      case "$tag" in ''|[!"$lower$upper$digit"_]*|*[!"$lower$upper$digit"_.-]*) return 1 ;; esac
      [ "${#tag}" -le 128 ] || return 1
      ref="${ref%:*}" ;;
  esac
  [ "${#ref}" -le 255 ] || return 1
  path="$ref"
  case "$ref" in
    */*)
      domain="${ref%%/*}"
      case "$domain" in
        *[.:]*|localhost|*["$upper"]*)
          path="${ref#*/}"
          case "$domain" in
            \[*\]|\[*\]:*)
              host="${domain#\[}"
              host="${host%%\]*}"
              port="${domain#*\]}"
              case "$host" in ''|*[!"$hex:"]*) return 1 ;; esac ;;
            *)
              host="${domain%%:*}"
              port=""
              case "$domain" in *:*) port=":${domain#*:}" ;; esac
              case "$host" in ''|[.-]*|*[.-]|*..*|*.-*|*-.*|*[!"$lower$upper$digit".-]*) return 1 ;; esac ;;
          esac
          case "$port" in
            '') ;;
            :*) case "${port#:}" in ''|*[!"$digit"]*) return 1 ;; esac ;;
            *) return 1 ;;
          esac ;;
      esac ;;
  esac
  case "$path" in
    ''|/*|*/|*//*|[._-]*|*[._-]|*[._-]/*|*/[._-]*|*..*|*___*|*._*|*_.*|*.-*|*-.*|*_-*|*-_*|*[!"$lower$digit"._/-]*) return 1 ;;
  esac
  return 0
}
valid_image_ref "$BASE_IMAGE" ||
  fail "BASE_IMAGE=$(printf '%q' "$BASE_IMAGE") is not an image reference: BASE_IMAGE must be [host[:port]/]path[:tag][@algorithm:hex], with path components of lower-case letters and digits joined by one . or _, by __ or by dashes, a tag of 1 to 128 letters, digits, _ . and - not beginning with . or -, a digest of 32 or more hex digits, and at most 255 characters before the tag"
case "$SOURCE_REVISION" in
  *[[:cntrl:]]*)
    fail "SOURCE_REVISION holds a control character (a line feed, carriage return, tab or other): SOURCE_REVISION may hold only one line of printable text, which meta.txt records unchanged" ;;
  *[![:space:]]*) ;;
  *)
    fail "SOURCE_REVISION holds only white space: SOURCE_REVISION must name the source revision that meta.txt records" ;;
esac
mkdir -p "$W" || fail "cannot create $W"

# One run at a time: concurrent runs would share captures-work/corpus/ and the corpus folder, and
# step 4 clears run1/ and run2/. The lock is the folder record.lock, taken by an atomic mkdir before
# any container or scratch work and holding its owner's pid. A run that cannot take it stops and
# deletes nothing. on_exit runs on every exit, after HUP, INT and TERM too, and ignores those
# signals from then on, so that a second signal cannot cut it short. It stops a readiness probe
# and a Docker operation still running, each with stop_owned (TERM, then KILL after a 3-s grace,
# at most 5 s), reaps each one that ended and says so of one that did not, and removes the lock
# only when this run took it. The exit status stays as it was.
LOCK="$W_ABS/record.lock"
LOCK_OWNED=""
on_exit() {
  trap '' HUP INT TERM
  if [ -n "${PROBE_PID:-}" ]; then
    if stop_owned "$PROBE_PID"; then
      { wait "$PROBE_PID"; } 2>/dev/null
    else
      echo "the readiness probe (pid $PROBE_PID) did not end after KILL and is left behind"
    fi
    PROBE_PID=""
  fi
  if [ -n "${JOB_PID:-}" ]; then
    if stop_owned "-$JOB_PID"; then
      { wait "$JOB_PID"; } 2>/dev/null
    else
      echo "the Docker operation of process group $JOB_PID did not end after KILL and is left behind"
    fi
    JOB_PID=""
  fi
  [ -z "$LOCK_OWNED" ] || rm -rf "$LOCK"
}
trap on_exit EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
if mkdir "$LOCK" 2>/dev/null; then
  LOCK_OWNED=1
  printf '%s\n' "$$" > "$LOCK/pid" || fail "cannot write $LOCK/pid"
else
  [ -d "$LOCK" ] || fail "cannot create $LOCK"
  # At most 21 characters of the owner's pid file, and only from a regular file, so that
  # neither a large file nor a FIFO holds this run up. Anything but 1 to 20 digits with no
  # leading 0 counts as no pid.
  owner=""
  [ -f "$LOCK/pid" ] && { IFS= read -r -n 21 owner < "$LOCK/pid"; } 2>/dev/null
  case "$owner" in ''|0*|*[!0123456789]*) owner="" ;; esac
  [ "${#owner}" -le 20 ] || owner=""
  if [ -z "$owner" ]; then
    fail "another record_corpus.sh run is taking $LOCK (no pid written yet): wait for it. If no record_corpus.sh run is active, remove the folder by hand (rm -r $(printf '%q' "$LOCK")); recording files are untouched"
  elif [ "$owner" != "$$" ] && kill -0 "$owner" 2>/dev/null; then
    fail "another record_corpus.sh run (pid $owner) holds $LOCK: wait for it to finish, because only one run may record at a time"
  else
    fail "$LOCK is a stale lock left by an interrupted run (pid $owner, no longer running): check that no record_corpus.sh run is active, then remove the folder by hand (rm -r $(printf '%q' "$LOCK")); recording files are untouched"
  fi
fi

# PROBE_PID: the process id of the readiness probe while one runs, empty otherwise, so that the
# exit cleanup can stop a probe an interrupted run would leave behind.
PROBE_PID=""
# probe_ready: shell-only readiness probe (SPEC.md O-16). It polls container NAME every 5 s until
# the vehu login shell runs and finds the environment file the start command sources, and takes
# at most 600 s of wall-clock time in all, stopping a probe included. Each probe runs in the
# background and is checked once a second. Probes and pauses fill the first 594 s by bash
# SECONDS: 600 s less a 3-s grace, 1 s for KILL to take effect, 1 s by which SECONDS, counting
# whole seconds, can lag real time, and 1 s for the once-a-second check. A probe still running
# when those 594 s are up is stopped (TERM, then KILL after the 3-s grace) and reaped; one that
# KILL has not ended 1 s later is stuck in the kernel and is left to the exit cleanup rather than
# waited for. A probe that exits non-zero means not ready yet, and the pause after it is 5 s, or
# what is left of the 594 s when less. It runs no M command, so it proves the shell and the
# environment file, not the database: the export before run 1 and the two-run identity check
# are the guard. 0 when ready, 1 on timeout.
probe_ready() {
  local limit=600 grace=3 start="$SECONDS" window left rc spent
  window=$((limit - grace - 3))
  while :; do
    left=$((window - (SECONDS - start)))
    [ "$left" -gt 0 ] || return 1
    docker exec "$NAME" su - vehu -c 'test -f /home/vehu/etc/env' < /dev/null >/dev/null 2>&1 &
    PROBE_PID=$!
    while kill -0 "$PROBE_PID" 2>/dev/null; do
      left=$((window - (SECONDS - start)))
      if [ "$left" -le 0 ]; then
        # Silenced as a group, so the shell's report of the stopped job is not printed either.
        {
          kill -TERM "$PROBE_PID"
          spent=0
          while [ "$spent" -lt "$grace" ] && kill -0 "$PROBE_PID"; do
            sleep 1
            spent=$((spent + 1))
          done
          kill -0 "$PROBE_PID" && kill -KILL "$PROBE_PID" && sleep 1
          # Still there 1 s after KILL: not waited for, and PROBE_PID keeps it for on_exit.
          kill -0 "$PROBE_PID" || { wait "$PROBE_PID"; PROBE_PID=""; }
        } 2>/dev/null
        return 1
      fi
      sleep 1
    done
    wait "$PROBE_PID"
    rc=$?
    PROBE_PID=""
    [ "$rc" -eq 0 ] && return 0
    left=$((window - (SECONDS - start)))
    [ "$left" -gt 0 ] || return 1
    [ "$left" -le 5 ] || left=5
    sleep "$left"
  done
}

# check_export <name> <status>: the fatal checks of the scope export captures-work/corpus/<name>.zwr,
# whose standard error is <name>.err and whose export_scope exited with <status>: no M error, a
# zero exit status, and at least one data line.
check_export() {
  if grep -qE '%YDB-E|%GTM-E' "$W/$1.zwr" "$W/$1.err"; then
    grep -hE '%YDB-E|%GTM-E' "$W/$1.zwr" "$W/$1.err" | sed -n 1,10p
    fail "$W/$1.zwr: the export printed an M error"
  fi
  [ "$2" -eq 0 ] || fail "$W/$1.zwr: the export exited with status $2; see $W/$1.err"
  grep -q '^\^' "$W/$1.zwr" || fail "$W/$1.zwr: the export is empty; see $W/$1.err"
}

# read_capped <file> <budget> <line bound> <handler> <remedy>: read <file> once, line by line, on
# descriptor 6, and call <handler> <line number> <line> for each line, its newline removed. An
# unterminated last line is a line too; an empty file calls nothing. Lengths are byte counts
# (LC_ALL=C). A file larger than <budget> bytes stops before it is read, and a line longer than
# <line bound> bytes, a bound the caller keeps below <budget>, stops when it is reached, so no
# read takes more than <line bound> + 1 bytes. The bytes read are counted against the size taken
# before reading: the run stops once they pass it, because the file grew, and at the end unless
# they equal it, because a NUL byte (which bash drops), a failed read or a change cut them short.
# Nothing is truncated: every stop is a FAIL naming <file> and ending with <remedy>.
read_capped() {
  local LC_ALL=C file="$1" budget="$2" bound="$3" handler="$4" remedy="$5" size line="" n=0 total=0
  size="$(wc -c < "$file")" || fail "cannot take the size of $file: $remedy"
  size="${size// /}"
  case "$size" in ''|*[!0123456789]*) fail "wc -c gave no byte count for $file: $size";; esac
  [ "$size" -le "$budget" ] || fail "$file holds $size bytes, more than its budget of $budget bytes: $remedy"
  exec 6< "$file" || fail "cannot read $file: $remedy"
  while IFS= read -r -n "$((bound + 1))" -u 6 line; do
    n=$((n + 1))
    [ "${#line}" -le "$bound" ] || fail "line $n of $file is longer than $bound bytes: $remedy"
    total=$((total + ${#line} + 1))
    [ "$total" -le "$size" ] || fail "$file grew while it was read, past the $size bytes it held before: $remedy"
    "$handler" "$n" "$line"
  done
  exec 6<&-
  total=$((total + ${#line}))
  [ "$total" -eq "$size" ] \
    || fail "read $total of the $size bytes of $file: it holds a NUL byte, could not be read whole or changed while it was read: $remedy"
  [ -z "$line" ] || "$handler" "$((n + 1))" "$line"
}

step "1. Prerequisites"
# BASE.txt and manifest.tsv are read by read_capped within the budgets src/golden.rs uses:
# 64 KiB (65536 bytes), its budget for BASE.txt, which load_corpus.sh writes as eight short
# key=value lines, and 1 MiB (1048576 bytes), its budget for input.txt and transcript.txt, for
# manifest.tsv, which corpus_gen writes as 232 lines and 105,241 bytes, the longest 1531 bytes.
# A BASE.txt line may fill its budget but for its newline. A manifest line may hold at most
# 16384 bytes, over ten times the longest, because the row checks below take time that grows
# with the square of a line's length, and the bound keeps each line's checks short.
BASE_BUDGET=65536
BASE_LINE_BOUND=65535
MANIFEST_BUDGET=1048576
MANIFEST_LINE_BOUND=16384
[ -f "$BASE" ] || fail "$BASE not found: run tests/golden/ifcap/corpus/scripts/load_corpus.sh first"
# base_line <line number> <line>: take the snapshot_id= and export_sha256= values, each the rest
# of its line after the key, exactly. A key given twice stops the run, since either value could
# be the one meant.
BASE_SNAP_ID=""
BASE_SNAP_LINE=""
BASE_EXPORT_SHA=""
BASE_SHA_LINE=""
base_line() {
  case "$2" in
    snapshot_id=*)
      [ -z "$BASE_SNAP_LINE" ] || fail "$BASE holds snapshot_id= more than once (lines $BASE_SNAP_LINE and $1): run load_corpus.sh again"
      BASE_SNAP_LINE="$1"
      BASE_SNAP_ID="${2#snapshot_id=}" ;;
    export_sha256=*)
      [ -z "$BASE_SHA_LINE" ] || fail "$BASE holds export_sha256= more than once (lines $BASE_SHA_LINE and $1): run load_corpus.sh again"
      BASE_SHA_LINE="$1"
      BASE_EXPORT_SHA="${2#export_sha256=}" ;;
  esac
}
read_capped "$BASE" "$BASE_BUDGET" "$BASE_LINE_BOUND" base_line "run load_corpus.sh again"
[ -n "$BASE_SNAP_ID" ] || fail "$BASE holds no snapshot_id= value: run load_corpus.sh again"
[ -n "$BASE_EXPORT_SHA" ] || fail "$BASE holds no export_sha256= value: run load_corpus.sh again"
[ -f "$MANIFEST" ] || fail "$MANIFEST not found: run cargo run --offline --locked --bin corpus_gen first"
# The manifest is read once by read_capped, so a failed or short read stops here, and each line is
# split into cells by parameter expansion only, so no word splitting or globbing touches a cell.
# Its cases are held in the index-aligned arrays IDS and CLASSES, in manifest order. read_capped
# reads on descriptor 6, and its read loop, with the handler it calls, runs no docker command, so
# nothing in it competes for standard input; every loop below walks those arrays, never a
# `while read` over standard input, which each `docker exec -i` would consume. Empty lines
# at the end of the file are ignored; an empty line before a later line is a row, and a bad one.
HEADER_TABS=""
COUNT=0
BAD=""
BAD_COUNT=0
BAD_SHOWN=20
EMPTY_FROM=""
SEEN=" "
IDS=()
CLASSES=()
# bad_row <line number>: count a bad manifest row in BAD_COUNT, and keep the line numbers of the
# first BAD_SHOWN in BAD for the FAIL line, which says how many more there are, so that BAD stays
# small however many rows are bad.
bad_row() {
  BAD_COUNT=$((BAD_COUNT + 1))
  [ "$BAD_COUNT" -gt "$BAD_SHOWN" ] || BAD="$BAD $1"
}
# manifest_line <line number> <line>: check one manifest line and add a good row's case to IDS and
# CLASSES, or count it with bad_row. A run of empty rows is held, at no cost per row, as the
# number of its first line, EMPTY_FROM, until a later line shows the run is not at the end and
# makes each of them a bad row.
manifest_line() {
  if [ -z "$2" ] && [ "$1" -gt 1 ]; then
    [ -n "$EMPTY_FROM" ] || EMPTY_FROM="$1"
    return 0
  fi
  local line="$2" tabs id cells class ok
  tabs="${line//[!$TAB]/}"
  id="${line%%"$TAB"*}"
  cells="${line#*"$TAB"}"
  class="${cells%%"$TAB"*}"
  # Line 1 is the manifest's eight-column header, byte for byte as corpus_gen writes it, so a
  # renamed, missing or extra column, or a trailing space, TAB or CR, stops the run here.
  if [ "$1" -eq 1 ]; then
    [ "$line" = "case${TAB}class${TAB}seed${TAB}order_ien${TAB}c_value${TAB}item_count${TAB}tags${TAB}expected_branch" ] \
      || fail "$MANIFEST line 1 is not the header case<TAB>class<TAB>seed<TAB>order_ien<TAB>c_value<TAB>item_count<TAB>tags<TAB>expected_branch: run corpus_gen again"
    HEADER_TABS="${#tabs}"
    return 0
  fi
  if [ -n "$EMPTY_FROM" ]; then
    while [ "$EMPTY_FROM" -lt "$1" ]; do
      bad_row "$EMPTY_FROM"
      EMPTY_FROM=$((EMPTY_FROM + 1))
    done
    EMPTY_FROM=""
  fi
  # Every row has the header's eight columns. Ids become paths, so only the three corpus series
  # pass, and a cell is taken exactly as written, so a space in an id or a class rejects the row.
  ok=1
  [ "${#tabs}" -eq "$HEADER_TABS" ] || ok=0
  case "$id" in CP-[0-9][0-9][0-9]|CE-[0-9][0-9]|CR-[0-9][0-9]) ;; *) ok=0;; esac
  case "$class" in CP|IN|BOUNDARY|CR) ;; *) ok=0;; esac
  if [ "$ok" -eq 0 ]; then
    bad_row "$1"
    return 0
  fi
  case "$SEEN" in *" $id "*) fail "$MANIFEST lists case id $id more than once (line $1)";; esac
  SEEN="$SEEN$id "
  IDS+=("$id")
  CLASSES+=("$class")
  COUNT=$((COUNT + 1))
}
read_capped "$MANIFEST" "$MANIFEST_BUDGET" "$MANIFEST_LINE_BOUND" manifest_line "run corpus_gen again"
# An empty manifest has no line 1, so it fails the header check as an empty line 1 does.
[ -n "$HEADER_TABS" ] || manifest_line 1 ""
[ "$BAD_COUNT" -le "$BAD_SHOWN" ] || BAD="$BAD and $((BAD_COUNT - BAD_SHOWN)) more"
[ -z "$BAD" ] || fail "$MANIFEST has rows that are not <case id><TAB><class> with the header's $((HEADER_TABS + 1)) columns, at lines:$BAD"
[ "$COUNT" -gt 0 ] || fail "$MANIFEST lists no case"
echo "cases: $COUNT (corpus_gen writes 231)"
MISSING=""
for id in "${IDS[@]}"; do
  [ -f "$C/$id/input.txt" ] || MISSING="$MISSING $id"
done
[ -z "$MISSING" ] || fail "input.txt missing for:$MISSING (run cargo run --offline --locked --bin corpus_gen)"
# Each inspect writes to a file, and its exit status is checked before the file is read, so a
# failed or stopped inspect never passes its output on; the file must hold exactly one line.
run_bounded "$DEADLINE_INSPECT" "docker image inspect $CORPUS_SNAP" \
  docker image inspect --format '{{.Id}}' "$CORPUS_SNAP" > "$W/snapshot-id.out" 2> "$W/snapshot-id.err"
RC=$?
check_deadline
[ "$RC" -eq 0 ] \
  || fail "image $CORPUS_SNAP not found or not readable (docker image inspect exited with status $RC; see $W/snapshot-id.err): run tests/golden/ifcap/corpus/scripts/load_corpus.sh first"
read_one_line "$W/snapshot-id.out" \
  || fail "docker image inspect did not print the id of $CORPUS_SNAP as one line of at most 4095 characters; see $W/snapshot-id.out"
SNAP_ID="$LINE"
[ -n "$SNAP_ID" ] || fail "image $CORPUS_SNAP not found: run tests/golden/ifcap/corpus/scripts/load_corpus.sh first"
[ "$SNAP_ID" = "$BASE_SNAP_ID" ] || fail "the id of $CORPUS_SNAP ($SNAP_ID) differs from $BASE snapshot_id ($BASE_SNAP_ID): run load_corpus.sh again"
run_bounded "$DEADLINE_INSPECT" "docker image inspect $BASE_IMAGE" \
  docker image inspect --format '{{index .RepoDigests 0}}' "$BASE_IMAGE" > "$W/base-digest.out" 2> "$W/base-digest.err"
RC=$?
check_deadline
[ "$RC" -eq 0 ] \
  || fail "cannot read the digest of $BASE_IMAGE (docker image inspect exited with status $RC; see $W/base-digest.err): pull it with docker pull $BASE_IMAGE"
read_one_line "$W/base-digest.out" \
  || fail "docker image inspect did not print the digest of $BASE_IMAGE as one line of at most 4095 characters; see $W/base-digest.out"
DIGEST="$LINE"
[ -n "$DIGEST" ] || fail "cannot read the digest of $BASE_IMAGE: pull it with docker pull $BASE_IMAGE"
echo "container:       $NAME"
echo "snapshot:        $CORPUS_SNAP ($SNAP_ID)"
echo "base image:      $BASE_IMAGE ($DIGEST)"
echo "source revision: $SOURCE_REVISION"

step "2. Container for run 1"
run_bounded "$DEADLINE_CONTAINER" "the restore of container $NAME from $CORPUS_SNAP (docker rm -f, then docker run -d)" \
  restore_container "$CORPUS_SNAP" || { check_deadline; fail "docker run failed for $CORPUS_SNAP"; }
probe_ready || fail "container not ready after 600s (shell-only readiness probe, SPEC.md O-16; check: docker logs $NAME)"
echo "container $NAME ready"

step "3. Start command"
# detect_start_command <file>: vc_detect_inquiry on container NAME, then INQUIRY_CMD and
# INQUIRY_NOTE written to <file>, one per line, because run_bounded runs it in a subshell whose
# variables do not come back. Its status is that of the write: the kit function's last command is
# an assignment, so its own status means nothing.
detect_start_command() {
  vc_detect_inquiry "$NAME"
  printf '%s\n' "$INQUIRY_CMD" "$INQUIRY_NOTE" > "$1"
}
rm -f "$W/start-command.txt" || fail "cannot remove $W/start-command.txt"
run_bounded "$DEADLINE_INSPECT" "the start-command detection (vc_detect_inquiry $NAME)" \
  detect_start_command "$W/start-command.txt"
RC=$?
check_deadline
[ "$RC" -eq 0 ] || fail "cannot write the detected start command to $W/start-command.txt (status $RC)"
# IFS= and -r keep both values byte for byte: the note can end in a space, and the command holds
# quotes, backslashes and $.
INQUIRY_CMD=""
INQUIRY_NOTE=""
{ IFS= read -r INQUIRY_CMD && IFS= read -r INQUIRY_NOTE; } 2>/dev/null < "$W/start-command.txt" \
  || fail "cannot read the detected start command back from $W/start-command.txt"
if [ -n "$INQUIRY_NOTE" ]; then
  echo "note: $INQUIRY_NOTE"
  fail "the start command would not be the README's (tests/golden/ifcap/README.md Part B step 1), so nothing is recorded"
fi
echo "start command: $INQUIRY_CMD"

step "4. Export before"
rm -rf "$W/run1" "$W/run2" || fail "cannot clear $W/run1 and $W/run2"
mkdir -p "$W/run1" "$W/run2" || fail "cannot create $W/run1 and $W/run2"
rm -f "$W/diffs.tsv" || fail "cannot remove $W/diffs.tsv"
: > "$W/diffs.tsv" || fail "cannot write $W/diffs.tsv"
run_bounded "$DEADLINE_BULK" "the scope export before run 1" export_scope "$W/export-before.zwr" "$W/export-before.err"
RC=$?
check_deadline
check_export export-before "$RC"
BEFORE_SHA="$(shasum -a 256 "$W/export-before.zwr" | cut -d' ' -f1)"
[ -n "$BEFORE_SHA" ] || fail "shasum -a 256 failed on $W/export-before.zwr"
[ "$BEFORE_SHA" = "$BASE_EXPORT_SHA" ] \
  || fail "the export before (sha256 $BEFORE_SHA) differs from $BASE export_sha256 ($BASE_EXPORT_SHA): the container does not hold the loaded corpus state"
echo "export before: $W/export-before.zwr (sha256 $BEFORE_SHA, equal to $BASE)"
# The run files of an earlier run go first: an interrupted run then leaves transcripts with no
# listing, which the corpus reader reports as an integrity failure instead of trusting.
rm -f "$C/meta.txt" "$C/run-summary.txt" "$C/SHA256SUMS" || fail "cannot remove the run files of an earlier run in $C"


step "5. Run 1 ($COUNT cases)"
# No pseudo-terminal (no -t), so the transcript holds exactly what the original wrote. Each case's
# input, transcript and standard error are opened first, on descriptors 3, 4 and 5, each checked,
# so a file that cannot be opened stops the run instead of passing for an exit status of the
# original. A case's exit status is evidence only: whatever it is, the transcript is kept as written.
: > "$W/run1/status.tsv" || fail "cannot write $W/run1/status.tsv"
n=0
RC1=()
for id in "${IDS[@]}"; do
  n=$((n + 1))
  exec 3< "$C/$id/input.txt" || fail "cannot read $C/$id/input.txt"
  exec 4> "$C/$id/transcript.txt" || fail "cannot write $C/$id/transcript.txt"
  exec 5> "$W/run1/$id.stderr" || fail "cannot write $W/run1/$id.stderr"
  docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" <&3 >&4 2>&5 3<&- 4>&- 5>&-
  rc=$?
  exec 3<&- 4>&- 5>&-
  RC1+=("$rc")
  printf '%s\t%s\n' "$id" "$rc" >> "$W/run1/status.tsv" || fail "cannot write $W/run1/status.tsv"
  echo "run 1: $n of $COUNT $id exit $rc"
done

step "6. Export after"
run_bounded "$DEADLINE_BULK" "the scope export after run 1" export_scope "$W/export-after.zwr" "$W/export-after.err"
RC=$?
check_deadline
check_export export-after "$RC"
# cmp exits 0 for equal files and 1 for different ones, the only difference collected; any other
# status is a failed comparison (a file it could not read), which is fatal.
cmp -s "$W/export-before.zwr" "$W/export-after.zwr"
cmp_rc=$?
case "$cmp_rc" in
  0)
    EXPORTS=IDENTICAL
    echo "export after equals export before: the inquiries changed no data"
    ;;
  1)
    EXPORTS=DIFFERENT
    printf 'DIFF\t%s\n' export >> "$W/diffs.tsv" || fail "cannot write $W/diffs.tsv"
    echo "export after DIFFERS from export before. First lines of the difference:"
    diff "$W/export-before.zwr" "$W/export-after.zwr" | sed -n 1,20p
    ;;
  *) fail "cmp could not compare $W/export-before.zwr with $W/export-after.zwr (status $cmp_rc)";;
esac
echo "exports=$EXPORTS"

step "7. Run 2 on a fresh container"
run_bounded "$DEADLINE_CONTAINER" "the restore of container $NAME from $CORPUS_SNAP (docker rm -f, then docker run -d)" \
  restore_container "$CORPUS_SNAP" || { check_deadline; fail "docker run failed for $CORPUS_SNAP"; }
probe_ready || fail "container not ready after 600s (shell-only readiness probe, SPEC.md O-16; check: docker logs $NAME)"
echo "container $NAME ready; start command as in step 3"
: > "$W/run2/status.tsv" || fail "cannot write $W/run2/status.tsv"
n=0
RC2=()
for id in "${IDS[@]}"; do
  n=$((n + 1))
  mkdir -p "$W/run2/$id" || fail "cannot create $W/run2/$id"
  exec 3< "$C/$id/input.txt" || fail "cannot read $C/$id/input.txt"
  exec 4> "$W/run2/$id/transcript.txt" || fail "cannot write $W/run2/$id/transcript.txt"
  exec 5> "$W/run2/$id.stderr" || fail "cannot write $W/run2/$id.stderr"
  docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" <&3 >&4 2>&5 3<&- 4>&- 5>&-
  rc=$?
  exec 3<&- 4>&- 5>&-
  RC2+=("$rc")
  printf '%s\t%s\n' "$id" "$rc" >> "$W/run2/status.tsv" || fail "cannot write $W/run2/status.tsv"
  echo "run 2: $n of $COUNT $id exit $rc"
done
RUNS=IDENTICAL
for id in "${IDS[@]}"; do
  # As in step 6: status 1 is a collected difference, any status above 1 a fatal failed comparison.
  cmp -s "$C/$id/transcript.txt" "$W/run2/$id/transcript.txt"
  cmp_rc=$?
  case "$cmp_rc" in
    0) ;;
    1)
      RUNS=DIFFERENT
      printf 'DIFF\t%s\n' "$id" >> "$W/diffs.tsv" || fail "cannot write $W/diffs.tsv"
      echo "$id: run 1 and run 2 DIFFER. First lines of the difference:"
      diff "$C/$id/transcript.txt" "$W/run2/$id/transcript.txt" | sed -n 1,20p
      ;;
    *) fail "cmp could not compare $C/$id/transcript.txt with $W/run2/$id/transcript.txt (status $cmp_rc)";;
  esac
done
echo "runs=$RUNS"

step "8. Paging detector"
# A paging pause prints ENTER '^' TO HALT: on its own, and END OF DISPLAY--PRESS RETURN OR ENTER
# '^' TO HALT: contains that text too, so a found order with no paging gives a = 2 and b = 1. Only
# an extra pause makes a exceed 2b. BOUNDARY cases are exempt: CE-25 answers the first pause with
# ^ and legitimately never reaches the second. A quarantine classifies, and never fails, the run.
HALT_PROMPT="ENTER '^' TO HALT: "
END_PROMPT="END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: "
# count_prompt <literal> <file>: set PROMPTS to the number of times <literal> occurs in <file>.
# grep exits 1 when nothing matches, a valid count of 0. A grep that cannot read the file, a count
# that cannot be written or read back, or one that is not a number, is fatal.
count_prompt() {
  LC_ALL=C grep -a -o -F "$1" "$2" | wc -l > "$W/paging.count"
  PIPE_ST=("${PIPESTATUS[@]}")
  [ "${PIPE_ST[1]}" -eq 0 ] || fail "cannot count the prompts of $2 into $W/paging.count"
  [ "${PIPE_ST[0]}" -le 1 ] || fail "grep could not read $2 (status ${PIPE_ST[0]})"
  read -r PROMPTS < "$W/paging.count" || fail "cannot read $W/paging.count"
  case "$PROMPTS" in ''|*[!0123456789]*) fail "$W/paging.count holds no count for $2: $PROMPTS";; esac
}
QUARANTINED_IDS=" "
QUARANTINED=0
i=0
while [ "$i" -lt "$COUNT" ]; do
  id="${IDS[$i]}"
  class="${CLASSES[$i]}"
  i=$((i + 1))
  case "$class" in
    IN|CP|CR)
      f="$C/$id/transcript.txt"
      count_prompt "$HALT_PROMPT" "$f"
      a="$PROMPTS"
      count_prompt "$END_PROMPT" "$f"
      b="$PROMPTS"
      if [ "$a" -gt "$((2 * b))" ]; then
        QUARANTINED_IDS="$QUARANTINED_IDS$id "
        QUARANTINED=$((QUARANTINED + 1))
        echo "QUARANTINED: $id ($a pauses for $b end-of-display prompts)"
      fi
      ;;
  esac
done
echo "quarantined: $QUARANTINED of $COUNT"


step "9. run-summary.txt"
# The format src/golden.rs reads: the three identity lines, one tab-separated case line per case in
# manifest order (id, status, run 1 exit status, run 2 exit status; the statuses are evidence for
# people, and the reader reads only the first two fields), then one DIFF line per difference.
{
  printf 'exports=%s\n' "$EXPORTS"
  printf 'export_sha256=%s\n' "$BEFORE_SHA"
  printf 'runs=%s\n' "$RUNS"
  i=0
  while [ "$i" -lt "$COUNT" ]; do
    id="${IDS[$i]}"
    case "$QUARANTINED_IDS" in *" $id "*) status=QUARANTINED;; *) status=RECORDED;; esac
    printf '%s\t%s\t%s\t%s\n' "$id" "$status" "${RC1[$i]}" "${RC2[$i]}"
    i=$((i + 1))
  done
  cat "$W/diffs.tsv"
} > "$C/run-summary.txt" || fail "cannot write $C/run-summary.txt"
echo "wrote $C/run-summary.txt"

step "10. meta.txt"
# The keys of golden::CORPUS_META_KEYS, each once and in that order. The context is the constant
# SPEC.md O-01 answer, because a recording run may run no probe. The start command is the one
# every case ran, with the resolved container name.
{
  printf 'image_digest=%s\n' "$DIGEST"
  printf 'snapshot_id=%s\n' "$SNAP_ID"
  printf 'source_revision=%s\n' "$SOURCE_REVISION"
  printf '%s\n' 'context={"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}'
  printf 'start_command=docker exec -i %s su - vehu -c "%s"\n' "$NAME" "$INQUIRY_CMD"
} > "$C/meta.txt" || fail "cannot write $C/meta.txt"
echo "wrote $C/meta.txt"

step "11. SHA256SUMS"
# Written last, over every file the run binds: BASE.txt, meta.txt, run-summary.txt and each case's
# input.txt and transcript.txt, as paths relative to the corpus folder, sorted bytewise. Text mode
# gives <64 lower-case hex digits><two spaces><path>, the form the corpus reader accepts.
{
  printf '%s\n' BASE.txt meta.txt run-summary.txt
  for id in "${IDS[@]}"; do
    printf '%s\n' "$id/input.txt" "$id/transcript.txt"
  done
} > "$W/sums.list" || fail "cannot write $W/sums.list"
LC_ALL=C sort "$W/sums.list" > "$W/sums.sorted" || fail "cannot sort $W/sums.list"
rm -f "$W/SHA256SUMS.new" || fail "cannot remove $W/SHA256SUMS.new"
# This loop reads a file, not standard input, and runs no docker command.
(
  cd "$C" || exit 1
  files=()
  while IFS= read -r f; do files+=("$f"); done < "$W_ABS/sums.sorted"
  shasum -a 256 "${files[@]}" > "$W_ABS/SHA256SUMS.new"
) || fail "shasum -a 256 failed in $C"
LISTED="$(wc -l < "$W/sums.sorted")" || fail "cannot count the lines of $W/sums.sorted"
LISTED="${LISTED// /}"
SUMS="$(wc -l < "$W/SHA256SUMS.new")" || fail "cannot count the lines of $W/SHA256SUMS.new"
SUMS="${SUMS// /}"
case "$LISTED" in ''|*[!0123456789]*) fail "wc -l gave no line count for $W/sums.sorted: $LISTED";; esac
case "$SUMS" in ''|*[!0123456789]*) fail "wc -l gave no line count for $W/SHA256SUMS.new: $SUMS";; esac
[ "$SUMS" = "$LISTED" ] || fail "$W/SHA256SUMS.new has $SUMS lines for $LISTED files"
BAD_SUMS="$(LC_ALL=C grep -cvE '^[0-9a-f]{64}  [A-Za-z0-9._/-]+$' "$W/SHA256SUMS.new")"
[ "$BAD_SUMS" = "0" ] || fail "$W/SHA256SUMS.new has $BAD_SUMS lines that are not <sha256><two spaces><path>"
cat "$W/SHA256SUMS.new" > "$C/SHA256SUMS" || { rm -f "$C/SHA256SUMS"; fail "cannot write $C/SHA256SUMS"; }
echo "wrote $C/SHA256SUMS ($SUMS files)"

step "12. Result"
# A removal that fails is fatal, as every failed docker command is, but the recording files are
# complete by now and stay as written: the container is the only thing left behind.
: > "$W/final-remove.err" \
  || fail "cannot write $W/final-remove.err, so container $NAME was not removed: remove it by hand. The recording files in $C (transcripts, run-summary.txt, meta.txt, SHA256SUMS) are already written and kept."
if ! run_bounded "$DEADLINE_CONTAINER" "docker rm -f $NAME" docker rm -f "$NAME" >/dev/null 2> "$W/final-remove.err"; then
  sed -n 1,10p "$W/final-remove.err"
  [ -z "$BOUNDED_EXPIRED" ] \
    || fail "$BOUNDED_EXPIRED, so container $NAME may remain: remove it by hand. The recording files in $C (transcripts, run-summary.txt, meta.txt, SHA256SUMS) are already written and kept."
  fail "docker rm -f $NAME failed, so container $NAME remains: remove it by hand. The recording files in $C (transcripts, run-summary.txt, meta.txt, SHA256SUMS) are already written and kept."
fi
NONZERO=0
for rc in "${RC1[@]}"; do
  [ "$rc" -eq 0 ] || NONZERO=$((NONZERO + 1))
done
EMPTY=""
for id in "${IDS[@]}"; do
  [ -s "$C/$id/transcript.txt" ] || EMPTY="$EMPTY $id"
done
echo "exports=$EXPORTS"
echo "runs=$RUNS"
echo "quarantined: $QUARANTINED of $COUNT"
echo "run 1 cases with a non-zero exit status (evidence only): $NONZERO"
[ -z "$EMPTY" ] || echo "empty transcripts, which check_corpus.sh and the corpus reader reject:$EMPTY"
if [ -s "$W/diffs.tsv" ]; then
  echo "differences:"
  cat "$W/diffs.tsv"
fi
echo
if [ "$EXPORTS" = IDENTICAL ] && [ "$RUNS" = IDENTICAL ]; then
  echo "RESULT: RECORDED $COUNT CASES. Next: tests/golden/ifcap/corpus/scripts/check_corpus.sh, then cargo test --offline --locked"
  exit 0
fi
echo "RESULT: DIFFERENCES FOUND"
exit 1
