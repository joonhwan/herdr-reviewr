# herdr-review dev tasks — run `just <task>` (https://github.com/casey/just)

# default: list tasks
default:
    @just --list

# format the code
fmt:
    cargo fmt --all

# check formatting (CI parity)
fmt-check:
    cargo fmt --all --check

# lint with clippy, warnings as errors (CI parity)
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# run the test suite
test:
    cargo test --all-features

# build (debug)
build:
    cargo build

# run reviewr in the current repo
run:
    cargo run

# build release and install the binary into bin/ for `herdr plugin link`
install:
    cargo build --release
    ./scripts/install.sh

# build release and swap it into the GitHub-installed plugin for local QA (docs/qa-install.md)
qa-install:
    cargo build --release
    ./scripts/qa-install.sh

# restore the released binary the last `just qa-install` replaced
qa-restore:
    ./scripts/qa-restore.sh

# everything CI runs, locally
ci: fmt-check lint test
    cargo build --release
