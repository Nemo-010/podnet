//! The path of this binary on a host with no `/proc` (T-401): then
//! `std::env::current_exe` fails, and `argv[0]` names the file.

use std::path::{Path, PathBuf};

/// The path of the binary that a parent podnet gives its child (`--detach`
/// runs the child with `argv[0]` "podnet").
pub const SELF_VARIABLE: &str = "PODNET_SELF_EXE";

/// The path of this binary: `current_exe`, else [`from_argv0`].
pub fn this_binary() -> Option<PathBuf> {
    current().ok()
}

/// As `std::env::current_exe`, with [`from_argv0`] when it fails: the error
/// of `current_exe` when neither gives a file.
pub fn current() -> std::io::Result<PathBuf> {
    match std::env::current_exe() {
        Ok(path) => Ok(path),
        Err(err) => from_argv0().ok_or(err),
    }
}

/// The file of [`SELF_VARIABLE`] (an absolute path), else of `argv[0]` of
/// this process ([`path_of`]).
pub fn from_argv0() -> Option<PathBuf> {
    std::env::var_os(SELF_VARIABLE)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_file())
        .or_else(|| std::env::args_os().next().and_then(|a| path_of(&a)))
}

/// The file of `ARGV0`: a path with a `/` (relative to the working
/// directory), else the first executable file of that name in `PATH`.
pub fn path_of(argv0: &std::ffi::OsStr) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let executable = |p: &Path| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let given = Path::new(argv0);
    if argv0.as_encoded_bytes().contains(&b'/') {
        let path = if given.is_absolute() {
            given.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(given)
        };
        return executable(&path).then_some(path);
    }
    let search = std::env::var_os("PATH")?;
    std::env::split_paths(&search)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(given))
        .find(|p| executable(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_found_in_path_and_a_path_is_taken_as_it_is() {
        let sh = path_of("sh".as_ref()).expect("sh in PATH");
        assert!(sh.is_absolute(), "{}", sh.display());
        assert_eq!(path_of("/bin/sh".as_ref()), Some(PathBuf::from("/bin/sh")));
        assert_eq!(path_of("/no/such/podnet".as_ref()), None);
        assert_eq!(path_of("no-such-program-of-podnet".as_ref()), None);
        // A file that is not executable is not a program.
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "x").unwrap();
        assert_eq!(path_of(plain.as_os_str()), None);
    }
}
