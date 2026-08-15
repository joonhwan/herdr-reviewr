//! The fake `herdr` the integration suites swap in for the real one (`specs/herdr-host.md`).
//!
//! Two callers reach it. `src/herdr.rs` spawns `$HERDR_BIN_PATH` for every herdr call reviewr
//! makes, and `herdr/pane.sh` runs the same variable for the pane actions — so one binary serves
//! `tests/send_flow.rs` and `tests/pane_actions.rs` alike, and a test reads back the argv it
//! recorded. Every answer is in the live envelope shape (`docs/herdr-api-notes.md`).
//!
//! It is a Rust binary rather than the `#!/bin/sh` scripts it replaces, which is what made both
//! suites `#![cfg(unix)]`. Windows has no shebang: `Command::new` on such a file fails with "not
//! a valid Win32 application". A `.cmd` shim cannot stand in either — Rust refuses to pass a
//! batch file any argument holding a newline, and every send carries its bracketed-paste payload
//! as exactly that. A real herdr is a binary too, so this is also the closer fake.
//!
//! Everything it answers comes from fixture files under `$FAKE_HERDR_DIR`, all optional:
//!
//! | file                  | effect                                                          |
//! | --------------------- | --------------------------------------------------------------- |
//! | `log`                 | written, not read: every invocation appends its argv, one a line |
//! | `fail`                | one argv prefix; a call starting with it fails `pane_not_found`  |
//! | `panes.json`          | the `pane list` answer (else one plain pane, `w1:p1`)            |
//! | `procinfo-<pane>.json`| that pane's `process-info` (else a plain shell, not the review UI) |
//! | `procfail-<pane>.json`| that pane's `process-info` fails, with this as its stderr        |
//! | `closefail-<pane>`    | that pane's `close` fails, with this as its stderr               |
//! | `configdir-hang`      | `plugin config-dir` hangs past the binary's answer bound         |
//! | `agents.json`         | the `agent list` answer                                          |

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// herdr's own refusal for a pane that is gone, as its callers see it: a JSON envelope on stderr
/// and a non-zero exit.
const PANE_NOT_FOUND: &str =
    r#"{"error":{"code":"pane_not_found","message":"pane w8:p1 not found"},"id":"cli:request"}"#;

/// Longer than the binary's `ANSWER_BOUND` (`src/herdr.rs`), so a test that hangs the lookup can
/// only pass by giving it up.
const HANG: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let dir = PathBuf::from(
        std::env::var("FAKE_HERDR_DIR").expect("FAKE_HERDR_DIR names the fixture directory"),
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Joined the way the shell fakes' `echo "$@"` joined it, so the log a test greps is
    // unchanged: one space between arguments, one newline per call.
    let call = args.join(" ");
    log(&dir, &call);

    // A prefix match, not equality: a test names `pane send-text w8:p1`, and what follows it is
    // the whole review.
    if let Ok(prefix) = fs::read_to_string(dir.join("fail"))
        && !prefix.is_empty()
        && call.starts_with(&prefix)
    {
        eprintln!("{PANE_NOT_FOUND}");
        return ExitCode::FAILURE;
    }

    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["pane", "list", ..] => print!("{}", pane_list(&dir)),
        ["pane", "process-info", "--pane", pane, ..] => return process_info(&dir, pane),
        ["pane", "close", pane, ..] => return close(&dir, pane),
        ["plugin", "config-dir", ..] => {
            if dir.join("configdir-hang").exists() {
                std::thread::sleep(HANG);
            }
            println!("{}", dir.display());
        }
        ["plugin", "pane", "open", ..] => {
            println!(
                r#"{{"result":{{"plugin_pane":{{"pane":{{"pane_id":"w1:p9","tab_id":"w1:t9"}}}}}}}}"#
            );
        }
        ["agent", "list", ..] => {
            print!("{}", fs::read_to_string(dir.join("agents.json")).expect("agents.json"));
        }
        ["tab", "list", ..] => {
            println!(r#"{{"result":{{"tabs":[{{"tab_id":"w8:t1","label":"Grip"}}]}}}}"#);
        }
        // `tab rename`, `pane send-text`, `agent focus`: recorded above, and answered with the
        // empty success their callers expect.
        _ => println!(r#"{{"result":{{}}}}"#),
    }
    ExitCode::SUCCESS
}

fn log(dir: &Path, call: &str) {
    let mut log =
        OpenOptions::new().create(true).append(true).open(dir.join("log")).expect("the call log");
    writeln!(log, "{call}").expect("record the call");
}

/// A pane id spelled as a fixture filename. Windows forbids `:` in a path, and forbids it
/// silently — a write lands in an NTFS alternate data stream nothing then finds — so the suites
/// fold it the same way when they lay the fixture down.
fn fixture_key(pane: &str) -> String {
    pane.replace(':', "-")
}

fn pane_list(dir: &Path) -> String {
    fs::read_to_string(dir.join("panes.json"))
        .unwrap_or_else(|_| r#"{"result":{"panes":[{"pane_id":"w1:p1"}]}}"#.to_string())
}

fn process_info(dir: &Path, pane: &str) -> ExitCode {
    let key = fixture_key(pane);
    if let Ok(failure) = fs::read_to_string(dir.join(format!("procfail-{key}.json"))) {
        eprint!("{failure}");
        return ExitCode::FAILURE;
    }
    match fs::read_to_string(dir.join(format!("procinfo-{key}.json"))) {
        Ok(info) => print!("{info}"),
        // The not-a-reviewr-pane baseline every placement test rests on: a plain login shell.
        Err(_) => println!(
            r#"{{"result":{{"process_info":{{"foreground_process_group_id":7,"foreground_processes":[{{"pid":7,"name":"zsh","argv0":"zsh","argv":["-zsh"],"cwd":"/"}}],"pane_id":"{pane}","shell_pid":1}}}}}}"#
        ),
    }
    ExitCode::SUCCESS
}

fn close(dir: &Path, pane: &str) -> ExitCode {
    if let Ok(failure) = fs::read_to_string(dir.join(format!("closefail-{}", fixture_key(pane)))) {
        eprint!("{failure}");
        return ExitCode::FAILURE;
    }
    println!(r#"{{"result":{{}}}}"#);
    ExitCode::SUCCESS
}
