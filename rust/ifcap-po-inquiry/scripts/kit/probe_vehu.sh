#!/usr/bin/env bash
# probe_vehu.sh: READ-ONLY look at the crate's capture settings and the data in the VEHU container.
# Run from the crate folder (rust/ifcap-po-inquiry/) after setup_vehu.sh says READY:
#   bash probe_vehu.sh
# It prints what the capture steps need and saves it to probe_report.txt. It changes nothing.
# Optional: NAME=vehu  ROOT='^PRC(442'  (the purchase-order global root, if SPEC.md gives a different one)

set -u
NAME="${NAME:-vehu}"
ROOT="${ROOT:-^PRC(442}"
REPORT="probe_report.txt"
exec > >(tee "$REPORT") 2>&1

if ! command -v docker >/dev/null 2>&1 && [ -x /Applications/Docker.app/Contents/Resources/bin/docker ]; then
  export PATH="/Applications/Docker.app/Contents/Resources/bin:$PATH"
fi

echo "== A. Where am I"
pwd
ls

echo; echo "== B. Files that hold the capture settings"
SPEC="$(find . -maxdepth 3 -name SPEC.md -not -path '*/target/*' 2>/dev/null | head -3)"
echo "SPEC.md found at: ${SPEC:-NOT FOUND}"
ls tests/golden/ifcap 2>/dev/null | sed 's/^/  tests\/golden\/ifcap\/: /'
README=tests/golden/ifcap/README.md

echo; echo "== C. README: docker_exec_args / export / load / case blocks"
if [ -f "$README" ]; then
  grep -n -i -A4 'docker_exec_args' "$README"
  echo "--- export"; grep -n -i -A4 'export' "$README"
  echo "--- load"; grep -n -i -A4 'load command' "$README"
  echo "--- GC-01"; grep -n -i -B1 -A6 'GC-01' "$README"
else echo "no $README"; fi

echo; echo "== D. SPEC: global roots / exit input / entry point / open items"
for f in $SPEC; do
  echo "--- $f"
  grep -n -i -A4 'global roots' "$f" | head -30
  grep -n -i -A2 'exit input' "$f" | head -12
  grep -n -i -A2 'entry point' "$f" | head -20
  grep -n -E 'O-0[0-9]' "$f" | head -12
done

echo; echo "== E. Is the container up?"
docker ps --filter "name=^${NAME}$" --format '{{.Names}} {{.Status}}'
docker ps --format '{{.Names}}' | grep -qx "$NAME" || { echo "container $NAME is not running. Run setup_vehu.sh first."; exit 1; }

echo; echo "== F. Data in the container (read only): $ROOT"
docker exec -i "$NAME" su - vehu -c 'mumps -dir' <<MEOF
write "header node: ",\$get(${ROOT},0)),!
set i=0,c=0 for  set i=\$order(${ROOT},i)) quit:i=""  quit:i'=+i  set c=c+1
write "order count (numeric subscripts after 0): ",c,!
write "--- first 20 orders: ien | node 0 | node 7",!
set i=0,n=0 for  set i=\$order(${ROOT},i)) quit:i=""  quit:i'=+i  quit:n>=20  set n=n+1 write i," | ",\$get(${ROOT},i,0)),!," | 7: ",\$get(${ROOT},i,7)),!
halt
MEOF

echo; echo "== G. Is the inquiry routine present? (name from SPEC entry point if known)"
echo "(skipped here; the entry point in SPEC.md decides which routine to check)"
echo; echo "DONE. Paste this whole screen (or $REPORT) back to Claude."
