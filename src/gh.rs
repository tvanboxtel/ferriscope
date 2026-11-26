use std::ffi::OsString;

/// Finder-launched apps need not inherit a user's shell PATH. Resolve the
/// executable only; leave the child's environment and gh authentication alone.
pub(crate) fn executable() -> Result<OsString, String> {
    #[cfg(target_os = "macos")]
    {
        use std::{env, path::PathBuf};
        let path = env::var_os("PATH").unwrap_or_default();
        let dirs = env::split_paths(&path)
            .filter(|dir| !dir.as_os_str().is_empty())
            .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
        find_in(dirs).map(PathBuf::into_os_string).ok_or_else(|| {
            "GitHub CLI (gh) not found on PATH or in the standard Homebrew locations. Install it with `brew install gh`, then sign in with `gh auth login`.".into()
        })
    }
    #[cfg(not(target_os = "macos"))]
    Ok("gh".into())
}

#[cfg(target_os = "macos")]
fn find_in(dirs: impl Iterator<Item = std::path::PathBuf>) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    dirs.map(|dir| dir.join("gh")).find(|p| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn path_wins_and_non_executables_do_not_hide_fallback() {
        let shell = tempfile::tempdir().unwrap();
        let brew = tempfile::tempdir().unwrap();
        let lookup = || find_in([shell.path().into(), brew.path().into()].into_iter());
        assert_eq!(lookup(), None);
        let preferred = shell.path().join("gh");
        let fallback = brew.path().join("gh");
        fs::write(&preferred, "").unwrap();
        fs::write(&fallback, "").unwrap();
        fs::set_permissions(&fallback, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(lookup(), Some(fallback));
        fs::set_permissions(&preferred, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(lookup(), Some(preferred));
    }
}
