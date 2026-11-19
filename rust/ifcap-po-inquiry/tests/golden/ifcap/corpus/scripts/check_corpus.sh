#!/usr/bin/env bash
# check_corpus.sh: offline file checks of a parity-corpus recording, the shell counterpart of the
# integrity rules of golden::load_corpus_run in src/golden.rs. It checks what record_corpus.sh
# wrote under tests/golden/ifcap/corpus/: every case in manifest.tsv has its input.txt and a
# non-empty transcript.txt, no corpus file holds a carriage return, the run files meta.txt,
# run-summary.txt and SHA256SUMS exist, every SHA256SUMS line matches its file's bytes, and every
# transcript is listed in SHA256SUMS.
# It is offline: it uses no container and runs no M command.
#
# Usage (from anywhere; it changes to the crate folder rust/ifcap-po-inquiry/ itself):
#   bash tests/golden/ifcap/corpus/scripts/check_corpus.sh
#
# Prerequisites:
#   - a recording by tests/golden/ifcap/corpus/scripts/record_corpus.sh;
#   - for the whole sequence, the snapshot image vehu-snap:gc-base, which load_corpus.sh builds on;
#   - bash (macOS /bin/bash 3.2 or later) and shasum.
#
# The team's order:
#   1. tests/golden/ifcap/corpus/scripts/load_corpus.sh
#   2. tests/golden/ifcap/corpus/scripts/record_corpus.sh
#   3. tests/golden/ifcap/corpus/scripts/check_corpus.sh     (this script)
#   4. cargo test --offline --locked
#
# It reads files only; writes nothing. Every check runs and each problem is collected, never
# fatal: after the last check it prints one PROBLEM: line per problem, then the result line.
#
# Exit status: 0 with CORPUS CHECK PASS <n> CASES when no problem was found; 1 with
# CORPUS CHECK FAIL <k> PROBLEMS when any was, or with a FAIL: line when the script does not sit
# in the crate's corpus scripts folder.

set -u
# Manifest words are split into case ids but never expanded as file name patterns, so a malformed
# id is reported as written.
set -f

step() { echo; echo "== $*"; }
PROBLEMS=""
K=0
add_problem() { PROBLEMS="${PROBLEMS}PROBLEM: $*"$'\n'; K=$((K + 1)); }
# step_result: how many problems the current step added, counted from STEP_K.
STEP_K=0
step_result() { echo "problems in this step: $((K - STEP_K))"; }

cd "$(dirname "$0")/../../../../.." || exit 1
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || { echo "FAIL: run from the crate's corpus scripts folder"; exit 1; }

C=tests/golden/ifcap/corpus
MANIFEST="$C/manifest.tsv"

step "1. Manifest"
STEP_K=$K
# VALID holds the valid case ids, space separated, in manifest order; later steps walk it.
VALID=""
N=0
if [ ! -f "$MANIFEST" ]; then
  add_problem "$MANIFEST: missing"
else
  HEADER="$(sed -n 1p "$MANIFEST")"
  if [ "$(printf '%s\n' "$HEADER" | cut -f1)" != "case" ] || [ "$(printf '%s\n' "$HEADER" | cut -f2)" != "class" ]; then
    add_problem "$MANIFEST: line 1 is not the header case<TAB>class<TAB>..."
  fi
  IDS="$(sed 1d "$MANIFEST" | cut -f1)"
  for id in $IDS; do
    # Ids become paths, so only the three corpus series pass.
    case "$id" in
      CP-[0-9][0-9][0-9]|CE-[0-9][0-9]|CR-[0-9][0-9]) ;;
      *) add_problem "$MANIFEST: $id: not a corpus case id"; continue ;;
    esac
    # A repeated id is counted and checked once, so that N counts cases rather than rows.
    case "$VALID " in
      *" $id "*) add_problem "$MANIFEST: $id: listed more than once"; continue ;;
    esac
    VALID="$VALID $id"
    N=$((N + 1))
  done
  # A manifest with no case would otherwise pass with nothing checked.
  [ "$N" -gt 0 ] || add_problem "$MANIFEST: lists no case"
fi
echo "cases: $N (corpus_gen writes 231)"
step_result

step "2. Case files"
STEP_K=$K
for id in $VALID; do
  [ -f "$C/$id/input.txt" ] || add_problem "$C/$id/input.txt: missing"
  test -s "$C/$id/transcript.txt" || add_problem "$C/$id/transcript.txt: missing or empty"
done
step_result

step "3. Carriage returns"
STEP_K=$K
# A word list, not an array: no path here holds a space (C is a constant and every id passed the
# series check), and bash 3.2 cannot expand an empty array under set -u.
CR_FILES="$MANIFEST $C/BASE.txt $C/meta.txt $C/run-summary.txt $C/SHA256SUMS"
for id in $VALID; do
  CR_FILES="$CR_FILES $C/$id/input.txt $C/$id/transcript.txt"
done
for f in $CR_FILES; do
  [ -f "$f" ] || continue
  LC_ALL=C grep -l $'\r' "$f" >/dev/null 2>&1
  case "$?" in
    0) add_problem "$f: carriage return (a pseudo-terminal was used?)" ;;
    1) ;;
    *) add_problem "$f: cannot be read" ;;
  esac
done
step_result

step "4. Run files"
STEP_K=$K
for name in meta.txt run-summary.txt SHA256SUMS; do
  [ -f "$C/$name" ] || add_problem "$C/$name: missing"
done
step_result

step "5. SHA256SUMS verification"
STEP_K=$K
if [ -f "$C/SHA256SUMS" ]; then
  out="$(cd "$C" && shasum -a 256 -c SHA256SUMS 2>/dev/null)"
  rc=$?
  ok=0
  added=0
  # One output line per listed file: <path>: OK, or a failure such as <path>: FAILED or
  # <path>: FAILED open or read. Split on newlines only, with no glob expansion (set -f).
  OLD_IFS="$IFS"
  IFS=$'\n'
  for line in $out; do
    case "$line" in
      *": OK") ok=$((ok + 1)) ;;
      *) add_problem "SHA256SUMS: $line"; added=$((added + 1)) ;;
    esac
  done
  IFS="$OLD_IFS"
  # A listing shasum cannot read at all, such as one with no well-formed line, prints no
  # failure line but still exits non-zero.
  if [ "$rc" -ne 0 ] && [ "$added" -eq 0 ]; then
    add_problem "SHA256SUMS: shasum -c exited $rc"
  fi
  echo "files that match their SHA256SUMS line: $ok"
else
  echo "skipped: no SHA256SUMS (counted in step 4)"
fi
step_result

step "6. Every transcript listed"
STEP_K=$K
if [ -f "$C/SHA256SUMS" ]; then
  # Each line is <64 hex digits><two spaces><path>, as record_corpus.sh writes it and
  # src/golden.rs reads it (a space and * in place of the two spaces also leaves the path at
  # column 67), so the listed path starts at column 67.
  LISTED="$(cut -c67- "$C/SHA256SUMS")"
  for id in $VALID; do
    [ -e "$C/$id/transcript.txt" ] || continue
    printf '%s\n' "$LISTED" | LC_ALL=C grep -qxF "$id/transcript.txt" \
      || add_problem "$C/$id/transcript.txt: not listed in SHA256SUMS"
  done
else
  echo "skipped: no SHA256SUMS (counted in step 4)"
fi
step_result

step "Result"
printf '%s' "$PROBLEMS"
if [ "$K" -eq 0 ]; then
  echo "CORPUS CHECK PASS $N CASES"
  exit 0
fi
echo "CORPUS CHECK FAIL $K PROBLEMS"
exit 1
