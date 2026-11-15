#!/bin/sh
# Inspect a directory on the host running this script.
#
# Usage: dir_status.sh [DIR]
#
# Prints KEY=value markers:
#   EXISTS=yes | WRITABLE=yes | CREATABLE=yes
#   TOTAL_BLOCKS=n AVAIL_BLOCKS=n BLOCK_SIZE=n   (from `stat -f`, preferred)
#
# When `stat -f` is unavailable the raw `df -Pk` table is printed instead and
# the Rust side parses it.
#
# When DIR does not exist (yet), the nearest existing ancestor is inspected
# instead, so the free-space figure and the "can be created" verdict stay
# meaningful before the first install.
#
# `ei` substitutes `__DIR__` with a shell-quoted path, so the script is
# compiled into the binary unchanged and can also be run by hand.

dir=${1:-__DIR__}

if [ -d "$dir" ]; then
  echo "EXISTS=yes"
  if [ -w "$dir" ] && [ -x "$dir" ]; then
    echo "WRITABLE=yes"
  fi
  probe="$dir"
else
  probe="$dir"
  while [ ! -e "$probe" ] && [ "$probe" != "/" ]; do
    probe=$(dirname "$probe")
  done
  if [ -d "$probe" ] && [ -w "$probe" ]; then
    echo "CREATABLE=yes"
  fi
fi

# The path actually measured. It differs from DIR when DIR does not exist:
# the nearest existing ancestor is used, and the caller should say so instead
# of blaming DIR for a full parent filesystem.
echo "PROBE=$probe"

# `stat -f` is unambiguous: "%b total blocks, %a blocks free, %S block size".
# Supported by both GNU coreutils and BusyBox. The multiplication happens on
# the Rust side so 32-bit ash on small routers cannot overflow.
space=$(stat -f -c '%b %a %S' "$probe" 2>/dev/null) || space=
if [ -n "$space" ]; then
  set -- $space
  if [ $# -eq 3 ]; then
    echo "TOTAL_BLOCKS=$1"
    echo "AVAIL_BLOCKS=$2"
    echo "BLOCK_SIZE=$3"
    exit 0
  fi
fi

# Fallback: `df -Pk` table. Emitted in full because BusyBox prints only the
# header when the path is missing, and `tail -n 1` would hide that.
df -Pk "$probe" 2>/dev/null
