//! Open a URL in the user's browser — the `PR` tab's only outward action.
//!
//! See `specs/forge-host.md` (external links). Mirrors the clipboard-tool probe in
//! `export.rs`: the first platform opener on `PATH` wins; none present errors clearly.

use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Platform openers, tried in order: macOS `open`, then the Linux `xdg-open`.
#[cfg(not(windows))]
const OPENERS: &[&str] = &["open", "xdg-open"];

/// Whether a non-zero exit from the opener means the URL did not open.
///
/// True on unix: `open` and `xdg-open` both report the handoff. False on Windows, where
/// `explorer.exe` routinely exits non-zero on a perfectly successful open, so believing it would
/// turn every working link into a failure on the status line.
#[cfg(not(windows))]
const EXIT_STATUS_REPORTS_DELIVERY: bool = true;
#[cfg(windows)]
const EXIT_STATUS_REPORTS_DELIVERY: bool = false;

/// The command that hands `url` to the platform's browser, and the tool's name for messages.
/// Errors only when no opener exists — on unix, when none of `OPENERS` is on `PATH`.
#[cfg(not(windows))]
fn opener_command(url: &str) -> Result<(Command, &'static str)> {
    let tool = OPENERS
        .iter()
        .copied()
        .find(|t| crate::proc::on_path(t))
        .context("no URL opener found (need `open` or `xdg-open`)")?;
    let mut cmd = Command::new(tool);
    cmd.arg(url);
    Ok((cmd, tool))
}

/// Windows has no `open`/`xdg-open` equivalent on `PATH`: `start` is a command-processor builtin,
/// not a program, so no `PATH` probe can ever find it. `explorer.exe` is the shipped-everywhere
/// program that hands a URL to the shell's protocol association — the user's default browser.
///
/// Deliberately not `cmd /C start`: a forge-supplied URL carries `%` percent-encoding and `&`,
/// which the command processor expands and splits on before the browser sees them. Spawning
/// `explorer.exe` directly passes the URL as one argv entry with no shell in the path.
/// `openable_url` remains the single gate, so only vetted `http(s)` ever reaches here.
/// See `specs/markdown.md` (links): the destination reaches the opener as one argument.
#[cfg(windows)]
// The `Result` never carries an error here, because Windows has exactly one opener and it always
// exists. It stays so both platforms present one signature to `open`, which would otherwise need
// its own `cfg` split for a difference callers do not have.
#[allow(clippy::unnecessary_wraps)]
fn opener_command(url: &str) -> Result<(Command, &'static str)> {
    let mut cmd = Command::new("explorer.exe");
    cmd.arg(url);
    Ok((cmd, "explorer.exe"))
}

/// Open `url` in the default browser via the platform opener. Errors when none is available (the
/// caller surfaces it to the status line). The opener hands the URL to the browser and exits at
/// once, so this waits for it — reaping the child rather than leaving a zombie, and returning
/// fast enough for a click handler (mirrors the codebase's synchronous tool calls).
pub fn open(url: &str) -> Result<()> {
    let (mut cmd, tool) = opener_command(url)?;
    let status = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("spawning {tool}"))?;
    if open_failed(status.success()) {
        anyhow::bail!("{tool} failed to open the URL");
    }
    Ok(())
}

/// Whether a finished opener means the URL did not open. A clean exit is never a failure; a dirty
/// one is a failure only where the exit status carries that meaning.
fn open_failed(succeeded: bool) -> bool {
    EXIT_STATUS_REPORTS_DELIVERY && !succeeded
}

/// Gate a markdown link destination before it reaches the OS opener
/// (`specs/markdown.md`): trimmed, case-insensitive `http://`/`https://` with something
/// after the scheme, and no control or bidirectional-override character anywhere — a
/// destination the display would sanitize must never open as different bytes.
pub fn openable_url(url: &str) -> Result<&str, &'static str> {
    let trimmed = url.trim();
    let hostile = trimmed.chars().any(crate::markdown::hostile_char);
    let b = trimmed.as_bytes();
    let schemed = (b.len() > 7 && b[..7].eq_ignore_ascii_case(b"http://"))
        || (b.len() > 8 && b[..8].eq_ignore_ascii_case(b"https://"));
    if !hostile && schemed { Ok(trimmed) } else { Err("unsupported link scheme") }
}

#[cfg(test)]
mod tests {
    use super::openable_url;

    /// A URL carrying every byte a shell would reinterpret: `%` percent-encoding, `&`, `?`, `^`.
    const HOSTILE_TO_A_SHELL: &str = "https://ci.example/a%20b?x=1&y=2^3&%PATH%";

    #[cfg(windows)]
    #[test]
    fn the_windows_opener_hands_the_url_to_the_shell_handler_untouched() {
        let (cmd, name) = super::opener_command(HOSTILE_TO_A_SHELL).expect("an opener");
        assert_eq!(name, "explorer.exe");
        assert_eq!(cmd.get_program(), "explorer.exe");
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        // One argv entry, byte for byte. Routing this through `cmd /C start` would let the
        // command processor expand `%PATH%` and split on `&` before the browser ever saw it.
        assert_eq!(args, vec![HOSTILE_TO_A_SHELL.to_string()]);
    }

    #[test]
    fn only_a_platform_whose_exit_status_means_delivery_reports_a_dirty_exit_as_failure() {
        // A clean exit is a success everywhere.
        assert!(!super::open_failed(true));
        // A dirty one is a failure on unix, where `open`/`xdg-open` report the handoff. On
        // Windows `explorer.exe` exits non-zero on plain successful opens, so believing it would
        // turn every working link into a failed one on the status line.
        assert_eq!(super::open_failed(false), cfg!(not(windows)));
    }

    #[test]
    fn the_url_guard_admits_http_and_https_case_insensitively() {
        assert_eq!(openable_url("https://ci.example/1"), Ok("https://ci.example/1"));
        assert_eq!(openable_url("HTTP://ci.example"), Ok("HTTP://ci.example"));
        assert_eq!(openable_url("  https://x.dev  "), Ok("https://x.dev"), "trimmed");
    }

    #[test]
    fn the_url_guard_rejects_other_schemes_and_hostile_bytes() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https:evil", // scheme without authority
            "https://",   // nothing after the scheme
            "ftp://host",
            "https://a\u{202e}b",   // bidi override
            "https://a\u{1b}[31mb", // control character
            "",
        ] {
            assert!(openable_url(bad).is_err(), "{bad:?} must not open");
        }
    }
}
