default:
    @ {{just_executable()}} --list --justfile {{justfile()}} --unsorted

# ============================================================
# Helpers
# ============================================================

bold text:
    @printf "\033[1m{{text}}\033[0m\n"

red text:
    @printf "\033[31m{{text}}\033[0m\n"

green text:
    @printf "\033[32m{{text}}\033[0m\n"

# ============================================================
# Firmware
# ============================================================

# Build firmware
build_firmware:
    #!/usr/bin/env bash
    set -euo pipefail

    source ~/export-esp.sh

    cd firmware

    set -a
    source .env
    set +a

    cargo build --profile release-lto --bin rustagon

# Build and flash firmware via USB
run_firmware:
    #!/usr/bin/env bash
    set -euo pipefail

    source ~/export-esp.sh

    cd firmware

    set -a
    source .env
    set +a

    espflash erase-parts otadata --partition-table partitions.csv

    cargo run --profile release-lto --bin rustagon

# Build firmware and deploy to remote server
deploy_firmware:
    #!/usr/bin/env bash
    set -euo pipefail

    source ~/export-esp.sh

    cd firmware

    set -a
    source .env
    set +a

    echo "Deploying version $FIRMWARE_VERSION"

    cargo build --profile release-lto -p firmware --bin rustagon

    target_file_name=../target/xtensa-esp32s3-none-elf/release-lto/rustagon
    dest_file_name=firmware.bin
    merged_file_name=web-flash-tool/merged.bin

    espflash save-image --chip esp32s3 --flash-size 8mb $target_file_name $dest_file_name

    espflash save-image --chip esp32s3 --flash-size 8mb --partition-table partitions.csv --merge $target_file_name $merged_file_name

    # Inject an empty FAT filesystem because formatting on device takes ages...
    # ./inject-fat.sh

    dd if=/dev/zero of=fat.img bs=1K count=3136
    mformat -S 5 -i fat.img ::
    # mcopy -i fat.img ../fat/* ::
    # mdir -i fat.img ::

    dd if=fat.img of=web-flash-tool/merged.bin oseek=5056 count=3136 bs=1024
    rm fat.img

    file_size=$(wc -c < "$dest_file_name")
    echo size=$file_size

    echo "{\"version\":$FIRMWARE_VERSION,\"size\":$file_size}" | ssh 192.168.49.1 "cat > /srv/rustagon/firmware/version.json"

    scp $dest_file_name 192.168.49.1:/srv/rustagon/firmware
    scp -r web-flash-tool/* 192.168.49.1:/srv/rustagon/firmware

# ============================================================
# Desktop
# ============================================================

# Build the desktop emulator
build_desktop:
    #!/usr/bin/env bash
    set -euo pipefail

    set -a
    source firmware/.env
    set +a

    cargo build -r -p desktop

# Build and run the desktop emulator (pick a data dir with fzf)
run_desktop:
    #!/usr/bin/env bash
    set -euo pipefail

    data_dir=$( { printf '%s\n' "$PWD/desktop/data" "$PWD/sdk/wasm"; \
                  find "$PWD/desktop/data" -maxdepth 1 -mindepth 1 -type d | sort; } \
                | fzf --prompt="Data dir> " --height=~10 --header="Select data directory" ) \
      || data_dir="$PWD/desktop/data"

    set -a
    source firmware/.env
    set +a

    cargo run -r -p desktop -- "$data_dir"

# Run the desktop emulator, auto-starting a WASM app from sdk/wasm
run_desktop_app file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_wasm {{file}}

    set -a
    source firmware/.env
    set +a

    cargo run -r -p desktop -- sdk/wasm/{{file}}.wsm

# ============================================================
# WASM SDK
# ============================================================

# Build all WASM programs
build_sdk:
    #!/usr/bin/env bash
    set -euo pipefail

    rm -rf target/wasm32-unknown-unknown/release-lto/*.wasm

    RUSTFLAGS="-C link-args=-z -C link-args=stack-size=32768 -Clink-arg=--initial-memory=65536 -C opt-level=z -C lto=true -C strip=symbols" cargo +nightly build --profile release-lto -p sdk --target wasm32-unknown-unknown

    rm -rf sdk/wasm/*.wsm

    cp -a target/wasm32-unknown-unknown/release-lto/*.wasm sdk/wasm/

    pushd sdk/wasm
    for f in *.wasm; do [[ -f "$f" ]] && mv "$f" "${f%.wasm}.wsm"; done
    popd

    just build_manifest

    just bold "WASM binaries have been placed in /wasm folder"

build_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    RUSTFLAGS="-C link-args=-z -C link-args=stack-size=32768 -Clink-arg=--initial-memory=65536 -C opt-level=z -C lto=true -C strip=symbols" cargo +nightly build --profile release-lto -p sdk --target wasm32-unknown-unknown --bin {{file}}

    cp target/wasm32-unknown-unknown/release-lto/{{file}}.wasm sdk/wasm/{{file}}.wsm

    just build_manifest

    just bold "WASM binary {{file}}.wsm built"

# Generate the WASM manifest.json from the built .wsm files
build_manifest:
    #!/usr/bin/env bash
    set -euo pipefail

    cargo run -q -p manifest-tool -- $PWD/sdk/wasm

# Build and deploy WASM apps to the remote server
deploy_sdk:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_sdk

    rm -rf web/public/wasm/*
    cp -rv sdk/wasm/* web/public/wasm/

    ssh 192.168.49.1 "rm -f /srv/rustagon/apps/*.wsm /srv/rustagon/apps/manifest.json"
    scp -r sdk/wasm/* 192.168.49.1:/srv/rustagon/apps

# Build and emulate a WASM app locally
emulate_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_sdk

    cargo run -r -p emulator $PWD/sdk/wasm/{{file}}.wsm

# Build and upload WASM to device via HTTP
run_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_sdk

    cargo run -r -p uploader http://rustagon.local/api/receive sdk/wasm/{{file}}.wsm

# Build and upload WASM as a file
upload_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_sdk

    cargo run -r -p uploader http://rustagon.local/api/file?{{file}}.wsm sdk/wasm/{{file}}.wsm

# ============================================================
# Web App
# ============================================================

# Build and deploy the web frontend to the remote server
deploy_web:
    #!/usr/bin/env bash
    set -euo pipefail

    cd web
    deno task build
    deno task compress

    scp -r dist/* 192.168.49.1:/srv/rustagon/demo

# ============================================================
# Combined
# ============================================================

# Deploy firmware, SDK, and web app to the remote server
deploy: deploy_firmware deploy_sdk deploy_web
