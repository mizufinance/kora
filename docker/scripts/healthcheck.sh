#!/bin/bash
set -e

MODE="${HEALTHCHECK_MODE:-p2p}"

case "$MODE" in
    dkg)
        [[ -f "/data/share.key" && -f "/data/output.json" ]]
        ;;
    p2p)
        nc -z localhost 30303
        ;;
    ready)
        [[ -f "/data/.ready" ]] && nc -z localhost 30303
        ;;
    *)
        exit 1
        ;;
esac
