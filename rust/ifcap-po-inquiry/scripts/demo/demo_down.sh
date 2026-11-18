#!/bin/bash
# demo_down.sh: stop what demo_up.sh started. Add --container to remove the VEHU container too.
set -u
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/demo_env.sh"
for n in bridge ttyd; do
  f="$RUN_DIR/$n.pid"
  if [ -f "$f" ]; then
    pid="$(cat "$f")"
    if kill -0 "$pid" 2>/dev/null; then kill "$pid" && echo "stopped $n (pid $pid)"; else echo "$n was not running"; fi
    rm -f "$f"
  fi
done
if [ "${1:-}" = "--container" ]; then
  demo_find_docker && docker rm -f "$VEHU_CONTAINER" >/dev/null 2>&1 && echo "removed container $VEHU_CONTAINER"
fi
