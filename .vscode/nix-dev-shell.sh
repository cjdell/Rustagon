#!/usr/bin/env bash
# Default shell for the VS Code integrated terminal: drops straight into the
# Rustagon flake devShell so `nix develop` never has to be typed by hand.
#
# Used as the `path` of the "nix develop" terminal profile in .vscode/settings.json.

# The `nix` binary is not on the minimal PATH VS Code gives to terminals
# (e.g. /usr/bin:/bin) — prepend the standard install location (same as .envrc).
PATH="/nix/var/nix/profiles/default/bin:$PATH"
export PATH

# Point zsh at a project-local rc (see .vscode/zsh/.zshrc) so it can re-assert
# the devShell's PATH after $HOME/.zshrc prepends ~/.cargo/bin and
# /opt/homebrew/bin (which would shadow the flake's cargo/just/rustc).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$(basename "${SHELL:-/bin/sh}")" = "zsh" ]; then
  export ZDOTDIR="$SCRIPT_DIR/zsh"
fi

# `nix develop --command <shell>` runs the shell attached to the terminal, so
# it behaves as a normal interactive shell (sources .zshrc/.bashrc, runs the
# flake's shellHook) but with the devShell's toolchain on PATH.
exec nix develop --command "${SHELL:-/bin/sh}"
