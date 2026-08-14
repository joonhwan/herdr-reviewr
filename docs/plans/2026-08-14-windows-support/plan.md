# Windows 11 support — Plan

No spec governs Windows yet. `herdr-plugin.toml` declares `platforms = ["macos", "linux"]`, and
`src/proc.rs` states the assumption outright: "Both shipped platforms are unix." This plan records
what a live Windows investigation established, and proposes the spec edits each phase needs.

**Status: Phase 1 landed.** D1 is decided. D2 and D3 remain open and gate Phases 2 and 3.

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
| B1  | `is_reviewr_pane` matches `argv0`/`argv[0]` basename against `herdr-reviewr`; the Windows binary is `herdr-reviewr.exe`. | **Unverified.** A live `claude` pane here reports `argv0: "claude"` with `name: "claude.exe"`, so `argv0` echoes the launch spelling. An absolute-path launch would carry `.exe`; a shim launch would not. Must be checked live. A case-insensitive `.exe` strip is safe either way. |
| B2  | Repo discovery reads `.foreground_cwd`; `pane list` on Windows omits the field.                        | Verified absent in this workspace. Falls back to `HERDR_PLUGIN_CONTEXT_JSON.focused_pane_cwd`; that path is unverified. A failure here refuses the open outright. |
| B3  | `install.sh` and `pane.sh` maintain `ln -sfn` symlinks under `$HOME/.local/…`.                         | Windows symlinks need Developer Mode or elevation, and the path convention is `%LOCALAPPDATA%`. Note the inverse of the macOS hazard in `CLAUDE.md`: a running `.exe` is **locked**, so replacement needs rename-then-replace, not in-place copy. |

`tests/pane_actions.rs` holds 28 tests driving `pane.sh` against a fake herdr, covering exactly the
subtle contracts (race-lost close converging, unreadable read refusing, shape-missing refusing, live
cwd precedence). Its first line is `#![cfg(unix)]`, so **none of them compiles on Windows.**

`tests/send_flow.rs` is `#![cfg(unix)]` too — corrected 2026-08-14, having been listed above as
part of the passing integration run. It contributes 0 tests on Windows, so the integration total
there is 367 from `app_flow`, `git_repo`, `render`, and `pr_candidates` alone. This matters more
than `pane_actions`: Send is the export path a Windows user reaches without the plugin, and it is
now unit-covered but not end-to-end covered. It is unix-only because it writes its fake herdr as a
shell script through `PermissionsExt`. Porting it needs a fake herdr Windows can execute, which is
the same problem Phase 2 item 4 solves for `pane_actions`, so it joins that item rather than
Phase 1.

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
- **D2 — pane lifecycle strategy.** Depend on git-bash plus `jq`; port `pane.sh` to PowerShell; or
  lift its decision logic into the binary as a subcommand, extending the `--resolve-plugin-config`
  precedent so one entry point serves every platform. The socket findings favor the third: bash,
  `jq`, and the parallel probe all become unnecessary, and B1/B2 become typed code.
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

1. Verify B1 and B2 live: link the plugin, invoke one action, read `herdr plugin log list`, and
   record the real `argv0` spelling and the cwd fields herdr provides. **Opening a pane is the
   user's keystroke** (`CLAUDE.md`); this step must not script pane opens.
2. Implement D2, resolving B1 and B2.
3. `herdr-plugin.toml`: add `"windows"` to `platforms`, and give the pane a launch command that
   needs no POSIX shell.
4. Port the 28 `pane_actions` tests and the `send_flow` suite off `#![cfg(unix)]`. Under D2's third
   option the fake herdr becomes a test binary reached through `HERDR_BIN_PATH`, which
   `src/herdr.rs:78` already honors, and one such binary serves both suites.

**Done when:** toggle/open/close and the `worktree.created` event behave per
`specs/herdr-host.md` on Windows; the pane-identity tests run on all three platforms; a second
toggle closes the pane it opened rather than stacking a duplicate.

### Phase 3 — build, release, QA

1. `.github/workflows/release.yml`: add `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`, with
   a zip asset and its `.sha256`.
2. `herdr/install.sh`: map the Windows targets, unpack a zip, and replace B3's symlinks with a
   Windows-appropriate install that never overwrites a running binary in place.
3. `justfile` and `scripts/*.sh`: make `install`, `qa-install`, and `qa-restore` work on Windows, or
   state plainly that they do not.
4. `scripts/bench_tui.py` imports `pty`, `fcntl`, and `termios` — Unix-only modules, so it fails at
   import. Either port it to ConPTY or record that the acceptance instrument is unavailable on
   Windows and that reload/render/git/highlight changes must be benchmarked on macOS or Linux.

**Done when:** `herdr plugin install` on a clean Windows box yields a working pane, and
`docs/qa-install.md` covers the Windows swap.

## Spec changes

Written when the phase lands, not before — several are still open decisions.

| spec                     | change                                                                                                     |
| ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `specs/review-model.md`  | **Landed.** The clipboard decision from D1. Not `specs/input.md`, which only maps the `copy` keybinding — the export contract lives with the review model. |
| `specs/markdown.md`      | **Landed.** A destination reaches the opener as one argument, and the Windows open reports on handoff. Not `specs/forge-host.md`, which never states how a link opens. |
| `specs/herdr-host.md`    | **Landed.** "clipboard utility" reworded to "clipboard export", since Windows spawns no utility. |
| `specs/herdr-host.md`    | Pane identity tolerating an executable suffix; install paths on a platform without usable symlinks. D3 if the non-goal is revisited. |
| `specs/forge-providers.md` | Only if `az` resolution turns out to be a stated contract rather than an implementation detail.           |

## Out of scope

- Windows 10, and any terminal other than the one herdr provides.
- WSL. A WSL build is a Linux build and already covered.
- OSC 52 clipboard. Separate decision, own payoff on Linux, blocked by the delivery-signal problem.
- Turn-tracking subscription. Blocked on D3.
- `git.exe` performance work. No Windows-specific slowness was observed; the acceptance instrument
  (Phase 3, item 4) does not run here, so no claim is made either way.

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
| `tests/pane_actions.rs`       | drop `#![cfg(unix)]`                               |
| `herdr/install.sh`, `.github/workflows/release.yml` | Windows targets and packaging  |
| `docs/herdr-api-notes.md`     | Socket API section (landed with this plan)          |
