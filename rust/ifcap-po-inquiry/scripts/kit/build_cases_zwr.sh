#!/usr/bin/env bash
# build_cases_zwr.sh: build tests/fixtures/ifcap/cases.zwr by README Part B step 6.
# Run from the crate folder:  bash build_cases_zwr.sh
# It is a text transformation only (no MUMPS runs). From the db-before.zwr of every READY case it takes
#   the nodes under ^PRC(442) (header, synthetic orders, their index nodes) and the synthetic vendor's
#   ^PRC(440) nodes, drops any line already present verbatim in a supporting fixture, keeps the container's
#   collation order, and writes the file under two ZWR header lines.
# An existing cases.zwr is never edited: without FORCE=1 the new file is written as cases.zwr.new.
set -u
G="tests/golden/ifcap"; F="tests/fixtures/ifcap"
[ -d "$G" ] && [ -d "$F" ] || { echo "run this from the crate folder (rust/ifcap-po-inquiry)"; exit 1; }
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
n=0; first=""
for d in "$G"/GC-0[1-5]; do
  [ -f "$d/db-before.zwr" ] && [ -f "$d/db-after.zwr" ] && [ -f "$d/transcript.txt" ] || continue
  cmp -s "$d/db-before.zwr" "$d/db-after.zwr" || { echo "skip $(basename "$d"): db-before and db-after differ"; continue; }
  grep -E '^\^PRC\(442,|^\^PRC\(440,990001,|^\^PRC\(440,"B","GC TEST VENDOR",990001\)' "$d/db-before.zwr" > "$WORK/part.$n" || true
  [ -z "$first" ] && first="$WORK/part.$n"
  [ "$n" -gt 0 ] && ! cmp -s "$first" "$WORK/part.$n" && echo "WARNING: $(basename "$d") holds different order nodes than the first case; cases were not all taken from the same snapshot"
  n=$((n+1))
done
[ "$n" -gt 0 ] || { echo "no usable case found; nothing built"; exit 1; }
# union: first case's lines in order, then any extra lines from the others (normally none)
cp "$first" "$WORK/union"
for p in "$WORK"/part.*; do [ "$p" = "$first" ] && continue; grep -vxFf "$WORK/union" "$p" >> "$WORK/union" || true; done
# drop lines already verbatim in a supporting fixture (data lines only, so header lines never match)
for f in "$F"/*.zwr; do case "$(basename "$f")" in cases.zwr) continue;; esac; grep '^\^' "$f"; done > "$WORK/fixlines"
grep -vxFf "$WORK/fixlines" "$WORK/union" > "$WORK/body" || true
{ echo "OSEHRA ZGO Export: IFCAP PO INQUIRY TEST CASES"; echo "ZWR"; cat "$WORK/body"; } > "$WORK/cases.zwr"
lines=$(grep -c '^\^' "$WORK/cases.zwr")
dest="$F/cases.zwr"
if [ -f "$dest" ] && [ "${FORCE:-0}" != "1" ]; then
  if cmp -s "$dest" "$WORK/cases.zwr"; then echo "cases.zwr already up to date ($lines nodes)"; exit 0; fi
  dest="$F/cases.zwr.new"; echo "cases.zwr exists and differs: wrote $dest instead. FORCE=1 replaces the original."
fi
cp "$WORK/cases.zwr" "$dest"; echo "wrote $dest: $lines nodes, built from $n case(s)"
echo "sha256: $(shasum -a 256 "$dest" | cut -d' ' -f1)"
