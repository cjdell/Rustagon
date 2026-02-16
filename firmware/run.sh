#!/bin/bash
set -euo pipefail

source ~/export-esp.sh

set -a
source .env
set +a

# Erase the OTA data if we're flashing via USB
espflash erase-parts otadata --partition-table partitions.csv

cargo run -r --bin rustagon
