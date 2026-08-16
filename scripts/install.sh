#!/usr/bin/env bash
# Put an already-built release binary into bin/ for `herdr plugin link`. Called by `just install`,
# which builds first.
#
# This lives in a file rather than inline in the justfile because a shebang recipe makes just write
# a temp script and translate its path with `cygpath`, which Git for Windows keeps in `usr/bin` —
# off PATH in a default install, the same gap that `sh` has (docs/plans/2026-08-14-windows-support).
# A recipe line calling this script needs only `sh`, which the plugin already requires.
set -euo pipefail

exe=""
case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) exe=".exe" ;;
esac

NEW="target/release/herdr-reviewr$exe"
[ -f "$NEW" ] || {
  echo "install: build first (cargo build --release)" >&2
  exit 1
}

mkdir -p bin
# Never overwrite in place: a running pane holds the old image, and Windows refuses the write while
# macOS poisons its code-signing cache. scripts/swap-binary.sh is the one place that knows how.
./scripts/swap-binary.sh "$NEW" "bin/herdr-reviewr$exe"
