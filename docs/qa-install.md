# QA-installing a local build

How to run a locally built `herdr-reviewr` inside the real herdr panes. Follow it exactly. Every
step exists because skipping it has broken a session before.

The installed plugin lives at `~/.config/herdr/plugins/github/persiyanov.reviewr-<hash>/` — under
`%APPDATA%\herdr\` on Windows — and its panes run `bin/herdr-reviewr` from that directory by
absolute path. QA means swapping that one file and restarting the panes.

Nothing here spells that path out. `scripts/qa-plugin-bin.sh` asks herdr for it
(`herdr plugin list --json`), which is what makes the same commands work on Windows, where the
tree sits elsewhere and herdr reports it as `\\?\C:\…`. That script also refuses a **linked**
checkout: a `plugin link` setup already runs the binary you build, so `just install` is the
command for it and there is no released binary to back up or restore.

## The one command

```
just qa-install
```

It builds the release binary, swaps it into the installed plugin, verifies the swap actually
runs, and prints what remains manual. The steps it performs, and why each one is load-bearing:

1. `cargo build --release`.
2. Back up the original binary to `bin/herdr-reviewr.release-backup`, only if no backup exists
   yet. A later run never overwrites the pristine release with an earlier QA build.
3. Replace the binary **through a fresh path**: rename the old one aside, `cp` to a staging name,
   `mv` into place, drop the displaced copy, then `codesign --force --sign -` on macOS.
4. Run `bin/herdr-reviewr --resolve-plugin-config` and require exit 0 before touching any pane.
5. Print the running panes still on the old binary.

## Rule 1: never overwrite the binary in place

Both platforms forbid it, from opposite ends.

**macOS accepts the overwrite and punishes it later.** `cp target/release/herdr-reviewr
<plugin>/bin/herdr-reviewr` onto an existing file keeps the old inode. macOS caches
code-signing state by inode, decides the file was tampered with, and SIGKILLs it at every
launch. The symptom: panes open and instantly die, the plugin scripts report
`configuration validation failed`, and the binary exits 137 with no output. Nothing in the logs
says why.

**Windows refuses it up front.** While a pane runs the binary, a delete returns access denied and
a copy over it reports the file in use — both measured. A rename is the one operation Windows
still allows on a running executable, and that is what the swap uses: displace the old file, write
the new one beside it, then delete the displaced copy (which only succeeds once its pane closes,
so a failure there is ignored). Git Bash's `rm` appears to work on a running `.exe` only because
MSYS emulates unlink with that same rename; nothing promises it, and `cmd` fails outright.

One script does this for every caller: `scripts/swap-binary.sh`. Do not improvise the swap.

## Rule 2: replacing the file does not touch running panes

A running pane keeps executing the binary image it was launched with, however many times the
file on disk changes. Refreshing inside reviewr (`r`) or reloading the herdr client does
nothing. Each reviewr pane must be closed and reopened. `pgrep -f herdr-reviewr` with start
times tells you which panes are still old; on Windows,
`tasklist /FI "IMAGENAME eq herdr-reviewr.exe"` lists them by image name only, so it counts every
reviewr on the machine rather than only the installed one.

## Rule 3: only the user restarts panes

The plugin's `open` and `toggle` actions act on the **focused workspace**, whatever
`HERDR_WORKSPACE_ID` says. Scripting them from outside herdr stacks every new pane into
whichever workspace happens to be focused. Closing is safe
(`herdr plugin action invoke close --plugin persiyanov.reviewr` sweeps the focused workspace's
reviewr panes), but opening is not. After the swap, tell the user: press the reviewr toggle in
each space you want on the new build. Do not automate it.

## Verify

- `"$(./scripts/qa-plugin-bin.sh)" --resolve-plugin-config` exits 0 and prints the config JSON.
- After the user reopens a pane, its start time is later than the swap:
  `ps -o lstart= -p $(pgrep -f herdr-reviewr)`, or on Windows
  `tasklist /V /FI "IMAGENAME eq herdr-reviewr.exe"`.

## Rollback

```
just qa-restore
```

It puts `herdr-reviewr.release-backup` back through the same swap, so the inode and lock rules
above hold for the restore too. Then close and reopen the panes, same as any other swap. A full
reinstall (`herdr plugin install persiyanov/herdr-reviewr`) also restores the released binary.
