#!/bin/sh
# KUAL entry point: detaches the dashboard loop so KUAL can return before the UI stops.
DIR="$(cd "$(dirname "$0")/.." && pwd)"
PIDFILE=/var/tmp/miqat.pid
if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
  exit 0
fi
nohup "$DIR/bin/loop.sh" >>"$DIR/miqat.log" 2>&1 &
echo $! >"$PIDFILE"
