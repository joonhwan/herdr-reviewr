#!/usr/bin/env bash
# Put back the released binary that the last `just qa-install` replaced. Called by `just qa-restore`.
#
# In a file rather than inline in the justfile for the reason recorded in scripts/install.sh: a
# shebang recipe drags in `cygpath`, a recipe line calling a script needs only `sh`.
set -euo pipefail

BIN="$(./scripts/qa-plugin-bin.sh)"
./scripts/swap-binary.sh "$BIN.release-backup" "$BIN"
echo "restored release binary at $BIN"
echo "next: close and reopen each reviewr pane with the toggle keybinding inside herdr."
