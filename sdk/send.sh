#!/usr/bin/env bash
set -euo pipefail

program=$1

curl --data "@wasm/$1.wsm" --progress-bar http://192.168.1.1/api/receive
