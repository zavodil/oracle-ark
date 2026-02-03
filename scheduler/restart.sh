#!/bin/bash
set -e

VERBOSE=""
while [[ $# -gt 0 ]]; do
  case $1 in
    -v|--verbose)
      VERBOSE="-e RUST_LOG=oracle_scheduler=debug"
      shift
      ;;
    *)
      echo "Usage: $0 [-v|--verbose]"
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "Building new image..."
docker build -t oracle-scheduler -f scheduler/Dockerfile .

echo "Replacing container..."
docker rm -f oracle-scheduler 2>/dev/null || true
docker run -d \
  --name oracle-scheduler \
  --env-file scheduler/.env \
  $VERBOSE \
  --restart unless-stopped \
  oracle-scheduler

echo "Done. Logs:"
docker logs -f oracle-scheduler
