#!/usr/bin/env bash
# Replace an executable through a fresh path, then ad-hoc re-sign on macOS.
#
# The one home for this dance, and both platforms need it for opposite reasons. macOS accepts an
# in-place overwrite and then punishes it: the old inode survives, the code-signing cache calls
# the binary tampered with, and every launch SIGKILLs (exit 137, no log line). Windows refuses
# the overwrite instead — measured, a delete on a running executable returns access denied and a
# copy over it reports the file in use, while a rename still succeeds. So displace the old file
# and write the new one beside it. `just install`, `just qa-install`, and `just qa-restore` all
# route through here.
#
#   swap-binary.sh <src> <dst>
set -euo pipefail

src="$1"
dst="$2"
[ -f "$src" ] || {
  echo "swap-binary: missing source $src" >&2
  exit 1
}

rm -f "$dst".old-* 2>/dev/null || true # displaced by an earlier swap, freed once its pane closed
if [ -e "$dst" ] && ! mv "$dst" "$dst.old-$$"; then
  echo "swap-binary: cannot replace $dst" >&2
  exit 1
fi
cp "$src" "$dst.staging"
mv "$dst.staging" "$dst"
rm -f "$dst.old-$$" 2>/dev/null || true # stays only while a process still runs the old image
[ "$(uname)" = "Darwin" ] && codesign --force --sign - "$dst"
exit 0
