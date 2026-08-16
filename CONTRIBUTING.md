# Contributing

Thanks for wanting to make reviewr better. This page gets you from clone to merged PR.

## Setup

You need Rust (the exact toolchain is pinned by `rust-toolchain.toml` and installs itself on
first build) and [`just`](https://github.com/casey/just).

```bash
git clone https://github.com/persiyanov/herdr-reviewr
cd herdr-reviewr
just test          # the test suite
just run           # run reviewr against this repo
cargo run -- ~/some/repo   # or against any repo
```

`just ci` runs exactly what CI runs: format check, clippy with warnings as errors, tests, and a
release build. Green there means green in CI.

On Windows the recipes run under `sh`, the same requirement the plugin states in `README.md`, and
nothing beyond it: a recipe that needs more than one shell line calls a script in `scripts/`
rather than carrying a shebang, because a shebang recipe makes `just` translate a temp path with
`cygpath` — another Git for Windows tool its default install leaves off `PATH`.

To test a change inside real herdr panes, `just qa-install` swaps your build into the installed
plugin and `just qa-restore` brings the release back. The details and the sharp edges live in
`docs/qa-install.md`.

## How this repo works

**Specs are the contract.** Every behavior lives in a spec under `specs/` (`overview.md` is the
map). A change to behavior lands in the spec and the code together, in the same PR. If you find
the code and a spec disagreeing, that is a bug report worth filing on its own.

**The changelog is written as you go.** User-visible changes add a bullet under `## [Unreleased]`
in `CHANGELOG.md`. That text becomes the release notes verbatim, so write it for the person
reading the release page.

**Performance changes bring numbers.** The PTY benchmark measures what a user feels — keypress to
painted frame:

```bash
python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture
```

On Windows the pty is ConPTY, reached through `pywinpty`. The script declares that dependency in
its own PEP 723 block, so run it with a tool that reads one — there is no virtualenv to create:

```bash
uv run scripts/bench_tui.py --binary target/release/herdr-reviewr.exe --fixture
```

Baselines live in `scripts/bench-results/`, each stamped with the `pty` it ran on. Compare medians
only within one platform: a unix pty hands back the child's bytes verbatim, while ConPTY keeps its
own screen buffer and re-encodes it, so a Windows run and a macOS run measure the same keypress
over different machinery. For anything touching the reload, render, git, or highlight paths, run
it before and after under the same system load and put both numbers in the PR.
`examples/bench_latency.rs` attributes a slow number to its component calls.

## Pull requests

- Keep one PR to one concern.
- `just ci` green, spec updated with the behavior, changelog bullet added.
- Tests live beside the code (unit) and in `tests/` (integration, against real git repos).
  Test names read as sentences: `a_tab_switch_paints_the_stashed_frame_and_defers_its_reload`.

## Releasing

Maintainers only — the process is `docs/RELEASING.md` in full. Short version: bump two versions,
finalize the changelog, tag `vX.Y.Z`, and CI builds the binaries and publishes the release with
the changelog section as its notes.
