#!/usr/bin/env bash
set -x
set -eo pipefail

RUNNING_CONTAINER=$(docker ps --filter 'name=redis' --format '{{.ID}}')
if [[ -n $RUNNING_CONTAINER ]]; then
    # if redis container is running, print instructions to kill it and exit
    echo >&2 "  docker kill ${RUNNING_CONTAINER}"
    exit 1
fi

docker run \
    -p 6379:6379 \
    -d \
    --name "redis_$(date +%s)" \
    redis:7

echo >&2 "Redis is ready to go!"
