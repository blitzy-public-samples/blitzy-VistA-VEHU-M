#!/usr/bin/env bash
# add_buyer_fixture.sh: close open item O-09 (GC-01, GC-02 and GC-05 fail at transcript line 25).
# Line 25 prints the buyer's name from ^VA(200,<buyer>,0) [PRCHDP1.m:59]. Every order's buyer is .5, and no
# fixture holds that node. This script writes ONE new supporting fixture, tests/fixtures/ifcap/200.zwr,
# holding only ^VA(200,.5,0) copied from the captures' own db-before.zwr. It edits no existing file.
# Run from the crate folder (rust/ifcap-po-inquiry):   bash add_buyer_fixture.sh
# Options (environment):  MASK=1  blank piece 3 of the node (a hashed access code) in the new fixture
#                         NOTEST=1  write the fixture and stop; do not run cargo test
set -u
G="tests/golden/ifcap"; F="tests/fixtures/ifcap"; OUT="$F/200.zwr"
[ -d "$G" ] && [ -d "$F" ] || { echo "run this from the crate folder (rust/ifcap-po-inquiry)"; exit 1; }
[ ! -e "$OUT" ] || { echo "$OUT already exists; leaving it alone"; exit 1; }
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
n=0; first=""
for d in "$G"/GC-0[1-5]; do
  [ -f "$d/db-before.zwr" ] || continue
  grep -F '^VA(200,.5,0)=' "$d/db-before.zwr" | grep -E '^\^VA\(200,\.5,0\)="' > "$WORK/node.$n" || true
  [ "$(grep -c . "$WORK/node.$n")" = "1" ] || { echo "$(basename "$d"): expected exactly one ^VA(200,.5,0) line in db-before.zwr"; exit 1; }
  [ -z "$first" ] && first="$WORK/node.$n"
  cmp -s "$first" "$WORK/node.$n" || { echo "$(basename "$d"): ^VA(200,.5,0) differs from the first case; stop and look"; exit 1; }
  n=$((n+1))
done
[ "$n" -gt 0 ] || { echo "no db-before.zwr found under $G"; exit 1; }
if [ "${MASK:-0}" = "1" ]; then
  awk '{ i=index($0,"=\""); key=substr($0,1,i-1); val=substr($0,i+2); sub(/"$/,"",val);
         m=split(val,a,"^"); a[3]=""; out=a[1]; for(k=2;k<=m;k++) out=out "^" a[k]; print key "=\"" out "\"" }' "$first" > "$WORK/node.out"
else
  cp "$first" "$WORK/node.out"
fi
{ echo "OSEHRA ZGO Export: NEW PERSON"; echo "ZWR"; cat "$WORK/node.out"; } > "$OUT"
echo "wrote $OUT (from $n capture(s)):"; sed -n '3p' "$OUT" | cut -c1-120
echo "sha256: $(shasum -a 256 "$OUT" | cut -d' ' -f1)"
[ "${NOTEST:-0}" = "1" ] && exit 0
echo; echo "Running: cargo test --offline --locked --no-fail-fast (tail follows)"
cargo test --offline --locked --no-fail-fast 2>&1 | tail -25
echo; echo "Verdicts in REPORT.md:"; grep -nE 'GC-0[1-5].*(PASS|FAIL|NOT COMPARED)' REPORT.md | head -10 | cut -c1-160
