#!/usr/bin/env bash
# load_orders.sh: put the five-case test purchase orders into the THROWAWAY VEHU container.
# Run from the crate folder after setup_vehu.sh says READY, and BEFORE save_state.sh:
#   bash load_orders.sh
#
# Why: the VEHU image holds no purchase orders at all (probe: order count 0), so the cases that need
# an order (GC-01, GC-02, GC-05) have nothing to look up. README Part B step 2 (d) has the team load
# synthetic records that meet its "Fixture profile". This does that, with these rules:
#   * it writes only into the container named vehu, never into the repository or the product;
#   * the records are fixed text below (three orders, one vendor, their index nodes);
#   * it never loads anything for GC-03 or GC-04 (those cases need the order to be ABSENT);
#   * it checks the records read back byte for byte, then shows what the ORIGINAL screen prints for
#     GC1001, so you can see that the loaded data produces a normal display.
# Run it again safely: it does nothing if the orders are already there (FORCE=1 reloads).

set -u
HERE="$(cd "$(dirname "$0")" && pwd)"; . "$HERE/vehu_common.sh"
NAME="${NAME:-vehu}"; REPORT="load_orders_report.txt"
exec > >(tee "$REPORT") 2>&1
fail() { echo; echo "STOPPED: $*"; echo "Paste this whole screen (or $REPORT) back to Claude."; exit 1; }
vc_find_docker || fail "docker not found. Run setup_vehu.sh first."
docker ps --format '{{.Names}}' | grep -qx "$NAME" || fail "container '$NAME' is not running. Run setup_vehu.sh first."

WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
cat > "$WORK/orders.zwr" <<'ZWR'
^PRC(440,990001,0)="GC TEST VENDOR^1 TEST STREET^^^^TESTVILLE^36^12345^^555-0100"
^PRC(440,"B","GC TEST VENDOR",990001)=""
^PRC(442,990001,0)="500-GC1001^1^123 GC TEST^3600160^1234^^^^^3261015^^^^^251"
^PRC(442,990001,1)="990001^^^^^D^^^^.5^^^^^3261001"
^PRC(442,990001,2,0)="^442.01^1^1"
^PRC(442,990001,2,1,0)="1^2^1^2631 GC BOC^^^^^125.5"
^PRC(442,990001,2,1,1,0)="^^1^1^3261001^"
^PRC(442,990001,2,1,1,1,0)="GC TEST ITEM ONE"
^PRC(442,990001,2,1,2)=251
^PRC(442,990001,2,"B",1,1)=""
^PRC(442,990001,7)="25^^"
^PRC(442,990002,0)="500-GC2002^1^123 GC TEST^3600160^1234^^^^^3261015^^^^^311.84"
^PRC(442,990002,1)="990001^^^^^D^^^^.5^^^^^3261002"
^PRC(442,990002,2,0)="^442.01^3^3"
^PRC(442,990002,2,1,0)="1^3^2^2631 GC BOC^^^^^19.95"
^PRC(442,990002,2,1,1,0)="^^1^1^3261002^"
^PRC(442,990002,2,1,1,1,0)="GC TEST ITEM A"
^PRC(442,990002,2,1,2)=59.85
^PRC(442,990002,2,2,0)="2^2^3^2631 GC BOC^^^^^125.5"
^PRC(442,990002,2,2,1,0)="^^1^1^3261002^"
^PRC(442,990002,2,2,1,1,0)="GC TEST ITEM B"
^PRC(442,990002,2,2,2)=251
^PRC(442,990002,2,3,0)="3^1^4^2631 GC BOC^^^^^.99"
^PRC(442,990002,2,3,1,0)="^^1^1^3261002^"
^PRC(442,990002,2,3,1,1,0)="GC TEST ITEM C"
^PRC(442,990002,2,3,2)=.99
^PRC(442,990002,2,"B",1,1)=""
^PRC(442,990002,2,"B",2,2)=""
^PRC(442,990002,2,"B",3,3)=""
^PRC(442,990002,7)="25^^"
^PRC(442,990005,0)="500-GC5005^1^123 GC TEST^3600160^1234^^^^^3261015^^^^^15"
^PRC(442,990005,1)="990001^^^^^D^^^^.5^^^^^3261005"
^PRC(442,990005,2,0)="^442.01^2^2"
^PRC(442,990005,2,1,0)="1^1^1^2631 GC BOC^^^^^10"
^PRC(442,990005,2,1,1,0)="^^1^1^3261005^"
^PRC(442,990005,2,1,1,1,0)="GC TEST ITEM X"
^PRC(442,990005,2,1,2)=10
^PRC(442,990005,2,2,0)="2^1^1^2631 GC BOC^^^^^5"
^PRC(442,990005,2,2,1,0)="^^1^1^3261005^"
^PRC(442,990005,2,2,1,1,0)="GC TEST ITEM Y"
^PRC(442,990005,2,2,2)=5
^PRC(442,990005,2,"B",1,1)=""
^PRC(442,990005,2,"B",2,2)=""
^PRC(442,990005,7)="45^^"
^PRC(442,"B","500-GC1001",990001)=""
^PRC(442,"B","500-GC2002",990002)=""
^PRC(442,"B","500-GC5005",990005)=""
^PRC(442,"C","GC1001",990001)=""
^PRC(442,"C","GC2002",990002)=""
^PRC(442,"C","GC5005",990005)=""
ZWR
echo "== 1. Before loading"
vc_detect_inquiry "$NAME"
docker exec -i "$NAME" su - vehu -c 'mumps -dir' > "$WORK/pre.out" 2>&1 <<'MEOF'
write "ORDERS_PRESENT=",($data(^PRC(442,990001))+$data(^PRC(442,990002))+$data(^PRC(442,990005))),!
set k="GC3003" write "GC3003=",$select($data(^PRC(442,"C",k)):"PRESENT",$extract($order(^PRC(442,"C",k)),1,$length(k))=k:"PRESENT",1:"absent"),!
write "VENDOR_PRESENT=",$data(^PRC(440,990001)),!
write "STATION_500=",$data(^PRC(411,500,0)),!
halt
MEOF
grep -E '^(ORDERS_PRESENT|GC3003|VENDOR_PRESENT|STATION_500)=' "$WORK/pre.out"
grep -q '^GC3003=absent' "$WORK/pre.out" || fail "GC3003 is not absent in the container: the no-match case needs it absent. See above."
grep -q '^STATION_500=[1-9]' "$WORK/pre.out" || echo "WARNING: ^PRC(411,500,0) not found in the container; the display may differ from the crate's assumptions."
already=$(grep '^ORDERS_PRESENT=' "$WORK/pre.out" | cut -d= -f2 | tr -d '[:space:]')
if [ "${already:-0}" != "0" ] && [ "${FORCE:-0}" != "1" ]; then
  echo "Orders already loaded (ORDERS_PRESENT=$already). Nothing to do. FORCE=1 reloads."
else
  echo; echo "== 2. Loading $(grep -c '^\^' "$WORK/orders.zwr") nodes into container $NAME (a throwaway container)"
  { sed 's/^/set /' "$WORK/orders.zwr"; echo "halt"; } | docker exec -i "$NAME" su - vehu -c 'mumps -dir' > "$WORK/load.out" 2>&1
  if grep -qE '%YDB-E|%GTM-E' "$WORK/load.out"; then echo "M errors during load:"; grep -E '%YDB-E|%GTM-E' "$WORK/load.out" | head; fail "load had errors"; fi
  echo "load finished with no M errors"
fi

echo; echo "== 3. Read the records back and compare byte for byte"
docker exec -i "$NAME" su - vehu -c "mumps -run %XCMD 'zwrite ^PRC(442,*) zwrite ^PRC(440,990001,*) zwrite ^PRC(440,\"B\",\"GC TEST VENDOR\",*)'" < /dev/null > "$WORK/back.zwr" 2> "$WORK/back.err"
missing=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  grep -qxF -- "$line" "$WORK/back.zwr" || { echo "  MISSING OR DIFFERENT: $line"; missing=$((missing+1)); }
done < "$WORK/orders.zwr"
[ "$missing" -eq 0 ] || { echo "first lines read back:"; head -5 "$WORK/back.zwr"; fail "$missing record lines did not read back as loaded"; }
echo "all $(grep -c '^\^' "$WORK/orders.zwr") loaded nodes read back identical"

echo; echo "== 4. What the ORIGINAL screen prints for GC1001 (the crate's start command, input: station default, GC1001, two returns, ^)"
printf '\nGC1001\n\n\n^\n' | docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" 2>&1 | cat -v | head -60
echo "-- end of GC1001 display. $INQUIRY_NOTE"
echo; echo "== 5. And for GC3003 (no such order)"
printf '\nGC3003\n^\n' | docker exec -i "$NAME" su - vehu -c "$INQUIRY_CMD" 2>&1 | cat -v | head -20
echo "-- end"
echo; echo "RESULT: LOADED. Next: freeze it with save_state.sh."
echo "Paste this whole screen to Claude: the two displays above show whether the loaded data looks like a normal order to the original."
