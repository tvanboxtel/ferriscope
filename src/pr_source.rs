//! A disposable GitHub source archive, not a Git checkout. Never materialize
//! links, special files, repository permissions, or paths outside our tempdir.
use futures_lite::io::AsyncReadExt;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Component, Path},
    process::Stdio,
};

const DOWNLOAD_LIMIT: u64 = 256 << 20;
const EXPANDED_LIMIT: u64 = 1024 << 20;

pub async fn download(repo: &str, head: &str) -> Result<tempfile::TempDir, String> {
    if head.len() != 40 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Missing exact GitHub commit ID.".into());
    }
    let source = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut archive = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut child = async_process::Command::new(crate::gh::executable()?)
        .args([
            "api",
            "--hostname",
            "github.com",
            &format!("repos/{repo}/tarball/{head}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Cannot start GitHub CLI: {e}"))?;
    let mut output = child.stdout.take().unwrap();
    let mut buf = [0; 65536];
    let mut total = 0;
    loop {
        let n = output.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > DOWNLOAD_LIMIT {
            return Err("Source download exceeds 256 MiB. Disable full PR source to review the diff instead.".into());
        }
        archive.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    if !child.status().await.map_err(|e| e.to_string())?.success() {
        return Err("GitHub could not download this commit’s source. Check repository access or disable full PR source.".into());
    }
    tokio::task::spawn_blocking(move || {
        use io::Seek;
        archive.rewind()?;
        extract(flate2::read::GzDecoder::new(archive), source.path())?;
        Ok::<_, io::Error>(source)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Could not unpack PR source: {e}. You can still review the diff."))
}

fn extract(input: impl Read, dest: &Path) -> io::Result<()> {
    let mut archive = tar::Archive::new(input.take(EXPANDED_LIMIT + 1));
    let mut has_file = false;
    for (i, entry) in archive.entries()?.enumerate() {
        if i >= 200_000 {
            return Err(io::Error::other("too many archive entries"));
        }
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if !kind.is_file() {
            continue;
        }
        let path = entry.path()?;
        let mut parts = path.components();
        if !matches!(parts.next(), Some(Component::Normal(_))) {
            return Err(io::Error::other("invalid archive root"));
        }
        let mut out = dest.to_owned();
        for part in parts {
            match part {
                Component::Normal(name) => out.push(name),
                _ => return Err(io::Error::other("invalid archive path")),
            }
        }
        if out == dest {
            return Err(io::Error::other("missing archive directory"));
        }
        fs::create_dir_all(out.parent().unwrap())?;
        let mut file = fs::File::create_new(out)?;
        let size = entry.size();
        if io::copy(&mut entry, &mut file)? != size {
            return Err(io::Error::other("incomplete source file"));
        }
        has_file = true;
    }
    if archive.into_inner().limit() == 0 {
        return Err(io::Error::other("expanded source exceeds 1 GiB"));
    }
    if !has_file {
        return Err(io::Error::other("empty source archive"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_pinned_public_source() {
        // Explicitly enabled read-only smoke test; does not invoke Codex.
        if std::env::var_os("FERRISCOPE_TEST_PUBLIC_ARCHIVE").is_none() {
            return;
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let source = runtime
            .block_on(download(
                "tvanboxtel/ferriscope",
                "530967534d791632a0df88ddb5263aaecebb1955",
            ))
            .unwrap();
        assert!(source.path().join("Cargo.toml").is_file());
        assert!(source.path().join("src/main.rs").is_file());
        let path = source.path().to_owned();
        drop(source);
        assert!(!path.exists());
    }
}
