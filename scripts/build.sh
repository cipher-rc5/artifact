#!/bin/bash
# Thin wrapper around `just`. All real recipes live in the justfile.
#   ./scripts/build.sh                # release build (just build)
#   ./scripts/build.sh dev            # cargo run
#   ./scripts/build.sh <recipe> [...] # any other just recipe
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v just >/dev/null 2>&1; then
    echo "error: 'just' is not installed. Run: brew install just" >&2
    exit 1
fi

if [ "$#" -eq 0 ]; then
    exec just build
fi

if [ "$1" = "dev" ]; then
    exec just run
fi

exec just "$@"
