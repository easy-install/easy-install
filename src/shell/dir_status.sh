#!/bin/sh
# Inspect a directory on the host running this script.
#
# Usage: dir_status.sh [DIR]
#
# Prints KEY=value markers followed by one `df -Pk` line:
#   EXISTS=yes | WRITABLE=yes | CREATABLE=yes
#   filesystem 1024-blocks used available capacity mountpoint
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

df -Pk "$probe" 2>/dev/null | tail -n 1
