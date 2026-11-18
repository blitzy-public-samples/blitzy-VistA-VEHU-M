#!/usr/bin/env bash
# capture_all.sh: record the five golden cases GC-01..GC-05 from the ORIGINAL MUMPS, build cases.zwr,
# and run the crate's own tests. Run from the crate folder after save_state.sh says SAVED:
#   bash capture_all.sh
#
# For each case it restores the frozen container (vehu-snap:gc-base), exports the data before, runs the
# original with the crate's start command and the case's typed lines, exports the data after, restores
# and runs it a second time to check the output repeats. It never edits a capture afterwards.
# A case folder that already exists is skipped (captures are never overwritten) unless FORCE=1.
# Writes: tests/golden/ifcap/GC-0N/{input,transcript,returned,db-before,db-after,meta,normalisation}
#         tests/fixtures/ifcap/cases.zwr (or cases.zwr.new if one exists)

set -u
HERE="$(cd "$(dirname "$0")" && pwd)"; . "$HERE/vehu_common.sh"
SNAP_IMAGE="${SNAP_IMAGE:-vehu-snap:gc-base}"; BASE_IMAGE="${BASE_IMAGE:-worldvista/vehu:202609}"
G="tests/golden/ifcap"; PROBE="vehu-probe"; POUT="$(mktemp)"
SOURCE_REVISION="${SOURCE_REVISION:-dd3fd0f3124c8178657d9fff4173c324da30f7ae}"
REPORT="capture_all_report.txt"
[ -f Cargo.toml ] && [ -f "$G/README.md" ] || { echo "STOPPED: run this from the crate folder (rust/ifcap-po-inquiry)."; exit 1; }
exec > >(tee "$REPORT") 2>&1
fail() { echo; echo "STOPPED: $*"; echo "Paste this whole screen (or $REPORT) back to Claude."; exit 1; }
step() { echo; echo "== $*"; }
vc_find_docker || fail "docker not found. Run setup_vehu.sh first."
docker image inspect "$SNAP_IMAGE" >/dev/null 2>&1 || fail "image $SNAP_IMAGE not found. Run load_orders.sh and save_state.sh first."
PLATFORM_ARGS="$(vc_platform_args)"

step "1. The HOME device the original uses (open item O-01: IOF and IOSL)"
docker rm -f "$PROBE" >/dev/null 2>&1
# shellcheck disable=SC2086
docker run -d $PLATFORM_ARGS --name "$PROBE" "$SNAP_IMAGE" >/dev/null || fail "could not start a probe container"
vc_wait_ready "$PROBE" 600 || fail "probe container not ready"
vc_detect_inquiry "$PROBE"
echo "start command: $INQUIRY_CMD"; [ -n "$INQUIRY_NOTE" ] && echo "note: $INQUIRY_NOTE"
PROBE_M='S U=\$C(94),DTIME=300,DT=\$\$DT^XLFDT D DUZ^XUP(.5) S IOP=\"HOME\",%ZIS=\"\" D ^%ZIS W !,\"IOSL=\",IOSL,!,\"IOM=\",\$G(IOM),!,\"DEV=\",\$G(IO),!,\"DOLLARI=\",\$I,!,\"IOF=\" F I=1:1:\$L(IOF) W \$A(IOF,I),\" \"'
PRE=""; case "$INQUIRY_CMD" in "source "*) PRE='source /home/vehu/etc/env && ';; esac
BIN="yottadb"; case "$INQUIRY_CMD" in *"mumps -run"*) BIN="mumps";; esac
docker exec -i "$PROBE" su - vehu -c "${PRE}${BIN} -run %XCMD \"$PROBE_M\"" < /dev/null > "$POUT" 2>&1
docker rm -f "$PROBE" >/dev/null 2>&1
cat -v "$POUT"
iof_codes="$(grep '^IOF=' "$POUT" | head -1 | cut -d= -f2-)"
iosl="$(grep '^IOSL=' "$POUT" | head -1 | cut -d= -f2- | tr -d '[:space:]')"
iom="$(grep '^IOM=' "$POUT" | head -1 | cut -d= -f2- | tr -d '[:space:]')"
dev="$(grep '^DEV=' "$POUT" | head -1 | cut -d= -f2-)"
[ -n "$iosl" ] || fail "could not read IOSL from the device probe (see output above). The probe command may need adjusting."
iof_json=""
for c in $iof_codes; do
  if [ "$c" -ge 32 ] && [ "$c" -le 126 ] && [ "$c" -ne 34 ] && [ "$c" -ne 92 ]; then iof_json="$iof_json$(printf "\\$(printf '%03o' "$c")")"
  else iof_json="$iof_json$(printf '\\u%04x' "$c")"; fi
done
CONTEXT="{\"DUZ\":\".5\",\"IOF\":\"$iof_json\",\"IOSL\":\"$iosl\",\"STATION\":\"500\"}"
echo "context: $CONTEXT"
case " $iof_codes " in *" 27 "*) echo; echo "*** STOP CONDITION (SPEC.md 'Workflow selection'): IOF holds an escape code, so the display is cursor-addressed. Captures continue so the evidence is recorded, but report this to the team. ***";; esac
rm -f "$POUT"

case_def() { # $1 id -> sets INPUT (printf format) and RECORD_IDS
  case "$1" in
    GC-01) INPUT='\nGC1001\n\n\n^\n'; RECORD_IDS='442:990001;440:990001';;
    GC-02) INPUT='\nGC2002\n\n\n^\n'; RECORD_IDS='442:990002;440:990001';;
    GC-03) INPUT='\nGC3003\n^\n';     RECORD_IDS='442:none';;
    GC-04) INPUT='\nINVALID!\n^\n';   RECORD_IDS='442:none';;
    GC-05) INPUT='\nGC5005\n\n\n^\n'; RECORD_IDS='442:990005;440:990001';;
  esac
}
MCMD="$(vc_mcmd "$VC_SCOPE_ROOTS")"
EXPORT_CMD="mumps -run %XCMD '$MCMD'"
SNAP_ID="$(docker image inspect --format '{{.Id}}' "$SNAP_IMAGE" 2>/dev/null || echo UNKNOWN)"
results=""
for id in GC-01 GC-02 GC-03 GC-04 GC-05; do
  step "$id"
  d="$G/$id"
  if [ -d "$d" ] && [ -n "$(ls -A "$d" 2>/dev/null)" ] && [ "${FORCE:-0}" != "1" ]; then
    echo "folder $d already holds files: skipped (captures are never overwritten; FORCE=1 to record again)"
    results="$results$id=SKIPPED "; continue
  fi
  mkdir -p "$d"; case_def "$id"
  printf "$INPUT" > "$d/input.txt"
  rm -f "$d/normalisation.txt"
  SETUP="HOME device ${dev:-unknown}, IOM=${iom:-unknown}, IOSL=$iosl; orders loaded by load_orders.sh into a throwaway container, frozen as $SNAP_IMAGE; read-only inquiry. ${INQUIRY_NOTE}"
  INQUIRY_CMD="$INQUIRY_CMD" EXPORT_CMD="$EXPORT_CMD" SNAP_IMAGE="$SNAP_IMAGE" BASE_IMAGE="$BASE_IMAGE" \
  ENTRY_POINT='EN3^PRCHRPT' CONTEXT="$CONTEXT" RECORD_IDS="$RECORD_IDS" SOURCE_REVISION="$SOURCE_REVISION" \
  SETUP="$SETUP" RETURNS_NOTHING=1 bash "$HERE/capture_case.sh" "$d"
  rc=$?
  case "$rc" in
    0) results="$results$id=RECORDED ";;
    3) results="$results$id=RECORDED-RUNS-DIFFER ";;
    *) results="$results$id=FAILED($rc) ";;
  esac
done
# the run-twice check: a case whose two runs differ keeps no normalisation.txt and is INCOMPLETE until rules exist

step "cases.zwr"
bash "$HERE/build_cases_zwr.sh"

step "The crate's own verdicts (cargo test)"
if command -v cargo >/dev/null 2>&1 || { [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"; }; then
  cargo test --offline --locked --test equivalence -- --nocapture 2>&1 | grep -E '^GC-0[1-5]|test result|error|FAIL|PASS|NOT COMPARED' | head -30
  echo "(REPORT.md in the crate folder was rewritten by the test; part 2 has each verdict and its cause)"
else echo "cargo not found; skipped"; fi

step "SUMMARY"
for r in $results; do echo "  $r"; done
echo
echo "Next: paste this screen to Claude. Expected right now: GC-03 and GC-04 PASS; GC-01, GC-02 and GC-05 FAIL until the"
echo "display after the order prompt is implemented (SPEC.md O-01). The transcripts you just recorded are the evidence for that."
