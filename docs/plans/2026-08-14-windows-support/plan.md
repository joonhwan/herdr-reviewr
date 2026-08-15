# Windows 11 support — Plan

No spec governs Windows yet. `herdr-plugin.toml` declares `platforms = ["macos", "linux"]`, and
`src/proc.rs` states the assumption outright: "Both shipped platforms are unix." This plan records
what a live Windows investigation established, and proposes the spec edits each phase needs.

**Status: all three phases landed.** D1 and D2 are decided. D3 remains open and gates nothing in
flight. `pane_actions` and `send_flow` now run on all three platforms — 677 tests on Windows, up
from 647 — and porting them found B6, a live defect in the pane sweep. One condition is still
unverified and needs a release to reach: `herdr plugin install` on a clean Windows box.

## Goal

`herdr-reviewr` runs on Windows 11 — first standalone (`cargo run` in any repo), then as a herdr
plugin pane with working toggle/open/close actions.

## Investigation record

Measured 2026-08-14 on Windows 11 Pro 10.0.26200, herdr 0.8.0, Rust 1.97.0, git 2.55.0.

| question                             | answer                                                                    |
| ------------------------------------ | ------------------------------------------------------------------------- |
| Does the crate compile?              | Yes. `cargo check --all-targets` clean, no source edits.                   |
| Do the tests pass?                   | Unit 268/270. Integration 367/367 (`app_flow`, `git_repo`, `render`, `pr_*`, `send_flow`). |
| Does herdr run on Windows?           | Yes. herdr 0.8.0, full CLI surface, this session ran inside a herdr pane.  |
| Does `herdr agent list` carry `cwd`? | Yes, native form (`D:\workspace\...`) plus `agent_status`.                 |
| Is the plugin installable?           | Untested. `platforms` excludes Windows and `herdr/install.sh` has no Windows target. |

### The core already ports

Both unit failures are POSIX assertions in the tests, not defects in the code under test.

- `git::tests::worktree_of_distinguishes_a_repo_from_a_plain_directory` — the test compares
  `worktree_of()` against `std::fs::canonicalize`, which yields `\\?\C:\…` where git yields
  `C:/…`. Production compares git output on both sides: `repo_root()` (`src/lib.rs:161`) and
  `TurnHost::member` (`src/world.rs:264`) each resolve through `git rev-parse --show-toplevel`,
  so the spellings match and membership resolves correctly.
- `world::tests::only_an_absolute_cwd_can_name_a_worktree` — the test feeds `/abs/path`, which
  is not absolute on Windows (no drive prefix). `worktree_cwd` is correct; the fixture is not.

Three existing choices are why the port is this cheap. Each already forbids a Windows-only
failure, so none needs revisiting:

- Every path read goes through `-c core.quotepath=false` with `-z` (`src/git.rs:19`), so
  non-ASCII and spaced paths survive verbatim.
- The turn baseline ref keys on an FNV-1a hash of the worktree path (`src/git.rs:1052`), so a
  drive-letter colon never reaches a ref name.
- `unsafe_code = "forbid"` blocked per-OS pointer tricks from accumulating.

Line endings are a verified non-issue, recorded so nobody re-investigates. The repo has no
`.gitattributes`, so a Windows checkout under `core.autocrlf=true` holds CRLF in the working tree
while git's blobs stay LF. `worktree_content` (`src/app.rs:3937`) reads those raw bytes, but every
consumer splits with `str::lines()`, which drops a trailing `\r`, and no site splits on `'\n'` by
hand. The `Changes` diff is unaffected either way, since git normalizes its own diff output.

### Platform adapters — the real defects

| id  | site                    | defect                                                                                          | evidence                                                                    |
| --- | ----------------------- | ----------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| A1  | `src/proc.rs:9`         | `on_path` tests `dir.join(name).is_file()`, so it never finds a `.exe`/`.cmd`/`.bat` and always returns false. | By inspection. Feeds A2 and A3.                                             |
| A2  | `src/export.rs:57`      | `CLIPBOARD_TOOLS` lists only `pbcopy`/`wl-copy`/`xclip`/`xsel`. Clipboard export is unreachable. | Comment already records it: "OSC 52 and Windows are roadmap."                |
| A3  | `src/browser.rs:11`     | `OPENERS` lists only `open`/`xdg-open`. Opening a PR link fails.                                 | `start` is a `cmd` builtin, not an executable, so the list cannot absorb it. |
| A4  | `src/azure_devops.rs:95`| `Command::new("az")` resolves only `az.exe`; Azure CLI ships `az.cmd`.                          | Not reproduced — `az` is absent here. `gh.exe` and `glab.exe` are unaffected.|

### Clipboard mechanism — measured

`clip.exe` decodes stdin with the **console codepage**, a shared per-console resource reviewr must
not mutate (the agent shares the pane's console). Same UTF-8 bytes, three codepages:

```
cp 65001 -> 리뷰 코멘트 한글        (this machine's default, so clip.exe looks fine here)
cp 949   -> 由щ럭 肄붾찘???쒓?      (Korean Windows default)
cp 437   -> δª¼δ╖░ ∞╜öδ⌐ÿφè╕ φò£Ω╕Ç
```

A UTF-16LE BOM does not fix it: the BOM survives as a literal U+FEFF in the clipboard.

| candidate                            | encoding                                   | latency               | verdict          |
| ------------------------------------ | ------------------------------------------ | --------------------- | ---------------- |
| `clip.exe`                           | codepage-dependent, corrupts on 949         | ~30 ms                | rejected         |
| `powershell -Command Set-Clipboard`  | argv crosses the ANSI boundary and corrupts | 250–373 ms (pwsh 476) | rejected         |
| `clipboard-win` 5.4.1                | UTF-16 conversion internal, lossless        | **0.32 ms**           | proposed         |
| OSC 52 escape                        | terminal-handled                            | 0 ms                  | separate concern |

`clipboard-win` was verified in a scratch crate carrying `unsafe_code = "forbid"`: it compiles and
round-trips `"리뷰 코멘트 — em dash + 한글\nsecond line"` exactly, in 317.6 µs. The lint is
per-crate, so a dependency's `unsafe` does not violate it. Gate it as
`[target.'cfg(windows)'.dependencies]` so no Unix dependency graph changes — `arboard` 3.6.1 is
the alternative but pulls `image`, `core-graphics`, and (on Linux) the wayland/x11 stacks this
project avoids by shelling out.

OSC 52 is the only candidate that also covers a Linux box with no `xclip`/`xsel`/`wl-copy`, but it
cannot report delivery, so it cannot satisfy the consume-on-success contract in `ExportTarget`. It
stays a separate decision, not a Windows blocker.

### Plugin packaging

`herdr/pane.sh` (286 lines) owns pane lifecycle and needs `bash`, `jq`, `mktemp -d`, `trap`,
background jobs with `wait`, `$'\n'` quoting, and `ln -sfn`. Three Windows defects:

| id  | defect                                                                                                | status                                                                                                    |
| --- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| B1  | `is_reviewr_pane` matches `argv0`/`argv[0]` basename against `herdr-reviewr`; the Windows binary is `herdr-reviewr.exe`. | **Resolved 2026-08-15**, in two layers. The spelling was worse than assumed: a live pane reports `argv0` as a full path in **backslashes** (`C:\Users\…\claude.EXE`), so the `split("/")` basename returned the whole string and matched nothing. Fixing that was necessary and not sufficient — see B5, which is what actually broke the toggle. |
| B5  | The pane's foreground process is the launcher, not the review UI.                                      | **Found by QA 2026-08-15, resolved.** The manifest launched the pane as `sh -c 'exec "$HERDR_PLUGIN_ROOT/bin/herdr-reviewr"'`. Windows has no `exec` that replaces a process image, so MSYS spawns and waits: the pane's process tree ran four deep (scoop `sh` shim → git `sh` → git `bash` → the binary) and herdr reported only the top. Identity read `sh` and every toggle opened another pane. The shim's `cwd` is `C:\WINDOWS\`, which is the same defect surfacing as `not a git repo: 'C:\WINDOWS\'` when a reviewr pane held focus. Fixed by removing the launcher on Windows: a `platforms = ["windows"]` pane entrypoint names the binary bare, and `pane.sh` puts the plugin's `bin` on the launched process's `PATH` through `plugin pane open --env`. The unix entrypoint is untouched. |
| B2  | Repo discovery reads `.foreground_cwd`; `pane list` on Windows omits the field.                        | **Resolved 2026-08-15.** The field is absent from every pane, confirmed. The fallback works: an invoked action's `HERDR_PLUGIN_CONTEXT_JSON` carries `focused_pane_cwd` as a native path. No code change needed. |
| B6  | The pane list is parsed with `jq`, and the Windows build of `jq` writes CRLF.                          | **Found by the ported tests 2026-08-15, resolved.** `pane_list` is the one multi-line `jq` read in `pane.sh`. A single-value read survives it — command substitution strips the trailing CR along with the newline — but a multi-line read keeps a CR on every line but the last, so every pane but the last was probed under an id herdr does not have and read as "not a reviewr pane". A `close` walked past its pane; a `toggle` stacked a second one. Fixed with `tr -d '\r'` on that read. QA missed it because it only ever bites a workspace whose reviewr pane is not last in the listing, and it is the answer to "what could the 28 tests have caught that QA did not" — the inverse of B5. |
| B3  | `install.sh` and `pane.sh` maintain `ln -sfn` symlinks under `$HOME/.local/…`.                         | **Resolved 2026-08-15.** Both skip the stable paths on Windows, where a symlink needs Developer Mode or elevation and MSYS falls back to copying. The lock half is now measured, and it is the inverse of the macOS hazard in `CLAUDE.md`: against a running `.exe` a delete returns access denied and a copy reports the file in use, while a rename succeeds — so every swap renames aside and writes beside. `rm` under Git Bash succeeds only because MSYS emulates unlink with that same rename. |

`tests/pane_actions.rs` holds 28 tests driving `pane.sh` against a fake herdr, covering exactly the
subtle contracts (race-lost close converging, unreadable read refusing, shape-missing refusing, live
cwd precedence). Its first line was `#![cfg(unix)]`, so **none of them compiled on Windows.**

`tests/send_flow.rs` was `#![cfg(unix)]` too — corrected 2026-08-14, having been listed above as
part of the passing integration run. It contributed 0 tests on Windows, so the integration total
there was 367 from `app_flow`, `git_repo`, `render`, and `pr_candidates` alone. That mattered more
than `pane_actions`: Send is the export path a Windows user reaches without the plugin, and it was
unit-covered but not end-to-end covered. Both were unix-only for one reason — each wrote its fake
herdr as a shell script through `PermissionsExt` — so both were fixed by the same move to a fake
herdr binary (Phase 2, item 4). The integration total on Windows is now 397, and the whole suite
677.

### herdr socket API

herdr exposes a versioned socket API — protocol 20, schema_version 1, 91 request methods, full JSON
schema via `herdr api schema --json`. The CLI is a thin client over it; `herdr --help` labels the
subcommands "helpers over the socket API", and CLI responses carry `id: "cli:<method>"`, meaning the
plugin already parses socket envelopes laundered through a subprocess. Transport, envelope, and
Windows specifics are recorded in `../herdr-api-notes.md` (Socket API).

Every call `pane.sh` and `src/herdr.rs` make exists as a method, and the socket is reachable from
`std` alone on both platforms — no crate, no `unsafe`. Measured against subprocess spawn:

```
socket  agent.list          0.46 ms/call      CLI  agent list          20.60 ms/call
socket  pane.process_info   0.55 ms/call      CLI  pane process-info   33.26 ms/call
```

This retires `pane.sh`'s most intricate machinery. Its concurrent per-pane probe — background jobs,
a `mktemp -d` scratch dir, per-index `.reviewr`/`.unreadable`/`.done` markers, a `trap` cleanup —
exists only to amortize spawn cost, as its own comment states ("the sweep costs one process-info
round-trip of wall clock, not one per pane"). At 0.55 ms a sequential sweep is cheaper than the
fork-and-collect dance.

## Open decisions

Each changes what gets built. None blocks Phase 1 except D1.

- **D1 — clipboard mechanism. Decided 2026-08-14: `clipboard-win` behind a `cfg(windows)` gate.**
  The rejected alternative was shipping Phase 1 with no Windows clipboard and leaving Send as the
  only export. Landed in `src/export.rs`; the contract sits in `specs/review-model.md` (export).
- **D2 — pane lifecycle strategy. Decided 2026-08-15: keep `pane.sh`, reach it through `sh`.**
  Every action, the event, and the build step run `["sh", "-c", "exec bash herdr/… <mode>"]`.
  The rejected alternatives were a PowerShell port of `pane.sh`, which would maintain the same
  subtle contracts twice, and lifting its logic into the binary as a subcommand, which stays the
  robust answer and is deferred rather than dismissed (see below). Measurements that decided it:
  - herdr resolves a command's first word on `PATH`. On Windows that reaches
    `C:\Windows\system32\bash.exe`, the **WSL launcher** — a live probe reported
    `Linux … microsoft-standard-WSL2`. WSL is not a worse shell, it is a different machine:
    `WSLENV` carried only `TERM`-family variables, so no `HERDR_*` variable crossed, and
    `pane.sh` would refuse at its workspace-context guard before reaching git or the socket.
  - `sh` reaches a real POSIX shell on all three platforms, and inside it `bash` resolves to that
    shell's own bash (`MINGW64_NT`, `/usr/bin/bash`) with `jq` and `git` both on `PATH`.
  - One action id carries one command for every platform. herdr honors a per-action `platforms`
    key and enforces it (`platform_unsupported`), but rejects a duplicate action id, so a
    Windows-only variant of `toggle` would need its own id and break the keybinding.
  - **The cost, accepted:** `sh` is on `PATH` here only because scoop shimmed it. Git for
    Windows keeps `sh.exe` in `bin/`, which its default install leaves off `PATH`, while
    `cmd/` holds only `git.exe` and friends. Where `sh` is missing the actions do nothing.
    `README.md` states the requirement. Removing it is the binary-subcommand option, which
    resolves `bash` from git's own location instead of from `PATH`.
- **D3 — socket transport.** `specs/herdr-host.md` Non-goals records "No herdr socket subscription.
  Turn tracking polls." That decision forbids **subscription** as the turn-tracking mechanism. It
  does not address replacing the subprocess with a socket client for the same polled request/response
  calls, which is behavior-neutral. Raising it because `pane.agent_status_changed` exists as an
  event, and the polling limitation the spec accepts at `specs/herdr-host.md:187` ("A turn shorter
  than one poll interval … is missed") is exactly what a subscription would remove. Revisiting the
  non-goal is a maintainer call, not a side effect of this plan.

## Phases

Phase 1 is independent of the herdr host. Phases 2 and 3 depend on D2, and Phase 2 must verify B1
and B2 live before its design settles.

### Phase 1 — standalone Windows

1. `src/proc.rs`: resolve executables through `PATHEXT` on Windows, keeping the Unix path as-is.
2. `src/export.rs`: split `Clipboard::export` on `cfg(windows)` per D1. `ExportTarget`, its
   messages, and the consume-on-success contract stay unchanged.
3. `src/browser.rs`: add a Windows opener that does not rely on a `cmd` builtin resolving on `PATH`,
   and keep `openable_url` as the single gate.
4. `src/azure_devops.rs`: resolve `az` including `.cmd`.
5. Fix the two POSIX test fixtures.
6. Spec edits (below) and a `CHANGELOG.md` entry.

**Done when:** `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` are green on
Windows and unchanged on macOS/Linux; a Korean-text comment round-trips through the clipboard
losslessly; a PR link opens in the default browser; `cargo run` in a Windows repo browses all three
scopes.

### Phase 2 — plugin pane

1. **Done.** B1 and B2 verified live by linking the plugin and invoking read-only actions; no pane
   was opened (`CLAUDE.md`).
2. **Done.** D2 implemented. `pane.sh` changed in two places only: `base` normalizes the launch
   spelling (B1), and the stable-launch-path block skips a platform without symlinks, since MSYS
   falls back to copying and a copy is the stale build that block exists to prevent.
3. **Done.** `platforms` carries `"windows"`. The pane command already used `sh -c`, so only the
   actions, the event, and the build step changed.
4. **Done 2026-08-15.** Both suites run on all three platforms. The fake herdr is now one binary,
   `tests/support/fake_herdr.rs`, behind a `test-support` feature so a `cargo install` of the
   crate still resolves to `herdr-reviewr` alone; `just test`, `just lint`, and CI all pass
   `--all-features`, and a run without it fails by name rather than skipping.

   **The `.cmd` route recorded here does not work**, and it was the plan's own assumption. Rust
   refuses to pass a batch file any argument holding a newline (its CVE-2024-24576 escaping),
   which is exactly the shape of a bracketed-paste payload, so a `.cmd` shim serves `pane_actions`
   and fails every `send_flow` send. A shell script fails earlier still: Windows has no shebang,
   so `Command::new` on one returns "not a valid Win32 application" — which is why the two tests
   that spawn the review binary directly needed the fake to be a binary too.

   Windows-only assertions joined at the same time: that an open names the `pane-windows`
   entrypoint and leads the pane's `PATH` with the plugin's `bin`, and that no stable launch path
   is created. Their unix halves stayed as they were, cfg-split rather than deleted.

Verified on Windows 11 10.0.26200. The `close` action ran end to end through the real herdr,
exiting 0 with `close: nothing open in <workspace>`, exercising the `sh`-to-`bash` chain, config
validation through the binary, the symlink skip, `herdr pane list`, the concurrent per-pane probe,
and `base` against a real Windows `argv0`. The user then ran the toggle by hand: one press logged
`opened wN:p9 (split) in wN` and the next logged `closed wN:p9 in wN`, the same pane, which is the
acceptance condition for this phase.

The user then ran `docs/qa-plugin-pane.md` T1 through T8 by hand, all passing: idempotent open,
converging close, the refusal outside a repository, the `worktree.created` event, the four
placements, and the absence of stable-path copies.

The two halves each caught what the other could not, which is the lesson to keep. QA drove the
whole of B5 out, and no test could have: that defect lives in how the host reports a process tree,
which every suite fakes. The ported tests then drove out B6, and QA could not have: that one needs
a workspace whose reviewr pane is not last in the listing, and every hand test so far had it last.

**Done when:** toggle/open/close and the `worktree.created` event behave per
`specs/herdr-host.md` on Windows; the pane-identity tests run on all three platforms; a second
toggle closes the pane it opened rather than stacking a duplicate.

### Phase 3 — build, release, QA

1. **Done 2026-08-15.** Both Windows targets joined the release matrix. The upload step already
   defaults to a zip on Windows and a tarball elsewhere, so only the attestation needed the
   archive name, which now comes from a per-target `ext`. The checksum sidecar keeps one
   spelling on every platform (`<name>-<target>.sha256`) because each target ships one archive.
2. **Done 2026-08-15.** `herdr/install.sh` maps the MSYS/Cygwin `uname` families to the two
   Windows triples, unpacks the zip through `unzip` or System32's bsdtar (the shell's own `tar`
   is GNU tar and cannot read one), and skips the stable-path symlinks per
   `specs/herdr-host.md`.
3. **Done 2026-08-15.** `just install` writes `bin/herdr-reviewr.exe`. `qa-install` and
   `qa-restore` no longer glob a unix path: `scripts/qa-plugin-bin.sh` asks herdr where the
   plugin lives, normalizes the `\\?\` spelling it reports on Windows, and refuses a linked
   checkout.
4. **Done 2026-08-15.** Ported to ConPTY. pywinpty 3 bridges the pty to a **socket** and exposes
   its descriptor, and `select` on Windows accepts sockets — so the timing loop is unchanged and
   shared, and only opening, reading, and writing the pty branch. The dependency is declared in
   the script's own PEP 723 block, Windows-only by marker, so unix keeps running it with no
   setup and the repo gains no packaging files.

   **The measurement is sound but not comparable to the existing baselines.** A unix pty is a
   pipe that returns the child's bytes; ConPTY is a renderer that keeps its own screen buffer
   and re-encodes it. Measured on this machine, three consecutive 8-iteration runs held their
   medians within ~1% of each other, and results span 2.5 ms to 520 ms with no floor, so nothing
   is being clipped to a repaint cadence. First-byte medians sit near 1.0 ms against ~0.4 ms on
   macOS, and the reload-bound scenarios near 520 ms against ~82 ms. `scripts/bench-results/`
   entries now carry a `pty` field, and `windows-baseline-5dfc51f.json` is the first Windows one.

Verified on Windows 11 10.0.26200. The install path was exercised end to end against a local
release fixture — mapping, checksum, zip unpack, and the replacement — including a reinstall over
a **running** executable, which succeeded while the process kept running. The premise behind that
replacement was measured rather than assumed: against a live process, a native delete returns
access denied and a copy over the file reports it in use, while a rename succeeds. Git Bash's `rm`
does succeed, but only because MSYS emulates unlink with that same rename.

`just install`, `just qa-install`, and `just qa-restore` all ran here. Only `install` completes on
this machine: the plugin is linked from this checkout rather than installed from GitHub, so the
other two stop at the refusal that case is supposed to produce.

**Done when:** `herdr plugin install` on a clean Windows box yields a working pane, and
`docs/qa-install.md` covers the Windows swap. The doc is written; the clean-box install is the
one condition still unverified, since it needs a release carrying the new Windows assets.

## Spec changes

Written when the phase lands, not before — several are still open decisions.

| spec                     | change                                                                                                     |
| ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `specs/review-model.md`  | **Landed.** The clipboard decision from D1. Not `specs/input.md`, which only maps the `copy` keybinding — the export contract lives with the review model. |
| `specs/markdown.md`      | **Landed.** A destination reaches the opener as one argument, and the Windows open reports on handoff. Not `specs/forge-host.md`, which never states how a link opens. |
| `specs/herdr-host.md`    | **Landed.** "clipboard utility" reworded to "clipboard export", since Windows spawns no utility. |
| `specs/herdr-host.md`    | **Landed.** Pane identity tolerating an executable suffix; install paths on a platform without usable symlinks. D3 stays open if the non-goal is revisited. |
| `specs/forge-providers.md` | Only if `az` resolution turns out to be a stated contract rather than an implementation detail.           |

## Out of scope

- Windows 10, and any terminal other than the one herdr provides.
- WSL. A WSL build is a Linux build and already covered.
- OSC 52 clipboard. Separate decision, own payoff on Linux, blocked by the delivery-signal problem.
- Turn-tracking subscription. Blocked on D3.
- `git.exe` performance work. The instrument runs here now, and the fixture's reload-bound
  scenarios paint in ~520 ms against ~82 ms on macOS. That gap is recorded, not investigated: the
  two numbers come from different ptys as well as different machines, so it is a question to open
  with a Windows-to-Windows A/B, not a finding.

## Likely files

| file                          | change                                             |
| ----------------------------- | -------------------------------------------------- |
| `src/proc.rs`                 | `PATHEXT` resolution on Windows                    |
| `src/export.rs`               | Windows clipboard per D1                           |
| `src/browser.rs`              | Windows URL opener                                 |
| `src/azure_devops.rs`         | `az.cmd` resolution                                |
| `src/git.rs`, `src/world.rs`  | test fixtures only                                 |
| `Cargo.toml`                  | `cfg(windows)` dependency per D1                   |
| `herdr/pane.sh`, `herdr-plugin.toml` | per D2                                      |
| `tests/pane_actions.rs`, `tests/send_flow.rs` | drop `#![cfg(unix)]`               |
| `tests/support/fake_herdr.rs` | the fake herdr, as a binary both suites share      |
| `herdr/install.sh`, `.github/workflows/release.yml` | Windows targets and packaging  |
| `docs/herdr-api-notes.md`     | Socket API section (landed with this plan)          |
