#!/bin/sh
set -Eeuo pipefail

dd if=/dev/zero of=fat.img bs=1K count=3136

mformat -S 5 -i fat.img ::
# mcopy -i fat.img ../fat/* ::
# mdir -i fat.img ::

dd if=fat.img of=web-flash-tool/merged.bin oseek=5056 count=3136 bs=1024

rm fat.img
