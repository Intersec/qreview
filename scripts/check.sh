#!/bin/sh
# The gate. One line per step, quiet until a step fails.
#
# A failing step prints its whole output, because a formatter failure carries
# no word you can grep for. `make check V=1` streams everything instead.
set -u

root=$(dirname "$0")/..
cd "$root" || exit 1

verbose=${V:-0}
failed=0

step() {
    name=$1
    dir=$2
    shift 2
    if [ "$verbose" = 1 ]; then
        printf '=== %s\n' "$name"
        if (cd "$dir" && "$@"); then
            printf 'ok    %s\n' "$name"
        else
            printf 'FAIL  %s\n' "$name"
            failed=1
        fi
        return
    fi
    if out=$(cd "$dir" && "$@" 2>&1); then
        printf 'ok    %s\n' "$name"
    else
        printf 'FAIL  %s\n' "$name"
        printf '%s\n' "$out"
        failed=1
    fi
}

if [ ! -d web/node_modules ]; then
    echo "web/node_modules is missing. Run: make setup"
    exit 1
fi

# The interface is built before the Rust steps: the binary embeds web/dist, so
# nothing on the Rust side compiles until that directory exists.
step "eslint"          web npm run --silent lint
step "prettier"        web npm run --silent format:check
step "vitest"          web npm run --silent test:run
step "vite build"      web npm run --silent build
step "cargo fmt"       .   cargo fmt --all -- --check
step "cargo clippy"    .   cargo clippy --all-targets -- -D warnings
step "cargo test"      .   cargo test --workspace
step "cargo build"     .   cargo build --release

exit $failed
