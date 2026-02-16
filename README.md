# Rustagon

A alternative firmware written in Rust for the Tildagon (the EMF Camp badge) that runs WebAssembly.

This firmware could easily be adapted to run on other ESP32-S3 based devices with PSRAM.

## TL;DR

A demo of the web app and emulator that runs on the badge can be found at:

https://demo.rustagon.chrisdell.info/fs

If you have a Tildagon, you can try the pre-compiled firmware by using the online install tool:

https://firmware.rustagon.chrisdell.info/

    ⚠️ This will erase all previous contents of the badge.

Once the firmware is installed, connect a phone or laptop to the WiFi network "Rustagon".

Your device should automatically open the web interface. If not, open a browser and navigate to:
http://192.168.1.1

Use the web app to add your WiFi credentials. Once saved and rebooted, the badge will connect to your WiFi network and display its assigned IP address.

## Quick Usage Guide

The buttons mostly follow the original Tildagon configuration.

    - A/D is menu up/down
    - C is select
    - "Boop" (labelled on the back of badge) will quit whatever app is currently running
    - "Bat" can be used to wake the badge up after it is powered off
    - All other buttons are use defined.

After installing some apps from the App Store, quit the App Store app (using "Boop") and find your downloaded apps in the "Files" section.

## Why WebAssembly on a Microcontroller?

Microcontrollers like the ESP32 lack an MMU, dynamic linking, and other features common in more costly “application-class” SoCs (like those in smartphones). This traditionally limits them to fixed, compiled firmware: no app stores, no dynamic updates.

The original Tildagon firmware used MicroPython to work around this, but as an interpreted high-level language, it comes with significant performance overhead.

WebAssembly (WASM) is more ideal for constrained devices due to be already being partially compiled. There is still a performance cost (due to being interpreted rather than JIT compiled) but it is much closer to native than MicroPython.

In addition, many languages can compile to WASM. So if you're not comfortable using Rust, you can potentially use C, C++, or even Swift, all without making any changes to the device's firmware.

## Components

### [Firmware](./firmware)

The firmware itself. Includes:

- WebAssembly runtime provided by `wasmi`
- Filesystem for storing WASM binary
- HTTP API to read/write files and control the badge
- Switch WiFi between AP Mode (default) and Station Mode
- Built-in web app for full device management usable on desktop/phone/tablet

### [Web App](./web)

The web app component which is bundled into the badge firmware.

- Remote control the badge (like VNC) using a WebSocket
- Manage files on the badge filesystem
- Configure WiFi: Toggle AP Mode, Add WiFi networks
- Emulator for WASM apps

### [SDK](./sdk)

A set of libraries and demos written in Rust which compile to WASM.

Additional SDK's for other languages coming soon.

### [Emulator](./emulator)

A command-line emulator (in addition to the web based one) which makes developing apps with the SDK much more convenient.
