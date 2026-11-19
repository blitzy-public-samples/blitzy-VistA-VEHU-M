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
#          tests/golden/ifcap/corpus/BASE.txt, and scratch files under captures-work/corpus/:
#          base-export.zwr and base-export.err, and every other intermediate file in a private
#          folder load.XXXXXX that mktemp -d makes there, removed at the end of a run that loads
#          and kept after a FAIL. Before it copies anything or runs any Docker command, it stops
#          with a FAIL when a file it reads or writes, or a folder above that file inside the crate
#          folder, is a symbolic link or of the wrong kind: each folder must be a folder, a file it
#          reads a regular file, and a file it writes absent or a regular file. BASE.txt is
#          checked again just before it is written.
# Never writes: the original image, vehu-snap:gc-base, or any committed file other than BASE.txt.
#
# One run at a time: a run holds the lock folder captures-work/corpus/load.lock until it ends,
# also on a FAIL or a signal; a signal that comes while the run takes the lock is acted on once
# the run holds it. A run that cannot remove the folder at its end reports so, and a run that
# loaded then ends with FAIL rather than RESULT: LOADED. A run killed outright (kill -9) leaves
# the folder behind; remove it by hand once no load_corpus.sh is running.
#
# Deadlines: every Docker operation runs under a wall-clock deadline, checked once a second, and
# one still running at its deadline is stopped with every process it started (TERM, then KILL
# after a 3-s grace) and ends the run with a FAIL: line naming the operation and its deadline.
# Nothing is retried. The deadlines are 120 s for each docker image inspect; 300 s for each
# container restore (docker rm -f, then docker run -d), for docker stop and for the final
# docker rm -f; 600 s for each readiness wait (vc_wait_ready, whose own count leaves out the time
# its docker exec takes); and 1800 s each for the presence check, the load, the read back,
# docker commit and the base export. A FAIL or a signal stops an operation still running the
# same way, within about 5 s, before the lock is removed. Each docker ps -a of the container
# checks (see NAME below) has the 120 s of a docker image inspect.
#
# Environment (each optional):
#   NAME=vehu                                                    container name (a letter or digit, then
#                                                                at least one letter, digit, _, . or -)
#   BASE_IMAGE=worldvista/vehu:202609                            the original image (printed as context)
#   SOURCE_REVISION=dd3fd0f3124c8178657d9fff4173c324da30f7ae     source revision (printed as context)
# NAME is destructive: the run force-removes container NAME, replaces it with its own, and later
# loads into, stops, commits, replaces and removes that one. Before the first removal it stops
# with a FAIL unless no container has that name, or the one that has it was started from
# vehu-snap:gc-base or vehu-snap:corpus-base (or an image made from one), as docker ps -a
# --filter ancestor= shows. It keeps the id of each container it starts, and stops with a FAIL
# rather than act on NAME once NAME no longer names that container.
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

# Path checks, the same text in load_corpus.sh and record_corpus.sh. Each path this script reads
# or writes, and each folder on the way to it from the crate folder (where the cd above lands)
# down, is checked before anything is read or written through it, so that a symbolic link or an
# entry of the wrong kind left there cannot send a read or a write outside the crate: it stops
# the run with a FAIL naming it. [ -d ] and [ -f ] follow a symbolic link, so [ -L ] is asked
# first and no link is ever followed.

# path_kind <path>: set KIND to what <path> is, never following a symbolic link: link (a symbolic
# link, dangling or not), folder, file (a regular file), other (a FIFO, device or socket) or absent.
path_kind() {
  if [ -L "$1" ]; then
    KIND=link
  elif [ -d "$1" ]; then
    KIND=folder
  elif [ -f "$1" ]; then
    KIND=file
  elif [ -e "$1" ]; then
    KIND=other
  else
    KIND=absent
  fi
}

# require_folder <path> <remedy>: stop with a FAIL unless <path> is a folder that is not a
# symbolic link. A missing <path> stops with "<path> not found: <remedy>", or passes when
# <remedy> is empty, for a folder this run makes. Returns 0.
require_folder() {
  path_kind "$1"
  case "$KIND" in
    folder) ;;
    link) fail "$1 is a symbolic link, which this script never follows: replace it with a real folder";;
    absent) [ -z "$2" ] || fail "$1 not found: $2";;
    *) fail "$1 is not a folder: move it away";;
  esac
  return 0
}

# require_folders <path> <remedy>: require_folder <folder> <remedy> on each folder of the
# relative <path>, from its first part down to <path> itself, so that no folder is looked into
# before the one that holds it has passed. Returns 0.
require_folders() {
  local rest="$1" folder=""
  while [ -n "$rest" ]; do
    folder="${folder:+$folder/}${rest%%/*}"
    case "$rest" in
      */*) rest="${rest#*/}";;
      *) rest="";;
    esac
    require_folder "$folder" "$2"
  done
}

# require_input <path> <remedy>: stop with a FAIL unless <path> is a regular file that is not a
# symbolic link, each FAIL ending with <remedy>. Returns 0.
require_input() {
  path_kind "$1"
  case "$KIND" in
    file) ;;
    link) fail "$1 is a symbolic link, which this script never reads through: remove it, then $2";;
    absent) fail "$1 not found: $2";;
    *) fail "$1 is not a regular file: move it away, then $2";;
  esac
  return 0
}

# require_output <path>: stop with a FAIL unless <path> is absent or a regular file that is not a
# symbolic link, so that writing it creates or replaces that one file. A symbolic link, dangling
# or not, a folder, a FIFO, a device or a socket stops the run. Returns 0.
require_output() {
  path_kind "$1"
  case "$KIND" in
    absent | file) ;;
    link) fail "$1 is a symbolic link, which this script never writes through: remove it";;
    *) fail "$1 is not a regular file, so it is not written: move it away";;
  esac
  return 0
}
# End of the path checks.

# captures-work and captures-work/corpus may be missing, and mkdir -p makes them, but neither may
# be a symbolic link or anything but a folder, before or after.
require_folders "$W" ""
mkdir -p "$W" || fail "cannot create $W"
require_folders "$W" "mkdir -p $W made no folder"

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
# on_exit: with signals ignored, stop a Docker operation still running (run_bounded's JOB_PID)
# with stop_owned (TERM, then KILL after a 3-s grace, at most 5 s), reap it once it has ended and
# say so if it has not, then remove the lock, and say once if it cannot be removed. It leaves
# the exit status as it is. It is the EXIT trap, and on_signal runs it before fail: a signal that
# comes as the run exits is handled inside the EXIT trap, where the exit in fail would end the
# run before the EXIT trap removed the lock.
on_exit() {
  trap '' HUP INT TERM
  if [ -n "${JOB_PID:-}" ]; then
    if stop_owned "-$JOB_PID"; then
      { wait "$JOB_PID"; } 2>/dev/null
    else
      echo "the Docker operation of process group $JOB_PID did not end after KILL and is left behind"
    fi
    JOB_PID=""
  fi
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
    # At most 21 characters of the holder's pid file, and only from a regular file, so that
    # neither a large file nor a FIFO holds this run up. Anything but 1 to 20 digits is unknown.
    HOLDER=
    [ -f "$LOCK/pid" ] && { IFS= read -r -n 21 HOLDER < "$LOCK/pid"; } 2>/dev/null
    case "$HOLDER" in
      *[!0123456789]*) HOLDER=;;
    esac
    [ "${#HOLDER}" -le 20 ] || HOLDER=
    fail "another run holds the lock $LOCK (pid ${HOLDER:-unknown}); if no other load_corpus.sh is running, remove that folder and start again"
  fi
  fail "cannot create the lock $LOCK"
fi
echo "$$" > "$LOCK/pid" || fail "cannot write $LOCK/pid"

# P: this run's private scratch folder, made once the lock is held by mktemp -d, which gives a
# new folder of mode 0700 under a fresh name, inside the checked W. Every intermediate file goes
# there, so that none an earlier run, or anyone else, left in W is written through or read. The
# base export and its standard error alone stay in W, as captures-work/corpus/base-export.zwr
# and base-export.err. A FAIL or a signal keeps P, since a FAIL line may name a file in it; a
# run that loads removes it before it reports RESULT, and a folder it cannot remove is a FAIL.
P="$(mktemp -d "$W/load.XXXXXX")" || fail "cannot create a private scratch folder in $W (mktemp -d)"
case "$P" in
  "$W"/load.?*) ;;
  *) fail "mktemp -d printed no folder name in $W: $P";;
esac
require_folder "$P" "mktemp -d printed it but made no folder"

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
    if [ "$((SECONDS - start))" -ge "$window" ]; then
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

# Container checks, the same text in load_corpus.sh and record_corpus.sh. restore_container
# force-removes whatever container NAME names, and later steps act on it again, so NAME alone
# never makes a container this script's to remove. Before the first restore, NAME may name no
# container, or one started from vehu-snap:gc-base or vehu-snap:corpus-base, the snapshots the
# corpus scripts and the kit's capture scripts start their disposable containers from; any
# other container stops the run with a FAIL and is left as it is. After each restore the id of
# the container now named NAME is kept in OWN_ID, and each later step that changes or removes
# that container first checks that NAME still names it. Docker is asked by docker ps -a alone,
# which lists stopped containers too and runs no M command, each call under the deadline of an
# inspect; its output goes to files in P and is read only after its exit status is checked, so
# a docker ps that fails or is stopped never passes for no container. These are checks, not
# locks: a container that takes NAME between a check and the step after it is not caught.
# FOUND_ID: the container the last container_lookup found, empty for none. OWN_ID: the container
# this run started last, empty before its first restore. IMAGE_ID: set by optional_image_id.
FOUND_ID=""
OWN_ID=""
IMAGE_ID=""

# is_id_hex <text>: true when <text> is 64 lower-case hexadecimal digits, the form of a full
# container id, and of an image id after its sha256: prefix.
is_id_hex() {
  case "$1" in *[!0123456789abcdef]*) return 1;; esac
  [ "${#1}" -eq 64 ]
}

# container_lookup [<note>]: set FOUND_ID to the full id of the container named exactly NAME,
# or to empty when no container has that name. docker ps -a --filter name= takes NAME as an
# unanchored regular expression, in which a . matches any character, so it may list other names
# too: the shell keeps only a line whose comma-separated names include NAME itself. Every line
# must be a 64-digit id, one space and the names, at most 4095 characters, ended by a line feed.
# A docker ps that fails or is stopped, a line of any other form, or two containers named NAME,
# stops the run with a FAIL ending with <note>.
container_lookup() {
  local note="${1:-}" rc line="" id names matches=0
  FOUND_ID=""
  run_bounded "$DEADLINE_INSPECT" "docker ps -a for container $NAME" \
    docker ps -a --no-trunc --filter "name=$NAME" --format '{{.ID}} {{.Names}}' > "$P/container-list.out" 2> "$P/container-list.err"
  rc=$?
  [ -z "$BOUNDED_EXPIRED" ] || fail "$BOUNDED_EXPIRED$note"
  [ "$rc" -eq 0 ] || fail "cannot list the containers named $NAME (docker ps -a exited with status $rc; see $P/container-list.err)$note"
  exec 7< "$P/container-list.out" || fail "cannot read $P/container-list.out$note"
  while IFS= read -r -n 4096 -u 7 line; do
    [ "${#line}" -lt 4096 ] || fail "docker ps -a printed a line longer than 4095 characters; see $P/container-list.out$note"
    id="${line%% *}"
    names="${line#* }"
    if ! is_id_hex "$id" || [ "$names" = "$line" ] || [ -z "$names" ]; then
      fail "docker ps -a printed a line that is not a 64-digit container id, one space and its names; see $P/container-list.out$note"
    fi
    case ",$names," in
      *",$NAME,"*)
        matches=$((matches + 1))
        FOUND_ID="$id";;
    esac
  done
  exec 7<&-
  [ -z "$line" ] || fail "docker ps -a printed a last line with no line feed; see $P/container-list.out$note"
  [ "$matches" -le 1 ] || fail "docker ps -a lists $matches containers named $NAME, where Docker allows one; see $P/container-list.out$note"
  return 0
}

# container_from <id> <image id>: 0 when the container <id> was started from the image <image
# id>, 1 when it was not. docker ps -a --filter ancestor= counts the image and every image made
# from it, and matches nothing for an image that does not exist; filters of two keys must both
# match, so the output is <id> or nothing. A docker ps that fails or is stopped, or that prints
# anything else, stops the run with a FAIL.
container_from() {
  local rc
  run_bounded "$DEADLINE_INSPECT" "docker ps -a for container $1 and image $2" \
    docker ps -a --no-trunc --filter "id=$1" --filter "ancestor=$2" --format '{{.ID}}' > "$P/container-from.out" 2> "$P/container-from.err"
  rc=$?
  check_deadline
  [ "$rc" -eq 0 ] || fail "cannot check the image of container $1 (docker ps -a exited with status $rc; see $P/container-from.err)"
  [ -s "$P/container-from.out" ] || return 1
  read_one_line "$P/container-from.out" && [ "$LINE" = "$1" ] && return 0
  fail "docker ps -a for container $1 and image $2 printed something other than that id or nothing; see $P/container-from.out"
}

# container_preflight <image id>...: before the first restore, stop with a FAIL unless no
# container is named NAME, or the one named NAME was started from one of the <image id>s, the
# ids of vehu-snap:gc-base and vehu-snap:corpus-base this script could read; an empty one is
# skipped. Only such a container is a disposable copy that restore_container may remove.
container_preflight() {
  local image
  container_lookup
  if [ -z "$FOUND_ID" ]; then
    echo "container $NAME: no container has that name yet"
    return 0
  fi
  for image in "$@"; do
    [ -n "$image" ] || continue
    if container_from "$FOUND_ID" "$image"; then
      echo "container $NAME: $FOUND_ID, started from $image, which the restore replaces"
      return 0
    fi
  done
  fail "container $NAME exists (id $FOUND_ID) and was not started from vehu-snap:gc-base or vehu-snap:corpus-base, so this script will not remove it: if it is disposable, remove it by hand (docker rm -f $NAME), or set NAME to a name no container has"
}

# container_started <image id> <image>: straight after a restore from <image>, whose id is
# <image id>, stop with a FAIL unless a container is named NAME and was started from <image
# id>, then keep its id in OWN_ID and print it. restore_container sends the new id that docker
# run prints to /dev/null, so it is read back this way.
container_started() {
  container_lookup
  [ -n "$FOUND_ID" ] || fail "no container is named $NAME after the restore from $2"
  container_from "$FOUND_ID" "$1" \
    || fail "container $NAME (id $FOUND_ID) was not started from $2 ($1), so it is not the container the restore started"
  OWN_ID="$FOUND_ID"
  echo "container $NAME: id $OWN_ID, started from $2"
}

# require_own_container <operation> [<note>]: before <operation> changes or removes container
# NAME, stop with a FAIL ending with <note> unless NAME still names OWN_ID, the container this
# run started last.
require_own_container() {
  local note="${2:-}"
  container_lookup "$note"
  [ -n "$OWN_ID" ] && [ "$FOUND_ID" = "$OWN_ID" ] \
    || fail "container $NAME is no longer the container this run started (id ${OWN_ID:-none}; $NAME now names ${FOUND_ID:-no container}): refusing to $1$note"
}

# optional_image_id <image> <file name>: set IMAGE_ID to the id of <image>, sha256: and 64
# hexadecimal digits, which docker image inspect writes to P/<file name>.out, read after its
# exit status is checked; or to empty when the inspect fails or prints no such id. The image need
# not exist: a missing id only leaves its containers out of those container_preflight allows. An
# inspect still running at its deadline stops the run with a FAIL.
optional_image_id() {
  local rc
  IMAGE_ID=""
  run_bounded "$DEADLINE_INSPECT" "docker image inspect $1" \
    docker image inspect --format '{{.Id}}' "$1" > "$P/$2.out" 2> "$P/$2.err"
  rc=$?
  check_deadline
  [ "$rc" -eq 0 ] && read_one_line "$P/$2.out" || return 0
  case "$LINE" in
    sha256:*) is_id_hex "${LINE#sha256:}" && IMAGE_ID="$LINE";;
  esac
  return 0
}
# End of the container checks.


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

# PREVIEW_BYTES: the bytes a FAIL shows of each M error line or corpus line it previews. A
# preview streams from its file, which stays whole on disk, so the shell holds none of the file,
# however large it is or however long its lines. 300 bytes show a whole corpus.zwr line and a
# usual M error message.
PREVIEW_BYTES=300

# no_m_errors <message> <file>...: stop with <message> when a <file> holds %YDB-E or %GTM-E,
# after the first 10 matching lines, each cut to its first PREVIEW_BYTES bytes. grep -q exits 0
# for a match, 1 for none and 2 or more when it cannot read a file. The files are only read. The
# preview's grep is quiet on errors, which the grep -q before it has already reported.
no_m_errors() {
  local message="$1" rc
  shift
  LC_ALL=C grep -qE '%YDB-E|%GTM-E' "$@"
  rc=$?
  case "$rc" in
    0)
      LC_ALL=C grep -hE '%YDB-E|%GTM-E' "$@" 2>/dev/null | LC_ALL=C cut -b "1-$PREVIEW_BYTES" | sed 10q
      fail "$message";;
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
# The path checks, before anything is copied and before any Docker command: corpus.zwr, the
# file read here, and the folders above it; the corpus folder and BASE.txt, written in step 8;
# and the base export and its standard error, written in step 7.
require_folders "${ZWR%/*}" "run cargo run --offline --locked --bin corpus_gen first"
require_input "$ZWR" "run cargo run --offline --locked --bin corpus_gen first"
require_folders "$C" "run cargo run --offline --locked --bin corpus_gen first"
require_output "$C/BASE.txt"
require_output "$W/base-export.zwr"
require_output "$W/base-export.err"
# Only the data lines: this skips the two header lines of corpus.zwr. The copy must succeed and
# hold every data line, so that a stale or partial copy from an earlier run is never loaded.
count_data_lines "$ZWR"
ZWR_NODES="$COUNT"
[ "$ZWR_NODES" -gt 0 ] || fail "$ZWR holds no data line"
grep '^\^' "$ZWR" > "$P/corpus-data.zwr"
RC=$?
[ "$RC" -eq 0 ] || fail "cannot copy the data lines of $ZWR to $P/corpus-data.zwr (grep status $RC)"
count_data_lines "$P/corpus-data.zwr"
NODES="$COUNT"
[ "$NODES" = "$ZWR_NODES" ] || fail "$P/corpus-data.zwr holds $NODES data lines, but $ZWR holds $ZWR_NODES"
# The snapshot ids say which container named NAME step 2 may replace (container_preflight).
# Each is read from a file, after the exit status is checked, so a failed or stopped inspect
# never passes its output on, and the file must hold exactly one line. vehu-snap:gc-base must
# exist; vehu-snap:corpus-base, made by an earlier run, need not.
run_bounded "$DEADLINE_INSPECT" "docker image inspect $GC_SNAP" \
  docker image inspect --format '{{.Id}}' "$GC_SNAP" > "$P/gc-base-id.out" 2> "$P/gc-base-id.err" \
  || { check_deadline; fail "image $GC_SNAP not found: make it with scripts/kit/load_orders.sh and scripts/kit/save_state.sh"; }
read_one_line "$P/gc-base-id.out" \
  || fail "docker image inspect did not print the id of $GC_SNAP as one line of at most 4095 characters; see $P/gc-base-id.out"
GC_ID="$LINE"
case "$GC_ID" in
  sha256:*) is_hex64 "${GC_ID#sha256:}";;
  *) false;;
esac || fail "the id of $GC_SNAP is not sha256: followed by 64 hexadecimal digits: $GC_ID"
optional_image_id "$CORPUS_SNAP" corpus-base-id
CORPUS_BASE_ID="$IMAGE_ID"
echo "container:       $NAME"
echo "base image:      $BASE_IMAGE"
echo "source revision: $SOURCE_REVISION"
echo "corpus nodes:    $NODES (from $ZWR)"
container_preflight "$GC_ID" "$CORPUS_BASE_ID"

step "2. Throwaway container from $GC_SNAP"
run_bounded "$DEADLINE_CONTAINER" "the restore of container $NAME from $GC_SNAP (docker rm -f, then docker run -d)" \
  restore_container "$GC_SNAP" || { check_deadline; fail "docker run failed"; }
container_started "$GC_ID" "$GC_SNAP"
# The kit's wait counts only its own pauses, not the time its docker exec takes, so run_bounded
# holds it to its 600 s of wall-clock time.
run_bounded "$DEADLINE_READY" "vc_wait_ready $NAME 600" vc_wait_ready "$NAME" 600
RC=$?
[ -z "$BOUNDED_EXPIRED" ] || fail "container not ready within $DEADLINE_READY s: $BOUNDED_EXPIRED"
[ "$RC" -eq 0 ] || fail "container not ready within $DEADLINE_READY s: vc_wait_ready $NAME 600 gave up (status $RC)"
echo "container $NAME ready"

step "3. Corpus orders must be absent"
# presence_check: the one $D loop over IENs 991001 to 991200, into present.out and present.err.
# A FOR scopes to the end of the line, so the IF lets the WRITE run once, on the last IEN.
presence_check() {
  docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'S N=0 F I=991001:1:991200 S:\$D(^PRC(442,I)) N=N+1 I I=991200 W \"CORPUS_PRESENT=\",N,!'" < /dev/null > "$P/present.out" 2> "$P/present.err"
}
run_bounded "$DEADLINE_BULK" "the presence check of IENs 991001 to 991200" presence_check
RC=$?
check_deadline
no_m_errors "the presence check printed an M error" "$P/present.out" "$P/present.err"
[ "$RC" -eq 0 ] || fail "the presence check exited with status $RC; see $P/present.out and $P/present.err"
grep -q '^CORPUS_PRESENT=' "$P/present.out" || fail "the presence check printed no CORPUS_PRESENT= line; see $P/present.out"
PRESENT="$(grep '^CORPUS_PRESENT=' "$P/present.out" | cut -d= -f2 | tr -d '[:space:]')"
echo "corpus orders present in 991001 to 991200: $PRESENT"
[ "$PRESENT" = "0" ] || fail "corpus orders already present: start again from $GC_SNAP"

step "4. Load $NODES corpus nodes"
# load_nodes <status file>: the load_orders.sh method, into load.out. It writes the pipeline's two
# exit statuses to <status file>, because run_bounded runs it in a subshell whose variables do not
# come back. && makes a failed sed the status of the group, and then no halt is sent.
load_nodes() {
  { sed 's/^/set /' "$P/corpus-data.zwr" && echo "halt"; } | docker exec -i "$NAME" su - vehu -c 'mumps -dir' > "$P/load.out" 2>&1
  printf '%s\n' "${PIPESTATUS[*]}" > "$1"
}
rm -f "$P/load.status" || fail "cannot remove $P/load.status"
require_own_container "load the corpus into $NAME"
run_bounded "$DEADLINE_BULK" "the load of $NODES corpus nodes" load_nodes "$P/load.status"
RC=$?
check_deadline
[ "$RC" -eq 0 ] || fail "cannot write the load pipeline's exit statuses to $P/load.status (status $RC)"
read_one_line "$P/load.status" || fail "cannot read the load pipeline's exit statuses from $P/load.status"
LOAD_STATUS="$LINE"
no_m_errors "load had errors" "$P/load.out"
[ "$LOAD_STATUS" = "0 0" ] || fail "load pipeline exited with status $LOAD_STATUS; see $P/load.out"
echo "load finished with no M errors"

step "5. Read back"
# read_back: every node of ^PRC(442), into readback.zwr and readback.err.
read_back() {
  docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'zwrite ^PRC(442,*)'" < /dev/null > "$P/readback.zwr" 2> "$P/readback.err"
}
run_bounded "$DEADLINE_BULK" "the read back of ^PRC(442)" read_back
RC=$?
check_deadline
no_m_errors "the read back printed an M error" "$P/readback.zwr" "$P/readback.err"
[ "$RC" -eq 0 ] || fail "the read back exited with status $RC; see $P/readback.err"
# Every loaded line that did not come back exactly goes to MISSING, which stays on disk whole,
# and a FAIL prints only its count and the first lines, each cut to PREVIEW_BYTES bytes. An
# empty read back makes every line missing. With -v, grep exits 0 when it writes such a line, 1
# when there is none, 2 or more on an error, a failed write included. A redirection that cannot
# open its file gives status 1 too, so the file is opened, and checked, before grep runs.
MISSING="$P/readback-missing.zwr"
exec 3> "$MISSING" || fail "cannot write $MISSING"
grep -Fxv -f "$P/readback.zwr" "$P/corpus-data.zwr" >&3
RC=$?
exec 3>&-
case "$RC" in
  0)
    # Every line of MISSING comes from corpus-data.zwr, so every one is a data line.
    count_data_lines "$MISSING"
    echo "$COUNT of $NODES loaded lines did not read back as loaded. First of them:"
    LC_ALL=C cut -b "1-$PREVIEW_BYTES" "$MISSING" | sed -e 's/^/  MISSING OR DIFFERENT: /' -e 10q
    echo "full list: $MISSING"
    echo "first lines read back:"
    LC_ALL=C cut -b "1-$PREVIEW_BYTES" "$P/readback.zwr" | sed 5q
    fail "the corpus did not read back byte for byte";;
  1) ;;
  *) fail "cannot compare $P/readback.zwr with $P/corpus-data.zwr (grep status $RC)";;
esac
echo "all $NODES loaded nodes read back identical"

step "6. Freeze as $CORPUS_SNAP"
require_own_container "stop $NAME"
run_bounded "$DEADLINE_CONTAINER" "docker stop $NAME" docker stop "$NAME" >/dev/null \
  || { check_deadline; fail "docker stop failed"; }
require_own_container "commit $NAME as $CORPUS_SNAP"
run_bounded "$DEADLINE_BULK" "docker commit $NAME $CORPUS_SNAP" docker commit "$NAME" "$CORPUS_SNAP" >/dev/null \
  || { check_deadline; fail "docker commit failed"; }
# The id is read from a file, after the exit status is checked, so a failed or stopped inspect
# never passes its output on, and the file must hold exactly one line.
run_bounded "$DEADLINE_INSPECT" "docker image inspect $CORPUS_SNAP" \
  docker image inspect --format '{{.Id}}' "$CORPUS_SNAP" > "$P/snapshot-id.out" 2> "$P/snapshot-id.err"
RC=$?
check_deadline
[ "$RC" -eq 0 ] || fail "cannot read the id of $CORPUS_SNAP (docker image inspect exited with status $RC); see $P/snapshot-id.err"
read_one_line "$P/snapshot-id.out" \
  || fail "docker image inspect did not print the id of $CORPUS_SNAP as one line of at most 4095 characters; see $P/snapshot-id.out"
SNAP_ID="$LINE"
case "$SNAP_ID" in
  sha256:*) is_hex64 "${SNAP_ID#sha256:}";;
  *) false;;
esac || fail "the id of $CORPUS_SNAP is not sha256: followed by 64 hexadecimal digits: $SNAP_ID"
echo "snapshot id: $SNAP_ID"

step "7. Base export from a fresh container"
require_own_container "replace $NAME with a fresh container from $CORPUS_SNAP"
run_bounded "$DEADLINE_CONTAINER" "the restore of container $NAME from $CORPUS_SNAP (docker rm -f, then docker run -d)" \
  restore_container "$CORPUS_SNAP" || { check_deadline; fail "could not start a container from $CORPUS_SNAP"; }
container_started "$SNAP_ID" "$CORPUS_SNAP"
run_bounded "$DEADLINE_READY" "vc_wait_ready $NAME 600" vc_wait_ready "$NAME" 600
RC=$?
[ -z "$BOUNDED_EXPIRED" ] || fail "container from $CORPUS_SNAP not ready within $DEADLINE_READY s: $BOUNDED_EXPIRED"
[ "$RC" -eq 0 ] || fail "container from $CORPUS_SNAP not ready within $DEADLINE_READY s: vc_wait_ready $NAME 600 gave up (status $RC)"
run_bounded "$DEADLINE_BULK" "the base export of the README Part B step 1 scope" \
  export_scope "$W/base-export.zwr" "$W/base-export.err"
RC=$?
check_deadline
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
# Checked again just before it is written, since the load and the export take a while.
require_output "$C/BASE.txt"
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
require_own_container "remove $NAME"
run_bounded "$DEADLINE_CONTAINER" "docker rm -f $NAME" docker rm -f "$NAME" >/dev/null
RC=$?
check_deadline
[ "$RC" -eq 0 ] || fail "cannot remove container $NAME (docker rm -f exited with status $RC)"
cat "$C/BASE.txt"
echo
rm -rf "$P" || fail "cannot remove the private scratch folder $P; remove that folder by hand"
release_lock || fail "cannot remove the lock $LOCK; remove that folder by hand"
echo "RESULT: LOADED. Next: tests/golden/ifcap/corpus/scripts/record_corpus.sh"
exit 0
