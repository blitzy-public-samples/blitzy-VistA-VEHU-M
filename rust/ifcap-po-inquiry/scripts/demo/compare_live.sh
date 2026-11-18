#!/bin/bash
# compare_live.sh: three-way byte comparison for each case.
#   capture   = tests/golden/ifcap/<case>/transcript.txt (recorded earlier)
#   original  = the ORIGINAL run again, right now, in the container, with the same stdin
#   rust      = the Rust port answering POST /api/rust/session with the same inputs
# Usage: compare_live.sh [case-dir ...]     (default: every case folder under $CASES_DIR)
# Exit 0 only when every case is PASS. Report: $RUN_DIR/compare/report.txt
set -u
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/demo_env.sh"
command -v python3 >/dev/null 2>&1 || { echo "python3 not found"; exit 2; }
demo_find_docker || { echo "docker not found"; exit 2; }
docker exec "$VEHU_CONTAINER" true >/dev/null 2>&1 || { echo "container $VEHU_CONTAINER is not running; run demo_up.sh"; exit 2; }
curl -fsS "$RUST_URL/api/status" >/dev/null 2>&1 || { echo "bridge not answering on $RUST_URL; run demo_up.sh"; exit 2; }
export VEHU_CONTAINER INQUIRY_CMD RUST_URL CASES_DIR RUN_DIR
exec python3 "$DEMO_SCRIPTS_DIR/compare_live.py" "$@"
