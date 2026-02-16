# SDK

## Quick Start

You will need the `wasm32-unknown-unknown` target installed:

    rustup target add wasm32-unknown-unknown

Build all examples:

    ./build.sh
    
WASM binaries will be placed in `/wasm`.

Run a binary with the emulator:

    cargo run -r --bin cube

NOTE: Emulator (in `../emulator` directory) needs to be built first.

All examples are stored within `src/bin`.
