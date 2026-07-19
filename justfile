default:
    @ {{just_executable()}} --list --justfile {{justfile()}} --unsorted

run_firmware:
    #!/usr/bin/env bash
    set -euo pipefail

    source ~/export-esp.sh

    cd firmware     # Need to inherit the `.cargo/config.toml`

    set -a
    source .env
    set +a

    # Erase the OTA data if we're flashing via USB
    espflash erase-parts otadata --partition-table partitions.csv

    cargo run -r --bin rustagon

deploy_firmware:
    #!/usr/bin/env bash
    set -euo pipefail

    source ~/export-esp.sh

    cd firmware     # Need to inherit the `.cargo/config.toml`

    set -a
    source .env
    set +a

    echo "Deploying version $FIRMWARE_VERSION"

    cargo build -r -p firmware --bin rustagon

    target_file_name=target/xtensa-esp32s3-none-elf/release/rustagon
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

build_wasm:
    #!/usr/bin/env bash
    set -euo pipefail

    SDK_DIR=$PWD/sdk

    rm -rf $SDK_DIR/../target/wasm32-unknown-unknown/release/*.wasm

    RUSTFLAGS="-C link-args=-z -C link-args=stack-size=32768 -Clink-arg=--initial-memory=65536 -C opt-level=z -C lto=true" cargo +nightly build -r -p sdk --target wasm32-unknown-unknown

    rm -rf $SDK_DIR/wasm/*.wsm

    cp -a $SDK_DIR/../target/wasm32-unknown-unknown/release/*.wasm $SDK_DIR/wasm/

    pushd $SDK_DIR/wasm
    for f in *.wasm; do [[ -f "$f" ]] && mv "$f" "${f%.wasm}.wsm"; done
    popd

    echo -e "\e[1mWASM binaries have been placed in /wasm folder"

emulate_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_wasm

    cargo run -r -p emulator $PWD/sdk/wasm/{{file}}.wsm

run_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_wasm

    cargo run -r -p uploader http://192.168.1.1/api/receive sdk/wasm/{{file}}.wsm

upload_wasm file:
    #!/usr/bin/env bash
    set -euo pipefail

    just build_wasm

    cargo run -r -p uploader http://192.168.1.1/api/file?{{file}}.wsm sdk/wasm/{{file}}.wsm
