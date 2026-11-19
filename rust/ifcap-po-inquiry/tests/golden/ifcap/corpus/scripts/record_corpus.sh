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
# Environment (each optional):
#   NAME=vehu                                                    container name
#   BASE_IMAGE=worldvista/vehu:202609                            the original image (its digest goes to meta.txt)
#   SOURCE_REVISION=dd3fd0f3124c8178657d9fff4173c324da30f7ae     source revision written to meta.txt

set -u
fail() { echo; echo "FAIL: $*"; exit 1; }
step() { echo; echo "== $*"; }

cd "$(dirname "$0")/../../../../.." || exit 1
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || fail "cannot find the crate folder (rust/ifcap-po-inquiry) five levels above this script."

KIT=scripts/kit/vehu_common.sh
[ -f "$KIT" ] || fail "$KIT not found: this script needs the committed recording kit."
. "$KIT"
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

# probe_ready: shell-only readiness probe (SPEC.md O-16). It polls container NAME every 5 s, for up
# to 600 s, until the vehu login shell runs and finds the environment file the start command
# sources. It runs no M command, so it proves the shell and the environment file, not the
# database: the export before run 1 and the two-run identity check are the guard. 1 on timeout.
probe_ready() {
  local waited=0
  while [ "$waited" -lt 600 ]; do
    docker exec "$NAME" su - vehu -c 'test -f /home/vehu/etc/env' < /dev/null >/dev/null 2>&1 && return 0
    sleep 5
    waited=$((waited + 5))
  done
  return 1
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

step "1. Prerequisites"
[ -f "$BASE" ] || fail "$BASE not found: run tests/golden/ifcap/corpus/scripts/load_corpus.sh first"
BASE_SNAP_ID="$(sed -n 's/^snapshot_id=//p' "$BASE")"
BASE_EXPORT_SHA="$(sed -n 's/^export_sha256=//p' "$BASE")"
[ -n "$BASE_SNAP_ID" ] || fail "$BASE holds no snapshot_id= value: run load_corpus.sh again"
[ -n "$BASE_EXPORT_SHA" ] || fail "$BASE holds no export_sha256= value: run load_corpus.sh again"
[ -f "$MANIFEST" ] || fail "$MANIFEST not found: run cargo run --offline --locked --bin corpus_gen first"
HEADER="$(sed -n 1p "$MANIFEST")"
[ "$(printf '%s\n' "$HEADER" | cut -f1)" = "case" ] && [ "$(printf '%s\n' "$HEADER" | cut -f2)" = "class" ] \
  || fail "$MANIFEST line 1 is not the header case<TAB>class<TAB>...: run corpus_gen again"
# One id:class word per case, in manifest order. Every loop below walks this list with `for`,
# unquoted on purpose so that it splits into words, and never with a `while read` over standard
# input, which each `docker exec -i` would consume.
ROWS="$(sed 1d "$MANIFEST" | cut -f1,2 | tr '\t' ':')"
COUNT=0
BAD=""
for row in $ROWS; do
  id="${row%%:*}"
  class="${row#*:}"
  # Ids become paths, so only the three corpus series pass.
  case "$id" in CP-[0-9][0-9][0-9]|CE-[0-9][0-9]|CR-[0-9][0-9]) ;; *) BAD="$BAD $row";; esac
  case "$class" in CP|IN|BOUNDARY|CR) ;; *) BAD="$BAD $row";; esac
  COUNT=$((COUNT + 1))
done
[ -z "$BAD" ] || fail "$MANIFEST has rows that are not <case id><TAB><class>:$BAD"
[ "$COUNT" -gt 0 ] || fail "$MANIFEST lists no case"
UNIQUE="$(for row in $ROWS; do printf '%s\n' "${row%%:*}"; done | LC_ALL=C sort -u | wc -l | tr -d ' ')"
[ "$UNIQUE" = "$COUNT" ] || fail "$MANIFEST lists a case id more than once ($UNIQUE distinct ids in $COUNT rows)"
echo "cases: $COUNT (corpus_gen writes 231)"
MISSING=""
for row in $ROWS; do
  id="${row%%:*}"
  [ -f "$C/$id/input.txt" ] || MISSING="$MISSING $id"
done
[ -z "$MISSING" ] || fail "input.txt missing for:$MISSING (run cargo run --offline --locked --bin corpus_gen)"
SNAP_ID="$(docker image inspect --format '{{.Id}}' "$CORPUS_SNAP" 2>/dev/null)"
[ -n "$SNAP_ID" ] || fail "image $CORPUS_SNAP not found: run tests/golden/ifcap/corpus/scripts/load_corpus.sh first"
[ "$SNAP_ID" = "$BASE_SNAP_ID" ] || fail "the id of $CORPUS_SNAP ($SNAP_ID) differs from $BASE snapshot_id ($BASE_SNAP_ID): run load_corpus.sh again"
DIGEST="$(docker image inspect --format '{{index .RepoDigests 0}}' "$BASE_IMAGE" 2>/dev/null)"
[ -n "$DIGEST" ] || fail "cannot read the digest of $BASE_IMAGE: pull it with docker pull $BASE_IMAGE"
echo "container:       $NAME"
echo "snapshot:        $CORPUS_SNAP ($SNAP_ID)"
echo "base image:      $BASE_IMAGE ($DIGEST)"
echo "source revision: $SOURCE_REVISION"

step "2. Container for run 1"
restore_container "$CORPUS_SNAP" || fail "docker run failed for $CORPUS_SNAP"
probe_ready || fail "container not ready after 600s (shell-only readiness probe, SPEC.md O-16; check: docker logs $NAME)"
echo "container $NAME ready"

step "3. Start command"
vc_detect_inquiry "$NAME"
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
export_scope "$W/export-before.zwr" "$W/export-before.err"
check_export export-before "$?"
BEFORE_SHA="$(shasum -a 256 "$W/export-before.zwr" | cut -d' ' -f1)"
[ -n "$BEFORE_SHA" ] || fail "shasum -a 256 failed on $W/export-before.zwr"
[ "$BEFORE_SHA" = "$BASE_EXPORT_SHA" ] \
  || fail "the export before (sha256 $BEFORE_SHA) differs from $BASE export_sha256 ($BASE_EXPORT_SHA): the container does not hold the loaded corpus state"
echo "export before: $W/export-before.zwr (sha256 $BEFORE_SHA, equal to $BASE)"
# The run files of an earlier run go first: an interrupted run then leaves transcripts with no
# listing, which the corpus reader reports as an integrity failure instead of trusting.
rm -f "$C/meta.txt" "$C/run-summary.txt" "$C/SHA256SUMS" || fail "cannot remove the run files of an earlier run in $C"


step "5. Run 1 ($COUNT cases)"
# No pseudo-terminal (no -t), so the transcript holds exactly what the original wrote. A case's
# exit status is evidence only: whatever it is, the transcript is kept as written.
: > "$W/run1/status.tsv" || fail "cannot write $W/run1/status.tsv"
n=0
for row in $ROWS; do
  id="${row%%:*}"
  n=$((n + 1))
  : > "$C/$id/transcript.txt" || fail "cannot write $C/$id/transcript.txt"
  docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" < "$C/$id/input.txt" > "$C/$id/transcript.txt" 2> "$W/run1/$id.stderr"
  rc=$?
  printf '%s\t%s\n' "$id" "$rc" >> "$W/run1/status.tsv" || fail "cannot write $W/run1/status.tsv"
  echo "run 1: $n of $COUNT $id exit $rc"
done

step "6. Export after"
export_scope "$W/export-after.zwr" "$W/export-after.err"
check_export export-after "$?"
if cmp -s "$W/export-before.zwr" "$W/export-after.zwr"; then
  EXPORTS=IDENTICAL
  echo "export after equals export before: the inquiries changed no data"
else
  EXPORTS=DIFFERENT
  printf 'DIFF\t%s\n' export >> "$W/diffs.tsv" || fail "cannot write $W/diffs.tsv"
  echo "export after DIFFERS from export before. First lines of the difference:"
  diff "$W/export-before.zwr" "$W/export-after.zwr" | sed -n 1,20p
fi
echo "exports=$EXPORTS"

step "7. Run 2 on a fresh container"
restore_container "$CORPUS_SNAP" || fail "docker run failed for $CORPUS_SNAP"
probe_ready || fail "container not ready after 600s (shell-only readiness probe, SPEC.md O-16; check: docker logs $NAME)"
echo "container $NAME ready; start command as in step 3"
: > "$W/run2/status.tsv" || fail "cannot write $W/run2/status.tsv"
n=0
for row in $ROWS; do
  id="${row%%:*}"
  n=$((n + 1))
  mkdir -p "$W/run2/$id" || fail "cannot create $W/run2/$id"
  docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" < "$C/$id/input.txt" > "$W/run2/$id/transcript.txt" 2> "$W/run2/$id.stderr"
  rc=$?
  printf '%s\t%s\n' "$id" "$rc" >> "$W/run2/status.tsv" || fail "cannot write $W/run2/status.tsv"
  echo "run 2: $n of $COUNT $id exit $rc"
done
RUNS=IDENTICAL
for row in $ROWS; do
  id="${row%%:*}"
  if ! cmp -s "$C/$id/transcript.txt" "$W/run2/$id/transcript.txt"; then
    RUNS=DIFFERENT
    printf 'DIFF\t%s\n' "$id" >> "$W/diffs.tsv" || fail "cannot write $W/diffs.tsv"
    echo "$id: run 1 and run 2 DIFFER. First lines of the difference:"
    diff "$C/$id/transcript.txt" "$W/run2/$id/transcript.txt" | sed -n 1,20p
  fi
done
echo "runs=$RUNS"

step "8. Paging detector"
# A paging pause prints ENTER '^' TO HALT: on its own, and END OF DISPLAY--PRESS RETURN OR ENTER
# '^' TO HALT: contains that text too, so a found order with no paging gives a = 2 and b = 1. Only
# an extra pause makes a exceed 2b. BOUNDARY cases are exempt: CE-25 answers the first pause with
# ^ and legitimately never reaches the second. A quarantine classifies, and never fails, the run.
HALT_PROMPT="ENTER '^' TO HALT: "
END_PROMPT="END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: "
QUARANTINED_IDS=" "
QUARANTINED=0
for row in $ROWS; do
  id="${row%%:*}"
  class="${row#*:}"
  case "$class" in
    IN|CP|CR)
      f="$C/$id/transcript.txt"
      a="$(LC_ALL=C grep -a -o -F "$HALT_PROMPT" "$f" | wc -l | tr -d ' ')"
      b="$(LC_ALL=C grep -a -o -F "$END_PROMPT" "$f" | wc -l | tr -d ' ')"
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
  for row in $ROWS; do
    id="${row%%:*}"
    case "$QUARANTINED_IDS" in *" $id "*) status=QUARANTINED;; *) status=RECORDED;; esac
    rc1="$(grep "^$id$TAB" "$W/run1/status.tsv" | cut -f2)"
    rc2="$(grep "^$id$TAB" "$W/run2/status.tsv" | cut -f2)"
    printf '%s\t%s\t%s\t%s\n' "$id" "$status" "$rc1" "$rc2"
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
  for row in $ROWS; do
    id="${row%%:*}"
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
LISTED="$(wc -l < "$W/sums.sorted" | tr -d ' ')"
SUMS="$(wc -l < "$W/SHA256SUMS.new" | tr -d ' ')"
[ "$SUMS" = "$LISTED" ] || fail "$W/SHA256SUMS.new has $SUMS lines for $LISTED files"
BAD_SUMS="$(LC_ALL=C grep -cvE '^[0-9a-f]{64}  [A-Za-z0-9._/-]+$' "$W/SHA256SUMS.new")"
[ "$BAD_SUMS" = "0" ] || fail "$W/SHA256SUMS.new has $BAD_SUMS lines that are not <sha256><two spaces><path>"
mv "$W/SHA256SUMS.new" "$C/SHA256SUMS" || fail "cannot write $C/SHA256SUMS"
echo "wrote $C/SHA256SUMS ($SUMS files)"

step "12. Result"
docker rm -f "$NAME" >/dev/null 2>&1
NONZERO="$(grep -cv "${TAB}0\$" "$W/run1/status.tsv")"
EMPTY=""
for row in $ROWS; do
  id="${row%%:*}"
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

