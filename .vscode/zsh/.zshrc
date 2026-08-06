# ZDOTDIR rc for the VS Code "nix develop" terminal profile
# (see .vscode/nix-dev-shell.sh). Sources the user's real zsh config so
# aliases/prompt behave normally, then re-asserts the nix devShell's PATH
# precedence.
#
# Why: $HOME/.zshrc re-prepends ~/.cargo/bin (rustup) and /opt/homebrew/bin,
# which would shadow the flake's cargo/just/rustc/deno and silently drop you
# onto the host rustup nightly instead of the pinned esp/stable toolchains.

if [ -f "$HOME/.zshrc" ]; then
  source "$HOME/.zshrc"
fi

# nix develop puts its /nix/store bins first; undo the re-prepend above by
# moving every /nix/* entry (store packages + the global nix profile) back to
# the front, preserving their existing relative order.
_nix_dirs=()
_other_dirs=()
for _p in ${(s.:.)PATH}; do
  if [[ $_p == /nix/* ]]; then
    _nix_dirs+=("$_p")
  else
    _other_dirs+=("$_p")
  fi
done
export PATH="${(j.:.)_nix_dirs}:${(j.:.)_other_dirs}"
unset _p _nix_dirs _other_dirs
