#!/bin/sh
# KUAL entry point: detaches the dashboard loop so KUAL can return before the UI stops.
DIR="$(cd "$(dirname "$0")/.." && pwd)"
PIDFILE=/var/tmp/miqat.pid
if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
  exit 0
fi
# KUAL runs us inside the UI's process tree, and `stop lab126_gui` kills that whole
# process group (eventually with KILL, which no trap survives). setsid moves the loop
# into its own session first.
setsid "$DIR/bin/loop.sh" >>"$DIR/miqat.log" 2>&1 </dev/null &
echo $! >"$PIDFILE"
