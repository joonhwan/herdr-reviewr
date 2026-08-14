//! Open a URL in the user's browser — the `PR` tab's only outward action.
//!
//! See `specs/markdown.md` (links). Unix mirrors the clipboard-tool probe in `export.rs`: the
//! first platform opener on `PATH` wins, and none present errors clearly. Windows has no such
//! program to probe and calls the shell association directly, the same split `export.rs` makes.

#[cfg(not(windows))]
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Platform openers, tried in order: macOS `open`, then the Linux `xdg-open`.
#[cfg(not(windows))]
const OPENERS: &[&str] = &["open", "xdg-open"];

/// Open `url` in the default browser via the first available opener. Errors when none is on
/// `PATH` (the caller surfaces it to the status line). The opener hands the URL to the browser
/// and exits at once, so this waits for it — reaping the child rather than leaving a zombie, and
/// returning fast enough for a click handler (mirrors the codebase's synchronous tool calls).
#[cfg(not(windows))]
pub fn open(url: &str) -> Result<()> {
    let tool = OPENERS
        .iter()
        .copied()
        .find(|t| crate::proc::on_path(t))
        .context("no URL opener found (need `open` or `xdg-open`)")?;
    let status = Command::new(tool)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("spawning {tool}"))?;
    if !status.success() {
        anyhow::bail!("{tool} failed to open the URL");
    }
    Ok(())
}

/// Open `url` in the default browser through the Windows shell's protocol association.
///
/// Windows has no `open`/`xdg-open` equivalent to spawn: `start` is a command-processor builtin,
/// not a program, so no `PATH` probe can find it. Every subprocess route puts a parser between
/// reviewr and the URL, and a forge URL carries exactly what those parsers mishandle. Two were
/// measured on Windows 11 and rejected:
///
/// - `cmd /C start` hands the destination to the command processor, which expands `%…%` and
///   splits on `&`.
/// - `explorer.exe` takes the URL but applies its own path-or-URL heuristic. Any query string
///   defeats it: `https://example.com/?x=1` silently opens a folder window instead of the link,
///   and its exit status reports success either way, so nothing can detect it.
///
/// `ShellExecuteW` is the association itself. The destination reaches it as one wide string with
/// no command line anywhere, so no parser can reinterpret it, and its documented result is a
/// genuine delivery signal rather than a process exit code. `openable_url` remains the single
/// gate, so only vetted `http(s)` ever arrives here.
/// See `specs/markdown.md` (links).
#[cfg(windows)]
pub fn open(url: &str) -> Result<()> {
    opener::open(url).with_context(|| format!("opening {url} in the default browser"))
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

    // The Windows opener has no unit test: it is one `ShellExecuteW` call, and the only way to
    // observe it is a browser window. What a test could have pinned — that no shell and no
    // command line stand between reviewr and the destination — the call shape now guarantees.
    // The acceptance check is manual (`docs/qa-scopes.md`, 브라우저 열기).

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
