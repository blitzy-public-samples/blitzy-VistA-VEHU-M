#!/bin/bash
# open_side_by_side.sh: tile two Chrome windows, original on the left, Rust on the right.
# The bridge forbids framing (X-Frame-Options: DENY), so two windows is the supported way.
set -u
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/demo_env.sh"
LEFT="http://127.0.0.1:$ORIG_PORT/"; RIGHT="$RUST_URL/"
if [ "$(uname)" != "Darwin" ]; then
  echo "Open these two URLs next to each other:"; echo "  left : $LEFT"; echo "  right: $RIGHT"; exit 0
fi
read -r W H <<EOS
$(osascript -e 'tell application "Finder" to get bounds of window of desktop' | awk -F', ' '{print $3, $4}')
EOS
HALF=$((W / 2))
osascript <<OSA
tell application "Google Chrome"
  activate
  make new window
  set URL of active tab of front window to "$LEFT"
  set bounds of front window to {0, 25, $HALF, $H}
  make new window
  set URL of active tab of front window to "$RIGHT"
  set bounds of front window to {$HALF, 25, $W, $H}
end tell
OSA
echo "left: $LEFT   right: $RIGHT"
echo "If macOS asks to let Terminal control Chrome, allow it (System Settings, Privacy and Security, Automation)."
