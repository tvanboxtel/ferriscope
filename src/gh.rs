use std::ffi::OsString;

/// Finder-launched apps need not inherit a user's shell PATH. Resolve the
/// executable only; leave the child's environment and gh authentication alone.
pub(crate) fn executable() -> Result<OsString, String> {
    resolve("gh").map_err(|e| format!("{e} Install GitHub CLI and run `gh auth login`."))
}

pub(crate) fn resolve(name: &str) -> Result<OsString, String> {
    #[cfg(unix)]
    {
        use std::{env, path::PathBuf};
        let path = env::var_os("PATH").unwrap_or_default();
        let dirs = env::split_paths(&path).filter(|dir| dir.is_absolute());
        #[cfg(target_os = "macos")]
        let dirs = dirs
            .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
            .chain(dirs::home_dir().map(|home| home.join(".local/bin")));
        let path = std::path::Path::new(name);
        let found = if path.is_absolute() {
            is_executable(path).then(|| path.to_owned())
        } else if path.components().count() > 1 {
            return Err(format!(
                "Use an absolute executable path instead of {name}."
            ));
        } else {
            find_in(name, dirs)
        };
        found
            .map(PathBuf::into_os_string)
            .ok_or_else(|| format!("{name} not found on PATH or in standard install locations."))
    }
    #[cfg(windows)]
    {
        use std::path::Path;
        let path = Path::new(name);
        let exe = match path.extension() {
            Some(ext) if ext.eq_ignore_ascii_case("exe") => path.to_owned(),
            None => path.with_extension("exe"),
            _ => return Err("Choose a native .exe, not a shell script.".into()),
        };
        let found = if exe.is_absolute() {
            exe.is_file().then_some(exe)
        } else if exe.components().count() == 1 {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(&exe))
                .find(|p| p.is_file())
        } else {
            None
        };
        found
            .map(|p| p.into_os_string())
            .ok_or_else(|| format!("Executable {name} not found."))
    }
}

#[cfg(unix)]
fn is_executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(unix)]
fn find_in(
    name: &str,
    dirs: impl Iterator<Item = std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    dirs.map(|dir| dir.join(name)).find(|p| is_executable(p))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn path_wins_and_non_executables_do_not_hide_fallback() {
        let shell = tempfile::tempdir().unwrap();
        let brew = tempfile::tempdir().unwrap();
        let lookup = || find_in("gh", [shell.path().into(), brew.path().into()].into_iter());
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
