#!/bin/bash
set -e

# run from test/
SRC=..
REPO=go
BOOTSTRAP="$(pwd)/bootstrap-go"

# build slug
cargo build --release --manifest-path "$SRC/Cargo.toml"
SLUG="$(readlink -f "$SRC/target/release/slug")"

# clone go
[ -d "$REPO/.git" ] || git clone https://go.googlesource.com/go "$REPO"

# fetch bootstrap because 2026 go won't build 2019 go
if [ ! -x "$BOOTSTRAP/go/bin/go" ]; then
    mkdir -p "$BOOTSTRAP"
    curl -sL https://go.dev/dl/go1.12.17.linux-amd64.tar.gz | tar -xz -C "$BOOTSTRAP"
fi
export GOROOT_BOOTSTRAP="$BOOTSTRAP/go"

cd "$REPO"
git fetch --tags -q

# use slug basic setup
"$SLUG" setup || true

# wipe any data from previous run
"$SLUG" clean || true

# Walk to regression
# 29bc4f1258 - regression
# e5f6e2d1c8 - fix
for commit in $(git log --format=%H -n 22 e5f6e2d1c8 -- src/encoding/json | tac); do
    echo "=== $commit ==="
    git checkout -q --detach "$commit"
    ( cd src && ./make.bash >/dev/null ) || { echo "build failed, skipping"; continue; }
    ./bin/go test -run='^$' -bench=BenchmarkCodeDecoder -benchtime=2s encoding/json > /tmp/bench.txt
    "$SLUG" -f /tmp/bench.txt -t go-testing@1.26.4 --local --record || true
done
