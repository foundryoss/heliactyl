#!/usr/bin/env bash
set -euo pipefail

# check_container.sh - check whether a Docker container (by name or ID) is running.
# Usage: ./check_container.sh [-w seconds] <container>
# Exit codes:
#   0 => container is running
#   1 => container exists but is not running
#   2 => docker CLI not found
#   3 => container not found

usage() {
  cat <<EOF
Usage: $0 [-w seconds] <container>

Checks whether the named Docker container is running. If -w is provided, the
script will retry until the container becomes running or the timeout expires.

Examples:
  $0 my-container
  $0 -w 30 my-container   # wait up to 30s for the container to start
EOF
  exit 1
}

WAIT=0
while getopts ":w:h" opt; do
  case "$opt" in
    w) WAIT="$OPTARG" ;;
    h) usage ;;
    *) usage ;;
  esac
done
shift $((OPTIND-1))

if [ $# -lt 1 ]; then
  usage
fi
CONTAINER="$1"

check_once() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker CLI not found. Install Docker Desktop (macOS) or Docker Engine (Linux)."
    return 2
  fi

  running=$(docker inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null || echo "not-found")
  if [ "$running" = "true" ]; then
    echo "Container '$CONTAINER' is running ✅"
    return 0
  elif [ "$running" = "false" ]; then
    echo "Container '$CONTAINER' exists but is not running."
    return 1
  else
    echo "Container '$CONTAINER' not found."
    return 3
  fi
}

if [ "$WAIT" -le 0 ]; then
  check_once
  exit $?
fi

end=$((SECONDS + WAIT))
while [ $SECONDS -le $end ]; do
  if check_once; then
    exit 0
  fi
  # If container doesn't exist, no point waiting
  last_status=$?
  if [ "$last_status" -eq 3 ]; then
    exit 3
  fi
  sleep 1
done

echo "Timed out waiting for container '$CONTAINER' to become running (waited $WAIT seconds)."
echo "If the container exists but is stopped, start it with: docker start $CONTAINER"
exit 1
