#!/usr/bin/env bash
# Swap a locally built herdr-reviewr into the GitHub-installed plugin for QA.
# The full procedure and every known failure mode: docs/qa-install.md.
set -euo pipefail

exe=""
case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) exe=".exe" ;;
esac

NEW="target/release/herdr-reviewr$exe"
[ -f "$NEW" ] || {
  echo "qa-install: build first (cargo build --release)" >&2
  exit 1
}

# The installed plugin's binary, resolved by asking herdr (scripts/qa-plugin-bin.sh). That script
# also refuses a linked checkout, which has no released binary to swap or roll back.
BIN="$("$(dirname "$0")/qa-plugin-bin.sh")"
[ -f "$BIN" ] || {
  echo "qa-install: no installed binary at $BIN" >&2
  exit 1
}

# Keep one pristine release for rollback. Never overwrite an existing backup.
[ -f "$BIN.release-backup" ] || cp "$BIN" "$BIN.release-backup"

# The replace-through-a-fresh-path swap lives in one place (scripts/swap-binary.sh): neither
# platform tolerates an overwrite in place — macOS SIGKILLs the reused inode, Windows refuses
# outright while a pane holds the file.
"$(dirname "$0")/swap-binary.sh" "$NEW" "$BIN"

# Prove the installed binary actually runs before touching any pane.
"$BIN" --resolve-plugin-config >/dev/null || {
  echo "qa-install: installed binary failed --resolve-plugin-config (exit $?)" >&2
  echo "qa-install: rolled-back copy available at $BIN.release-backup" >&2
  exit 1
}
echo "installed: $BIN"

# Running panes keep executing the old binary image. Only a pane restart picks this up.
if [ -n "$exe" ]; then
  # tasklist filters by image name only, so this reports every reviewr process on the machine,
  # including one launched from this checkout.
  live=$(tasklist //NH //FI "IMAGENAME eq herdr-reviewr.exe" 2>/dev/null | grep -c herdr-reviewr || true)
  if [ "${live:-0}" -gt 0 ]; then
    echo "note: $live running reviewr process(es) still use the OLD binary"
  fi
else
  live=$(pgrep -f "$BIN" || true)
  if [ -n "$live" ]; then
    echo "note: running reviewr panes still use the OLD binary (pids:" $live ")"
  fi
fi
echo "next: close and reopen each reviewr pane with the toggle keybinding inside herdr."
echo "rollback: just qa-restore"
