#!/usr/bin/env bash
# herdr `[[build]]` step: download the prebuilt herdr-reviewr binary for this platform from the
# matching GitHub Release into the plugin's bin/ dir. Runs on `herdr plugin install` (a managed
# checkout); `herdr plugin link` skips the build step — for a local checkout, build from source
# with `cargo install --path .`.
#
# The build runs with the plugin checkout as the working directory, so we resolve the plugin root
# from this script's location rather than $HERDR_PLUGIN_ROOT (build commands may not receive the
# runtime env). At runtime the pane command reads $HERDR_PLUGIN_ROOT/bin/herdr-reviewr.
set -euo pipefail

NAME="herdr-reviewr"
# Fork build: the release assets live on this fork, not upstream, until the Windows work merges.
REPO="joonhwan/herdr-reviewr"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/bin"

# The release tag matches the manifest version, so a checkout always pulls its own release.
VERSION="$(grep -m1 '^version' "$ROOT/herdr-plugin.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
TAG="v${VERSION}"

# Map the running platform to the release target triple. On Windows this script runs under the
# `sh` the manifest reaches (herdr-plugin.toml), so `uname -s` reports the MSYS/Cygwin flavour
# rather than a bare "Windows" — match the family, since the version suffix varies per install.
os="$(uname -s)"
arch="$(uname -m)"
case "$os-$arch" in
  Darwin-arm64)              target="aarch64-apple-darwin" ;;
  Darwin-x86_64)             target="x86_64-apple-darwin" ;;
  Linux-aarch64 | Linux-arm64) target="aarch64-unknown-linux-musl" ;;
  Linux-x86_64)              target="x86_64-unknown-linux-musl" ;;
  MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64)   target="x86_64-pc-windows-msvc" ;;
  MINGW*-aarch64 | MSYS*-aarch64 | CYGWIN*-aarch64 | MINGW*-arm64 | MSYS*-arm64 | CYGWIN*-arm64)
                             target="aarch64-pc-windows-msvc" ;;
  *)
    echo "$NAME: no prebuilt binary for $os-$arch — build from source with 'cargo install --path .'" >&2
    exit 1
    ;;
esac

# The release ships a zip for the Windows targets and a tarball everywhere else, which is what
# the release workflow's upload step produces per target (.github/workflows/release.yml).
case "$target" in
  *-pc-windows-msvc) is_windows=1; exe=".exe"; ext="zip"    ;;
  *)                 is_windows=0; exe="";     ext="tar.gz" ;;
esac

archive="${NAME}-${target}.${ext}"
# taiki-e's checksum sidecar drops the archive extension: <name>-<target>.sha256, not <archive>.sha256.
checksum="${NAME}-${target}.sha256"
base="https://github.com/${REPO}/releases/download/${TAG}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Release-asset downloads are eventually-consistent: GitHub's CDN can 404 for a few minutes
# after a release publishes, even though the asset exists. Retry (incl. on 404) so an install
# right after a release doesn't fail spuriously.
dl() { curl -fsSL --retry 5 --retry-delay 3 --retry-all-errors --retry-connrefused "$1" -o "$2"; }

echo "$NAME: downloading $archive ($TAG)"
dl "$base/$archive" "$tmp/$archive"
dl "$base/$checksum" "$tmp/$checksum"

echo "$NAME: verifying checksum"
expected="$(awk '{print $1}' "$tmp/$checksum")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$archive" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tmp/$archive" | awk '{print $1}')"
fi
if [ "$expected" != "$actual" ]; then
  echo "$NAME: checksum mismatch (expected $expected, got $actual)" >&2
  exit 1
fi

if [ "$ext" = "zip" ]; then
  # The shell's own `tar` is GNU tar, which cannot read a zip. `unzip` ships with Git for
  # Windows; bsdtar has been `tar.exe` in System32 since Windows 10 1803 and reads one, so name
  # it by path rather than letting `tar` resolve to the MSYS build.
  if command -v unzip >/dev/null 2>&1; then
    unzip -q -o "$tmp/$archive" -d "$tmp"
  elif [ -x "${SYSTEMROOT:-C:/Windows}/System32/tar.exe" ]; then
    "${SYSTEMROOT:-C:/Windows}/System32/tar.exe" -xf "$tmp/$archive" -C "$tmp"
  else
    echo "$NAME: cannot unpack $archive — no 'unzip' on PATH and no System32\\tar.exe" >&2
    exit 1
  fi
else
  tar -xzf "$tmp/$archive" -C "$tmp"
fi

# Replace through a fresh path, never in place. Both hazards are the same shape from opposite
# ends. Windows refuses an in-place swap while a pane runs the binary: measured, a delete
# returns access denied and an overwrite reports the file in use, while a rename still
# succeeds — so displace the old file and write the new one beside it. (`rm` appears to work
# under this shell only because MSYS emulates unlink with that same rename; nothing promises
# it.) macOS instead accepts the overwrite and punishes it: the inode survives, the
# code-signing cache calls the binary tampered with, and every launch SIGKILLs
# (docs/qa-install.md, Rule 1).
DST="$BIN_DIR/$NAME$exe"
mkdir -p "$BIN_DIR"
rm -f "$DST".old-* 2>/dev/null || true # displaced by an earlier install, freed once its pane closed
if [ -e "$DST" ] && ! mv "$DST" "$DST.old-$$"; then
  echo "$NAME: cannot replace $DST — close the panes running it and reinstall" >&2
  exit 1
fi
install -m 0755 "$tmp/$NAME$exe" "$DST"
rm -f "$DST.old-$$" 2>/dev/null || true # stays only while a pane still runs the old image
echo "$NAME: installed $DST"

# Stable launch paths (specs/herdr-host.md, Install paths): symlinks into the installed
# plugin, never copies, so a launch after an uninstall fails loudly instead of running a
# stale build. An existing symlink re-points on every install — the hash-suffixed plugin
# root moves — but anything else at the path is left alone: a user's own binary there
# (`cargo install --root ~/.local`) must survive, and `ln -sfn` onto a real directory
# would nest inside it. The binary is already installed, so a skipped or failed link
# warns without failing the install. This build may run in a staging checkout herdr
# renames afterwards, so the links aim at the runtime root when herdr provides one, and
# every action re-points them at the live root regardless (herdr/pane.sh).
#
# A platform that cannot link keeps no stable path at all. Windows is such a platform: its
# symlinks need Developer Mode or elevation, and MSYS falls back to copying — which is exactly
# the stale build these links exist to prevent. herdr/pane.sh skips the same block.
LINK_ROOT="${HERDR_PLUGIN_ROOT:-$ROOT}"
link_binary() {
  if mkdir -p "$1" 2>/dev/null && { [ -L "$1/$NAME" ] || [ ! -e "$1/$NAME" ]; } &&
    ln -sfn "$LINK_ROOT/bin/$NAME" "$1/$NAME" 2>/dev/null; then
    echo "$NAME: linked $1/$NAME"
  else
    echo "$NAME: warning: could not link $1/$NAME" >&2
  fi
}
if [ "$is_windows" -eq 1 ]; then
  echo "$NAME: no stable launch path on this platform; panes run $DST directly"
else
  link_binary "$HOME/.local/state/herdr/plugins/persiyanov.reviewr/bin"
  if [ -d "$HOME/.local/bin" ]; then
    link_binary "$HOME/.local/bin"
  fi
fi
