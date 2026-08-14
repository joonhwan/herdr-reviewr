//! Small helpers for locating external command-line tools.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Whether `name` resolves to an executable on `PATH` — a dependency-free `which`. Shared by the
/// clipboard probe (`export.rs`) and the URL-opener probe (`browser.rs`).
#[must_use]
pub fn on_path(name: &str) -> bool {
    resolve(name).is_some()
}

/// Where `name` resolves to on `PATH`, or `None` when nothing there runs it.
///
/// On unix that is the bare name in the first `PATH` directory holding it. On Windows a program
/// is `name` plus one of the `PATHEXT` suffixes, and `CreateProcess` — what `Command::new` uses —
/// only ever appends `.exe`, so a `.cmd` or `.bat` launcher is invisible to it. Callers that must
/// run such a tool spawn this resolved path instead of the bare name (`azure_devops.rs`).
///
/// Only `PATH` is searched. Windows shells also try the current directory first; deliberately not
/// copied, since a review pane must not run a program because of where the user happens to stand.
#[must_use]
pub fn resolve(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    // Unix ignores `PATHEXT`; reading it unconditionally keeps one code path.
    let pathext = std::env::var_os("PATHEXT");
    search(std::env::split_paths(&path), &spellings(name, pathext.as_deref()), Path::is_file)
}

/// The first `dir`-joined spelling `exists` accepts, directory-major: a directory is tried against
/// every spelling before the next one, so an earlier `PATH` entry always shadows a later one.
fn search(
    dirs: impl IntoIterator<Item = PathBuf>,
    spellings: &[String],
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    dirs.into_iter().find_map(|dir| spellings.iter().map(|s| dir.join(s)).find(|c| exists(c)))
}

/// On unix the only spelling of a program is its own name.
#[cfg(not(windows))]
fn spellings(name: &str, _pathext: Option<&OsStr>) -> Vec<String> {
    vec![name.to_string()]
}

/// What Windows uses when `PATHEXT` is unset or is not valid Unicode. `.CMD` and `.BAT` carry the
/// tools that ship as launcher scripts rather than executables (`az`, `glab` under some installs).
#[cfg(windows)]
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC";

/// Every spelling of `name` Windows would consider a program, in `PATHEXT` order.
///
/// A name that already carries a `PATHEXT` extension is tried bare first — `gh.exe` must not
/// have to be found as `gh.exe.com`. A name whose extension `PATHEXT` does not list is just a
/// name (`foo.bar` is a file, not a program), so only the suffixed spellings stand.
#[cfg(windows)]
fn spellings(name: &str, pathext: Option<&OsStr>) -> Vec<String> {
    let raw = pathext.and_then(OsStr::to_str).unwrap_or(DEFAULT_PATHEXT);
    // A hand-edited `PATHEXT` can hold empty entries, stray spaces, and suffixes without the dot.
    let exts: Vec<String> = raw
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty() && *e != ".")
        .map(|e| if e.starts_with('.') { e.to_string() } else { format!(".{e}") })
        .collect();
    let carried = name.rsplit_once('.').map(|(_, ext)| ext);
    let already_a_program =
        carried.is_some_and(|ext| exts.iter().any(|e| e[1..].eq_ignore_ascii_case(ext)));

    let mut out = Vec::with_capacity(exts.len() + usize::from(already_a_program));
    if already_a_program {
        out.push(name.to_string());
    }
    out.extend(exts.iter().map(|e| format!("{name}{e}")));
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::search;

    /// The `exists` predicate for a fake filesystem: the listed paths, compared as written.
    fn fake<'a>(present: &'a [&'a str]) -> impl Fn(&Path) -> bool + use<'a> {
        move |path: &Path| present.iter().any(|p| Path::new(p) == path)
    }

    #[test]
    fn the_first_directory_holding_any_spelling_wins() {
        let dirs = [PathBuf::from("/a"), PathBuf::from("/b")];
        let found =
            search(dirs.iter().cloned(), &["tool".to_string()], fake(&["/b/tool", "/a/other"]));
        assert_eq!(found, Some(PathBuf::from("/b/tool")));
    }

    #[test]
    fn a_directory_is_tried_against_every_spelling_before_the_next_directory() {
        // Directory-major order: `/a` is exhausted before `/b` is touched, so `/a`'s second
        // spelling beats `/b`'s first. This is how a `PATH` shadow is meant to work.
        let dirs = [PathBuf::from("/a"), PathBuf::from("/b")];
        let spellings = ["az.exe".to_string(), "az.cmd".to_string()];
        let found = search(dirs.iter().cloned(), &spellings, fake(&["/a/az.cmd", "/b/az.exe"]));
        assert_eq!(found, Some(PathBuf::from("/a/az.cmd")));
    }

    #[test]
    fn nothing_present_resolves_to_nothing() {
        let dirs = [PathBuf::from("/a")];
        assert_eq!(search(dirs.iter().cloned(), &["tool".to_string()], fake(&[])), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_looks_for_the_bare_name_only() {
        use super::spellings;
        assert_eq!(spellings("az", None), vec!["az".to_string()]);
        assert_eq!(spellings("xdg-open", Some("/ignored".as_ref())), vec!["xdg-open".to_string()]);
    }

    #[cfg(windows)]
    mod windows {
        use super::super::{on_path, spellings};

        #[test]
        fn a_program_that_windows_always_ships_is_found_on_the_real_path() {
            // The whole defect this replaces: probing `dir.join(name).is_file()` never sees a
            // suffix, so every Windows probe answered "absent". `cmd` lives in System32, which
            // is on `PATH` on every Windows install.
            assert!(on_path("cmd"), "cmd.exe must resolve through PATHEXT");
            assert!(!on_path("herdr-reviewr-no-such-tool"), "an absent name stays absent");
        }

        #[test]
        fn an_extensionless_name_gains_every_pathext_suffix_in_order() {
            let got = spellings("az", Some(".COM;.EXE;.BAT;.CMD".as_ref()));
            assert_eq!(got, vec!["az.COM", "az.EXE", "az.BAT", "az.CMD"]);
        }

        #[test]
        fn a_missing_pathext_falls_back_to_the_windows_default() {
            // Azure CLI ships `az.cmd` and no `az.exe`, so `.CMD` must be in the fallback.
            let got = spellings("az", None);
            assert!(got.iter().any(|s| s.eq_ignore_ascii_case("az.exe")), "{got:?}");
            assert!(got.iter().any(|s| s.eq_ignore_ascii_case("az.cmd")), "{got:?}");
            assert!(got.iter().any(|s| s.eq_ignore_ascii_case("az.bat")), "{got:?}");
        }

        #[test]
        fn a_name_already_carrying_an_executable_extension_is_tried_bare_first() {
            let got = spellings("gh.exe", Some(".COM;.EXE".as_ref()));
            assert_eq!(got.first().map(String::as_str), Some("gh.exe"));
        }

        #[test]
        fn the_extension_already_carried_is_recognised_case_insensitively() {
            let got = spellings("GH.EXE", Some(".com;.exe".as_ref()));
            assert_eq!(got.first().map(String::as_str), Some("GH.EXE"));
        }

        #[test]
        fn an_extension_pathext_does_not_list_is_not_treated_as_executable() {
            // `foo.bar` is a name, not a program: only the suffixed spellings may match.
            let got = spellings("foo.bar", Some(".EXE".as_ref()));
            assert_eq!(got, vec!["foo.bar.EXE"]);
        }

        #[test]
        fn blank_and_dotless_pathext_entries_are_tolerated() {
            // A hand-edited `PATHEXT` can hold empty entries and suffixes written without the dot.
            let got = spellings("az", Some(".EXE;;CMD; .BAT ".as_ref()));
            assert_eq!(got, vec!["az.EXE", "az.CMD", "az.BAT"]);
        }
    }
}
