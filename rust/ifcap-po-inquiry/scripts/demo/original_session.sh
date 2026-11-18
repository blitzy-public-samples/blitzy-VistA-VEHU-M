#!/bin/bash
# original_session.sh: one interactive session of the ORIGINAL option PRCH DISPLAY PO
# (EN3^PRCHRPT) inside the VEHU container. ttyd runs this once per browser connection,
# so every page load is a fresh session. You can also run it by hand in any terminal.
set -u
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/demo_env.sh"
demo_find_docker || { echo "docker not found"; sleep 30; exit 1; }
if ! docker exec "$VEHU_CONTAINER" true >/dev/null 2>&1; then
  echo "Container '$VEHU_CONTAINER' is not running. Run scripts/demo/demo_up.sh first."
  sleep 30; exit 1
fi
exec docker exec -it "$VEHU_CONTAINER" su - vehu -c "$INQUIRY_CMD"
