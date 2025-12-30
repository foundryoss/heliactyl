#!/usr/bin/env bash
set -euo pipefail

# check_docker.sh - simple helper to test whether the Docker daemon is running.
# Usage: ./check_docker.sh [-w seconds]
# Returns:
#   0 => Docker is running
#   1 => Docker CLI found but daemon not running
#   2 => Docker CLI not found

usage() {
  cat <<EOF
Usage: $0 [-w seconds]

Checks whether the Docker daemon is running. If -w is provided, the script
will retry for up to <seconds> waiting for Docker to become available.

Examples:
  $0            # quick check
  $0 -w 30      # wait up to 30 seconds for Docker to start
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

check_once() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker CLI not found. Install Docker Desktop (macOS) or the Docker Engine (Linux)."
    return 2
  fi

  if docker info >/dev/null 2>&1; then
    echo "Docker is running ✅"
    return 0
  else
    echo "Docker CLI found but daemon is not responding."
    return 1
  fi
}

if [ "$WAIT" -le 0 ]; then
  check_once
  exit $?
fi

# Wait-and-retry mode
end=$((SECONDS + WAIT))
while [ $SECONDS -le $end ]; do
  if check_once; then
    exit 0
  fi
  sleep 1
  # Show a small progress indicator every 5 seconds
  if (( SECONDS % 5 == 0 )); then
    remaining=$(( end - SECONDS ))
    printf "Waiting for Docker... %ds remaining\r" "$remaining"
  fi
done

echo "\nTimed out waiting for Docker (waited $WAIT seconds)."
if command -v docker >/dev/null 2>&1; then
  echo "On macOS, try: open -a Docker"
  echo "On Linux, try: sudo systemctl start docker"
fi
exit 1
