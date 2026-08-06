{
  description = "Rustagon dev shell: ESP32-S3 (xtensa) Rust toolchain + WASM SDK stable + desktop/web tooling";

  # ============================================================
  # Inputs
  # ============================================================
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      lib = nixpkgs.lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      # esp-rs/rust-build prebuilt Rust fork. The esp-rs/rust fork is built with
      # `--release-channel=nightly`, which is why the firmware's
      # `[unstable] build-std = ["alloc", "core"]` (firmware/.cargo/config.toml)
      # and esp-alloc's `nightly` feature work on it. Version must be >= 1.95
      # (esp-alloc 0.10 on git main pins rust-version = "1.95.0").
      espRustVersion = "1.95.0.0";

      # espressif/crosstool-NG unified Xtensa GCC toolchain (ships
      # `xtensa-esp-elf-gcc`; we alias the per-chip names the rust target spec
      # expects, see espGccAliases below).
      espGccVersion = "15.2.0_20250920";

      perSystem =
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustSystem =
            {
              x86_64-linux = "x86_64-unknown-linux-gnu";
              aarch64-linux = "aarch64-unknown-linux-gnu";
              aarch64-darwin = "aarch64-apple-darwin";
            }
            .${system};

          # sha256 of the *tarball file* (from the GitHub release API digest).
          rustSha256 =
            {
              x86_64-linux = "aad2fb24baeab6ad61c41f002f136fe0b416ef39a501d29750ab66c18a699433";
              aarch64-linux = "0c7d88e6805f9b77a048f307fcb1df0c65863a306c278123dc771a9cb6d2844c";
              aarch64-darwin = "543add96e452cc598d39d1dfdf0f2aff6cfdc54ae56ce2e63ad157c6778b5d2b";
            }
            .${system};
          rustSrcSha256 = "708bee337ac2d41c0e861af93047bffec91a052fbcb4957925d840a41f327717";

          gccSystem =
            {
              x86_64-linux = "x86_64-linux-gnu";
              aarch64-linux = "aarch64-linux-gnu";
              aarch64-darwin = "aarch64-apple-darwin";
            }
            .${system};

          # Unpacked-content hashes (nix-prefetch-url --unpack) — verified in
          # michalrus/esp-rust-nix-sandbox for the same release.
          gccSha256 =
            {
              x86_64-linux = "sha256-TMjkfwsm9xwPYIowTrOgU+/Cst5uKV0xJH8sbxcTIlc=";
              aarch64-linux = "sha256-SL3wIxnkcYJw04A9J1GTmpLvlE1iby5HdtLYmFwRaps=";
              aarch64-darwin = "sha256-O0gXFHa127y5hzwRJeXcvs3ZtF2eK93YJcwut9P9gok=";
            }
            .${system};

          # ============================================================
          # ESP32-S3 Rust compiler fork (prebuilt from esp-rs/rust-build)
          # ============================================================
          espRust = pkgs.stdenv.mkDerivation {
            pname = "esp-rust";
            version = espRustVersion;

            src = pkgs.fetchurl {
              name = "rust-${espRustVersion}-${rustSystem}.tar.xz";
              url = "https://github.com/esp-rs/rust-build/releases/download/v${espRustVersion}/rust-${espRustVersion}-${rustSystem}.tar.xz";
              sha256 = rustSha256;
            };
            rustSrcSrc = pkgs.fetchurl {
              name = "rust-src-${espRustVersion}.tar.xz";
              url = "https://github.com/esp-rs/rust-build/releases/download/v${espRustVersion}/rust-src-${espRustVersion}.tar.xz";
              sha256 = rustSrcSha256;
            };

            nativeBuildInputs = [
              pkgs.makeWrapper
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              pkgs.autoPatchelfHook
              pkgs.pkg-config
            ]
            ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.darwin.autoSignDarwinBinariesHook ];
            buildInputs = lib.optionals pkgs.stdenv.isLinux [
              pkgs.stdenv.cc.cc
              pkgs.zlib
            ];

            installPhase = ''
              runHook preInstall

              mkdir -p $out
              tar -xJf $src -C $out --strip-components=1
              # rustup-style installer; installs rustc/rustdoc/cargo/rustfmt/clippy
              # plus rust-std for the host and the Xtensa targets. Invoke via
              # `bash` — the script's `#!/usr/bin/env bash` shebang doesn't work
              # inside the Nix build sandbox (no /usr/bin/env).
              (cd $out && bash install.sh --destdir=$out --prefix= --disable-ldconfig)
              chmod -R +w $out

              # rust-src ships as a separate artifact; needed for -Zbuild-std.
              # The tarball nests a `rust-src` dir inside `rust-src-nightly/`,
              # hence strip-components=2 to land at lib/rustlib/src directly.
              mkdir -p rust-src
              tar -xJf $rustSrcSrc -C rust-src --strip-components=2
              mkdir -p $out/lib/rustlib
              cp -r rust-src/lib/rustlib/src $out/lib/rustlib/

              # Expose the fork's cargo under a non-conflicting name; the `cargo`
              # shim on PATH (see below) dispatches to it for all non-`+stable`
              # invocations.
              mv $out/bin/cargo $out/bin/cargo-esp

              # The prebuilt binaries are nightly-channel (Xtensa fork); make sure
              # they can find the ESP GCC (target linker, via the per-chip
              # dynconfig wrappers) and a host C compiler (build scripts / host
              # codegen).
              for exe in $out/bin/rustc $out/bin/rustdoc $out/bin/cargo-esp; do
                wrapProgram "$exe" --prefix PATH : ${
                  lib.makeBinPath [
                    espGccAliases
                    espGcc
                    pkgs.stdenv.cc
                  ]
                }
              done

              runHook postInstall
            '';

            # Stripping on darwin destroys the .rmeta sections in the bundled rlibs.
            dontStrip = pkgs.stdenv.isDarwin;
            # Skip nixpkgs' autoSignDarwinBinariesHook. The esp-rs prebuilt
            # binaries are already ad-hoc code-signed and dontStrip preserves
            # those signatures, so re-signing is redundant. The hook spawns a
            # `sigtool` subprocess per file — 106k files in this tarball makes
            # fixupPhase look like a hang on macOS (~20-40 min of churning,
            # zero output).
            darwinDontCodeSign = pkgs.stdenv.isDarwin;
            meta.mainProgram = "rustc";
          };

          # ============================================================
          # Unified Xtensa GCC toolchain
          # ============================================================
          # NOTE: the unified toolchain defaults to a *big-endian* generic Xtensa
          # core. The per-chip (little-endian) configs are selected via the
          # dynconfig mechanism — see espGccAliases below.
          espGcc = pkgs.stdenv.mkDerivation {
            pname = "esp-gcc-xtensa";
            version = espGccVersion;

            src = pkgs.fetchzip {
              name = "xtensa-esp-elf-${espGccVersion}-${gccSystem}";
              url = "https://github.com/espressif/crosstool-NG/releases/download/esp-${espGccVersion}/xtensa-esp-elf-${espGccVersion}-${gccSystem}.tar.xz";
              hash = gccSha256;
            };

            nativeBuildInputs =
              lib.optionals pkgs.stdenv.isLinux [
                pkgs.autoPatchelfHook
                pkgs.pkg-config
              ]
              ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.darwin.autoSignDarwinBinariesHook ];
            buildInputs = lib.optionals pkgs.stdenv.isLinux [
              pkgs.stdenv.cc.cc
              pkgs.zlib
            ];

            installPhase = ''
              cp -r . $out
            '';
            # NOTE: no `darwinDontCodeSign` here. On Apple Silicon the
            # toolchain's `ld` must dlopen the `xtensa_esp32s3.so` dynconfig
            # library, and that only works when the upstream ad-hoc signatures
            # are re-signed so `ld` and the `.so` share a team ID. Skipping the
            # hook makes every firmware link fail with "different Team IDs".
            # Only ~2.6k files, so signing is a few seconds (unlike espRust).
          };

          # Per-chip `xtensa-esp32s3-elf-*` aliases for the unified toolchain.
          # The rust target spec links via `xtensa-esp32s3-elf-gcc`; without
          # selecting the chip config the final link fails with cross-endian
          # errors (the prebuilt little-endian esp-wifi blobs can't merge into
          # the toolchain's big-endian default target). The compiler drivers
          # therefore pass `-mdynconfig=xtensa_esp32s3.so` and point
          # XTENSA_GNU_CONFIG at the chip config library (the driver only sets
          # it to the bare filename, which dlopen can't resolve from the Nix
          # store without LD_LIBRARY_PATH).
          espGccAliases =
            pkgs.runCommand "esp-gcc-xtensa-aliases"
              {
                nativeBuildInputs = [ pkgs.makeWrapper ];
              }
              ''
                mkdir -p $out/bin
                for f in ${espGcc}/bin/xtensa-esp-elf-*; do
                  [ -x "$f" ] || continue
                  b="$(basename "$f")"
                  ln -s "$f" "$out/bin/xtensa-esp32s3-elf-''${b#xtensa-esp-elf-}"
                done
                for driver in gcc cc g++ c++; do
                  if [ -e "$out/bin/xtensa-esp32s3-elf-$driver" ]; then
                    rm "$out/bin/xtensa-esp32s3-elf-$driver"
                    makeWrapper "${espGcc}/bin/xtensa-esp-elf-$driver" "$out/bin/xtensa-esp32s3-elf-$driver" \
                      --set XTENSA_GNU_CONFIG "${espGcc}/lib/xtensa_esp32s3.so" \
                      --add-flags "-mdynconfig=xtensa_esp32s3.so"
                  fi
                done
              '';

          # ============================================================
          # Stock stable for the WASM SDK
          # ============================================================
          # The SDK uses no nightly features, but still needs a toolchain with
          # the wasm32-unknown-unknown target std (the ESP fork only ships
          # Xtensa targets). `stable.latest` is resolved from the locked
          # rust-overlay revision, so it stays reproducible.
          stable = pkgs.rust-bin.fromRustupToolchainFile (
            pkgs.writeText "rust-toolchain-stable.toml" ''
              [toolchain]
              channel = "stable"
              components = ["rust-src"]
              targets = ["wasm32-unknown-unknown"]
            ''
          );

          # `cargo` shim: `cargo +stable ...` → stock stable (WASM SDK);
          # everything else → the ESP toolchain's cargo (firmware/desktop/tools).
          # Each branch pins RUSTC/RUSTDOC to its own toolchain: cargo resolves
          # rustc via $RUSTC/PATH, and the shell's rustc is the ESP fork, which
          # has no wasm32-unknown-unknown std.
          cargo = pkgs.writeShellScriptBin "cargo" ''
            if [[ "$1" == "+stable" ]]; then
              shift
              export RUSTC="${stable}/bin/rustc"
              export RUSTDOC="${stable}/bin/rustdoc"
              exec "${stable}/bin/cargo" "$@"
            fi
            export RUSTC="${espRust}/bin/rustc"
            export RUSTDOC="${espRust}/bin/rustdoc"
            exec "${espRust}/bin/cargo-esp" "$@"
          '';

          x11Libs = with pkgs; [
            libxkbcommon
            xorg.libX11
            xorg.libXrandr
            xorg.libXinerama
            xorg.libXcursor
          ];

          shellPackages = [
            cargo
            espRust # rustc / rustdoc / rustfmt / clippy (esp fork)
            espGcc # raw unified xtensa-esp-elf-* tools
            espGccAliases # per-chip xtensa-esp32s3-elf-* names (dynconfig wrappers)
            pkgs.rust-analyzer
            pkgs.espflash
            pkgs.picocom
            pkgs.just
            pkgs.fzf
            pkgs.mtools # mformat (deploy_firmware FAT image)
            pkgs.openssh # ssh / scp (deploy recipes)
            pkgs.deno # web app
            pkgs.wasm-tools # WASM size audits
            pkgs.pkg-config
            pkgs.git
            pkgs.nixpkgs-fmt
            # Loads the devShell env into the IDE/shell via `.envrc` (see
            # https://direnv.net). Keep in sync with the brew-installed copy.
            pkgs.direnv
          ]
          ++ lib.optionals pkgs.stdenv.isLinux x11Libs;
        in
        {
          devShell = pkgs.mkShell {
            name = "rustagon-devshell";

            packages = shellPackages;

            env = {
              RUST_SRC_PATH = "${espRust}/lib/rustlib/src/rust/library";
              ESPFLASH_SKIP_UPDATE_CHECK = "true";
              # Lets the unified xtensa-esp-elf tools (as/ld invoked directly, or
              # cc1/collect2 subprocesses) locate the ESP32-S3 dynconfig library.
              XTENSA_GNU_CONFIG = "${espGcc}/lib/xtensa_esp32s3.so";
            }
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              # minifb (desktop emulator) dlopens X11/xkbcommon at runtime;
              # needed on NixOS where they are not in the system loader path.
              LD_LIBRARY_PATH = lib.makeLibraryPath x11Libs;
            };

            shellHook = ''
              echo "Rustagon dev shell"
              echo "  firmware : just build_firmware   (esp rustc ${espRustVersion} + xtensa gcc)"
              echo "  sdk      : just build_sdk        (cargo +stable → stock stable)"
              echo "  desktop  : just build_desktop"
              rustc --version
            '';
          };

          # Exposed individually so they can be built/tested on their own
          # (`nix build .#esp-rust`, `nix build .#esp-gcc-xtensa`, ...).
          inherit
            espRust
            espGcc
            espGccAliases
            stable
            cargo
            ;
        };
    in
    {
      devShells = lib.genAttrs systems (system: {
        default = (perSystem system).devShell;
      });
      packages = lib.genAttrs systems (system: {
        inherit (perSystem system)
          espRust
          espGcc
          espGccAliases
          stable
          cargo
          ;
      });
    };
}
