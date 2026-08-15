#!/usr/bin/env bash
# Print the path of the herdr-installed reviewr binary: the one file `just qa-install` swaps and
# `just qa-restore` puts back. Both route through here, and nothing else resolves that path.
#
# Ask herdr rather than glob a hash-suffixed directory. The glob only ever knew the unix layout
# (`~/.config/herdr/...`); herdr keeps the same tree under `%APPDATA%` on Windows, and it reports
# the root as an extended-length path (`\\?\C:\...`) that no POSIX tool opens.
set -euo pipefail

ID="persiyanov.reviewr"

json="$(herdr plugin list --json --plugin "$ID" 2>/dev/null)" || {
  echo "qa: cannot reach herdr — is it running?" >&2
  exit 1
}
root="$(printf '%s' "$json" | jq -r '.result.plugins[0].plugin_root // empty' 2>/dev/null)"
kind="$(printf '%s' "$json" | jq -r '.result.plugins[0].source.kind // empty' 2>/dev/null)"

if [ -z "$root" ]; then
  echo "qa: $ID is not installed — 'herdr plugin install persiyanov/herdr-reviewr'" >&2
  exit 1
fi
# A linked checkout runs the binary this repo builds, so there is nothing to swap into it and a
# backup would shadow the developer's own build. `just install` is the command for that setup.
if [ "$kind" = "local" ]; then
  echo "qa: $ID is linked from a local checkout, not installed — use 'just install' instead" >&2
  exit 1
fi

exe=""
case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*)
  exe=".exe"
  root="${root#'\\?\'}"
  root="$(cygpath -u "$root")"
  ;;
esac

printf '%s\n' "$root/bin/herdr-reviewr$exe"
