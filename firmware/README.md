# Rustagon Firmware

## Building

You will need to install Rust and `cargo` for your platform. You also need the ESP toolchain installer and flashing tools:

    cargo install espup espflash
    
Install the toolchain:

    espup install
    
Then source the environment

    . ~/export-esp.sh

After that, you should be able to build and flash your badge with:

    cargo run -r --bin rustagon

(Make sure the `web` firmware has been built first as it is injected into the binary)
