#!/bin/bash
set -euo pipefail

source ~/export-esp.sh

set -a
source .env
set +a

echo "Deploying version $FIRMWARE_VERSION"

cargo build -r --bin rustagon

target_file_name=target/xtensa-esp32s3-none-elf/release/rustagon
dest_file_name=firmware.bin
merged_file_name=web-flash-tool/merged.bin

espflash save-image --chip esp32s3 --flash-size 8mb $target_file_name $dest_file_name

espflash save-image --chip esp32s3 --flash-size 8mb --partition-table partitions.csv --merge $target_file_name $merged_file_name

# Inject an empty FAT filesystem because formatting on device takes ages...
./inject-fat.sh

file_size=$(wc -c < "$dest_file_name")

echo size=$file_size

echo "{\"version\":$FIRMWARE_VERSION,\"size\":$file_size}" | ssh 192.168.49.1 "cat > /srv/rustagon/firmware/version.json"

scp $dest_file_name 192.168.49.1:/srv/rustagon/firmware

scp -r web-flash-tool/* 192.168.49.1:/srv/rustagon/firmware
