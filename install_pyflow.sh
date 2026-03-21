#!/bin/sh
set -eu

# This file installs the app into the user's home directory and makes sure it is on PATH.

NAME_UPPER="Pyflow"
NAME="pyflow"

APP_DIR="$HOME/$NAME"
BIN_DIR="$HOME/.local/bin"
TARGET_PATH="$BIN_DIR/$NAME"

if [ ! -f "./$NAME" ]; then
  printf "Error: expected executable ./%s in the current directory.\n" "$NAME" >&2
  exit 1
fi

chmod +x "./$NAME"

mkdir -p "$APP_DIR"
mkdir -p "$BIN_DIR"

cp "./$NAME" "$APP_DIR/$NAME"
cp "./$NAME" "$TARGET_PATH"
chmod +x "$APP_DIR/$NAME" "$TARGET_PATH"

add_path_line() {
  RC_FILE="$1"
  LINE='export PATH="$HOME/.local/bin:$PATH"'

  if [ -f "$RC_FILE" ]; then
    if grep -Fqx "$LINE" "$RC_FILE"; then
      return
    fi
  fi

  {
    printf '\n# Added by Pyflow installer\n'
    printf '%s\n' "$LINE"
  } >> "$RC_FILE"
}

add_path_line "$HOME/.profile"

if [ -n "${BASH_VERSION:-}" ]; then
  add_path_line "$HOME/.bashrc"
fi

if [ -n "${ZSH_VERSION:-}" ]; then
  add_path_line "$HOME/.zshrc"
fi

if [ -f "$HOME/.bashrc" ]; then
  add_path_line "$HOME/.bashrc"
fi

if [ -f "$HOME/.zshrc" ]; then
  add_path_line "$HOME/.zshrc"
fi

case ":$PATH:" in
  *":$BIN_DIR:"*)
    PATH_ALREADY_SET="yes"
    ;;
  *)
    PATH_ALREADY_SET="no"
    ;;
esac

printf "\nInstalled %s to:\n  %s\n" "$NAME_UPPER" "$APP_DIR/$NAME"
printf "\nCommand installed to:\n  %s\n" "$TARGET_PATH"

if [ "$PATH_ALREADY_SET" = "yes" ]; then
  printf "\n%s is already available on PATH in this shell.\n" "$NAME"
  printf "You can launch it now with:\n  %s\n" "$NAME"
else
  printf "\nAdded %s to your shell startup files.\n" "$BIN_DIR"
  printf "After restarting your terminal, you can launch it with:\n  %s\n" "$NAME"
  printf "\nTo use it immediately in this shell, run:\n  export PATH=\"%s:\$PATH\"\n" "$BIN_DIR"
fi