#!/bin/sh
# Daily backup of the SuperBot data dir (Bots, chats, memory, routines, sessions).
# Keeps the last 7 copies. Install: see deploy/README-backup.md
set -eu
SRC=${1:-/home/ubuntu/superbot/data}
DST=${2:-/home/ubuntu/backups}
KEEP=7
mkdir -p "$DST"
chmod 700 "$DST"
stamp=$(date +%Y%m%d-%H%M)
tar -C "$(dirname "$SRC")" -czf "$DST/data-$stamp.tar.gz" "$(basename "$SRC")"
find "$DST" -name 'data-*.tar.gz' -type f | sort | head -n -"$KEEP" | xargs -r rm -f
echo "backup: $DST/data-$stamp.tar.gz"
