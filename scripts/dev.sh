#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
CACHE_ROOT="$REPO_ROOT/.cache"

mkdir -p "$CACHE_ROOT"

export CARGO_HOME="$CACHE_ROOT/cargo-home"
export CARGO_TARGET_DIR="$CACHE_ROOT/cargo-target"
export X360CE_APP_DATA_DIR="$CACHE_ROOT/app-data"

COMMIT="dev"
if command -v git >/dev/null 2>&1; then
    COMMIT=$(git -C "$REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || printf 'dev')
fi

export X360CE_DEV_VERSION="v0.0.0-$COMMIT"

echo "x360ce dev cache: $CACHE_ROOT"
echo "x360ce dev version: $X360CE_DEV_VERSION"

cd "$REPO_ROOT"
exec cargo run --bin x360ce -- "$@"
