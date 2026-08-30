#!/bin/sh
# Convenience wrapper: configure + build + run Mystical Arcana end-to-end.
#
# Usage:
#   ./scripts/build.sh            # configure + build only
#   ./scripts/build.sh run        # + run the game
#   ./scripts/build.sh gdb        # + run under gdb for stack traces
#   ./scripts/build.sh obs        # + run + curl the observatory for one frame

set -e

# Source environment
. "$(dirname "$0")/env.sh"

ROOT=/home/z/my-project/mystical-arcana
BUILD=$ROOT/build
BIN=$BUILD/mystical_arcana

# Configure
if [ ! -f "$BUILD/build.ninja" ]; then
    echo "==> cmake configure"
    mkdir -p "$BUILD"
    (cd "$BUILD" && env RUSTUP_HOME="$HOME/.rustup" CARGO_HOME="$HOME/.cargo" cmake -G Ninja \
        -DRust_RUSTUP=NOTFOUND \
        -DRust_COMPILER="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc" \
        -DRust_CARGO="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo" \
        ..)
fi

# Build
echo "==> ninja build"
(cd "$BUILD" && env RUSTUP_HOME="$HOME/.rustup" CARGO_HOME="$HOME/.cargo" ninja)

case "$1" in
    run)
        echo "==> running Mystical Arcana"
        exec "$BIN"
        ;;
    gdb)
        echo "==> running under gdb"
        exec gdb \
            -ex "set environment LD_LIBRARY_PATH $LD_LIBRARY_PATH" \
            -ex "set environment VK_ICD_FILENAMES $VK_ICD_FILENAMES" \
            -ex "set environment VK_LAYER_PATH $VK_LAYER_PATH" \
            -ex "run" \
            -ex "bt" \
            --args "$BIN"
        ;;
    obs)
        echo "==> running + curl observatory"
        "$BIN" &
        PID=$!
        sleep 2
        echo "---- /debug/state ----"
        curl -s http://localhost:8080/debug/state || true
        echo ""
        echo "---- /frame.png ----"
        curl -s -o /tmp/frame.png http://localhost:8080/frame.png || true
        file /tmp/frame.png || true
        wait $PID
        ;;
    "" )
        echo "==> build complete. Run with: $0 run|gdb|obs"
        ;;
esac
