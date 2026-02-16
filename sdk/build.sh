#!/usr/bin/env bash
set -euo pipefail

HERE=$(cd "$(dirname "$BASH_SOURCE")"; cd -P "$(dirname "$(readlink "$BASH_SOURCE" || echo .)")"; pwd)

rm -rf $HERE/target/wasm32-unknown-unknown/release/*.wasm

cargo build -r

rm -rf $HERE/wasm/*.wsm

cp -a $HERE/target/wasm32-unknown-unknown/release/*.wasm wasm/

pushd $HERE/wasm
for f in *.wasm; do [[ -f "$f" ]] && mv "$f" "${f%.wasm}.wsm"; done
popd

echo -e "\e[1mWASM binaries have been placed in /wasm folder"
